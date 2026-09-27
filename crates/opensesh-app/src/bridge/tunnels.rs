//! `Tunnels` QML singleton (Sprint 9): the tunnels of `tunnels.toml` with what they are doing,
//! and what the Tunnels view does with them. The tunnels run in `crate::tunnels`; this object
//! shows them, saves the file through the background writer, and reloads it when it changes on
//! disk. Test runs start without the user's tunnels and never write the file.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        /// Qt string list type from cxx-qt-lib.
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        /// The tunnels.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, list, READ, NOTIFY = changed)]
        #[qproperty(i32, count, READ, NOTIFY = changed)]
        #[qproperty(i32, running, READ, NOTIFY = changed)]
        #[qproperty(bool, read_only, cxx_name = "readOnly", READ, NOTIFY = changed)]
        type Tunnels = super::TunnelsRust;

        /// `list`, `count`, `running` or `readOnly` changed.
        #[qsignal]
        fn changed(self: Pin<&mut Self>);

        /// Tunnel `id` asks something (a host key, a password) that waits in its row.
        #[qsignal]
        #[cxx_name = "needsAnswer"]
        fn needs_answer(self: Pin<&mut Self>, id: QString, name: QString);

        /// `save-failed` or `read-only`, with technical detail.
        #[qsignal]
        fn problem(self: Pin<&mut Self>, kind: QString, detail: QString);

        /// Switches tunnel `id` on or off.
        #[qinvokable]
        #[cxx_name = "setOn"]
        fn set_on(self: Pin<&mut Self>, id: &QString, on: bool) -> bool;

        /// Adds a tunnel, or replaces the one with the same id, from the editor's JSON (`id`
        /// empty for a new one; `name`, `kind`, `host`, `target`, `bindAddress`, `bindPort`,
        /// `destinationHost`, `destinationPort`, `tied`, `autostart`, `reconnect`). Its id, or
        /// empty when it can't be saved (see `check`).
        #[qinvokable]
        fn save(self: Pin<&mut Self>, json: &QString) -> QString;

        /// Why the tunnel in `json` can't be saved; empty when it can.
        #[qinvokable]
        fn check(self: &Self, json: &QString) -> QString;

        /// Removes tunnel `id`.
        #[qinvokable]
        fn remove(self: Pin<&mut Self>, id: &QString) -> bool;

        /// A copy of tunnel `id` (off); its id.
        #[qinvokable]
        fn duplicate(self: Pin<&mut Self>, id: &QString) -> QString;

        /// Answers question `prompt` of tunnel `id` (see `TerminalItem.answerPrompt`).
        #[qinvokable]
        #[cxx_name = "answerPrompt"]
        fn answer_prompt(
            self: Pin<&mut Self>,
            id: &QString,
            prompt: i32,
            action: &QString,
            secrets: &QStringList,
        ) -> bool;

        /// Whether `address` only listens on the loopback.
        #[qinvokable]
        #[cxx_name = "isLoopback"]
        fn is_loopback(self: &Self, address: &QString) -> bool;

        /// The forwards of `~/.ssh/config` as JSON: `[{key, alias, host, hostName, kind,
        /// bindAddress, bindPort, destinationHost, destinationPort, known}]`. `host` is the
        /// saved host they would go through (empty when the host isn't in OpenSesh); `known`
        /// says a tunnel like it exists already.
        #[qinvokable]
        #[cxx_name = "importCandidates"]
        fn import_candidates(self: &Self) -> QString;

        /// Imports the candidates with these keys as tunnels tied to their hosts; how many.
        #[qinvokable]
        #[cxx_name = "importForwards"]
        fn import_forwards(self: Pin<&mut Self>, keys: &QStringList) -> i32;
    }

    impl cxx_qt::Initialize for Tunnels {}
    impl cxx_qt::Threading for Tunnels {}
}

