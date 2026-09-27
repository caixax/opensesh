//! The tunnels (Sprint 9, ADR 0029): what `tunnels.toml` holds, and each tunnel running on the
//! SSH runtime.
//!
//! - **Independent** tunnels open a connection of their own, through a saved host or quick-connect
//!   text, and keep it up ([`tunnel::keep_connected`]); their questions (host key, password) wait
//!   here for the Tunnels view to answer.
//! - **Tied** tunnels follow the terminal sessions of their saved host: the terminal registry
//!   says here when a session's connection is up or gone ([`session_live`]), and a tied tunnel
//!   runs while one is, on it (no second login).
//!
//! The state lives here, not in a QObject, because the registry and the SSH runtime report from
//! their own threads. The `Tunnels` singleton reads [`views`] and is told when something changed.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

use opensesh_core::tunnels::{Kind, Tunnel, TunnelsFile};
use opensesh_ssh::connect::{self, Connection};
use opensesh_ssh::prompt::{Answer, Asker, Prompt, Request};
use opensesh_ssh::tunnel::{
    self, Connections, Connector, Counts, Endpoint, Forward, Kept, Link, Report, Running, Traffic,
};
use opensesh_term::backend::TermSize;
use tokio::sync::watch;

/// What the Tunnels view shows of a tunnel.
#[derive(Debug, Clone)]
pub struct View {
    /// Its definition.
    pub tunnel: Tunnel,
    /// Switched on.
    pub on: bool,
    /// `stopped`, `connecting`, `waiting` (tied: no session; remote: between connections),
    /// `running`, `retrying` or `failed`.
    pub state: &'static str,
    /// The port it listens on (here, or on the server for a remote tunnel), once it does.
    pub port: u16,
    /// For `failed`: an error code (`listen`, `auth`, `host-key`, `lost`...).
    pub code: &'static str,
    /// Why it failed, or why it is retrying.
    pub detail: String,
    /// For `retrying`: seconds until the next attempt.
    pub retry_in: u64,
    /// Its traffic since OpenSesh started.
    pub counts: Counts,
    /// The first question waiting for an answer, with its id.
    pub prompt: Option<(u64, Prompt)>,
}

struct Entry {
    tunnel: Tunnel,
    on: bool,
    /// Independent: how its own connection is doing.
    link: Option<Link>,
    /// Tied: a session to its host is up.
    session: bool,
    listening: Option<u16>,
    /// A remote tunnel between connections.
    waiting: bool,
    failure: Option<String>,
    traffic: Arc<Traffic>,
    requests: VecDeque<Request>,
    /// Stops everything the run started when dropped.
    active: Option<Active>,
    /// Grows with each run: callbacks of an older run are ignored.
    run: u64,
}

struct Active {
    _stop: watch::Sender<bool>,
    _kept: Option<Kept>,
    _running: Option<Running>,
}

/// The live connections of the terminal sessions of one saved host.
struct Sessions {
    panes: Vec<(i32, Arc<Connection>)>,
    sender: watch::Sender<Option<Arc<Connection>>>,
}

#[derive(Default)]
struct State {
    entries: Vec<Entry>,
    /// Unknown top-level keys of the file, and whether it may be written.
    extra: toml::Table,
    read_only: bool,
    sessions: HashMap<String, Sessions>,
    notify: Option<Arc<dyn Fn() + Send + Sync>>,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State::default()));

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

impl State {
    fn changed(&self) {
        if let Some(notify) = &self.notify {
            notify();
        }
    }

    fn entry(&mut self, id: &str) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|entry| entry.tunnel.id == id)
    }

    /// The connections of `host`'s sessions (created empty when there are none yet).
    fn sessions_of(&mut self, host: &str) -> Connections {
        self.sessions
            .entry(host.to_owned())
            .or_insert_with(|| Sessions {
                panes: Vec::new(),
                sender: watch::channel(None).0,
            })
            .sender
            .subscribe()
    }
}

impl Entry {
    fn new(tunnel: Tunnel) -> Self {
        Self {
            tunnel,
            on: false,
            link: None,
            session: false,
            listening: None,
            waiting: false,
            failure: None,
            traffic: Arc::new(Traffic::default()),
            requests: VecDeque::new(),
            active: None,
            run: 0,
        }
    }

