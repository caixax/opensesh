//! Remote desktop panes (Sprint 13, ADR 0034), without Qt: each RDP pane's session runs in the
//! helper program `opensesh-rdp` (next to the app), started here and spoken to with
//! `opensesh-rdp-protocol`. A thread reads what it says into the pane's [`Frame`] and [`State`]
//! and wakes the pane's item; another writes what the item asks.
//!
//! Connections are kept by pane id (like terminal sessions), so a pane's item can be made again
//! (a split, a tab moved) without reconnecting; closing the tab ends them.
//!
//! What a pane connects to comes from a saved RDP host or `rdp://` text ([`plan_for`]); the
//! password from its keychain identity, else asked in the pane. A host with jump hosts is reached
//! through a local tunnel: an SSH connection to the last jump host forwards a port here to the
//! server (the pane opens it, and the connection keeps it). A test run connects only to the RDP
//! test server started next to the app ([`start_test_server`]), and through its SSH test server.

use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

use crossbeam_channel::Sender;
use opensesh_core::hosts::{Host, Protocol, RdpOptions, target};
use opensesh_rdp_protocol::frame::Frame;
use opensesh_rdp_protocol::{
    Connect, Control, Event, FromHelper, PointerPicture, Status, ToHelper,
};
use opensesh_ssh::spec::ConnectSpec;
use opensesh_ssh::tunnel::{Kept, Running};
use secrecy::SecretString;

use crate::bridge::app_info::is_test_run;
use crate::keychain::{self, Job};

/// The helper's program name.
fn helper_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

/// Where a program shipped with the app is: next to it, or (Linux packages) in
/// `../lib/opensesh/`.
fn beside_app(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let file = helper_name(name);
    [
        dir.join(&file),
        dir.join("..").join("lib").join("opensesh").join(&file),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A certificate the helper asks about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate {
    /// `SHA256:...`.
    pub fingerprint: String,
    /// Its subject.
    pub subject: String,
    /// Its key's kind.
    pub key_type: String,
}

/// A pointer change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointerChange {
    /// The system's arrow.
    Default,
    /// None.
    Hidden,
    /// A picture.
    Picture(PointerPicture),
}

/// Wakes the pane's item (queues its `drain` on the GUI thread). Must return at once.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// What happened since the item last looked.
#[derive(Default)]
pub struct State {
    /// The latest state.
    pub status: Option<Status>,
    /// A certificate waiting for an answer.
    pub certificate: Option<Certificate>,
    /// The latest pointer.
    pub pointer: Option<PointerChange>,
    /// The server's clipboard text.
    pub clipboard: Option<String>,
    /// The frame changed.
    pub dirty: bool,
    /// The helper ended (or couldn't start): why.
    pub helper_ended: Option<String>,
    waker: Option<Waker>,
    notified: bool,
}

impl std::fmt::Debug for State {
    /// Clipboard text is left out.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("status", &self.status)
            .field("certificate", &self.certificate)
            .field("dirty", &self.dirty)
            .finish_non_exhaustive()
    }
}

/// A pane's RDP session: the helper program, the frame and the state.
pub struct Connection {
    to: Sender<ToHelper>,
    child: Mutex<Option<Child>>,
    /// The remote screen.
    pub frame: Mutex<Frame>,
    /// What the item hasn't seen yet.
    pub state: Mutex<State>,
    /// The latest state and why the helper ended, for an item made again (a tab moved).
    latest: Mutex<(Option<Status>, Option<String>)>,
    /// The local tunnel through the jump hosts: its SSH connection and its forward.
    jump: Mutex<Option<(Kept, Running)>>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection").finish_non_exhaustive()
    }
}

impl Connection {
    /// Starts the helper. Nothing is sent until the item says to connect.
    ///
    /// # Errors
    ///
    /// Why the helper couldn't start (not installed next to the app, or it failed to run).
    pub fn start() -> Result<Arc<Self>, String> {
        let helper = beside_app("opensesh-rdp").ok_or_else(|| {
            "OpenSesh's RDP helper (opensesh-rdp) isn't next to the app: reinstall OpenSesh, or \
             run `cargo xtask rdp` in a development build"
                .to_owned()
        })?;
        Self::start_program(&helper)
    }