use core::pin::Pin;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use opensesh_core::fsutil;
use opensesh_core::tunnels::{self as model, Kind, TUNNELS_FILE, Tunnel, TunnelsFile};
use opensesh_core::watch::FileWatcher;
use opensesh_import::ssh_config;
use opensesh_ssh::prompt::Answer;
use serde_json::{Value as Json, json};

use crate::bridge::app_info::is_test_run;
use crate::saves::SaveTracker;
use crate::services;
use crate::tunnels::{self as service, View};

const RELOAD_DEBOUNCE: Duration = Duration::from_millis(250);

/// Rust state behind `Tunnels`.
#[derive(Default)]
pub struct TunnelsRust {
    list: QString,
    count: i32,
    running: i32,
    read_only: bool,
    file: Option<PathBuf>,
    saves: SaveTracker,
    watcher: Option<FileWatcher>,
    /// Tunnels that had a question at the last refresh.
    asking: HashSet<String>,
    /// A refresh is queued already.
    queued: Arc<AtomicBool>,
}

impl std::fmt::Debug for TunnelsRust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TunnelsRust")
            .field("count", &self.count)
            .field("file", &self.file)
            .finish_non_exhaustive()
    }
}

fn qstring(text: &str) -> QString {
    QString::from(text)
}

/// The saved host's name, or the tunnel's text.
fn host_label(tunnel: &Tunnel) -> String {
    if tunnel.host.is_empty() {
        return tunnel.target.clone();
    }
    crate::hosts::current()
        .file
        .host(&tunnel.host)
        .map_or_else(|| tunnel.host.clone(), |host| host.name.clone())
}

fn view_json(view: &View) -> Json {
    let tunnel = &view.tunnel;
    let prompt = view
        .prompt
        .as_ref()
        .map(|(id, prompt)| crate::bridge::terminal_view::ssh_prompt_json(*id, prompt))
        .unwrap_or_default();
    json!({
        "id": tunnel.id,
        "name": tunnel.name,
        "kind": tunnel.kind.as_str(),
        "host": tunnel.host,
        "target": tunnel.target,
        "hostName": host_label(tunnel),
        "bindAddress": tunnel.bind_address,
        "bindPort": tunnel.bind_port,
        "destinationHost": tunnel.destination_host,
        "destinationPort": tunnel.destination_port,
        "tied": tunnel.tied,
        "autostart": tunnel.autostart,
        "reconnect": tunnel.reconnect,
        "exposed": tunnel.exposed(),
        "on": view.on,
        "state": view.state,
        "port": view.port,
        "code": view.code,
        "detail": view.detail,
        "retryIn": view.retry_in,
        "sent": view.counts.sent,
        "received": view.counts.received,
        "open": view.counts.open,
        "total": view.counts.total,
        "prompt": prompt,
    })
}

/// A tunnel from the editor's JSON, keeping what the existing one with its id doesn't show.
fn from_json(text: &str) -> Result<Tunnel, String> {
    let value: Json = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let string = |key: &str| value[key].as_str().unwrap_or_default().trim().to_owned();
    let port = |key: &str| {
        value[key]
            .as_u64()
            .and_then(|port| u16::try_from(port).ok())
            .unwrap_or(0)
    };
    let flag = |key: &str, default: bool| value[key].as_bool().unwrap_or(default);
    let kind = Kind::parse(&string("kind")).ok_or("the kind is not local, remote or dynamic")?;
    let id = string("id");
    let mut tunnel = service::views()
        .into_iter()
        .map(|view| view.tunnel)
        .find(|tunnel| !id.is_empty() && tunnel.id == id)
        .unwrap_or_else(|| Tunnel::new(kind));
    tunnel.name = string("name");
    tunnel.kind = kind;
    tunnel.host = string("host");
    tunnel.target = if tunnel.host.is_empty() {
        string("target")
    } else {
        String::new()
    };
    tunnel.bind_address = string("bindAddress");
    tunnel.bind_port = port("bindPort");
    if kind == Kind::Dynamic {
        tunnel.destination_host.clear();
        tunnel.destination_port = 0;
    } else {
        tunnel.destination_host = string("destinationHost");
        tunnel.destination_port = port("destinationPort");
    }
    tunnel.tied = flag("tied", false);
    tunnel.autostart = flag("autostart", false);
    tunnel.reconnect = flag("reconnect", true);
    Ok(tunnel)
}