    fn view(&self) -> View {
        let mut view = View {
            tunnel: self.tunnel.clone(),
            on: self.on,
            state: "stopped",
            port: self.listening.unwrap_or(0),
            code: "",
            detail: String::new(),
            retry_in: 0,
            counts: self.traffic.counts(),
            prompt: self
                .requests
                .front()
                .map(|request| (request.id, request.prompt.clone())),
        };
        if !self.on {
            return view;
        }
        if let Some(failure) = &self.failure {
            view.state = "failed";
            view.code = "listen";
            view.detail.clone_from(failure);
            return view;
        }
        view.state = match &self.link {
            Some(Link::Ended { code, reason }) => {
                view.code = code;
                view.detail.clone_from(reason);
                "failed"
            }
            Some(Link::Retrying { in_secs, reason }) => {
                view.retry_in = *in_secs;
                view.detail.clone_from(reason);
                "retrying"
            }
            Some(Link::Connecting) => "connecting",
            _ if self.tunnel.tied && !self.session => "waiting",
            _ if self.waiting => "waiting",
            _ if self.listening.is_some() => "running",
            _ => "connecting",
        };
        view
    }

    /// Stops the run (if any) and forgets what it said.
    fn halt(&mut self) {
        self.active = None;
        self.run += 1;
        self.link = None;
        self.listening = None;
        self.waiting = false;
        self.failure = None;
        for request in self.requests.drain(..) {
            request.answer(Answer::Cancel);
        }
    }
}

/// Where the Tunnels singleton hears that something changed (from any thread).
pub fn set_notify(notify: Arc<dyn Fn() + Send + Sync>) {
    state().notify = Some(notify);
}

/// Takes the tunnels of `file`; with `autostart`, those marked so start (never in test runs).
pub fn load(file: TunnelsFile, autostart: bool) {
    let mut state = state();
    for entry in &mut state.entries {
        entry.halt();
    }
    state.extra = file.extra;
    state.read_only = file.read_only;
    state.entries = file.tunnels.into_iter().map(Entry::new).collect();
    if autostart {
        let ids: Vec<String> = state
            .entries
            .iter()
            .filter(|entry| entry.tunnel.autostart)
            .map(|entry| entry.tunnel.id.clone())
            .collect();
        for id in ids {
            start(&mut state, &id);
        }
    }
    state.changed();
}

/// The file changed outside OpenSesh: new definitions replace the old ones (a running tunnel
/// whose definition changed starts again), removed tunnels stop, new ones stay off.
pub fn reload(file: TunnelsFile) {
    let mut state = state();
    state.extra = file.extra;
    state.read_only = file.read_only;
    let mut entries = Vec::with_capacity(file.tunnels.len());
    let mut restart = Vec::new();
    for tunnel in file.tunnels {
        let position = state
            .entries
            .iter()
            .position(|entry| entry.tunnel.id == tunnel.id);
        match position {
            Some(position) => {
                let mut entry = state.entries.remove(position);
                if entry.tunnel != tunnel {
                    let on = entry.on;
                    entry.halt();
                    entry.tunnel = tunnel;
                    if on {
                        restart.push(entry.tunnel.id.clone());
                    }
                }
                entries.push(entry);
            }
            None => entries.push(Entry::new(tunnel)),
        }
    }
    for mut removed in std::mem::take(&mut state.entries) {
        removed.halt();
    }
    state.entries = entries;
    for id in restart {
        start(&mut state, &id);
    }
    state.changed();
}

/// The file to write, unless it must not be (it came from a newer OpenSesh or couldn't be read).
#[must_use]
pub fn to_file() -> Option<TunnelsFile> {
    let state = state();
    (!state.read_only).then(|| TunnelsFile {
        tunnels: state
            .entries
            .iter()
            .map(|entry| entry.tunnel.clone())
            .collect(),
        extra: state.extra.clone(),
        read_only: false,
    })
}

/// Every tunnel, in order.
#[must_use]
pub fn views() -> Vec<View> {
    state().entries.iter().map(Entry::view).collect()
}

