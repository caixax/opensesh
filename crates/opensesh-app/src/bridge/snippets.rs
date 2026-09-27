//! `Snippets` QML singleton (Sprint 10): the snippets and macros of `snippets.toml`, what the
//! Snippets view, the quick picker and the side panel do with them, running them on panes
//! (`crate::snippets`) and recording macros. The file is saved through the background writer
//! and reloaded when it changes on disk. Test runs start without the user's snippets and never
//! write the file.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// The snippets.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, list, READ, NOTIFY = changed)]
        #[qproperty(QString, folders, READ, NOTIFY = changed)]
        #[qproperty(QString, tags, READ, NOTIFY = changed)]
        #[qproperty(i32, count, READ, NOTIFY = changed)]
        #[qproperty(bool, read_only, cxx_name = "readOnly", READ, NOTIFY = changed)]
        type Snippets = super::SnippetsRust;

        /// The list, the folders, the tags or `readOnly` changed.
        #[qsignal]
        fn changed(self: Pin<&mut Self>);

        /// A run of snippet `id` ended in pane `pane` (0: before any pane, like a locked
        /// vault): `code` empty when everything was typed, else why it stopped (`gone`,
        /// `timeout`, `secret-locked`, `secret-unknown`, `no-secret`, `missing`), with `detail`.
        #[qsignal]
        #[cxx_name = "runEnded"]
        fn run_ended(self: Pin<&mut Self>, id: QString, pane: i32, code: QString, detail: QString);

        /// `save-failed` or `read-only`, with technical detail.
        #[qsignal]
        fn problem(self: Pin<&mut Self>, kind: QString, detail: QString);

        /// Adds a snippet, or replaces the one with the same id, from the editor's JSON (see
        /// `list`; `id` empty for a new one). Its id, or empty when it can't be saved (`check`).
        #[qinvokable]
        fn save(self: Pin<&mut Self>, json: &QString) -> QString;

        /// Why the snippet in `json` can't be saved; empty when it can.
        #[qinvokable]
        fn check(self: &Self, json: &QString) -> QString;

        /// Removes snippet `id`.
        #[qinvokable]
        fn remove(self: Pin<&mut Self>, id: &QString) -> bool;

        /// A copy of snippet `id`, right after it (without its shortcut); its id.
        #[qinvokable]
        fn duplicate(self: Pin<&mut Self>, id: &QString) -> QString;

        /// The values last used for snippet `id`'s variables, as a JSON object.
        #[qinvokable]
        #[cxx_name = "lastValues"]
        fn last_values(self: &Self, id: &QString) -> QString;

        /// Runs snippet `id` in the panes of `panes` (a JSON list of pane ids), with `values`
        /// for its variables (a JSON object). Each pane's end comes as `runEnded`. False when
        /// the snippet doesn't exist.
        #[qinvokable]
        fn run(self: Pin<&mut Self>, id: &QString, values: &QString, panes: &QString) -> bool;

        /// Starts recording what is typed in pane `pane` as a macro.
        #[qinvokable]
        #[cxx_name = "recordStart"]
        fn record_start(self: Pin<&mut Self>, pane: i32) -> bool;

        /// Stops recording pane `pane`: the steps, as JSON (see `list`).
        #[qinvokable]
        #[cxx_name = "recordStop"]
        fn record_stop(self: Pin<&mut Self>, pane: i32) -> QString;

        /// Whether pane `pane` records a macro.
        #[qinvokable]
        #[cxx_name = "isRecording"]
        fn is_recording(self: &Self, pane: i32) -> bool;
    }

    impl cxx_qt::Initialize for Snippets {}
    impl cxx_qt::Threading for Snippets {}
}

use core::pin::Pin;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use opensesh_core::fsutil;
use opensesh_core::snippets::{DEFAULT_TIMEOUT_MS, SNIPPETS_FILE, Snippet, SnippetsFile, Step};
use opensesh_core::watch::FileWatcher;
use serde_json::{Value as Json, json};

