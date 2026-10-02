//! The client against the in-process server: every security type and version, the encodings in
//! turn, keys, the pointer, the clipboard both ways, the cursor, resizing and read-only sessions.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers"
)]

use std::time::Duration;

use opensesh_vnc::VncError;
use opensesh_vnc::canvas::Canvas;
use opensesh_vnc::client::{self, Input, Settings, Update};
use opensesh_vnc::decode;
use opensesh_vnc::messages::Version;
use opensesh_vnc::security::vencrypt;
use opensesh_vnc::testing::{self, Offer, PASSWORD, Rules, SERVER_TEXT};
use tokio::sync::mpsc;

fn settings(port: u16) -> Settings {
    Settings {
        address: "127.0.0.1".into(),
        port,
        server_name: "localhost".into(),
        user: testing::USER.into(),
        shared: true,
        read_only: false,
        quality: Some(6),
        compression: 6,
        timeout: Duration::from_secs(10),
    }
}

/// A session's other end: what the app would draw.
struct App {
    inputs: mpsc::UnboundedSender<Input>,
    updates: mpsc::Receiver<Update>,
    canvas: Canvas,
    clipboard: Option<String>,
    cursor: bool,
    session: tokio::task::JoinHandle<Result<(), VncError>>,
}

impl App {
    async fn start(settings: Settings, password: &str) -> Result<Self, VncError> {
        let connected = client::connect(&settings, password, |_| async { true }).await?;
        let (inputs, receiver) = mpsc::unbounded_channel();
        let (out, updates) = mpsc::channel(64);
        let session =
            tokio::spawn(async move { client::run(connected, &settings, receiver, out).await });
        Ok(Self {
            inputs,
            updates,
            canvas: Canvas::default(),
            clipboard: None,
            cursor: false,
            session,
        })
    }

    async fn until(&mut self, what: &str, state: impl Fn(&App) -> bool) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while !state(self) {
            let update = tokio::time::timeout_at(deadline, self.updates.recv())
                .await
                .unwrap_or_else(|_| panic!("no {what}"))
                .unwrap_or_else(|| panic!("the session ended before {what}"));
            match update {
                Update::Size { width, height } => self.canvas.resize(width, height),
                Update::Pixels { area, rgba } => self.canvas.put_rgbx(area, &rgba),
                Update::Cursor(_) => self.cursor = true,
                Update::Clipboard(text) => self.clipboard = Some(text),
            }
        }
    }

    fn pixel(&self, x: u16, y: u16) -> [u8; 3] {
        if x < self.canvas.width() && y < self.canvas.height() {
            self.canvas.pixel(x, y)
        } else {
            [0, 0, 0]
        }
    }
}

