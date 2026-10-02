//! `RdpItem`: a remote desktop for QML, RDP (Sprint 13, ADR 0034) or VNC (Sprint 14, ADR 0035).
//!
//! The C++ base `FramebufferItemBase` (`cpp/framebuffer_item.h`) draws the desktop in tiles and
//! turns Qt input into calls to its pure virtual functions; this Rust QObject derives from it.
//! Its session is a [`crate::rdp::Connection`] (the helper program), kept by pane id so the item
//! can be made again without reconnecting.
//!
//! **Questions** are asked as the SSH overlay asks them (the same `connection` and `prompt`
//! JSON, and `answerPrompt`): the server's certificate (unless it is remembered in
//! `trusted_certificates.toml`), the password (unless the host's identity has one, or again
//! after a refusal), and the jump hosts' own questions (host keys, passwords).
//!
//! **Jump hosts.** A host with jump hosts gets a local tunnel first: an SSH connection to the
//! last one (kept up by `opensesh_ssh::tunnel::keep_connected`) and a forward from a port on
//! 127.0.0.1 to the server; the helper connects there once the SSH connection is up. The
//! certificate is still checked against the server's name and port.
//!
//! **QML API** (besides the base's `scaleMode`, `backgroundColor`, `desktopWidth`,
//! `desktopHeight`, `wantedWidth`, `wantedHeight`, `escapeRequested()`,
//! `localClipboardChanged()` and `pixelAt(x, y)`):
//! - properties: `paneId`, `host`, `target`, `keyboardLocale` (Qt's input locale name), and
//!   read-only `connection`, `prompt`, `running` (the desktop shows), `connectionSerial`;
//! - invokables: `start()`, `answerPrompt(id, action, secrets)`, `reconnect()`,
//!   `disconnect()`, `sendCtrlAltDel()`, `resizeDesktop()` (to the item's size, in dynamic
//!   mode), `offerClipboard()` (this computer's clipboard to the server), and `sendText(text)`
//!   (characters as Unicode keys; a line break reconnects a disconnected pane, as the overlay's
//!   Reconnect does).
//!
//! **Test runs** never touch the user's clipboard: the server's text is kept for
//! `testClipboard()`, and `setTestClipboard(text)` stands for this computer's clipboard.