/// Adds `tunnel`, or replaces the one with its id (starting it again if it ran).
///
/// # Errors
///
/// What makes the tunnel unusable ([`Tunnel::problem`]).
pub fn upsert(tunnel: Tunnel) -> Result<String, &'static str> {
    if let Some(problem) = tunnel.problem() {
        return Err(problem);
    }
    let id = tunnel.id.clone();
    let mut state = state();
    match state.entry(&id) {
        Some(entry) => {
            if entry.tunnel != tunnel {
                let on = entry.on;
                entry.halt();
                entry.tunnel = tunnel;
                if on {
                    start(&mut state, &id);
                }
            }
        }
        None => state.entries.push(Entry::new(tunnel)),
    }
    state.changed();
    Ok(id)
}

/// Removes tunnel `id` (stopping it).
pub fn remove(id: &str) -> bool {
    let mut state = state();
    let Some(position) = state.entries.iter().position(|entry| entry.tunnel.id == id) else {
        return false;
    };
    state.entries.remove(position).halt();
    state.changed();
    true
}

/// A copy of tunnel `id`, off, right after it; its id.
pub fn duplicate(id: &str) -> Option<String> {
    let mut state = state();
    let position = state
        .entries
        .iter()
        .position(|entry| entry.tunnel.id == id)?;
    let mut copy = state.entries[position].tunnel.clone();
    copy.id = opensesh_core::hosts::new_id();
    copy.autostart = false;
    let id = copy.id.clone();
    state.entries.insert(position + 1, Entry::new(copy));
    state.changed();
    Some(id)
}

/// Switches tunnel `id` on or off.
pub fn set_on(id: &str, on: bool) -> bool {
    let mut state = state();
    let Some(entry) = state.entry(id) else {
        return false;
    };
    if entry.on == on {
        return true;
    }
    if on {
        start(&mut state, id);
    } else {
        entry.halt();
        entry.on = false;
    }
    state.changed();
    true
}

/// Answers question `request` of tunnel `id`.
pub fn answer(id: &str, request: u64, answer: Answer) -> bool {
    let mut state = state();
    let Some(entry) = state.entry(id) else {
        return false;
    };
    let Some(position) = entry
        .requests
        .iter()
        .position(|pending| pending.id == request)
    else {
        return false;
    };
    if let Some(pending) = entry.requests.remove(position) {
        pending.answer(answer);
    }
    state.changed();
    true
}

/// Stops every tunnel (at exit).
pub fn stop_all() {
    let mut state = state();
    for entry in &mut state.entries {
        entry.halt();
        entry.on = false;
    }
}

/// A terminal session of saved host `host` (in pane `pane`) is up on `connection`, or gone
/// (`None`). Tunnels tied to the host follow.
pub fn session_live(host: &str, pane: i32, connection: Option<Arc<Connection>>) {
    let mut state = state();
    state.sessions_of(host);
    let Some(sessions) = state.sessions.get_mut(host) else {
        return;
    };
    sessions.panes.retain(|(id, _)| *id != pane);
    if let Some(connection) = connection {
        sessions.panes.push((pane, connection));
    }
    let current = sessions
        .panes
        .first()
        .map(|(_, connection)| Arc::clone(connection));
    let up = current.is_some();
    // Only a change of connection wakes the tunnels.
    let same = match (&*sessions.sender.borrow(), &current) {
        (Some(old), Some(new)) => Arc::ptr_eq(old, new),
        (None, None) => true,
        _ => false,
    };
    if !same {
        sessions.sender.send_replace(current);
    }
    for entry in &mut state.entries {
        if entry.tunnel.tied && entry.tunnel.host == host {
            entry.session = up;
        }
    }
    state.changed();
}

/// Terminal pane `pane` closed: its session no longer carries tied tunnels.
pub fn session_closed(pane: i32) {
    let hosts: Vec<String> = state()
        .sessions
        .iter()
        .filter(|(_, sessions)| sessions.panes.iter().any(|(id, _)| *id == pane))
        .map(|(host, _)| host.clone())
        .collect();
    for host in hosts {
        session_live(&host, pane, None);
    }
}