    fn start_program(helper: &Path) -> Result<Arc<Self>, String> {
        let mut command = Command::new(helper);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("the RDP helper didn't start: {error}"))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err("the RDP helper has no pipes".to_owned());
        };
        let (to, outbox) = crossbeam_channel::unbounded::<ToHelper>();
        let connection = Arc::new(Self {
            to,
            child: Mutex::new(Some(child)),
            frame: Mutex::new(Frame::default()),
            state: Mutex::new(State::default()),
            latest: Mutex::new((None, None)),
            jump: Mutex::new(None),
        });
        let writer = std::thread::Builder::new()
            .name("opensesh-rdp-to".to_owned())
            .spawn(move || write_loop(stdin, &outbox));
        let reader = Arc::clone(&connection);
        let read = std::thread::Builder::new()
            .name("opensesh-rdp-from".to_owned())
            .spawn(move || reader.read_loop(stdout));
        if writer.is_err() || read.is_err() {
            connection.kill();
            return Err("could not start a thread for the RDP helper".to_owned());
        }
        Ok(connection)
    }

    /// Sends `message` to the helper.
    pub fn send(&self, message: ToHelper) {
        let _ = self.to.send(message);
    }

    /// Wakes `waker` on every change from now on (and now, if something is waiting).
    pub fn attach(&self, waker: Waker) {
        let mut state = lock(&self.state);
        state.waker = Some(Arc::clone(&waker));
        state.notified = true;
        drop(state);
        waker();
    }

    /// Takes what happened: called by the woken item, which is woken again on the next change.
    pub fn take(&self) -> State {
        let mut state = lock(&self.state);
        let waker = state.waker.take();
        let taken = std::mem::take(&mut *state);
        state.waker = waker;
        taken
    }

    fn notify(&self, change: impl FnOnce(&mut State)) {
        let mut state = lock(&self.state);
        change(&mut state);
        if state.notified {
            return;
        }
        if let Some(waker) = state.waker.clone() {
            state.notified = true;
            drop(state);
            waker();
        }
    }

    fn read_loop(&self, stdout: std::process::ChildStdout) {
        let mut input = BufReader::new(stdout);
        let ended = loop {
            match FromHelper::read(&mut input) {
                Ok(Some(message)) => self.receive(message),
                Ok(None) => break "the RDP helper ended".to_owned(),
                Err(error) => break format!("the RDP helper said something unexpected: {error}"),
            }
        };
        lock(&self.latest).1 = Some(ended.clone());
        self.notify(|state| state.helper_ended = Some(ended));
    }

    /// The latest state, and why the helper ended (if it did).
    #[must_use]
    pub fn latest(&self) -> (Option<Status>, Option<String>) {
        lock(&self.latest).clone()
    }

    fn receive(&self, message: FromHelper) {
        match message {
            FromHelper::Event(Event::Size { width, height }) => {
                lock(&self.frame).resize(width, height);
                self.notify(|state| state.dirty = true);
            }
            FromHelper::Pixels(pixels) => {
                lock(&self.frame).copy_rect(pixels.rect, &pixels.rgba);
                self.notify(|state| state.dirty = true);
            }
            FromHelper::Event(Event::Status(status)) => {
                lock(&self.latest).0 = Some(status.clone());
                self.notify(|state| state.status = Some(status));
            }
            FromHelper::Event(Event::Certificate {
                fingerprint,
                subject,
                key_type,
            }) => self.notify(|state| {
                state.certificate = Some(Certificate {
                    fingerprint,
                    subject,
                    key_type,
                });
            }),
            FromHelper::Event(Event::PointerDefault) => {
                self.notify(|state| state.pointer = Some(PointerChange::Default));
            }
            FromHelper::Event(Event::PointerHidden) => {
                self.notify(|state| state.pointer = Some(PointerChange::Hidden));
            }
            FromHelper::Pointer(picture) => {
                self.notify(|state| state.pointer = Some(PointerChange::Picture(picture)));
            }
            FromHelper::Clipboard(text) => self.notify(|state| state.clipboard = Some(text)),
        }
    }

    /// Keeps the tunnel through the jump hosts (none: closes it).
    pub fn set_jump(&self, jump: Option<(Kept, Running)>) {
        let previous = std::mem::replace(&mut *lock(&self.jump), jump);
        drop(previous);
    }

    /// Ends the session, the helper and the tunnel.
    pub fn close(&self) {
        self.set_jump(None);
        self.send(ToHelper::Control(Control::Disconnect));
        // The helper ends by itself; one that doesn't is ended a moment later.
        let child = lock(&self.child).take();
        if let Some(mut child) = child {
            let _ = std::thread::Builder::new()
                .name("opensesh-rdp-end".to_owned())
                .spawn(move || {
                    for _ in 0..30 {
                        if matches!(child.try_wait(), Ok(Some(_))) {
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                });
        }
    }

    fn kill(&self) {
        if let Some(mut child) = lock(&self.child).take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.close();
    }
}

fn write_loop(stdin: ChildStdin, outbox: &crossbeam_channel::Receiver<ToHelper>) {
    let mut output = BufWriter::new(stdin);
    while let Ok(message) = outbox.recv() {
        let end = matches!(message, ToHelper::Control(Control::Disconnect));
        if message.write(&mut output).is_err() || end {
            return;
        }
    }
}

static CONNECTIONS: LazyLock<Mutex<HashMap<i32, Arc<Connection>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The connection of pane `id`, if it has one.
#[must_use]
pub fn get(id: i32) -> Option<Arc<Connection>> {
    lock(&CONNECTIONS).get(&id).cloned()
}

/// Keeps `connection` as pane `id`'s.
pub fn keep(id: i32, connection: Arc<Connection>) {
    if let Some(previous) = lock(&CONNECTIONS).insert(id, connection) {
        previous.close();
    }
}

/// Ends pane `id`'s connection (its tab closed). Returns whether it had one.
pub fn close(id: i32) -> bool {
    let connection = lock(&CONNECTIONS).remove(&id);
    connection.is_some_and(|connection| {
        connection.close();
        true
    })
}

/// Ends every connection (the app is closing).
pub fn close_all() {
    let all: Vec<Arc<Connection>> = lock(&CONNECTIONS).drain().map(|(_, c)| c).collect();
    for connection in all {
        connection.close();
    }
}

/// What a pane connects to.
#[derive(Debug, Clone)]
pub struct Plan {
    /// The helper's connect message, without the size (the pane's).
    pub connect: Connect,
    /// Where certificates are remembered: the server's name and port.
    pub server: (String, u16),
    /// The keychain identity holding the password, if any.
    pub identity: Option<String>,
    /// The host's fixed desktop size, for when the desktop doesn't follow the pane.
    pub size: Option<(u16, u16)>,
    /// The SSH connection to the last jump host, when there are jump hosts: the helper then
    /// connects to a local tunnel to `connect`'s address and port.
    pub jump: Option<ConnectSpec>,
}

/// The port of the test run's RDP test server (0 until it starts).
static TEST_SERVER: AtomicU16 = AtomicU16::new(0);

/// The test server's process, ended with the app (its input closes).
static TEST_SERVER_PROCESS: Mutex<Option<Child>> = Mutex::new(None);

/// Test runs only: starts the RDP test server next to the app (`cargo xtask rdp
/// --test-server`) and returns its port; 0 when it can't.
#[must_use]
pub fn start_test_server() -> u16 {
    if !is_test_run() {
        return 0;
    }
    let running = TEST_SERVER.load(Ordering::Relaxed);
    if running != 0 {
        return running;
    }
    let Some(program) = beside_app("opensesh-rdp-test-server") else {
        tracing::warn!("no RDP test server next to the app (cargo xtask rdp --test-server)");
        return 0;
    };
    let mut command = Command::new(program);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let Ok(mut child) = command.spawn() else {
        return 0;
    };
    let port = child.stdout.take().and_then(|stdout| {
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).ok()?;
        line.trim().strip_prefix("port=")?.parse::<u16>().ok()
    });
    let Some(port) = port else {
        let _ = child.kill();
        return 0;
    };
    *lock(&TEST_SERVER_PROCESS) = Some(child);
    TEST_SERVER.store(port, Ordering::Relaxed);
    port
}

