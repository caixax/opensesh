//! The helper's conversation against the in-process RDP server (ADR 0034), as the app has it:
//! the certificate question, a refused password and a new one, the desktop's pixels, keys and a
//! click, the clipboard both ways, a resize, and the end.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers"
)]

use std::time::{Duration, Instant};

use opensesh_rdp::session::Out;
use opensesh_rdp_protocol::frame::Frame;
use opensesh_rdp_protocol::{Connect, Control, Event, FromHelper, Status, ToHelper};
use opensesh_rdp_testing::{PASSWORD, SERVER_TEXT, USER};
use tokio::sync::mpsc::{Receiver, Sender};

struct App {
    to: Sender<ToHelper>,
    from: Receiver<FromHelper>,
    frame: Frame,
    clipboard: Option<String>,
}

impl App {
    async fn say(&self, message: ToHelper) {
        self.to.send(message).await.unwrap();
    }

    /// Reads messages (applying pixels) until `wanted` matches one.
    async fn until(
        &mut self,
        what: &str,
        wanted: impl Fn(&FromHelper, &App) -> bool,
    ) -> FromHelper {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let message = tokio::time::timeout(left, self.from.recv())
                .await
                .unwrap_or_else(|_| panic!("no {what}"))
                .unwrap_or_else(|| panic!("the helper ended before {what}"));
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

    /// Like [`Self::until`], for a state that may already hold.
    async fn until_state(&mut self, what: &str, state: impl Fn(&App) -> bool) {
        if !state(self) {
            self.until(what, |_, app| state(app)).await;
        }
    }

    fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let at = (y * usize::from(self.frame.width()) + x) * 4;
        let pixels = self.frame.pixels();
        [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
    }
}

fn status(message: &FromHelper) -> Option<&Status> {
    match message {
        FromHelper::Event(Event::Status(status)) => Some(status),
        _ => None,
    }
}

fn close(a: [u8; 4], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 10)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_as_the_app_has_it() {
    // RUST_LOG=debug shows the session and the server.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
    let server = opensesh_rdp_testing::serve().unwrap();
    let (to, inbox) = tokio::sync::mpsc::channel(64);
    let (out, from) = tokio::sync::mpsc::channel(64);
    let helper = tokio::spawn(opensesh_rdp::drive(inbox, Out(out)));
    let mut app = App {
        to,
        from,
        frame: Frame::default(),
        clipboard: None,
    };

    // A wrong password first: the certificate is asked about once, then the refusal.
    app.say(ToHelper::Password("wrong".into())).await;
    app.say(ToHelper::Control(Control::Connect(Connect {
        address: "127.0.0.1".into(),
        port: server.port,
        server_name: "localhost".into(),
        user: USER.into(),
        domain: None,
        width: 640,
        height: 400,
        scale_factor: 100,
        keyboard_layout: 0x0409,
        clipboard: true,
        timeout_secs: 10,
        client_name: "tests".into(),
    })))
    .await;
    let question = app
        .until("the certificate question", |m, _| {
            matches!(m, FromHelper::Event(Event::Certificate { .. }))
        })
        .await;
    let FromHelper::Event(Event::Certificate {
        fingerprint,
        subject,
        key_type,
    }) = question
    else {
        unreachable!()
    };
    assert!(fingerprint.starts_with("SHA256:"), "{fingerprint}");
    assert!(subject.contains("opensesh-rdp-test-server"), "{subject}");
    assert_eq!(key_type, "ECDSA");
    app.say(ToHelper::Control(Control::Certificate { accept: true }))
        .await;
    let refused = app
        .until("the refusal", |m, _| {
            matches!(status(m), Some(Status::Disconnected { .. }))
        })
        .await;
    assert!(
        matches!(status(&refused), Some(Status::Disconnected { code, .. }) if code == "auth"),
        "{refused:?}"
    );

    // The right one: no second certificate question, then the desktop.
    app.say(ToHelper::Password(PASSWORD.into())).await;
    app.say(ToHelper::Control(Control::Reconnect)).await;
    let connected = app
        .until("the desktop", |m, _| {
            matches!(
                status(m),
                Some(Status::Connected { .. } | Status::Disconnected { .. })
            )
        })
        .await;
    assert_eq!(
        status(&connected),
        Some(&Status::Connected {
            width: 640,
            height: 400
        })
    );
    // The window's title bar and body, as the server paints them.
    app.until_state("the server's pixels", |app| {
        app.frame.width() == 640 && close(app.pixel(320, 200), [0xEC, 0xEF, 0xF4])
    })
    .await;
    // Opaque, whatever the codec left in the fourth byte.
    assert_eq!(app.pixel(320, 200)[3], 0xFF);

    // Keys repaint the square; a click and the wheel reach the server.
    let before = app.pixel(40, 40);
    for pressed in [true, false] {
        app.say(ToHelper::Control(Control::Key {
            code: 0x1E,
            extended: false,
            pressed,
        }))
        .await;
    }
    app.say(ToHelper::Control(Control::Move { x: 100, y: 50 }))
        .await;
    app.say(ToHelper::Control(Control::Button {
        button: opensesh_rdp_protocol::Button::Left,
        pressed: true,
    }))
    .await;
    app.say(ToHelper::Control(Control::Wheel {
        vertical: 120,
        horizontal: 0,
    }))
    .await;
    app.until_state("the square in a new colour", |app| {
        app.pixel(40, 40) != before
    })
    .await;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let seen = server.seen();
        if seen.keys.contains(&(0x1E, false, true))
            && seen.keys.contains(&(0x1E, false, false))
            && seen.moves.contains(&(100, 50))
            && seen.clicks == 1
            && !seen.wheel.is_empty()
        {
            break;
        }
        assert!(Instant::now() < deadline, "input missing: {seen:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // The clipboard: the server's text arrives; ours reaches it.
    app.until_state("the server's clipboard", |app| {
        app.clipboard.as_deref() == Some(SERVER_TEXT)
    })
    .await;
    app.say(ToHelper::Clipboard("copied in OpenSesh".into()))
        .await;
    let deadline = Instant::now() + Duration::from_secs(10);
    while server.seen().clipboard.as_deref() != Some("copied in OpenSesh") {
        assert!(
            Instant::now() < deadline,
            "the clipboard didn't reach the server"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // A resize: through display control, or by connecting again.
    app.say(ToHelper::Control(Control::Resize {
        width: 800,
        height: 500,
    }))
    .await;
    app.until_state("the new size", |app| {
        app.frame.width() == 800 && app.frame.height() == 500
    })
    .await;
    assert!(server.seen().sizes.contains(&(800, 500)));

    app.say(ToHelper::Control(Control::Disconnect)).await;
    tokio::time::timeout(Duration::from_secs(10), helper)
        .await
        .expect("the helper ended")
        .unwrap();
}