/// What the engine forwards for `tunnel`.
fn forward_of(tunnel: &Tunnel) -> Forward {
    let unbracketed = |address: &str| {
        address
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(address)
            .to_owned()
    };
    let here = |address: &str| match address {
        "" | "*" => "0.0.0.0".to_owned(),
        address => unbracketed(address),
    };
    let to = Endpoint::new(
        unbracketed(&tunnel.destination_host),
        tunnel.destination_port,
    );
    match tunnel.kind {
        Kind::Local => Forward::Local {
            bind: Endpoint::new(here(&tunnel.bind_address), tunnel.bind_port),
            to,
        },
        Kind::Dynamic => Forward::Dynamic {
            bind: Endpoint::new(here(&tunnel.bind_address), tunnel.bind_port),
        },
        // The server takes "" for every interface (OpenSSH's `*`).
        Kind::Remote => Forward::Remote {
            bind: Endpoint::new(
                match tunnel.bind_address.as_str() {
                    "*" => String::new(),
                    address => unbracketed(address),
                },
                tunnel.bind_port,
            ),
            to,
        },
    }
}

/// Runs a callback of run `run` of tunnel `id` on its entry, unless a newer run replaced it.
fn update(id: &str, run: u64, change: impl FnOnce(&mut Entry)) {
    let mut state = state();
    let Some(entry) = state.entry(id) else {
        return;
    };
    if entry.run != run {
        return;
    }
    change(entry);
    state.changed();
}

/// Starts tunnel `id` (the lock is held).
fn start(state: &mut State, id: &str) {
    let Some(runtime) = opensesh_ssh::runtime() else {
        return;
    };
    let handle = runtime.handle().clone();
    let tied_sessions = {
        let Some(entry) = state.entry(id) else {
            return;
        };
        (entry.tunnel.tied).then(|| entry.tunnel.host.clone())
    };
    let sessions = tied_sessions.map(|host| state.sessions_of(&host));
    let Some(entry) = state.entry(id) else {
        return;
    };
    entry.halt();
    entry.on = true;
    let run = entry.run;
    let tunnel = entry.tunnel.clone();
    let forward = forward_of(&tunnel);
    let traffic = Arc::clone(&entry.traffic);
    let report: tunnel::ReportSink = {
        let id = tunnel.id.clone();
        Arc::new(move |report| {
            update(&id, run, |entry| match report {
                Report::Listening(port) => {
                    entry.listening = Some(port);
                    entry.waiting = false;
                    entry.failure = None;
                }
                Report::Waiting => entry.waiting = true,
                // It can't run (its port is taken, the server refused it): its connection has
                // nothing to carry. Switching it on again tries again.
                Report::Failed(message) => {
                    entry.failure = Some(message);
                    entry.active = None;
                    for request in entry.requests.drain(..) {
                        request.answer(Answer::Cancel);
                    }
                }
            });
        })
    };
    let (stop, stopped) = watch::channel(false);
    if let Some(sessions) = sessions {
        entry.session = sessions.borrow().is_some();
        handle.spawn(tied(
            handle.clone(),
            forward,
            sessions,
            traffic,
            report,
            stopped,
        ));
        entry.active = Some(Active {
            _stop: stop,
            _kept: None,
            _running: None,
        });
        return;
    }
    let asker: Asker = {
        let id = tunnel.id.clone();
        Arc::new(move |request: Request| {
            let mut state = self::state();
            match state.entry(&id) {
                Some(entry) if entry.run == run => {
                    entry.requests.push_back(request);
                    state.changed();
                }
                // A question of a run that ended: nobody will answer it.
                _ => request.answer(Answer::Cancel),
            }
        })
    };
    let connector = match connector(&tunnel, asker) {
        Ok(connector) => connector,
        Err(detail) => {
            entry.link = Some(Link::Ended {
                code: "invalid",
                reason: detail,
            });
            return;
        }
    };
    let link: tunnel::LinkSink = {
        let id = tunnel.id.clone();
        Arc::new(move |link| update(&id, run, |entry| entry.link = Some(link)))
    };
    let kept = tunnel::keep_connected(&handle, connector, tunnel.reconnect, link);
    let running =
        tunnel::start_counting(&handle, forward, kept.connections.clone(), traffic, report);
    entry.active = Some(Active {
        _stop: stop,
        _kept: Some(kept),
        _running: Some(running),
    });
    drop(stopped);
}

