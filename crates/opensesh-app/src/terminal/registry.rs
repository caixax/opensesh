//! The terminal sessions of the main window, owned by Rust and keyed by tab id (PLAN §3.2: the
//! session manager; research decision D13).
//!
//! A session outlives the QML item that shows it: a `TerminalItem` *attaches* to the session of
//! its tab and detaches when it is destroyed, so a later sprint can move a terminal between split
//! panes without restarting the shell. Only closing the tab ([`close`]) ends the session.
//!
//! Threads: the engine calls the session's notify callback on its own thread. The callback only
//! records what happened in the entry's [`SessionState`] (under a short mutex) and, once per
//! batch, *wakes* the attached item through its [`Waker`], which queues a closure on the GUI
//! thread (`CxxQtThread::queue`). The GUI side then takes the batch with
//! [`SessionEntry::take_events`]. Nothing here calls Qt while holding a lock, except the waker,
//! which only posts an event.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

use opensesh_term::backend::{BackendError, TermSize};
use opensesh_term::palette::Palette;
use opensesh_term::pty;
use opensesh_term::session::{Notice, Notify, Session, SessionConfig, SessionError};
use opensesh_term::shell::ShellCommand;

/// Queues the attached item's `drain` on the GUI thread. Returns `false` when the item is gone
/// (the closure was not queued).
pub type Waker = Arc<dyn Fn() -> bool + Send + Sync>;

/// Why a session could not start.
#[derive(Debug)]
pub enum StartError {
    /// The PTY backend's thread could not be created.
    Backend(BackendError),
    /// The engine thread could not be created.
    Session(SessionError),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Backend(_) => "could not start the terminal backend",
            Self::Session(_) => "could not start the terminal engine",
        })
    }
}

impl std::error::Error for StartError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Backend(error) => Some(error),
            Self::Session(error) => Some(error),
        }
    }
}

impl From<BackendError> for StartError {
    fn from(error: BackendError) -> Self {
        Self::Backend(error)
    }
}

impl From<SessionError> for StartError {
    fn from(error: SessionError) -> Self {
        Self::Session(error)
    }
}

/// What the program told the GUI that stays true until it changes (the item mirrors it).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionInfo {
    /// Window title set by the program (OSC 0/2), `None` for the default.
    pub title: Option<String>,
    /// Working directory reported by the shell (OSC 7), if any.
    pub working_directory: Option<String>,
    /// Set once the program ended: its exit code, or `None` when it was killed or failed to
    /// start.
    pub exit: Option<Option<i32>>,
}

/// What happened since the attached item last took the events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Events {
    /// The screen changed (take a snapshot).
    pub dirty: bool,
    /// The program rang the bell.
    pub bell: bool,
    /// The title or the working directory changed.
    pub info: bool,
    /// The program ended ([`SessionInfo::exit`] is set).
    pub exited: bool,
}

impl Events {
    /// Whether anything happened.
    #[must_use]
    pub fn any(self) -> bool {
        self.dirty || self.bell || self.info || self.exited
    }
}

/// Shared between the engine's callback and the GUI thread.
#[derive(Default)]
struct SessionState {
    info: SessionInfo,
    events: Events,
    /// The attached item: its token and its waker.
    attachment: Option<(u64, Waker)>,
    /// A wake-up is queued and not yet taken by the attached item.
    queued: bool,
}

impl SessionState {
    /// Whether the item with `token` is the attached one.
    fn is_attached(&self, token: u64) -> bool {
        self.attachment
            .as_ref()
            .is_some_and(|(attached, _)| *attached == token)
    }

    fn record(&mut self, notice: Notice) {
        match notice {
            Notice::Dirty => self.events.dirty = true,
            Notice::Title(title) => {
                self.info.title = Some(title);
                self.events.info = true;
            }
            Notice::ResetTitle => {
                self.info.title = None;
                self.events.info = true;
            }
            Notice::WorkingDirectory(directory) => {
                self.info.working_directory = Some(directory);
                self.events.info = true;
            }
            Notice::Bell => self.events.bell = true,
            Notice::Exited(code) => {
                self.info.exit = Some(code);
                self.events.exited = true;
            }
            // The snapshot's cursor carries the blinking wish; nothing to do here.
            Notice::CursorBlinking(_) => {}
        }
    }

