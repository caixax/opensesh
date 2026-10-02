//! A VNC session as the app drives a remote desktop (ADR 0035): the same messages as the RDP
//! helper's (`opensesh-rdp-protocol`), so one pane runs either. The difference is where it runs:
//! the RDP helper is a program, a VNC session a task in the app.
//!
//! [`drive`] waits for the password and [`Control::Connect`], connects (asking the app about a
//! VeNCrypt server's certificate before any password goes), runs the session, and after a
//! disconnection waits for [`Control::Reconnect`]. Keys come as keysyms ([`Control::Keysym`]) or
//! characters; the pointer's buttons and the wheel become RFB's button mask.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use opensesh_rdp_protocol::frame::Rect;
use opensesh_rdp_protocol::{
    Button, Connect, Control, Event, FromHelper, Pixels, PointerPicture, Status, ToHelper,
};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use crate::VncError;
use crate::client::{self, Input, Settings, Update};
use crate::keysym;

/// Where a session's messages go (the app's connection). Must return at once.
pub type Out = Arc<dyn Fn(FromHelper) + Send + Sync>;

/// What a session remembers between connections.
#[derive(Default)]
struct Memory {
    password: Zeroizing<String>,
    /// The certificate accepted for this pane (asked again only if it changes).
    trusted: Option<String>,
}

fn settings(connect: &Connect) -> Settings {
    Settings {
        address: connect.address.clone(),
        port: connect.port,
        server_name: connect.server_name.clone(),
        user: connect.user.clone(),
        shared: connect.shared,
        read_only: connect.read_only,
        quality: connect.quality,
        compression: 6,
        timeout: Duration::from_secs(connect.timeout_secs.max(1)),
    }
}

/// Why a connection ended, as the app's state says it.
fn disconnected(error: &VncError) -> Status {
    let code = match error {
        VncError::Auth(_) => "auth",
        VncError::Refused(_) => "refused",
        VncError::Timeout => "timeout",
        VncError::Io(_) => "network",
        VncError::Unsupported(_) => "unsupported",
        VncError::Protocol(_) => "protocol",
    };
    Status::Disconnected {
        code: code.to_owned(),
        reason: error.to_string(),
    }
}

/// The conversation with the app, until it ends it.
pub async fn drive(mut inbox: mpsc::UnboundedReceiver<ToHelper>, out: Out) {
    let mut memory = Memory::default();
    let mut connect = loop {
        match inbox.recv().await {
            Some(ToHelper::Password(password)) => memory.password = Zeroizing::new(password),
            Some(ToHelper::Control(Control::Connect(connect))) => break connect,
            Some(ToHelper::Control(Control::Disconnect)) | None => return,
            Some(_) => {}
        }
    };
    loop {
        let ended = session(&connect, &mut memory, &mut inbox, &out).await;
        let Some(status) = ended else {
            return;
        };
        out(FromHelper::Event(Event::Status(status)));
        // Disconnected: until the app says to connect again (or goes).
        loop {
            match inbox.recv().await {
                Some(ToHelper::Password(password)) => memory.password = Zeroizing::new(password),
                Some(ToHelper::Control(Control::Reconnect)) => break,
                Some(ToHelper::Control(Control::Resize { width, height })) => {
                    connect.width = width;
                    connect.height = height;
                }
                Some(ToHelper::Control(Control::Disconnect)) | None => return,
                Some(_) => {}
            }
        }
    }
}