#[cxx_qt::bridge(namespace = "opensesh")]
pub mod qobject {
    // Qt types live in the global C++ namespace, not in the bridge's.
    #[namespace = ""]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        /// Qt string list type from cxx-qt-lib.
        type QStringList = cxx_qt_lib::QStringList;
    }

    unsafe extern "C++" {
        include!("opensesh-app/framebuffer_item.h");
        /// The C++ base (`cpp/framebuffer_item.h`).
        type FramebufferItemBase;
    }

    extern "RustQt" {
        /// A remote desktop pane.
        #[qobject]
        #[qml_element]
        #[base = FramebufferItemBase]
        #[qproperty(i32, pane_id, cxx_name = "paneId")]
        #[qproperty(QString, host)]
        #[qproperty(QString, target)]
        #[qproperty(QString, keyboard_locale, cxx_name = "keyboardLocale")]
        #[qproperty(QString, connection, READ, NOTIFY = session_changed)]
        #[qproperty(QString, prompt, READ, NOTIFY = session_changed)]
        #[qproperty(bool, running, READ, NOTIFY = session_changed)]
        #[qproperty(i32, connection_serial, cxx_name = "connectionSerial", READ, NOTIFY = session_changed)]
        type RdpItem = super::RdpItemRust;

        /// `connection`, `prompt` or `running` changed.
        #[qsignal]
        #[cxx_name = "sessionChanged"]
        fn session_changed(self: Pin<&mut RdpItem>);

        /// Connects (or attaches to the pane's running session).
        #[qinvokable]
        fn start(self: Pin<&mut RdpItem>);

        /// Answers question `id`: `trust-once`, `trust-save`, `submit` (the password) or
        /// `cancel`. Returns whether it was still waiting.
        #[qinvokable]
        #[cxx_name = "answerPrompt"]
        fn answer_prompt(
            self: Pin<&mut RdpItem>,
            id: i32,
            action: &QString,
            secrets: &QStringList,
        ) -> bool;

        /// Connects again after a disconnection.
        #[qinvokable]
        fn reconnect(self: Pin<&mut RdpItem>);

        /// Ends the session (the pane stays, disconnected).
        #[qinvokable]
        fn disconnect(self: Pin<&mut RdpItem>);

        /// Sends Ctrl+Alt+Del.
        #[qinvokable]
        #[cxx_name = "sendCtrlAltDel"]
        fn send_ctrl_alt_del(self: Pin<&mut RdpItem>);

        /// Asks the server for the item's size (dynamic mode).
        #[qinvokable]
        #[cxx_name = "resizeDesktop"]
        fn resize_desktop(self: Pin<&mut RdpItem>);

        /// Offers this computer's clipboard text to the server (when the host shares it).
        #[qinvokable]
        #[cxx_name = "offerClipboard"]
        fn offer_clipboard(self: Pin<&mut RdpItem>);

        /// Types `text` (Unicode keys); a line break reconnects a disconnected pane.
        #[qinvokable]
        #[cxx_name = "sendText"]
        fn send_text(self: Pin<&mut RdpItem>, text: &QString);

        /// Test runs only: what the server last copied (empty otherwise).
        #[qinvokable]
        #[cxx_name = "testClipboard"]
        fn test_clipboard(self: &RdpItem) -> QString;

        /// Test runs only: stands for this computer's clipboard, and offers it to the server.
        #[qinvokable]
        #[cxx_name = "setTestClipboard"]
        fn set_test_clipboard(self: Pin<&mut RdpItem>, text: &QString);

        /// The connection's session changed: read it (queued by the connection's waker).
        fn drain(self: Pin<&mut RdpItem>);
    }

    unsafe extern "RustQt" {
        /// Called by the renderer on the render thread, GUI thread blocked.
        #[cxx_override]
        #[cxx_name = "fillFramebuffer"]
        fn fill_framebuffer(self: Pin<&mut RdpItem>);

        /// A key.
        #[cxx_override]
        #[cxx_name = "handleKey"]
        fn handle_key(
            self: Pin<&mut RdpItem>,
            native_scan_code: u32,
            key: i32,
            text: &QString,
            pressed: bool,
            auto_repeat: bool,
        );

        /// A press, release or move, in desktop pixels.
        #[cxx_override]
        #[cxx_name = "handlePointer"]
        fn handle_pointer(self: Pin<&mut RdpItem>, kind: i32, button: i32, x: i32, y: i32);

        /// The wheel.
        #[cxx_override]
        #[cxx_name = "handleWheel"]
        fn handle_wheel(self: Pin<&mut RdpItem>, angle_x: i32, angle_y: i32);

        /// Focus.
        #[cxx_override]
        #[cxx_name = "handleFocusChange"]
        fn handle_focus_change(self: Pin<&mut RdpItem>, focused: bool);

        /// The item's size in device pixels.
        #[cxx_override]
        #[cxx_name = "handleWantedSize"]
        fn handle_wanted_size(self: Pin<&mut RdpItem>, width: i32, height: i32);

        /// A new size (render thread, from `fillFramebuffer`).
        #[inherit]
        #[cxx_name = "resizeFramebuffer"]
        fn resize_framebuffer(self: Pin<&mut RdpItem>, width: i32, height: i32);

        /// Pixels of the frame (render thread, from `fillFramebuffer`).
        #[inherit]
        #[cxx_name = "writePixels"]
        fn write_pixels(
            self: Pin<&mut RdpItem>,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            frame: &[u8],
            frame_stride: i32,
        );

        /// The server's pointer picture.
        #[inherit]
        #[cxx_name = "setRemotePointer"]
        fn set_remote_pointer(
            self: Pin<&mut RdpItem>,
            rgba: &[u8],
            width: i32,
            height: i32,
            hot_x: i32,
            hot_y: i32,
        );

        /// The pointer hidden, or the arrow.
        #[inherit]
        #[cxx_name = "setPointerHidden"]
        fn set_pointer_hidden(self: Pin<&mut RdpItem>, hidden: bool);

        /// This computer's clipboard.
        #[inherit]
        #[cxx_name = "setClipboardText"]
        fn set_clipboard_text(self: Pin<&mut RdpItem>, text: &QString);

        /// This computer's clipboard.
        #[inherit]
        #[cxx_name = "clipboardText"]
        fn clipboard_text(self: &RdpItem) -> QString;

        /// `fit`, `actual` or `dynamic`.
        #[inherit]
        #[cxx_name = "scaleMode"]
        fn scale_mode(self: &RdpItem) -> QString;

        /// The item's size in device pixels.
        #[inherit]
        #[cxx_name = "wantedWidth"]
        fn wanted_width(self: &RdpItem) -> i32;

        /// The item's size in device pixels.
        #[inherit]
        #[cxx_name = "wantedHeight"]
        fn wanted_height(self: &RdpItem) -> i32;

        /// `QQuickItem::update()`.
        #[inherit]
        fn update(self: Pin<&mut RdpItem>);
    }

    impl cxx_qt::Threading for RdpItem {}
}

