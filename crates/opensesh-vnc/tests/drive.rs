//! A VNC session driven as the app drives a remote desktop: the certificate question, a refused
//! password then the right one, the desktop, keys, the clipboard both ways, Ctrl+Alt+Del, and the
//! end.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers"
)]

use std::sync::Arc;
use std::time::Duration;

use opensesh_rdp_protocol::frame::Frame;
use opensesh_rdp_protocol::{Connect, Control, Event, FromHelper, Status, ToHelper};
use opensesh_vnc::security::vencrypt;
use opensesh_vnc::testing::{self, Offer, PASSWORD, Rules, SERVER_TEXT};
use tokio::sync::mpsc;

struct App {
    to: mpsc::UnboundedSender<ToHelper>,
    from: mpsc::UnboundedReceiver<FromHelper>,
    frame: Frame,
    clipboard: Option<String>,
}

impl App {
    fn say(&self, message: ToHelper) {
        self.to.send(message).unwrap();
    }

    async fn until(
        &mut self,
        what: &str,
        wanted: impl Fn(&FromHelper, &App) -> bool,
    ) -> FromHelper {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let message = tokio::time::timeout_at(deadline, self.from.recv())
                .await
                .unwrap_or_else(|_| panic!("no {what}"))
                .unwrap_or_else(|| panic!("the session ended before {what}"));
            match &message {
                FromHelper::Event(Event::Size { width, height }) => {
                    self.frame.resize(*width, *height)
                }
                FromHelper::Pixels(pixels) => self.frame.copy_rect(pixels.rect, &pixels.rgba),
                FromHelper::Clipboard(text) => self.clipboard = Some(text.clone()),
                _ => {}
            }
            if wanted(&message, self) {
                return message;
            }
        }
    }

    fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        let at = (y * usize::from(self.frame.width()) + x) * 4;
        let pixels = self.frame.pixels();
        if at + 3 > pixels.len() {
            return [0, 0, 0];
        }
        [pixels[at], pixels[at + 1], pixels[at + 2]]
    }
}

fn status(message: &FromHelper) -> Option<&Status> {
    match message {
        FromHelper::Event(Event::Status(status)) => Some(status),
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_as_the_app_has_it() {
    let server = testing::serve(Rules {
        offers: vec![Offer::VeNCrypt(&[vencrypt::X509_VNC])],
        ..Rules::default()
    })
    .await
    .unwrap();
    let (to, inbox) = mpsc::unbounded_channel();
    let (sender, from) = mpsc::unbounded_channel();
    let out: opensesh_vnc::drive::Out = Arc::new(move |message| {
        let _ = sender.send(message);
    });
    let session = tokio::spawn(opensesh_vnc::drive::drive(inbox, out));
    let mut app = App {
        to,
        from,
        frame: Frame::default(),
        clipboard: None,
    };

    // A wrong password first: the certificate is asked about, then the refusal.
    app.say(ToHelper::Password("wrong".into()));
    app.say(ToHelper::Control(Control::Connect(Connect {
        address: "127.0.0.1".into(),
        port: server.port,
        server_name: "localhost".into(),
        user: testing::USER.into(),
        domain: None,
        width: 640,
        height: 400,
        scale_factor: 100,
        keyboard_layout: 0x0409,
        clipboard: true,
        timeout_secs: 10,
        client_name: "tests".into(),
        read_only: false,
        quality: Some(6),
        shared: true,
    })));
    let question = app
        .until("the certificate question", |m, _| {
            matches!(m, FromHelper::Event(Event::Certificate { .. }))
        })
        .await;
    let FromHelper::Event(Event::Certificate { subject, .. }) = question else {
        unreachable!()
    };
    assert!(subject.contains("opensesh-vnc-test-server"), "{subject}");
    app.say(ToHelper::Control(Control::Certificate { accept: true }));
    let refused = app
        .until("the refusal", |m, _| {
            matches!(status(m), Some(Status::Disconnected { .. }))
        })
        .await;
    assert!(
        matches!(status(&refused), Some(Status::Disconnected { code, .. }) if code == "auth"),
        "{refused:?}"
    );

    // The right one: the certificate isn't asked about again; the desktop takes the pane's size.
    app.say(ToHelper::Password(PASSWORD.into()));
    app.say(ToHelper::Control(Control::Reconnect));
    app.until("the desktop", |m, _| {
        matches!(status(m), Some(Status::Connected { .. }))
    })
    .await;
    app.until("the pane's size and its pixels", |_, app| {
        app.frame.width() == 640 && app.pixel(320, 200) == [0xEC, 0xEF, 0xF4]
    })
    .await;
    assert_eq!(server.seen().sizes, vec![(640, 400)]);

    // A key, as a keysym, repaints the square; Ctrl+Alt+Del goes as three keys.
    app.say(ToHelper::Control(Control::Keysym {
        keysym: 0x61,
        pressed: true,
    }));
    app.say(ToHelper::Control(Control::Keysym {
        keysym: 0x61,
        pressed: false,
    }));
    app.until("the key's colour", |_, app| {
        app.pixel(40, 40) == testing::KEY_COLOURS[1]
    })
    .await;
    app.say(ToHelper::Control(Control::CtrlAltDel));
    app.until("Ctrl+Alt+Del's colour", |_, app| {
        app.pixel(40, 40) == testing::KEY_COLOURS[0]
    })
    .await;

    // The clipboard both ways.
    app.say(ToHelper::Clipboard("copied in OpenSesh".into()));
    app.until("the clipboard square", |_, app| {
        app.pixel(100, 40) == testing::CLIPBOARD_COLOUR
    })
    .await;
    server.copy(SERVER_TEXT);
    app.until("the server's clipboard", |_, app| {
        app.clipboard.as_deref() == Some(SERVER_TEXT)
    })
    .await;

    app.say(ToHelper::Control(Control::Disconnect));
    tokio::time::timeout(Duration::from_secs(5), session)
        .await
        .expect("the session ended")
        .unwrap();
}