async fn until_seen(
    server: &testing::TestServer,
    what: &str,
    check: impl Fn(&testing::Seen) -> bool,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !check(&server.seen()) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the server didn't see {what}: {:?}",
            server.seen()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn a_session_with_vnc_authentication() {
    let server = testing::serve(Rules::default()).await.unwrap();
    // A wrong password is refused with the server's reason.
    let refused = App::start(settings(server.port), "wrong")
        .await
        .err()
        .unwrap();
    assert!(
        matches!(&refused, VncError::Auth(reason) if reason.contains("wrong password")),
        "{refused}"
    );

    let mut app = App::start(settings(server.port), PASSWORD).await.unwrap();
    // The window's body, and the key square in its first colour (the first update: Tight).
    app.until("the desktop", |app| {
        app.pixel(512, 384) == [0xEC, 0xEF, 0xF4] && app.pixel(40, 40) == testing::KEY_COLOURS[0]
    })
    .await;
    app.until("the cursor", |app| app.cursor).await;
    let seen = server.seen();
    assert_eq!(seen.shared, vec![true]);
    assert_eq!(
        seen.encodings[..5],
        [
            decode::COPY_RECT,
            decode::TIGHT,
            decode::ZRLE,
            decode::HEXTILE,
            decode::RAW
        ]
    );
    assert!(seen.encodings.contains(&(decode::QUALITY_0 + 6)));

    // A key repaints the square (the next update: ZRLE).
    for down in [true, false] {
        app.inputs.send(Input::Key { keysym: 0x61, down }).unwrap();
    }
    app.until("the key's colour", |app| {
        app.pixel(40, 40) == testing::KEY_COLOURS[1]
    })
    .await;
    until_seen(&server, "the key", |seen| {
        seen.keys == [(0x61, true), (0x61, false)]
    })
    .await;

    // The pointer, and the clipboard both ways (the next update: Hextile).
    app.inputs
        .send(Input::Pointer {
            buttons: 1,
            x: 100,
            y: 50,
        })
        .unwrap();
    app.inputs
        .send(Input::Clipboard("copied in OpenSesh".into()))
        .unwrap();
    app.until("the clipboard square", |app| {
        app.pixel(100, 40) == testing::CLIPBOARD_COLOUR
    })
    .await;
    until_seen(&server, "the pointer and the clipboard", |seen| {
        seen.pointer.contains(&(1, 100, 50))
            && seen.clipboard.as_deref() == Some("copied in OpenSesh")
    })
    .await;
    server.copy(SERVER_TEXT);
    app.until("the server's clipboard", |app| {
        app.clipboard.as_deref() == Some(SERVER_TEXT)
    })
    .await;

    // A new size, through SetDesktopSize (the next full update: Raw).
    app.inputs
        .send(Input::Resize {
            width: 800,
            height: 600,
        })
        .unwrap();
    app.until("the new size", |app| {
        app.canvas.width() == 800 && app.pixel(400, 300) == [0xEC, 0xEF, 0xF4]
    })
    .await;
    let seen = server.seen();
    assert_eq!(seen.sizes, vec![(800, 600)]);
    for encoding in [decode::TIGHT, decode::ZRLE, decode::HEXTILE, decode::RAW] {
        assert!(
            seen.sent.contains(&encoding),
            "{encoding} not used: {:?}",
            seen.sent
        );
    }

    // The app goes: the session ends without an error.
    drop(app.inputs);
    tokio::time::timeout(Duration::from_secs(5), app.session)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn vencrypt_with_a_certificate() {
    // X509Plain: the certificate is shown before the user name and password go.
    let server = testing::serve(Rules {
        offers: vec![
            Offer::VeNCrypt(&[vencrypt::X509_PLAIN, vencrypt::X509_VNC]),
            Offer::VncAuth,
        ],
        ..Rules::default()
    })
    .await
    .unwrap();
    let asked = std::sync::Arc::new(std::sync::Mutex::new(None));
    let kept = std::sync::Arc::clone(&asked);
    let connected = client::connect(&settings(server.port), PASSWORD, |certificate| async move {
        *kept.lock().unwrap() = Some(certificate);
        true
    })
    .await
    .unwrap();
    let certificate = asked.lock().unwrap().clone().unwrap();
    assert!(certificate.fingerprint.starts_with("SHA256:"));
    assert!(
        certificate.subject.contains("opensesh-vnc-test-server"),
        "{}",
        certificate.subject
    );
    assert_eq!(certificate.key_type, "ECDSA");
    assert_eq!(connected.init.name, testing::NAME);
    assert!(connected.encrypted);
    assert_eq!(server.seen().users, vec![testing::USER.to_owned()]);

    // A certificate refused: nothing more is sent.
    let refused = client::connect(&settings(server.port), PASSWORD, |_| async { false })
        .await
        .err()
        .unwrap();
    assert!(matches!(refused, VncError::Refused(_)), "{refused}");
    assert_eq!(server.seen().users.len(), 1);

    // X509Vnc, and a session over TLS.
    let server = testing::serve(Rules {
        offers: vec![Offer::VeNCrypt(&[vencrypt::X509_VNC])],
        ..Rules::default()
    })
    .await
    .unwrap();
    let mut app = App::start(settings(server.port), PASSWORD).await.unwrap();
    app.until("the desktop over TLS", |app| {
        app.pixel(512, 384) == [0xEC, 0xEF, 0xF4]
    })
    .await;

    // Anonymous TLS only: explained.
    let server = testing::serve(Rules {
        offers: vec![Offer::VeNCrypt(&[257, 258])],
        ..Rules::default()
    })
    .await
    .unwrap();
    let error = client::connect(&settings(server.port), PASSWORD, |_| async { true })
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("anonymous TLS"), "{error}");
}

#[tokio::test]
async fn older_versions_no_authentication_and_read_only() {
    // 3.3: the server picks VNC authentication.
    let server = testing::serve(Rules {
        version: Version::V3_3,
        ..Rules::default()
    })
    .await
    .unwrap();
    let mut app = App::start(settings(server.port), PASSWORD).await.unwrap();
    app.until("the desktop (3.3)", |app| {
        app.pixel(512, 384) == [0xEC, 0xEF, 0xF4]
    })
    .await;
    let refused = App::start(settings(server.port), "wrong")
        .await
        .err()
        .unwrap();
    assert!(matches!(refused, VncError::Auth(_)), "{refused}");

    // 3.7 without authentication, read-only: input never reaches the server.
    let server = testing::serve(Rules {
        version: Version::V3_7,
        offers: vec![Offer::None],
        ..Rules::default()
    })
    .await
    .unwrap();
    let mut read_only = settings(server.port);
    read_only.read_only = true;
    let mut app = App::start(read_only, "").await.unwrap();
    app.until("the desktop (3.7)", |app| {
        app.pixel(512, 384) == [0xEC, 0xEF, 0xF4]
    })
    .await;
    app.inputs
        .send(Input::Key {
            keysym: 0x61,
            down: true,
        })
        .unwrap();
    app.inputs
        .send(Input::Pointer {
            buttons: 1,
            x: 5,
            y: 5,
        })
        .unwrap();
    app.inputs.send(Input::Clipboard("no".into())).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let seen = server.seen();
    assert!(
        seen.keys.is_empty() && seen.pointer.is_empty() && seen.clipboard.is_none(),
        "{seen:?}"
    );

    // Nothing listening: the reason, quickly.
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let error = client::connect(&settings(closed), "", |_| async { true })
        .await
        .err()
        .unwrap();
    assert!(matches!(error, VncError::Io(_)), "{error}");
}