impl qobject::Tunnels {
    /// Rebuilds what QML reads from the service.
    fn refresh(mut self: Pin<&mut Self>) {
        self.queued.store(false, Ordering::Relaxed);
        let views = service::views();
        let list = Json::Array(views.iter().map(view_json).collect()).to_string();
        let running = views.iter().filter(|view| view.state == "running").count();
        let asking: HashSet<String> = views
            .iter()
            .filter(|view| view.prompt.is_some())
            .map(|view| view.tunnel.id.clone())
            .collect();
        let new_questions: Vec<(String, String)> = views
            .iter()
            .filter(|view| view.prompt.is_some() && !self.asking.contains(&view.tunnel.id))
            .map(|view| {
                let name = if view.tunnel.name.is_empty() {
                    host_label(&view.tunnel)
                } else {
                    view.tunnel.name.clone()
                };
                (view.tunnel.id.clone(), name)
            })
            .collect();
        let changed = list != self.list.to_string();
        {
            let mut state = self.as_mut().rust_mut();
            state.asking = asking;
            if changed {
                state.list = QString::from(&list);
                state.count = i32::try_from(views.len()).unwrap_or(i32::MAX);
                state.running = i32::try_from(running).unwrap_or(i32::MAX);
            }
        }
        if changed {
            self.as_mut().changed();
        }
        for (id, name) in new_questions {
            self.as_mut().needs_answer(qstring(&id), qstring(&name));
        }
    }

    /// Queues a save of the file (not in test runs: they keep changes in memory).
    fn save_file(mut self: Pin<&mut Self>) {
        let (Some(path), Some(services)) = (
            self.file.clone(),
            services::get().filter(|_| !is_test_run()),
        ) else {
            return;
        };
        let Some(file) = service::to_file() else {
            self.as_mut().problem(
                qstring("read-only"),
                QString::from(&path.display().to_string()),
            );
            return;
        };
        let seq = self.as_mut().rust_mut().saves.queue();
        let qt_thread = self.qt_thread();
        services.writer.write(
            path,
            file.to_toml_string().into_bytes(),
            fsutil::DEFAULT_BACKUPS,
            Some(Box::new(move |path, result| {
                let failure = result
                    .err()
                    .map(|error| format!("{}: {error}", path.display()));
                let _ = qt_thread.queue(move |object| object.save_done(seq, failure));
            })),
        );
    }

    fn save_done(mut self: Pin<&mut Self>, seq: u64, failure: Option<String>) {
        self.as_mut().rust_mut().saves.finished(seq);
        if let Some(detail) = failure {
            tracing::warn!("could not save {TUNNELS_FILE}: {detail}");
            self.as_mut()
                .problem(qstring("save-failed"), QString::from(&detail));
        }
        if self.as_mut().rust_mut().saves.take_reload() {
            self.reload_in_background();
        }
    }

    fn reload_in_background(self: Pin<&mut Self>) {
        let Some(path) = self.file.clone() else {
            return;
        };
        let qt_thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("opensesh-tunnels".to_owned())
            .spawn(move || {
                let (file, warnings) = TunnelsFile::load_file(&path);
                for warning in &warnings {
                    tracing::warn!("{TUNNELS_FILE}: {warning}");
                }
                let _ = qt_thread.queue(move |object| object.apply_disk(file));
            });
        if let Err(error) = spawned {
            tracing::warn!("could not reload {TUNNELS_FILE}: {error}");
        }
    }