    /// Wakes the attached item once per batch of events.
    fn wake(&mut self) {
        if self.queued || !self.events.any() {
            return;
        }
        if let Some((_, waker)) = &self.attachment {
            self.queued = waker();
        }
    }
}

/// One terminal session and what the GUI needs to know about it.
pub struct SessionEntry {
    id: i32,
    session: Session,
    state: Arc<Mutex<SessionState>>,
}

impl std::fmt::Debug for SessionEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionEntry")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

fn lock(state: &Mutex<SessionState>) -> MutexGuard<'_, SessionState> {
    // A panic while holding this lock aborts through the crash hook anyway; keep going.
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Tokens that tell attachments apart (an item that detached must not take a new item's events).
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

/// A new attachment token.
#[must_use]
pub fn next_token() -> u64 {
    NEXT_TOKEN.fetch_add(1, Ordering::Relaxed)
}

impl SessionEntry {
    /// Starts a session with `start`, which gets the notify callback to pass to
    /// `Session::start`.
    fn start_with(
        id: i32,
        start: impl FnOnce(Notify) -> Result<Session, StartError>,
    ) -> Result<Self, StartError> {
        let state = Arc::new(Mutex::new(SessionState::default()));
        let notify: Notify = {
            let state = Arc::clone(&state);
            Arc::new(move |notice| {
                let mut state = lock(&state);
                state.record(notice);
                state.wake();
            })
        };
        let session = start(notify)?;
        Ok(Self { id, session, state })
    }

    /// The tab id this session belongs to.
    #[must_use]
    pub fn id(&self) -> i32 {
        self.id
    }

    /// The engine handle.
    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// A copy of the title, directory and exit state.
    #[must_use]
    pub fn info(&self) -> SessionInfo {
        lock(&self.state).info.clone()
    }

    /// Attaches an item: from now on `waker` is called when something happens. Replaces any
    /// previous attachment. If events are already waiting, the item is woken at once.
    pub fn attach(&self, token: u64, waker: Waker) {
        let mut state = lock(&self.state);
        state.attachment = Some((token, waker));
        state.queued = false;
        state.wake();
    }

    /// Detaches the item with `token` (nothing happens if another item attached since).
    pub fn detach(&self, token: u64) {
        let mut state = lock(&self.state);
        if state.is_attached(token) {
            state.attachment = None;
            state.queued = false;
        }
    }

    /// Takes the events for the item with `token`, with the current info. `None` if that item
    /// is not the attached one (a stale wake-up).
    #[must_use]
    pub fn take_events(&self, token: u64) -> Option<(Events, SessionInfo)> {
        let mut state = lock(&self.state);
        if !state.is_attached(token) {
            return None;
        }
        state.queued = false;
        let events = std::mem::take(&mut state.events);
        Some((events, state.info.clone()))
    }

    /// Ends the session in the background (idempotent).
    pub fn shutdown(&self) {
        self.session.shutdown();
        let mut state = lock(&self.state);
        state.attachment = None;
        state.queued = false;
    }
}

/// Every open session, by tab id.
static SESSIONS: LazyLock<Mutex<HashMap<i32, Arc<SessionEntry>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn sessions() -> MutexGuard<'static, HashMap<i32, Arc<SessionEntry>>> {
    SESSIONS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Settings of a new local session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalOptions {
    /// Grid and cell size.
    pub size: TermSize,
    /// Colors.
    pub palette: Palette,
}

/// A shell that reads no startup files and keeps no history, for the smoke test: it must not
/// depend on the user's rc files or write to their shell history.
fn hermetic_shell() -> ShellCommand {
    if cfg!(windows) {
        ShellCommand::program("cmd.exe").arg("/d").arg("/q")
    } else {
        ShellCommand::program("/bin/sh")
            .env("ENV", "")
            .env("HISTFILE", "/dev/null")
    }
}

/// Starts a local shell with the engine defaults (ADR 0012) and the given size and colors.
fn start_local(options: LocalOptions, notify: Notify) -> Result<Session, StartError> {
    let command = if crate::bridge::app_info::is_smoke_test() {
        hermetic_shell()
    } else {
        ShellCommand::user_shell()
    };
    let (backend, events) = pty::spawn(command, options.size)?;
    let config = SessionConfig {
        size: options.size,
        palette: options.palette,
        ..SessionConfig::default()
    };
    Ok(Session::start(backend, events, config, notify)?)
}

/// The session of tab `id`, if it is open.
#[must_use]
pub fn get(id: i32) -> Option<Arc<SessionEntry>> {
    sessions().get(&id).cloned()
}

/// The session of tab `id`, starting a local shell for it if there is none yet.
///
/// # Errors
/// [`StartError`] if a thread for the new session could not be created.
pub fn open_local(id: i32, options: LocalOptions) -> Result<Arc<SessionEntry>, StartError> {
    open_with(id, |notify| start_local(options, notify))
}

/// [`open_local`] with any way of starting the session (tests use a replay backend).
fn open_with(
    id: i32,
    start: impl FnOnce(Notify) -> Result<Session, StartError>,
) -> Result<Arc<SessionEntry>, StartError> {
    let mut sessions = sessions();
    if let Some(entry) = sessions.get(&id) {
        return Ok(Arc::clone(entry));
    }
    let entry = Arc::new(SessionEntry::start_with(id, start)?);
    tracing::info!(id, "terminal session started");
    sessions.insert(id, Arc::clone(&entry));
    Ok(entry)
}

/// Ends the session of tab `id` and forgets it. Returns whether there was one.
pub fn close(id: i32) -> bool {
    let entry = sessions().remove(&id);
    match entry {
        Some(entry) => {
            entry.shutdown();
            tracing::info!(id, "terminal session closed");
            true
        }
        None => false,
    }
}

/// Ends the session of tab `id` (if any) and starts a new local shell for it.
///
/// # Errors
/// [`StartError`] if a thread for the new session could not be created.
pub fn restart_local(id: i32, options: LocalOptions) -> Result<Arc<SessionEntry>, StartError> {
    close(id);
    open_local(id, options)
}

/// How many sessions are open.
#[must_use]
pub fn count() -> usize {
    sessions().len()
}

/// Ends every session (the main window closed).
pub fn shutdown_all() {
    let entries: Vec<Arc<SessionEntry>> = sessions().drain().map(|(_, entry)| entry).collect();
    if !entries.is_empty() {
        tracing::info!(count = entries.len(), "ending the terminal sessions");
    }
    for entry in entries {
        entry.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, Instant};

    use opensesh_term::backend;
    use opensesh_term::snapshot::{Damage, Frame};

    use super::*;

    /// Ids for these tests, far from the ones the app uses, so tests running in parallel never
    /// share a session.
    fn test_id() -> i32 {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        1_000_000 + i32::try_from(NEXT.fetch_add(1, Ordering::Relaxed)).unwrap()
    }

    fn replay(id: i32, bytes: &[u8]) -> Arc<SessionEntry> {
        let bytes = bytes.to_vec();
        open_with(id, move |notify| {
            let (backend, events) = backend::replay(bytes);
            let config = SessionConfig {
                size: TermSize::new(20, 4),
                ..SessionConfig::default()
            };
            Ok(Session::start(backend, events, config, notify)?)
        })
        .unwrap()
    }

    fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// A waker that counts its calls.
    fn counting_waker() -> (Waker, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let waker: Waker = Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            true
        });
        (waker, calls)
    }

    #[test]
    fn a_session_is_opened_once_per_id_and_closed_with_its_tab() {
        let id = test_id();
        let entry = replay(id, b"hello");
        assert_eq!(entry.id(), id);
        // Opening the same id again returns the same session.
        let again = open_with(id, |_| unreachable!("the session exists")).unwrap();
        assert!(Arc::ptr_eq(&entry, &again));
        assert!(get(id).is_some());
        assert!(close(id));
        assert!(get(id).is_none());
        assert!(!close(id));
    }

    #[test]
    fn notices_wake_the_attached_item_once_per_batch() {
        let id = test_id();
        let entry = replay(id, b"\x1b]2;build log\x07hello\x07");
        let (waker, calls) = counting_waker();
        let token = next_token();
        entry.attach(token, waker);
        wait_for("a wake-up", || calls.load(Ordering::SeqCst) > 0);
        wait_for("the title and the bell", || {
            let state = lock(&entry.state);
            state.events.info && state.events.bell
        });
        // Until the item takes the events, further notices don't wake it again.
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let (events, info) = entry.take_events(token).unwrap();
        assert!(events.dirty && events.bell && events.info);
        assert_eq!(info.title.as_deref(), Some("build log"));
        assert!(info.exit.is_none());
        // Taking them again gives nothing new.
        let (events, _) = entry.take_events(token).unwrap();
        assert!(!events.any());
        close(id);
    }

    #[test]
    fn a_detached_item_gets_nothing_and_a_new_item_gets_the_waiting_events() {
        let id = test_id();
        let entry = replay(id, b"hello");
        let (first, first_calls) = counting_waker();
        let first_token = next_token();
        entry.attach(first_token, first);
        wait_for("a wake-up", || first_calls.load(Ordering::SeqCst) > 0);
        entry.detach(first_token);
        assert!(entry.take_events(first_token).is_none());

        // The events are still there for the next item, which is woken when it attaches.
        let (second, second_calls) = counting_waker();
        let second_token = next_token();
        entry.attach(second_token, second);
        assert_eq!(second_calls.load(Ordering::SeqCst), 1);
        let (events, _) = entry.take_events(second_token).unwrap();
        assert!(events.dirty);
        // A stale detach of the first item doesn't detach the second one.
        entry.detach(first_token);
        assert!(entry.take_events(second_token).is_some());
        close(id);
    }

    #[test]
    fn clearing_the_history_empties_the_scrollback() {
        let id = test_id();
        let output: Vec<u8> = (0..30)
            .flat_map(|n| format!("line {n}\r\n").into_bytes())
            .collect();
        let entry = replay(id, &output);
        let mut frame = Frame::default();
        wait_for("the output", || {
            entry.session().snapshot(&mut frame);
            frame.history_size > 0
        });
        entry.session().clear_history();
        entry.session().snapshot(&mut frame);
        assert_eq!(frame.history_size, 0);
        assert!(entry.session().text_dump().contains("line 29"));
        close(id);
    }

    #[test]
    fn a_new_view_gets_every_row() {
        let id = test_id();
        let entry = replay(
            id, b"one
two",
        );
        let mut frame = Frame::default();
        wait_for("the output", || {
            entry.session().snapshot(&mut frame);
            entry.session().text_dump().contains("two")
        });
        // Nothing changed since: only the cursor row.
        entry.session().snapshot(&mut frame);
        assert_eq!(frame.damage, Damage::Partial);
        assert!(frame.rows.len() < 4);
        // A view that has no copy of the grid asks for all of it.
        entry.session().snapshot_full(&mut frame);
        assert_eq!(frame.damage, Damage::Full);
        assert_eq!(frame.rows.len(), 4);
        close(id);
    }

    #[test]
    fn shutting_everything_down_empties_the_registry_of_those_sessions() {
        let id = test_id();
        let entry = replay(id, b"x");
        let (waker, _) = counting_waker();
        entry.attach(next_token(), waker);
        entry.shutdown();
        assert!(lock(&entry.state).attachment.is_none());
        close(id);
    }
}