/// One connection: `None` when the app ended it, else why it ended.
async fn session(
    connect: &Connect,
    memory: &mut Memory,
    inbox: &mut mpsc::UnboundedReceiver<ToHelper>,
    out: &Out,
) -> Option<Status> {
    let settings = settings(connect);
    out(FromHelper::Event(Event::Status(Status::Connecting {
        label: connect.server_name.clone(),
    })));
    let password = memory.password.clone();
    let trusted = memory.trusted.clone();
    let mut accepted = None;
    let mut app_left = false;
    let connected = {
        let decide = |certificate: crate::tls::Certificate| {
            let accepted = &mut accepted;
            let app_left = &mut app_left;
            let inbox = &mut *inbox;
            async move {
                if trusted.as_deref() == Some(certificate.fingerprint.as_str()) {
                    return true;
                }
                out(FromHelper::Event(Event::Certificate {
                    fingerprint: certificate.fingerprint.clone(),
                    subject: certificate.subject,
                    key_type: certificate.key_type,
                }));
                loop {
                    match inbox.recv().await {
                        Some(ToHelper::Control(Control::Certificate { accept })) => {
                            if accept {
                                *accepted = Some(certificate.fingerprint);
                            }
                            return accept;
                        }
                        Some(ToHelper::Control(Control::Disconnect)) | None => {
                            *app_left = true;
                            return false;
                        }
                        Some(_) => {}
                    }
                }
            }
        };
        client::connect(&settings, &password, decide).await
    };
    if app_left {
        return None;
    }
    if accepted.is_some() {
        memory.trusted = accepted;
    }
    let connected = match connected {
        Ok(connected) => connected,
        Err(error) => return Some(disconnected(&error)),
    };
    let (width, height) = (connected.init.width, connected.init.height);
    out(FromHelper::Event(Event::Status(Status::Connected {
        width,
        height,
    })));
    if !connected.encrypted {
        out(FromHelper::Event(Event::Unencrypted));
    }

    let (inputs, receiver) = mpsc::unbounded_channel();
    let (sender, mut updates) = mpsc::channel(64);
    let running =
        tokio::spawn(async move { client::run(connected, &settings, receiver, sender).await });
    let mut running = std::pin::pin!(running);
    let mut pointer = Pointer::default();
    let mut keys: HashSet<u32> = HashSet::new();
    // A size asked for before the session could take it (the pane's, at once).
    if (connect.width, connect.height) != (width, height) && connect.width >= 200 {
        let _ = inputs.send(Input::Resize {
            width: connect.width,
            height: connect.height,
        });
    }
    loop {
        tokio::select! {
            ended = &mut running => {
                return Some(match ended {
                    Ok(Ok(())) => Status::Disconnected {
                        code: "ended".to_owned(),
                        reason: "the server ended the session".to_owned(),
                    },
                    Ok(Err(error)) => disconnected(&error),
                    Err(_) => Status::Disconnected {
                        code: "protocol".to_owned(),
                        reason: "the session stopped".to_owned(),
                    },
                });
            }
            update = updates.recv() => {
                let Some(update) = update else { continue };
                match update {
                    Update::Size { width, height } => {
                        out(FromHelper::Event(Event::Size { width, height }));
                    }
                    Update::Pixels { area, rgba } => out(FromHelper::Pixels(Pixels {
                        rect: Rect {
                            x: area.x,
                            y: area.y,
                            width: area.width,
                            height: area.height,
                        },
                        rgba,
                    })),
                    Update::Cursor(cursor) => out(FromHelper::Pointer(PointerPicture {
                        width: cursor.width,
                        height: cursor.height,
                        hot_x: cursor.hot_x,
                        hot_y: cursor.hot_y,
                        rgba: cursor.rgba,
                    })),
                    Update::Clipboard(text) => {
                        if connect.clipboard {
                            out(FromHelper::Clipboard(text));
                        }
                    }
                }
            }
            message = inbox.recv() => {
                let message = message?;
                for input in inputs_of(message, connect, &mut pointer, &mut keys, memory) {
                    if let Some(input) = input {
                        let _ = inputs.send(input);
                    } else {
                        // The app ended the session.
                        return None;
                    }
                }
            }
        }
    }
}

/// The pointer's position and buttons.
#[derive(Debug, Default)]
struct Pointer {
    x: u16,
    y: u16,
    buttons: u8,
}

impl Pointer {
    fn event(&self) -> Input {
        Input::Pointer {
            buttons: self.buttons,
            x: self.x,
            y: self.y,
        }
    }

    /// One wheel step: the button down, then up.
    fn wheel(&self, bit: u8) -> [Input; 2] {
        [
            Input::Pointer {
                buttons: self.buttons | bit,
                x: self.x,
                y: self.y,
            },
            self.event(),
        ]
    }
}