    fn apply_disk(mut self: Pin<&mut Self>, file: TunnelsFile) {
        if self.saves.pending() {
            self.as_mut().rust_mut().saves.reload_wanted = true;
            return;
        }
        let read_only = file.read_only;
        let current = service::to_file();
        if current.as_ref() != Some(&file) {
            tracing::info!("{TUNNELS_FILE} changed on disk; tunnels reloaded");
            service::reload(file);
        }
        self.as_mut().rust_mut().read_only = read_only;
        self.refresh();
    }

    /// See the bridge declaration.
    pub fn set_on(self: Pin<&mut Self>, id: &QString, on: bool) -> bool {
        let done = service::set_on(&id.to_string(), on);
        self.refresh();
        done
    }

    /// See the bridge declaration.
    pub fn save(mut self: Pin<&mut Self>, json: &QString) -> QString {
        let saved = from_json(&json.to_string())
            .and_then(|tunnel| service::upsert(tunnel).map_err(str::to_owned));
        match saved {
            Ok(id) => {
                self.as_mut().save_file();
                self.refresh();
                QString::from(&id)
            }
            Err(problem) => {
                tracing::debug!("a tunnel wasn't saved: {problem}");
                QString::default()
            }
        }
    }

    /// See the bridge declaration.
    pub fn check(&self, json: &QString) -> QString {
        match from_json(&json.to_string()) {
            Ok(tunnel) => tunnel.problem().map(qstring).unwrap_or_default(),
            Err(problem) => QString::from(&problem),
        }
    }

    /// See the bridge declaration.
    pub fn remove(mut self: Pin<&mut Self>, id: &QString) -> bool {
        let removed = service::remove(&id.to_string());
        if removed {
            self.as_mut().save_file();
        }
        self.refresh();
        removed
    }

    /// See the bridge declaration.
    pub fn duplicate(mut self: Pin<&mut Self>, id: &QString) -> QString {
        let copy = service::duplicate(&id.to_string());
        if copy.is_some() {
            self.as_mut().save_file();
        }
        self.refresh();
        copy.map(|id| QString::from(&id)).unwrap_or_default()
    }

    /// See the bridge declaration.
    pub fn answer_prompt(
        self: Pin<&mut Self>,
        id: &QString,
        prompt: i32,
        action: &QString,
        secrets: &QStringList,
    ) -> bool {
        let answer = match action.to_string().as_str() {
            "trust-once" => Answer::TrustOnce,
            "trust-save" => Answer::TrustAndRemember,
            "submit" => Answer::Secrets(
                secrets
                    .iter()
                    .map(|secret| secrecy::SecretString::from(secret.to_string()))
                    .collect(),
            ),
            _ => Answer::Cancel,
        };
        let answered = service::answer(&id.to_string(), u64::try_from(prompt).unwrap_or(0), answer);
        self.refresh();
        answered
    }

    /// See the bridge declaration.
    pub fn is_loopback(&self, address: &QString) -> bool {
        model::is_loopback(&address.to_string())
    }

    /// The forwards of `~/.ssh/config`, with the saved host each would go through.
    fn candidates() -> Vec<(String, String, String, ssh_config::SshForward)> {
        let Some(home) = opensesh_core::paths::home_dir() else {
            return Vec::new();
        };
        let config = ssh_config::load(&home.join(".ssh").join("config"), &home);
        let library = crate::hosts::current();
        let mut out = Vec::new();
        for host in &config.hosts {
            // A linked host keeps its name as id; a copied one is found by its name.
            let linked = format!("{}{}", opensesh_core::hosts::LINKED_PREFIX, host.alias);
            let saved = library
                .file
                .host(&linked)
                .or_else(|| {
                    library
                        .file
                        .hosts
                        .iter()
                        .find(|saved| saved.name == host.alias)
                })
                .map(|saved| saved.id.clone())
                .unwrap_or_default();
            for (index, forward) in host.forwards.iter().enumerate() {
                let key = format!("{}#{}#{index}", host.alias, forward.line);
                out.push((key, host.alias.clone(), saved.clone(), forward.clone()));
            }
        }
        out
    }