use crate::bridge::app_info::is_test_run;
use crate::saves::SaveTracker;
use crate::services;
use crate::snippets::{self as runner, Outcome};

const RELOAD_DEBOUNCE: Duration = Duration::from_millis(250);

/// Rust state behind `Snippets`.
#[derive(Default)]
pub struct SnippetsRust {
    list: QString,
    folders: QString,
    tags: QString,
    count: i32,
    read_only: bool,
    snippets: SnippetsFile,
    file: Option<PathBuf>,
    saves: SaveTracker,
    watcher: Option<FileWatcher>,
    /// The values last used for each snippet's variables (this session only).
    last_values: HashMap<String, HashMap<String, String>>,
}

impl std::fmt::Debug for SnippetsRust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnippetsRust")
            .field("count", &self.count)
            .field("file", &self.file)
            .finish_non_exhaustive()
    }
}

fn step_json(step: &Step) -> Json {
    match step {
        Step::Send(text) => json!({ "kind": "send", "text": text }),
        Step::Delay(ms) => json!({ "kind": "delay", "ms": ms }),
        Step::WaitFor {
            pattern,
            timeout_ms,
        } => json!({ "kind": "wait", "pattern": pattern, "timeout": timeout_ms }),
    }
}

fn steps_json(steps: &[Step]) -> Json {
    Json::Array(steps.iter().map(step_json).collect())
}

fn snippet_json(snippet: &Snippet) -> Json {
    json!({
        "id": snippet.id,
        "name": snippet.name,
        "folder": snippet.folder,
        "tags": snippet.tags,
        "description": snippet.description,
        "shortcut": snippet.shortcut,
        "text": snippet.text,
        "steps": steps_json(&snippet.steps),
        "macro": snippet.is_macro(),
        "variables": snippet.variables(),
        "secrets": snippet.secrets(),
    })
}

fn step_from(value: &Json) -> Result<Step, String> {
    let number = |key: &str| value[key].as_u64().unwrap_or(0);
    match value["kind"].as_str() {
        Some("send") => Ok(Step::Send(
            value["text"].as_str().unwrap_or_default().to_owned(),
        )),
        Some("delay") => Ok(Step::Delay(number("ms"))),
        Some("wait") => Ok(Step::WaitFor {
            pattern: value["pattern"].as_str().unwrap_or_default().to_owned(),
            timeout_ms: match number("timeout") {
                0 => DEFAULT_TIMEOUT_MS,
                ms => ms,
            },
        }),
        _ => Err("a step is send, delay or wait".to_owned()),
    }
}