/// A tied tunnel: runs while its host has a session, stops when the last one goes.
async fn tied(
    handle: tokio::runtime::Handle,
    forward: Forward,
    mut sessions: Connections,
    traffic: Arc<Traffic>,
    report: tunnel::ReportSink,
    mut stopped: watch::Receiver<bool>,
) {
    let mut running: Option<Running> = None;
    loop {
        let up = sessions
            .borrow_and_update()
            .as_ref()
            .is_some_and(|connection| !connection.is_closed());
        if up && running.is_none() {
            running = Some(tunnel::start_counting(
                &handle,
                forward.clone(),
                sessions.clone(),
                Arc::clone(&traffic),
                Arc::clone(&report),
            ));
        } else if !up && running.is_some() {
            running = None;
        }
        tokio::select! {
            changed = sessions.changed() => if changed.is_err() {
                return;
            },
            _ = stopped.changed() => return,
        }
    }
}

/// Makes the connections of an independent tunnel: through its saved host or its text.
fn connector(tunnel: &Tunnel, asker: Asker) -> Result<Connector, String> {
    // The terminal settings don't matter to a connection that only forwards.
    let size = TermSize::new(80, 24);
    let start = if tunnel.host.is_empty() {
        crate::ssh::for_target(&tunnel.target, size, "xterm-256color")
    } else {
        crate::ssh::for_host(&tunnel.host, size, "xterm-256color")
    }?;
    let spec = start.connect;
    Ok(Arc::new(move || {
        let spec = spec.clone();
        let asker = Arc::clone(&asker);
        Box::pin(async move { connect::connect(&spec, &asker, &connect::quiet()).await })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards() {
        let tunnel = Tunnel {
            bind_address: "*".into(),
            bind_port: 1080,
            ..Tunnel::new(Kind::Dynamic)
        };
        assert_eq!(
            forward_of(&tunnel),
            Forward::Dynamic {
                bind: Endpoint::new("0.0.0.0", 1080)
            }
        );
        let tunnel = Tunnel {
            bind_address: "[::1]".into(),
            bind_port: 8080,
            destination_host: "[2001:db8::1]".into(),
            destination_port: 80,
            ..Tunnel::new(Kind::Local)
        };
        assert_eq!(
            forward_of(&tunnel),
            Forward::Local {
                bind: Endpoint::new("::1", 8080),
                to: Endpoint::new("2001:db8::1", 80)
            }
        );
        let tunnel = Tunnel {
            bind_address: "*".into(),
            destination_host: "localhost".into(),
            destination_port: 3000,
            ..Tunnel::new(Kind::Remote)
        };
        assert_eq!(
            forward_of(&tunnel),
            Forward::Remote {
                bind: Endpoint::new("", 0),
                to: Endpoint::new("localhost", 3000)
            }
        );
    }

    #[test]
    fn views_follow_the_state() {
        let mut entry = Entry::new(Tunnel {
            host: "H1".into(),
            tied: true,
            ..Tunnel::new(Kind::Dynamic)
        });
        assert_eq!(entry.view().state, "stopped");
        entry.on = true;
        assert_eq!(entry.view().state, "waiting");
        entry.session = true;
        assert_eq!(entry.view().state, "connecting");
        entry.listening = Some(1080);
        assert_eq!((entry.view().state, entry.view().port), ("running", 1080));
        entry.failure = Some("can't listen".into());
        assert_eq!(entry.view().state, "failed");

        let mut entry = Entry::new(Tunnel {
            target: "a@b".into(),
            ..Tunnel::new(Kind::Dynamic)
        });
        entry.on = true;
        entry.link = Some(Link::Retrying {
            in_secs: 4,
            reason: "lost".into(),
        });
        let view = entry.view();
        assert_eq!((view.state, view.retry_in), ("retrying", 4));
        entry.link = Some(Link::Ended {
            code: "auth",
            reason: "no".into(),
        });
        assert_eq!((entry.view().state, entry.view().code), ("failed", "auth"));
    }
}