/// What a message from the app sends to the server; `None` in the list ends the session.
fn inputs_of(
    message: ToHelper,
    connect: &Connect,
    pointer: &mut Pointer,
    keys: &mut HashSet<u32>,
    memory: &mut Memory,
) -> Vec<Option<Input>> {
    let key = |keysym: u32, down: bool| Some(Input::Key { keysym, down });
    match message {
        ToHelper::Password(password) => {
            memory.password = Zeroizing::new(password);
            Vec::new()
        }
        ToHelper::Clipboard(text) => {
            if connect.clipboard {
                vec![Some(Input::Clipboard(text))]
            } else {
                Vec::new()
            }
        }
        ToHelper::Control(control) => match control {
            Control::Disconnect => vec![None],
            Control::Keysym { keysym, pressed } => {
                if pressed {
                    keys.insert(keysym);
                } else {
                    keys.remove(&keysym);
                }
                vec![key(keysym, pressed)]
            }
            Control::Unicode { character, pressed } => {
                vec![key(keysym::of_char(character), pressed)]
            }
            Control::Move { x, y } => {
                pointer.x = x;
                pointer.y = y;
                vec![Some(pointer.event())]
            }
            Control::Button { button, pressed } => {
                let bit = match button {
                    Button::Left => 1,
                    Button::Middle => 2,
                    Button::Right => 4,
                    // RFB's mask has no room for them.
                    Button::Back | Button::Forward => return Vec::new(),
                };
                if pressed {
                    pointer.buttons |= bit;
                } else {
                    pointer.buttons &= !bit;
                }
                vec![Some(pointer.event())]
            }
            Control::Wheel {
                vertical,
                horizontal,
            } => {
                let mut events = Vec::new();
                for (amount, positive, negative) in [(vertical, 8_u8, 16_u8), (horizontal, 64, 32)]
                {
                    if amount == 0 {
                        continue;
                    }
                    // One step a notch (120), at least one.
                    let steps = (i32::from(amount).abs() / 120).clamp(1, 20);
                    let bit = if amount > 0 { positive } else { negative };
                    for _ in 0..steps {
                        events.extend(pointer.wheel(bit).map(Some));
                    }
                }
                events
            }
            Control::ReleaseAll => {
                let mut events: Vec<Option<Input>> =
                    keys.drain().map(|keysym| key(keysym, false)).collect();
                if pointer.buttons != 0 {
                    pointer.buttons = 0;
                    events.push(Some(pointer.event()));
                }
                events
            }
            Control::CtrlAltDel => {
                const KEYS: [u32; 3] = [0xffe3, 0xffe9, 0xffff];
                let mut events: Vec<Option<Input>> =
                    KEYS.iter().map(|keysym| key(*keysym, true)).collect();
                events.extend(KEYS.iter().rev().map(|keysym| key(*keysym, false)));
                events
            }
            Control::Resize { width, height } => vec![Some(Input::Resize { width, height })],
            Control::Connect(_)
            | Control::Certificate { .. }
            | Control::Key { .. }
            | Control::Locks { .. }
            | Control::Reconnect => Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connect() -> Connect {
        Connect {
            address: "127.0.0.1".into(),
            port: 5900,
            server_name: "desk".into(),
            user: String::new(),
            domain: None,
            width: 800,
            height: 600,
            scale_factor: 100,
            keyboard_layout: 0x0409,
            clipboard: true,
            timeout_secs: 5,
            client_name: "tests".into(),
            read_only: false,
            quality: None,
            shared: true,
        }
    }

    #[test]
    fn the_pointer_wheel_and_keys() {
        let connect = connect();
        let mut pointer = Pointer::default();
        let mut keys = HashSet::new();
        let mut memory = Memory::default();
        let mut run = |control| {
            inputs_of(
                ToHelper::Control(control),
                &connect,
                &mut pointer,
                &mut keys,
                &mut memory,
            )
        };
        assert_eq!(
            run(Control::Move { x: 3, y: 4 }),
            vec![Some(Input::Pointer {
                buttons: 0,
                x: 3,
                y: 4
            })]
        );
        assert_eq!(
            run(Control::Button {
                button: Button::Right,
                pressed: true
            }),
            vec![Some(Input::Pointer {
                buttons: 4,
                x: 3,
                y: 4
            })]
        );
        // Two notches down: button 5 (16) twice, the right button still held.
        let wheel = run(Control::Wheel {
            vertical: -240,
            horizontal: 0,
        });
        assert_eq!(wheel.len(), 4);
        assert_eq!(
            wheel[0],
            Some(Input::Pointer {
                buttons: 20,
                x: 3,
                y: 4
            })
        );
        assert_eq!(
            wheel[1],
            Some(Input::Pointer {
                buttons: 4,
                x: 3,
                y: 4
            })
        );
        run(Control::Keysym {
            keysym: 0x61,
            pressed: true,
        });
        // Released: the key, then the buttons.
        assert_eq!(
            run(Control::ReleaseAll),
            vec![
                Some(Input::Key {
                    keysym: 0x61,
                    down: false
                }),
                Some(Input::Pointer {
                    buttons: 0,
                    x: 3,
                    y: 4
                })
            ]
        );
        let ctrl_alt_del = run(Control::CtrlAltDel);
        assert_eq!(ctrl_alt_del.len(), 6);
        assert_eq!(
            ctrl_alt_del[5],
            Some(Input::Key {
                keysym: 0xffe3,
                down: false
            })
        );
        assert_eq!(run(Control::Disconnect), vec![None]);
    }
}