use core::pin::Pin;
use std::sync::Arc;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use opensesh_core::hosts::{Protocol, Scaling};
use opensesh_core::trusted_certificates::Trust;
use opensesh_rdp_protocol::keys::{self, Platform};
use opensesh_rdp_protocol::{Button, Control, Status, ToHelper};
use opensesh_ssh::connect;
use opensesh_ssh::prompt::{Answer, Asker, HostKeyKind, HostKeyQuestion, Prompt, Request};
use opensesh_ssh::tunnel::{self, Endpoint, Forward, Link, Report};

use crate::bridge::app_info::is_test_run;
use crate::rdp::{self, Connection, PointerChange};

/// A question waiting for the user.
#[derive(Debug)]
enum Question {
    /// The server's certificate.
    Certificate(rdp::Certificate),
    /// The password; `first`: the helper hasn't connected yet.
    Password { first: bool },
    /// A jump host's question.
    Jump(Request),
}

/// The Rust side of `RdpItem`.
pub struct RdpItemRust {
    pane_id: i32,
    host: QString,
    target: QString,
    keyboard_locale: QString,
    connection: QString,
    prompt: QString,
    running: bool,
    connection_serial: i32,
    session: Option<Arc<Connection>>,
    plan: Option<rdp::Plan>,
    question: Option<(i32, Question)>,
    next_question: i32,
    /// `Connect` went to the helper.
    connect_sent: bool,
    /// The helper ended: connecting again starts a new one.
    helper_gone: bool,
    /// The tunnel's port here, once it listens (kept for a tunnel opened again).
    jump_port: Option<u16>,
    /// The jump host's SSH connection is up.
    jump_up: bool,
    /// The tunnel ended (an error that needs the user): connecting again opens it again.
    jump_ended: bool,
    /// Test runs: the clipboards (the server's text, and this computer's stand-in).
    test_clipboard: (String, String),
    /// VNC: the keys down, with the keysyms they went down with.
    keysyms: opensesh_vnc::keysym::Pressed,
    /// The frame's generation last drawn (a new one is drawn whole).
    drawn_generation: u64,
}

impl Default for RdpItemRust {
    fn default() -> Self {
        Self {
            pane_id: 0,
            host: QString::default(),
            target: QString::default(),
            keyboard_locale: QString::default(),
            connection: QString::default(),
            prompt: QString::default(),
            running: false,
            connection_serial: 0,
            session: None,
            plan: None,
            question: None,
            next_question: 1,
            connect_sent: false,
            helper_gone: false,
            jump_port: None,
            jump_up: false,
            jump_ended: false,
            test_clipboard: (String::new(), String::new()),
            keysyms: opensesh_vnc::keysym::Pressed::default(),
            drawn_generation: u64::MAX,
        }
    }
}

impl std::fmt::Debug for RdpItemRust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RdpItemRust")
            .field("pane_id", &self.pane_id)
            .finish_non_exhaustive()
    }
}

/// Where the keys come from: Windows and XKB scan codes map to RDP's; elsewhere Qt's keys do.
const fn platform() -> Platform {
    if cfg!(windows) {
        Platform::Windows
    } else if cfg!(target_os = "linux") {
        Platform::Xkb
    } else {
        Platform::Other
    }
}

/// `connection` for the overlay (the SSH panes' format).
fn status_json(status: &Status) -> String {
    let value = match status {
        Status::Connecting { label } => serde_json::json!({
            "state": "connecting", "index": 0, "count": 1, "label": label,
        }),
        Status::Authenticating { label } => serde_json::json!({
            "state": "authenticating", "label": label,
        }),
        Status::Connected { .. } => serde_json::json!({ "state": "connected" }),
        Status::Disconnected { code, reason } => serde_json::json!({
            "state": "disconnected", "code": code, "reason": reason, "retryIn": -1,
        }),
    };
    value.to_string()
}

/// A Qt mouse button as RDP's.
const fn button(qt_button: i32) -> Option<Button> {
    match qt_button {
        1 => Some(Button::Left),
        2 => Some(Button::Right),
        4 => Some(Button::Middle),
        8 => Some(Button::Back),
        16 => Some(Button::Forward),
        _ => None,
    }
}

impl qobject::RdpItem {
    fn send(&self, message: ToHelper) {
        if let Some(session) = &self.session {
            session.send(message);
        }
    }

    fn set_status(mut self: Pin<&mut Self>, status: &Status) {
        let running = matches!(status, Status::Connected { .. });
        {
            let mut this = self.as_mut().rust_mut();
            this.connection = QString::from(&status_json(status));
            if running && !this.running {
                this.connection_serial += 1;
            }
            this.running = running;
        }
        self.session_changed();
    }

    fn server(&self) -> (String, u16) {
        self.plan
            .as_ref()
            .map(|plan| plan.server.clone())
            .unwrap_or_default()
    }

