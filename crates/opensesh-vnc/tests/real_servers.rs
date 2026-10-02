//! The client against real servers (ADR 0035), started by `scripts/vnc-test-servers.sh` in CI:
//! TigerVNC with VNC authentication and with VeNCrypt X509Vnc, x11vnc, and wayvnc with VeNCrypt's
//! user name and password. Ignored unless asked for (`-- --ignored`), as they need the servers.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers"
)]

use std::time::Duration;

use opensesh_vnc::canvas::Canvas;
use opensesh_vnc::client::{self, Input, Settings, Update};
use tokio::sync::mpsc;

/// The servers' password (scripts/vnc-test-servers.sh).
const PASSWORD: &str = "osvnc-ci";

fn settings(port: u16, user: &str) -> Settings {
    Settings {
        address: "127.0.0.1".into(),
        port,
        server_name: "localhost".into(),
        user: user.into(),
        shared: true,
        read_only: false,
        quality: Some(6),
        compression: 6,
        timeout: Duration::from_secs(20),
    }
}

struct App {
    inputs: mpsc::UnboundedSender<Input>,
    updates: mpsc::Receiver<Update>,
    canvas: Canvas,
    clipboard: Option<String>,
    rectangles: usize,
}

impl App {
    async fn connect(port: u16, user: &str, certificate: Option<&str>) -> Self {
        let settings = settings(port, user);
        let expected = certificate.map(str::to_owned);
        let connected = client::connect(&settings, PASSWORD, |shown| async move {
            let expected = expected.expect("a certificate the server shouldn't have shown");
            assert!(shown.subject.contains(&expected), "{}", shown.subject);
            true
        })
        .await
        .unwrap_or_else(|error| panic!("port {port}: {error}"));
        let (inputs, receiver) = mpsc::unbounded_channel();
        let (out, updates) = mpsc::channel(64);
        tokio::spawn(async move { client::run(connected, &settings, receiver, out).await });
        Self {
            inputs,
            updates,
            canvas: Canvas::default(),
            clipboard: None,
            rectangles: 0,
        }
    }

    async fn until(&mut self, what: &str, state: impl Fn(&App) -> bool) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        while !state(self) {
            let update = tokio::time::timeout_at(deadline, self.updates.recv())
                .await
                .unwrap_or_else(|_| panic!("no {what}"))
                .unwrap_or_else(|| panic!("the session ended before {what}"));
            match update {
                Update::Size { width, height } => self.canvas.resize(width, height),
                Update::Pixels { area, rgba } => {
                    self.canvas.put_rgbx(area, &rgba);
                    self.rectangles += 1;
                }
                Update::Cursor(_) => {}
                Update::Clipboard(text) => self.clipboard = Some(text),
            }
        }
    }

    /// The desktop showed something (a terminal), not only black.
    fn drawn(&self) -> bool {
        self.rectangles > 0
            && self
                .canvas
                .rgba()
                .chunks_exact(4)
                .any(|pixel| pixel[..3] != [0, 0, 0])
    }
}

/// The X display's clipboard.
fn clipboard_of(display: &str) -> String {
    let output = std::process::Command::new("xclip")
        .args(["-display", display, "-selection", "clipboard", "-o"])
        .output()
        .expect("xclip");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Copies `text` on the X display (xclip stays in the background to serve it).
fn copy_on(display: &str, text: &str) {
    use std::io::Write as _;
    let mut child = std::process::Command::new("xclip")
        .args(["-display", display, "-selection", "clipboard", "-i"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("xclip");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).unwrap();
    }
    child.wait().unwrap();
}

#[tokio::test]
#[ignore = "needs scripts/vnc-test-servers.sh"]
async fn tigervnc_with_vnc_authentication() {
    let mut app = App::connect(5901, "", None).await;
    app.until("the desktop", |app| {
        app.canvas.width() == 1024 && app.drawn()
    })
    .await;
    for down in [true, false] {
        app.inputs.send(Input::Key { keysym: 0x61, down }).unwrap();
    }
    app.inputs
        .send(Input::Pointer {
            buttons: 0,
            x: 100,
            y: 100,
        })
        .unwrap();
    // The clipboard both ways.
    app.inputs
        .send(Input::Clipboard("from OpenSesh".into()))
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while clipboard_of(":1") != "from OpenSesh" {
        assert!(
            std::time::Instant::now() < deadline,
            "the text didn't reach the X clipboard"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    copy_on(":1", "from the X session");
    app.until("the session's clipboard", |app| {
        app.clipboard.as_deref() == Some("from the X session")
    })
    .await;
    // A new size (Xvnc takes SetDesktopSize).
    app.inputs
        .send(Input::Resize {
            width: 800,
            height: 600,
        })
        .unwrap();
    app.until("the new size", |app| {
        app.canvas.width() == 800 && app.canvas.height() == 600
    })
    .await;
}

#[tokio::test]
#[ignore = "needs scripts/vnc-test-servers.sh"]
async fn tigervnc_with_vencrypt() {
    let mut app = App::connect(5902, "", Some("opensesh-vnc-ci")).await;
    app.until("the desktop over TLS", |app| {
        app.canvas.width() == 1024 && app.drawn()
    })
    .await;
}

#[tokio::test]
#[ignore = "needs scripts/vnc-test-servers.sh"]
async fn x11vnc() {
    let mut app = App::connect(5903, "", None).await;
    app.until("the desktop", |app| {
        app.canvas.width() == 1024 && app.drawn()
    })
    .await;
    for down in [true, false] {
        app.inputs.send(Input::Key { keysym: 0x61, down }).unwrap();
    }
    app.inputs
        .send(Input::Pointer {
            buttons: 0,
            x: 10,
            y: 10,
        })
        .unwrap();
    // Still connected afterwards.
    app.until("more updates", |app| app.rectangles > 1).await;
}

#[tokio::test]
#[ignore = "needs scripts/vnc-test-servers.sh"]
async fn wayvnc_with_a_user_name_and_password() {
    let mut app = App::connect(5904, "opensesh", Some("opensesh-vnc-ci")).await;
    app.until("the desktop", |app| {
        app.canvas.width() == 1024 && app.drawn()
    })
    .await;
    for down in [true, false] {
        app.inputs.send(Input::Key { keysym: 0x61, down }).unwrap();
    }
}