    /// See the bridge declaration.
    pub fn import_candidates(&self) -> QString {
        let existing: Vec<Tunnel> = service::views()
            .into_iter()
            .map(|view| view.tunnel)
            .collect();
        let list: Vec<Json> = Self::candidates()
            .into_iter()
            .map(|(key, alias, host, forward)| {
                let tunnel = forward.to_tunnel(&alias, &host);
                let known = existing.iter().any(|seen| {
                    seen.host == tunnel.host
                        && seen.kind == tunnel.kind
                        && seen.bind_port == tunnel.bind_port
                        && seen.destination_host == tunnel.destination_host
                        && seen.destination_port == tunnel.destination_port
                });
                json!({
                    "key": key,
                    "alias": alias,
                    "host": host,
                    "hostName": if host.is_empty() { alias.clone() } else { host_label(&tunnel) },
                    "kind": tunnel.kind.as_str(),
                    "bindAddress": tunnel.bind_address,
                    "bindPort": tunnel.bind_port,
                    "destinationHost": tunnel.destination_host,
                    "destinationPort": tunnel.destination_port,
                    "known": known,
                })
            })
            .collect();
        QString::from(&Json::Array(list).to_string())
    }

    /// See the bridge declaration.
    pub fn import_forwards(mut self: Pin<&mut Self>, keys: &QStringList) -> i32 {
        let wanted: HashSet<String> = keys.iter().map(ToString::to_string).collect();
        let mut imported = 0;
        for (key, alias, host, forward) in Self::candidates() {
            if host.is_empty() || !wanted.contains(&key) {
                continue;
            }
            if service::upsert(forward.to_tunnel(&alias, &host)).is_ok() {
                imported += 1;
            }
        }
        if imported > 0 {
            self.as_mut().save_file();
        }
        self.refresh();
        imported
    }
}

impl cxx_qt::Initialize for qobject::Tunnels {
    fn initialize(mut self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        let queued = Arc::clone(&self.queued);
        service::set_notify(Arc::new(move || {
            // Many changes in a row make one refresh.
            if !queued.swap(true, Ordering::Relaxed) {
                let _ = qt_thread.queue(|object| object.refresh());
            }
        }));
        // The counters move without other changes: refresh them while tunnels run.
        if let Some(runtime) = opensesh_ssh::runtime() {
            let qt_thread = self.qt_thread();
            runtime.spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    if service::views().iter().any(|view| view.on)
                        && qt_thread.queue(|object| object.refresh()).is_err()
                    {
                        return;
                    }
                }
            });
        }
        // Test runs start without the user's tunnels, and keep changes in memory.
        let path = services::get().map(|services| services.paths.config_dir().join(TUNNELS_FILE));
        if is_test_run() || path.is_none() {
            service::load(TunnelsFile::default(), false);
            self.refresh();
            return;
        }
        let Some(path) = path else {
            return;
        };
        // One small file read at startup, before the first frame.
        let (file, warnings) = TunnelsFile::load_file(&path);
        for warning in &warnings {
            tracing::warn!("{TUNNELS_FILE}: {warning}");
        }
        {
            let mut state = self.as_mut().rust_mut();
            state.read_only = file.read_only;
            state.file = Some(path.clone());
        }
        service::load(file, true);
        let qt_thread = self.qt_thread();
        match FileWatcher::spawn(&path, RELOAD_DEBOUNCE, move || {
            let _ = qt_thread.queue(|object| object.reload_in_background());
        }) {
            Ok(watcher) => self.as_mut().rust_mut().watcher = Some(watcher),
            Err(error) => tracing::warn!("{TUNNELS_FILE} won't hot-reload: {error}"),
        }
        self.refresh();
    }
}