    fn ask(mut self: Pin<&mut Self>, question: Question) {
        let id = self.next_question;
        let json = match &question {
            Question::Certificate(certificate) => {
                let (host, port) = self.server();
                let store = rdp::certificates();
                let kind = match store.check(&host, port, &certificate.fingerprint) {
                    Ok(Trust::Changed { known }) => HostKeyKind::Changed {
                        known_fingerprint: known,
                        file: store.path().display().to_string(),
                        line: 0,
                    },
                    _ => HostKeyKind::New {
                        other_types: Vec::new(),
                    },
                };
                let prompt = Prompt::HostKey(HostKeyQuestion {
                    host,
                    port,
                    key_type: certificate.key_type.clone(),
                    fingerprint: certificate.fingerprint.clone(),
                    kind,
                });
                // The overlay words a certificate as one, and shows whose it says it is.
                let text = crate::bridge::terminal_view::ssh_prompt_json(
                    u64::try_from(id).unwrap_or(0),
                    &prompt,
                );
                let mut value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                if let Some(object) = value.as_object_mut() {
                    object.insert("certificate".to_owned(), true.into());
                    object.insert("subject".to_owned(), certificate.subject.clone().into());
                }
                value.to_string()
            }
            Question::Password { first } => {
                let target = self
                    .plan
                    .as_ref()
                    .map(|plan| {
                        let user = &plan.connect.user;
                        let name = &plan.server.0;
                        if user.is_empty() {
                            name.clone()
                        } else {
                            format!("{user}@{name}")
                        }
                    })
                    .unwrap_or_default();
                crate::bridge::terminal_view::ssh_prompt_json(
                    u64::try_from(id).unwrap_or(0),
                    &Prompt::Password {
                        target,
                        retry: !first,
                    },
                )
            }
            Question::Jump(request) => crate::bridge::terminal_view::ssh_prompt_json(
                u64::try_from(id).unwrap_or(0),
                &request.prompt,
            ),
        };
        // A question it replaces is cancelled (its request dropped).
        drop(self.as_mut().rust_mut().question.take());
        {
            let mut this = self.as_mut().rust_mut();
            this.next_question += 1;
            this.question = Some((id, question));
            this.prompt = QString::from(&json);
        }
        self.session_changed();
    }

    /// The desktop follows the item's size.
    fn dynamic(&self) -> bool {
        self.scale_mode().to_string() == Scaling::Dynamic.as_str()
    }

    /// The desktop size to ask for: the host's fixed one (when the desktop doesn't follow the
    /// pane), else the item's.
    fn desktop_size(&self) -> (u16, u16) {
        let fixed = self.plan.as_ref().and_then(|plan| plan.size);
        if let Some(size) = fixed.filter(|_| !self.dynamic()) {
            return size;
        }
        // Display control takes an even width, and 200 to 8192 pixels a side (MS-RDPEDISP
        // 2.2.2.2.1).
        let side = |value: i32, default: u16| {
            u16::try_from(value)
                .ok()
                .filter(|side| (200..=8192).contains(side))
                .unwrap_or(default)
        };
        (
            side(self.wanted_width(), 1280) & !1,
            side(self.wanted_height(), 800),
        )
    }

    /// Sends the password, then connects (`first`) or connects again.
    fn connect_with(mut self: Pin<&mut Self>, password: String, first: bool) {
        if first {
            let Some(plan) = self.plan.clone() else {
                return;
            };
            let (width, height) = self.desktop_size();
            let mut connect = plan.connect;
            connect.width = width;
            connect.height = height;
            // A VNC desktop keeps the server's size unless it follows the pane.
            if plan.protocol == Protocol::Vnc && !self.dynamic() {
                connect.width = 0;
                connect.height = 0;
            }
            // Through the jump hosts: the tunnel's end here.
            if let (Some(_), Some(port)) = (&plan.jump, self.jump_port) {
                "127.0.0.1".clone_into(&mut connect.address);
                connect.port = port;
            }
            connect.keyboard_layout = keys::layout_id(&self.keyboard_locale.to_string());
            self.send(ToHelper::Password(password));
            self.send(ToHelper::Control(Control::Connect(connect)));
            self.as_mut().rust_mut().connect_sent = true;
        } else {
            self.send(ToHelper::Password(password));
            self.send(ToHelper::Control(Control::Reconnect));
        }
    }

    /// A new helper's first steps: the tunnel through the jump hosts (when there are some),
    /// then the password.
    fn begin(self: Pin<&mut Self>) {
        let jump = self.plan.as_ref().is_some_and(|plan| plan.jump.is_some());
        if jump && !self.jump_up {
            self.open_jump();
        } else {
            self.begin_password();
        }
    }