impl qobject::Snippets {
    /// A snippet from the editor's JSON, keeping what the existing one with its id doesn't show.
    fn parse_editor(&self, text: &str) -> Result<Snippet, String> {
        let value: Json = serde_json::from_str(text).map_err(|error| error.to_string())?;
        let string = |key: &str| value[key].as_str().unwrap_or_default().to_owned();
        let id = string("id");
        let mut snippet = self
            .snippets
            .get(&id)
            .cloned()
            .unwrap_or_else(|| Snippet::new(""));
        snippet.name = string("name").trim().to_owned();
        snippet.folder = opensesh_core::snippets::normalize_folder(&string("folder"));
        snippet.tags = value["tags"]
            .as_array()
            .map(|tags| {
                tags.iter()
                    .filter_map(|tag| tag.as_str().map(|tag| tag.trim().to_owned()))
                    .filter(|tag| !tag.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        snippet.description = string("description");
        snippet.shortcut = string("shortcut").trim().to_owned();
        let steps = value["steps"]
            .as_array()
            .map(|steps| steps.iter().map(step_from).collect::<Result<Vec<_>, _>>())
            .transpose()?
            .unwrap_or_default();
        if value["macro"].as_bool().unwrap_or(false) {
            snippet.steps = steps;
            snippet.text.clear();
        } else {
            snippet.steps.clear();
            snippet.text = string("text");
        }
        Ok(snippet)
    }

    /// Publishes the list, the folders and the tags.
    fn refresh(mut self: Pin<&mut Self>) {
        let list =
            Json::Array(self.snippets.snippets.iter().map(snippet_json).collect()).to_string();
        let folders = Json::from(self.snippets.folders()).to_string();
        let tags = Json::Array(
            self.snippets
                .tags()
                .into_iter()
                .map(|(tag, count)| json!({ "tag": tag, "count": count }))
                .collect(),
        )
        .to_string();
        {
            let mut state = self.as_mut().rust_mut();
            state.count = i32::try_from(state.snippets.snippets.len()).unwrap_or(i32::MAX);
            state.read_only = state.snippets.read_only;
            state.list = QString::from(&list);
            state.folders = QString::from(&folders);
            state.tags = QString::from(&tags);
        }
        self.changed();
    }

    /// Queues a save of the file (not in test runs: they keep changes in memory).
    fn save_file(mut self: Pin<&mut Self>) {
        let (Some(path), Some(services)) = (
            self.file.clone(),
            services::get().filter(|_| !is_test_run()),
        ) else {
            return;
        };
        if self.snippets.read_only {
            self.as_mut().problem(
                QString::from("read-only"),
                QString::from(&path.display().to_string()),
            );
            return;
        }
        let text = self.snippets.to_toml_string();
        let seq = self.as_mut().rust_mut().saves.queue();
        let qt_thread = self.qt_thread();
        services.writer.write(
            path,
            text.into_bytes(),
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
            tracing::warn!("could not save {SNIPPETS_FILE}: {detail}");
            self.as_mut()
                .problem(QString::from("save-failed"), QString::from(&detail));
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
            .name("opensesh-snippets".to_owned())
            .spawn(move || {
                let (file, warnings) = SnippetsFile::load_file(&path);
                for warning in &warnings {
                    tracing::warn!("{SNIPPETS_FILE}: {warning}");
                }
                let _ = qt_thread.queue(move |object| object.apply_disk(file));
            });
        if let Err(error) = spawned {
            tracing::warn!("could not reload {SNIPPETS_FILE}: {error}");
        }
    }

    fn apply_disk(mut self: Pin<&mut Self>, file: SnippetsFile) {
        if self.saves.pending() {
            self.as_mut().rust_mut().saves.reload_wanted = true;
            return;
        }
        if self.snippets != file {
            tracing::info!("{SNIPPETS_FILE} changed on disk; snippets reloaded");
            self.as_mut().rust_mut().snippets = file;
            self.refresh();
        }
    }

    /// See the bridge declaration.
    pub fn save(mut self: Pin<&mut Self>, json: &QString) -> QString {
        let snippet = match self.parse_editor(&json.to_string()) {
            Ok(snippet) => snippet,
            Err(problem) => {
                tracing::debug!("a snippet wasn't saved: {problem}");
                return QString::default();
            }
        };
        if snippet.problem().is_some() {
            return QString::default();
        }
        let id = snippet.id.clone();
        {
            let mut state = self.as_mut().rust_mut();
            match state
                .snippets
                .snippets
                .iter_mut()
                .find(|seen| seen.id == id)
            {
                Some(existing) => *existing = snippet,
                None => state.snippets.snippets.push(snippet),
            }
        }
        self.as_mut().save_file();
        self.refresh();
        QString::from(&id)
    }

    /// See the bridge declaration.
    pub fn check(&self, json: &QString) -> QString {
        match self.parse_editor(&json.to_string()) {
            Ok(snippet) => snippet
                .problem()
                .map(|problem| QString::from(&problem))
                .unwrap_or_default(),
            Err(problem) => QString::from(&problem),
        }
    }

    /// See the bridge declaration.
    pub fn remove(mut self: Pin<&mut Self>, id: &QString) -> bool {
        let id = id.to_string();
        let before = self.snippets.snippets.len();
        self.as_mut()
            .rust_mut()
            .snippets
            .snippets
            .retain(|snippet| snippet.id != id);
        let removed = self.snippets.snippets.len() != before;
        if removed {
            self.as_mut().save_file();
            self.refresh();
        }
        removed
    }

    /// See the bridge declaration.
    pub fn duplicate(mut self: Pin<&mut Self>, id: &QString) -> QString {
        let id = id.to_string();
        let Some(position) = self
            .snippets
            .snippets
            .iter()
            .position(|snippet| snippet.id == id)
        else {
            return QString::default();
        };
        let mut copy = self.snippets.snippets[position].clone();
        copy.id = opensesh_core::hosts::new_id();
        copy.shortcut.clear();
        let new_id = copy.id.clone();
        self.as_mut()
            .rust_mut()
            .snippets
            .snippets
            .insert(position + 1, copy);
        self.as_mut().save_file();
        self.refresh();
        QString::from(&new_id)
    }

    /// See the bridge declaration.
    pub fn last_values(&self, id: &QString) -> QString {
        let values = self
            .last_values
            .get(&id.to_string())
            .cloned()
            .unwrap_or_default();
        QString::from(&json!(values).to_string())
    }

    /// See the bridge declaration.
    pub fn run(mut self: Pin<&mut Self>, id: &QString, values: &QString, panes: &QString) -> bool {
        let id = id.to_string();
        let Some(snippet) = self.snippets.get(&id).cloned() else {
            return false;
        };
        let values: HashMap<String, String> =
            serde_json::from_str::<HashMap<String, Json>>(&values.to_string())
                .unwrap_or_default()
                .into_iter()
                .map(|(key, value)| {
                    let text = match value {
                        Json::String(text) => text,
                        other => other.to_string(),
                    };
                    (key, text)
                })
                .collect();
        let panes: Vec<i32> = serde_json::from_str(&panes.to_string()).unwrap_or_default();
        self.as_mut()
            .rust_mut()
            .last_values
            .insert(id.clone(), values.clone());
        let qt_thread = self.qt_thread();
        let report: runner::ReportSink = Arc::new(move |pane, outcome| {
            let id = id.clone();
            let _ = qt_thread.queue(move |object| {
                let (code, detail) = match outcome {
                    Outcome::Done => ("", String::new()),
                    Outcome::Stopped { code, detail } => (code, detail),
                };
                object.run_ended(
                    QString::from(&id),
                    pane,
                    QString::from(code),
                    QString::from(&detail),
                );
            });
        });
        runner::run(snippet, values, panes, report);
        true
    }

    /// See the bridge declaration.
    pub fn record_start(self: Pin<&mut Self>, pane: i32) -> bool {
        runner::record_start(pane)
    }

    /// See the bridge declaration.
    pub fn record_stop(self: Pin<&mut Self>, pane: i32) -> QString {
        QString::from(&steps_json(&runner::record_stop(pane)).to_string())
    }

    /// See the bridge declaration.
    pub fn is_recording(&self, pane: i32) -> bool {
        runner::recording(pane)
    }
}

impl cxx_qt::Initialize for qobject::Snippets {
    fn initialize(mut self: Pin<&mut Self>) {
        let path = services::get().map(|services| services.paths.config_dir().join(SNIPPETS_FILE));
        // Test runs start without the user's snippets, and keep changes in memory.
        if is_test_run() {
            self.refresh();
            return;
        }
        let Some(path) = path else {
            self.refresh();
            return;
        };
        // One small file read at startup, before the first frame.
        let (file, warnings) = SnippetsFile::load_file(&path);
        for warning in &warnings {
            tracing::warn!("{SNIPPETS_FILE}: {warning}");
        }
        {
            let mut state = self.as_mut().rust_mut();
            state.snippets = file;
            state.file = Some(path.clone());
        }
        let qt_thread = self.qt_thread();
        match FileWatcher::spawn(&path, RELOAD_DEBOUNCE, move || {
            let _ = qt_thread.queue(|object| object.reload_in_background());
        }) {
            Ok(watcher) => self.as_mut().rust_mut().watcher = Some(watcher),
            Err(error) => tracing::warn!("{SNIPPETS_FILE} won't hot-reload: {error}"),
        }
        self.refresh();
    }
}
