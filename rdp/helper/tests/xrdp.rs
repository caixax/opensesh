//! The helper against a real server: xrdp (ADR 0034), in the CI job that installs one. It runs
//! only when `OPENSESH_TEST_XRDP` names the server (`host:port`), with the account in
//! `OPENSESH_TEST_XRDP_USER` and `OPENSESH_TEST_XRDP_PASSWORD`; elsewhere it passes at once.
//! When `OPENSESH_TEST_XRDP_CLIPBOARD` is set, the session copies that text when it starts, and
//! it must arrive here.
//!
//! It checks what the in-process server can't: TLS without NLA (xrdp doesn't do CredSSP), xrdp's
//! own bitmap updates, keys and the mouse, the clipboard from a real X session, and a resize
//! (display control, or connecting again).

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
use tokio::sync::mpsc::{Receiver, Sender};

struct App {
    to: Sender<ToHelper>,
    from: Receiver<FromHelper>,
    frame: Frame,
    pixels: usize,
    clipboard: Option<String>,
}

impl App {
    async fn say(&self, message: ToHelper) {
        self.to.send(message).await.unwrap();
    }

    /// Reads messages (applying pixels) until `state` holds; fails on a disconnection.
    async fn until(&mut self, what: &str, state: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !state(self) {
            let left = deadline.saturating_duration_since(Instant::now());
            let message = tokio::time::timeout(left, self.from.recv())
                .await
                .unwrap_or_else(|_| panic!("no {what}"))
                .unwrap_or_else(|| panic!("the helper ended before {what}"));
            match message {
                FromHelper::Event(Event::Size { width, height }) => {
                    self.frame.resize(width, height);
                }
                FromHelper::Pixels(pixels) => {
                    self.frame.copy_rect(pixels.rect, &pixels.rgba);
                    self.pixels += 1;
                }
                FromHelper::Clipboard(text) => self.clipboard = Some(text),
                FromHelper::Event(Event::Certificate { fingerprint, .. }) => {
                    assert!(fingerprint.starts_with("SHA256:"), "{fingerprint}");
                    self.say(ToHelper::Control(Control::Certificate { accept: true }))
                        .await;
                }
                FromHelper::Event(Event::Status(Status::Disconnected { code, reason })) => {
                    panic!("disconnected waiting for {what}: {code}: {reason}");
                }
                _ => {}
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_real_xrdp_server() {
    let Ok(server) = std::env::var("OPENSESH_TEST_XRDP") else {
        return;
    };
    // RUST_LOG (set in CI) shows the session; xrdp has no NLA, so no NTLM messages.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
    let (address, port) = server
        .rsplit_once(':')
        .expect("OPENSESH_TEST_XRDP is host:port");
    let user = std::env::var("OPENSESH_TEST_XRDP_USER").expect("OPENSESH_TEST_XRDP_USER");
    let password =
        std::env::var("OPENSESH_TEST_XRDP_PASSWORD").expect("OPENSESH_TEST_XRDP_PASSWORD");
    let (to, inbox) = tokio::sync::mpsc::channel(64);
    let (out, from) = tokio::sync::mpsc::channel(256);
    let helper = tokio::spawn(opensesh_rdp::drive(inbox, Out(out)));
    let mut app = App {
        to,
        from,
        frame: Frame::default(),
        pixels: 0,
        clipboard: None,
    };
    app.say(ToHelper::Password(password)).await;
    app.say(ToHelper::Control(Control::Connect(Connect {
        address: address.to_owned(),
        port: port.parse().expect("a port"),
        server_name: address.to_owned(),
        user,
        domain: None,
        width: 1024,
        height: 768,
        scale_factor: 100,
        keyboard_layout: 0x0409,
        clipboard: true,
        timeout_secs: 30,
        client_name: "opensesh-ci".into(),
    })))
    .await;
    app.until("the desktop", |app| {
        app.frame.width() == 1024 && app.frame.height() == 768 && app.pixels > 0
    })
    .await;

    // Keys and the mouse go through without ending the session.
    for pressed in [true, false] {
        app.say(ToHelper::Control(Control::Key {
            code: 0x1E,
            extended: false,
            pressed,
        }))
        .await;
    }
    app.say(ToHelper::Control(Control::Move { x: 200, y: 200 }))
        .await;

    // The session's clipboard.
    if let Ok(text) = std::env::var("OPENSESH_TEST_XRDP_CLIPBOARD") {
        app.until("the session's clipboard", |app| {
            app.clipboard.as_deref() == Some(text.as_str())
        })
        .await;
    }
    app.say(ToHelper::Clipboard("copied in OpenSesh".into()))
        .await;

    // A resize: through display control, or by connecting again.
    app.say(ToHelper::Control(Control::Resize {
        width: 800,
        height: 600,
    }))
    .await;
    app.until("the new size", |app| {
        app.frame.width() == 800 && app.frame.height() == 600
    })
    .await;

    app.say(ToHelper::Control(Control::Disconnect)).await;
    tokio::time::timeout(Duration::from_secs(10), helper)
        .await
        .expect("the helper ended")
        .unwrap();
}