    /// Opens the tunnel through the jump hosts: an SSH connection to the last one, kept up, and
    /// a forward from 127.0.0.1 to the server. The connection goes on once both are ready.
    fn open_jump(mut self: Pin<&mut Self>) {
        let Some(plan) = self.plan.clone() else {
            return;
        };
        let (Some(spec), Some(session), Some(runtime)) =
            (plan.jump, self.session.clone(), opensesh_ssh::runtime())
        else {
            return;
        };
        let label = spec
            .hops
            .last()
            .map(|hop| hop.host.clone())
            .unwrap_or_default();
        {
            let mut this = self.as_mut().rust_mut();
            this.jump_up = false;
            this.jump_ended = false;
        }
        self.as_mut().set_status(&Status::Connecting { label });
        let thread = self.qt_thread();
        let asker: Asker = {
            let thread = thread.clone();
            Arc::new(move |request: Request| {
                // An item that is gone drops the request: the question is cancelled.
                let _ = thread.queue(move |item| item.ask(Question::Jump(request)));
            })
        };
        let connector: tunnel::Connector = Arc::new(move || {
            let spec = spec.clone();
            let asker = Arc::clone(&asker);
            Box::pin(async move { connect::connect(&spec, &asker, &connect::quiet()).await })
        });
        let link: tunnel::LinkSink = {
            let thread = thread.clone();
            Arc::new(move |link: Link| {
                let _ = thread.queue(move |item| item.jump_link(link));
            })
        };
        let report: tunnel::ReportSink = Arc::new(move |report: Report| {
            let _ = thread.queue(move |item| item.jump_report(report));
        });
        let kept = tunnel::keep_connected(runtime.handle(), connector, true, link);
        // The same port as before, when it is opened again: the helper knows that one.
        let forward = Forward::Local {
            bind: Endpoint::new("127.0.0.1", self.jump_port.unwrap_or(0)),
            to: Endpoint::new(plan.connect.address, plan.connect.port),
        };
        let running = tunnel::start(runtime.handle(), forward, kept.connections.clone(), report);
        session.set_jump(Some((kept, running)));
    }

    /// The jump host's SSH connection changed.
    fn jump_link(mut self: Pin<&mut Self>, link: Link) {
        match link {
            Link::Connected => {
                self.as_mut().rust_mut().jump_up = true;
                self.jump_ready();
            }
            Link::Connecting => self.as_mut().rust_mut().jump_up = false,
            // Shown as the SSH panes show a connection about to be tried again.
            Link::Retrying { in_secs, reason } => {
                tracing::info!("RDP jump host: trying again in {in_secs} s: {reason}");
                let json = serde_json::json!({
                    "state": "disconnected", "code": "jump",
                    "reason": format!("the jump host: {reason}"),
                    "retryIn": in_secs,
                });
                {
                    let mut this = self.as_mut().rust_mut();
                    this.jump_up = false;
                    this.connection = QString::from(&json.to_string());
                    this.running = false;
                }
                self.session_changed();
            }
            Link::Ended { code, reason } => {
                tracing::info!("RDP jump host: ended ({code}): {reason}");
                self.jump_failed(code, &reason);
            }
        }
    }

    /// The tunnel's forward changed.
    fn jump_report(mut self: Pin<&mut Self>, report: Report) {
        match report {
            Report::Listening(port) => {
                self.as_mut().rust_mut().jump_port = Some(port);
                self.jump_ready();
            }
            Report::Failed(reason) => self.jump_failed("listen", &reason),
            Report::Waiting => {}
        }
    }

    /// Goes on once the tunnel listens and its connection is up.
    fn jump_ready(self: Pin<&mut Self>) {
        if !self.jump_up || self.jump_port.is_none() || self.running {
            return;
        }
        if self.connect_sent {
            self.send(ToHelper::Control(Control::Reconnect));
        } else {
            self.begin_password();
        }
    }

    /// The tunnel ended: says why; connecting again opens it again.
    fn jump_failed(mut self: Pin<&mut Self>, code: &str, reason: &str) {
        if let Some(session) = &self.session {
            session.set_jump(None);
        }
        {
            let mut this = self.as_mut().rust_mut();
            this.jump_up = false;
            this.jump_ended = true;
        }
        self.set_status(&Status::Disconnected {
            code: code.to_owned(),
            reason: format!("the jump host: {reason}"),
        });
    }