/// What pane `host_id` (a saved host) or `target_text` (`rdp://` text) connects to.
///
/// # Errors
///
/// When the host is gone, isn't RDP, or has no address.
pub fn plan_for(host_id: &str, target_text: &str) -> Result<Plan, String> {
    let library = crate::hosts::current();
    let host: Host = if host_id.is_empty() {
        let parsed = target::parse(target_text).map_err(|error| error.to_string())?;
        if parsed.protocol != Protocol::Rdp {
            return Err(format!(
                "{} isn't a remote desktop",
                parsed.protocol.as_str()
            ));
        }
        parsed.to_host()
    } else {
        library
            .file
            .host(host_id)
            .cloned()
            .ok_or("the host no longer exists")?
    };
    let resolved = library.file.resolve(&host);
    let identity = resolved.identity().map(str::to_owned);
    let user = resolved
        .user()
        .map(str::to_owned)
        .or_else(|| identity.as_deref().and_then(keychain::identity_user))
        .unwrap_or_default();
    let address = host.address.trim().to_owned();
    if address.is_empty() {
        return Err("the host has no address".to_owned());
    }
    let port = resolved.port().unwrap_or(3389);
    let RdpOptions {
        domain, clipboard, ..
    } = host.rdp.clone();
    let mut connect = Connect {
        address: address.clone(),
        port,
        server_name: address.clone(),
        user,
        domain: domain.filter(|domain| !domain.trim().is_empty()),
        width: 0,
        height: 0,
        scale_factor: 100,
        keyboard_layout: 0x0409,
        clipboard: clipboard.unwrap_or(true),
        timeout_secs: 20,
        client_name: client_name(),
    };
    if is_test_run() {
        let port = TEST_SERVER.load(Ordering::Relaxed);
        if port == 0 {
            return Err("a test run connects only to its own RDP server".to_owned());
        }
        "127.0.0.1".clone_into(&mut connect.address);
        connect.port = port;
        "tester".clone_into(&mut connect.user);
        connect.domain = None;
    }
    let jump = crate::ssh::jump_connect_for(&library.file, &host)?;
    Ok(Plan {
        connect,
        server: (address, port),
        identity: if is_test_run() { None } else { identity },
        size: host.rdp.size(),
        jump,
    })
}