    /// The password step: the identity's (from the keychain worker), else asked.
    fn begin_password(mut self: Pin<&mut Self>) {
        let identity = self.plan.as_ref().and_then(|plan| plan.identity.clone());
        let (Some(identity), Some(runtime)) = (identity, opensesh_ssh::runtime()) else {
            self.ask(Question::Password { first: true });
            return;
        };
        let label = self.server().0;
        self.as_mut().set_status(&Status::Connecting { label });
        let thread = self.qt_thread();
        runtime.spawn(async move {
            let password = rdp::identity_password(&identity).await;
            let _ = thread.queue(move |mut item| match password {
                Ok(Some(password)) => {
                    use secrecy::ExposeSecret as _;
                    item.as_mut()
                        .connect_with(password.expose_secret().to_owned(), true);
                }
                Ok(None) => item.ask(Question::Password { first: true }),
                Err(_) => item.set_status(&Status::Disconnected {
                    code: "locked".to_owned(),
                    reason: "the vault is locked".to_owned(),
                }),
            });
        });
    }

    /// See the bridge declaration.
    pub fn start(mut self: Pin<&mut Self>) {
        if self.session.is_some() {
            return;
        }
        let id = self.pane_id;
        let plan = match rdp::plan_for(&self.host.to_string(), &self.target.to_string()) {
            Ok(plan) => plan,
            Err(reason) => {
                self.set_status(&Status::Disconnected {
                    code: "invalid".to_owned(),
                    reason,
                });
                return;
            }
        };
        self.as_mut().rust_mut().plan = Some(plan);
        // The pane's running session (its item was made again).
        if let Some(existing) = rdp::get(id) {
            let (status, ended) = existing.latest();
            {
                let mut this = self.as_mut().rust_mut();
                this.connect_sent = status.is_some();
                this.helper_gone = ended.is_some();
            }
            self.as_mut().attach(existing);
            if let Some(status) = status {
                self.as_mut().set_status(&status);
            }
            return;
        }
        let vnc = self
            .plan
            .as_ref()
            .is_some_and(|plan| plan.protocol == Protocol::Vnc);
        let session = match if vnc {
            Connection::start_vnc()
        } else {
            Connection::start()
        } {
            Ok(session) => session,
            Err(reason) => {
                self.as_mut().rust_mut().helper_gone = true;
                self.set_status(&Status::Disconnected {
                    code: "no-helper".to_owned(),
                    reason,
                });
                return;
            }
        };
        rdp::keep(id, Arc::clone(&session));
        {
            let mut this = self.as_mut().rust_mut();
            this.connect_sent = false;
            this.helper_gone = false;
        }
        self.as_mut().attach(session);
        self.begin();
    }

    fn attach(mut self: Pin<&mut Self>, session: Arc<Connection>) {
        let thread = self.qt_thread();
        {
            let mut this = self.as_mut().rust_mut();
            this.session = Some(Arc::clone(&session));
            this.drawn_generation = u64::MAX;
        }
        session.attach(Arc::new(move || {
            let _ = thread.queue(|item| item.drain());
        }));
    }

    /// See the bridge declaration.
    pub fn drain(mut self: Pin<&mut Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let state = session.take();
        if let Some(status) = &state.status {
            self.as_mut().set_status(status);
            // A refused password is asked for again.
            if matches!(status, Status::Disconnected { code, .. } if code == "auth") {
                self.as_mut().ask(Question::Password { first: false });
            }
        }
        if let Some(certificate) = state.certificate {
            let (host, port) = self.server();
            match rdp::certificates().check(&host, port, &certificate.fingerprint) {
                Ok(Trust::Known) => {
                    self.send(ToHelper::Control(Control::Certificate { accept: true }));
                }
                _ => self.as_mut().ask(Question::Certificate(certificate)),
            }
        }
        match state.pointer {
            Some(PointerChange::Default) => self.as_mut().set_pointer_hidden(false),
            Some(PointerChange::Hidden) => self.as_mut().set_pointer_hidden(true),
            Some(PointerChange::Picture(picture)) => self.as_mut().set_remote_pointer(
                &picture.rgba,
                i32::from(picture.width),
                i32::from(picture.height),
                i32::from(picture.hot_x),
                i32::from(picture.hot_y),
            ),
            None => {}
        }
        if let Some(text) = state.clipboard {
            if is_test_run() {
                self.as_mut().rust_mut().test_clipboard.0 = text;
            } else {
                self.as_mut().set_clipboard_text(&QString::from(&text));
            }
        }
        if let Some(reason) = state.helper_ended {
            self.as_mut().rust_mut().helper_gone = true;
            // A disconnection already said why; a helper that ended while connected didn't.
            let said = self.connection.to_string().contains("\"disconnected\"");
            if !said {
                self.as_mut().set_status(&Status::Disconnected {
                    code: "ended".to_owned(),
                    reason,
                });
            }
        }
        if state.dirty {
            self.as_mut().update();
        }
    }

    /// See the bridge declaration.
    pub fn answer_prompt(
        mut self: Pin<&mut Self>,
        id: i32,
        action: &QString,
        secrets: &QStringList,
    ) -> bool {
        if self
            .question
            .as_ref()
            .is_none_or(|(waiting, _)| *waiting != id)
        {
            return false;
        }
        let question = {
            let mut this = self.as_mut().rust_mut();
            this.prompt = QString::default();
            this.question.take()
        };
        let Some((_, question)) = question else {
            return false;
        };
        self.as_mut().session_changed();
        let action = action.to_string();
        match question {
            Question::Certificate(certificate) => {
                let accept = action == "trust-once" || action == "trust-save";
                if action == "trust-save" {
                    let (host, port) = self.server();
                    if let Err(error) =
                        rdp::certificates().remember(&host, port, &certificate.fingerprint)
                    {
                        tracing::warn!("the certificate was not remembered: {error}");
                    }
                }
                self.send(ToHelper::Control(Control::Certificate { accept }));
            }
            Question::Password { first } => {
                if action == "submit" {
                    let password = secrets
                        .iter()
                        .next()
                        .map(ToString::to_string)
                        .unwrap_or_default();
                    self.connect_with(password, first);
                } else if first {
                    self.set_status(&Status::Disconnected {
                        code: "cancelled".to_owned(),
                        reason: "no password was given".to_owned(),
                    });
                }
            }
            Question::Jump(request) => request.answer(match action.as_str() {
                "trust-once" => Answer::TrustOnce,
                "trust-save" => Answer::TrustAndRemember,
                "submit" => Answer::Secrets(
                    secrets
                        .iter()
                        .map(|secret| secrecy::SecretString::from(secret.to_string()))
                        .collect(),
                ),
                _ => Answer::Cancel,
            }),
        }
        true
    }

    /// See the bridge declaration.
    pub fn reconnect(mut self: Pin<&mut Self>) {
        if self.running || self.question.is_some() {
            return;
        }
        if self.helper_gone || self.session.is_none() {
            // A new helper.
            rdp::close(self.pane_id);
            {
                let mut this = self.as_mut().rust_mut();
                this.session = None;
                this.helper_gone = false;
            }
            self.start();
        } else if self.jump_ended {
            self.open_jump();
        } else if self.connect_sent {
            self.send(ToHelper::Control(Control::Reconnect));
        } else {
            self.begin();
        }
    }

    /// See the bridge declaration.
    pub fn disconnect(mut self: Pin<&mut Self>) {
        if self.session.is_none() || self.helper_gone {
            return;
        }
        // The helper ends; connecting again starts a new one.
        self.send(ToHelper::Control(Control::Disconnect));
        {
            let mut this = self.as_mut().rust_mut();
            this.helper_gone = true;
            this.question = None;
            this.prompt = QString::default();
        }
        self.set_status(&Status::Disconnected {
            code: "closed".to_owned(),
            reason: "you disconnected".to_owned(),
        });
    }

    /// See the bridge declaration.
    pub fn send_ctrl_alt_del(self: Pin<&mut Self>) {
        if self.running {
            self.send(ToHelper::Control(Control::CtrlAltDel));
        }
    }

    /// See the bridge declaration.
    pub fn resize_desktop(self: Pin<&mut Self>) {
        if !self.dynamic() || !self.running {
            return;
        }
        let (width, height) = self.desktop_size();
        let (shown_width, shown_height) = {
            let frame = self.session.as_ref().map(|session| {
                let frame = session
                    .frame
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                (frame.width(), frame.height())
            });
            frame.unwrap_or_default()
        };
        if (width, height) != (shown_width, shown_height) {
            self.send(ToHelper::Control(Control::Resize { width, height }));
        }
    }

    /// See the bridge declaration.
    pub fn offer_clipboard(self: Pin<&mut Self>) {
        let shared = self
            .plan
            .as_ref()
            .is_some_and(|plan| plan.connect.clipboard);
        if !shared || !self.running {
            return;
        }
        let text = if is_test_run() {
            self.test_clipboard.1.clone()
        } else {
            self.clipboard_text().to_string()
        };
        if !text.is_empty() {
            self.send(ToHelper::Clipboard(text));
        }
    }

    /// See the bridge declaration.
    pub fn test_clipboard(&self) -> QString {
        if is_test_run() {
            QString::from(&self.test_clipboard.0)
        } else {
            QString::default()
        }
    }

    /// See the bridge declaration.
    pub fn set_test_clipboard(mut self: Pin<&mut Self>, text: &QString) {
        if !is_test_run() {
            return;
        }
        self.as_mut().rust_mut().test_clipboard.1 = text.to_string();
        self.offer_clipboard();
    }