/// This computer's name, as the server shows it.
fn client_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "OpenSesh".to_owned())
}

/// The password of `identity` from the keychain: `Ok(None)` when it has none, `Err("locked")`
/// when the vault is locked.
///
/// # Errors
///
/// `locked` when the vault holds it and is locked.
pub async fn identity_password(identity: &str) -> Result<Option<SecretString>, &'static str> {
    let (reply, answer) = tokio::sync::oneshot::channel();
    if !keychain::request(Job::ConnectionSecrets {
        identity: identity.to_owned(),
        reply,
    }) {
        return Ok(None);
    }
    match answer.await {
        Ok(Ok(secrets)) => Ok(secrets.password),
        Ok(Err("locked")) => Err("locked"),
        Ok(Err(_)) | Err(_) => Ok(None),
    }
}

/// The trusted certificates store: the config folder's, or a test run's own.
#[must_use]
pub fn certificates() -> opensesh_core::trusted_certificates::TrustedCertificates {
    use opensesh_core::trusted_certificates::{FILE, TrustedCertificates};
    let folder = if is_test_run() {
        std::env::temp_dir().join("opensesh-test-rdp")
    } else {
        crate::services::get().map_or_else(std::env::temp_dir, |services| {
            services.paths.config_dir().to_path_buf()
        })
    };
    TrustedCertificates::new(folder.join(FILE))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    #[test]
    fn a_helper_that_isnt_a_program_says_why() {
        let error = Connection::start_program(Path::new("opensesh-no-such-helper")).unwrap_err();
        assert!(error.contains("didn't start"), "{error}");
    }

    #[test]
    fn client_names() {
        assert!(!client_name().is_empty());
    }
}