    /// See the bridge declaration.
    pub fn send_text(self: Pin<&mut Self>, text: &QString) {
        let text = text.to_string();
        if !self.running {
            if text.contains(['\r', '\n']) {
                self.reconnect();
            }
            return;
        }
        for character in text.chars() {
            for pressed in [true, false] {
                self.send(ToHelper::Control(Control::Unicode { character, pressed }));
            }
        }
    }

    /// See the bridge declaration.
    pub fn fill_framebuffer(mut self: Pin<&mut Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let mut frame = session
            .frame
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (width, height) = (i32::from(frame.width()), i32::from(frame.height()));
        if width == 0 || height == 0 {
            return;
        }
        let stride = width * 4;
        let generation = frame.generation();
        let rects = frame.take_dirty();
        if generation != self.drawn_generation {
            self.as_mut().rust_mut().drawn_generation = generation;
            self.as_mut().resize_framebuffer(width, height);
            self.as_mut()
                .write_pixels(0, 0, width, height, frame.pixels(), stride);
            return;
        }
        for rect in rects {
            self.as_mut().write_pixels(
                i32::from(rect.x),
                i32::from(rect.y),
                i32::from(rect.width),
                i32::from(rect.height),
                frame.pixels(),
                stride,
            );
        }
    }

    /// See the bridge declaration.
    pub fn handle_key(
        mut self: Pin<&mut Self>,
        native_scan_code: u32,
        key: i32,
        text: &QString,
        pressed: bool,
        _auto_repeat: bool,
    ) {
        if !self.running {
            return;
        }
        // VNC: X keysyms, a key going up as it went down.
        if self
            .plan
            .as_ref()
            .is_some_and(|plan| plan.protocol == Protocol::Vnc)
        {
            let text = text.to_string();
            let keysym = {
                let mut this = self.as_mut().rust_mut();
                if pressed {
                    this.keysyms.press(key, native_scan_code, &text)
                } else {
                    this.keysyms.release(key, native_scan_code, &text)
                }
            };
            if let Some(keysym) = keysym {
                self.send(ToHelper::Control(Control::Keysym { keysym, pressed }));
            }
            return;
        }
        if let Some(scancode) = keys::scancode(platform(), native_scan_code, key) {
            self.send(ToHelper::Control(Control::Key {
                code: scancode.code,
                extended: scancode.extended,
                pressed,
            }));
            return;
        }
        // No scan code: the characters themselves, on press.
        if pressed {
            for character in text.to_string().chars().filter(|c| !c.is_control()) {
                for down in [true, false] {
                    self.send(ToHelper::Control(Control::Unicode {
                        character,
                        pressed: down,
                    }));
                }
            }
        }
    }

    /// See the bridge declaration.
    pub fn handle_pointer(self: Pin<&mut Self>, kind: i32, qt_button: i32, x: i32, y: i32) {
        if !self.running {
            return;
        }
        let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) else {
            return;
        };
        self.send(ToHelper::Control(Control::Move { x, y }));
        if let (0 | 1, Some(button)) = (kind, button(qt_button)) {
            self.send(ToHelper::Control(Control::Button {
                button,
                pressed: kind == 0,
            }));
        }
    }

    /// See the bridge declaration.
    pub fn handle_wheel(self: Pin<&mut Self>, angle_x: i32, angle_y: i32) {
        if !self.running {
            return;
        }
        let clamp = |value: i32| i16::try_from(value.clamp(-32_000, 32_000)).unwrap_or(0);
        self.send(ToHelper::Control(Control::Wheel {
            vertical: clamp(angle_y),
            horizontal: clamp(angle_x),
        }));
    }

    /// See the bridge declaration.
    pub fn handle_focus_change(mut self: Pin<&mut Self>, focused: bool) {
        if !focused && self.running {
            // Nothing stays pressed on the server while the pane can't see the keys come up.
            self.as_mut().rust_mut().keysyms.release_all();
            self.send(ToHelper::Control(Control::ReleaseAll));
        }
    }

    /// See the bridge declaration.
    pub fn handle_wanted_size(self: Pin<&mut Self>, _width: i32, _height: i32) {
        // QML waits for the size to settle, then calls `resizeDesktop`.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_and_states() {
        assert_eq!(button(1), Some(Button::Left));
        assert_eq!(button(2), Some(Button::Right));
        assert_eq!(button(4), Some(Button::Middle));
        assert_eq!(button(0), None);
        let text = status_json(&Status::Disconnected {
            code: "auth".to_owned(),
            reason: "no".to_owned(),
        });
        assert!(text.contains("\"state\":\"disconnected\""), "{text}");
        assert!(text.contains("\"retryIn\":-1"), "{text}");
    }
}
