//! `Keybindings` QML singleton: the shortcuts the user changed, in `keybindings.toml`
//! (PLAN §4.2, §6.4).
//!
//! Actions keep their default shortcut in QML (`OsAction.defaultShortcut`); their `shortcut` is
//! `Keybindings.shortcut(actionId, defaultShortcut)`, re-read whenever `revision` changes. Only
//! differences from the defaults are stored. The file is watched and applied live; saves go
//! through the background writer.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// The user's keyboard shortcuts.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, revision, READ, NOTIFY = changed)]
        #[qproperty(bool, read_only, cxx_name = "readOnly", READ, NOTIFY = changed)]
        #[qproperty(QString, path, READ, NOTIFY = changed)]
        type Keybindings = super::KeybindingsRust;

        /// The shortcuts changed (here or on disk).
        #[qsignal]
        fn changed(self: Pin<&mut Self>);

        /// `save-failed` or `read-only`, with technical detail.
        #[qsignal]
        fn problem(self: Pin<&mut Self>, kind: QString, detail: QString);

        /// The shortcut of `actionId`: the user's, or `defaultSequence`.
        #[qinvokable]
        fn shortcut(self: &Self, action_id: &QString, default_sequence: &QString) -> QString;

        /// Whether the user changed the shortcut of `actionId`.
        #[qinvokable]
        #[cxx_name = "isCustom"]
        fn is_custom(self: &Self, action_id: &QString) -> bool;

        /// Sets the shortcut of `actionId` (`""` for none); equal to `defaultSequence` means back
        /// to the default.
        #[qinvokable]
        #[cxx_name = "setShortcut"]
        fn set_shortcut(
            self: Pin<&mut Self>,
            action_id: &QString,
            sequence: &QString,
            default_sequence: &QString,
        );

        /// Back to the default shortcut of `actionId`.
        #[qinvokable]
        fn reset(self: Pin<&mut Self>, action_id: &QString);

        /// Back to every default shortcut.
        #[qinvokable]
        #[cxx_name = "resetAll"]
        fn reset_all(self: Pin<&mut Self>);

        /// Whether `sequence` takes a key terminal programs need (Ctrl+letter, a bare key).
        #[qinvokable]
        #[cxx_name = "takesTerminalKey"]
        fn takes_terminal_key(self: &Self, sequence: &QString) -> bool;
    }

    impl cxx_qt::Initialize for Keybindings {}
    impl cxx_qt::Threading for Keybindings {}
}

use core::pin::Pin;
use std::path::PathBuf;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use opensesh_core::fsutil;
use opensesh_core::keybindings::{self, KEYBINDINGS_FILE, Keybindings};
use opensesh_core::watch::FileWatcher;

use crate::bridge::app_info::is_test_run;
use crate::saves::SaveTracker;
use crate::services;

const RELOAD_DEBOUNCE: Duration = Duration::from_millis(250);

/// Rust state behind `Keybindings`.
#[derive(Default)]
pub struct KeybindingsRust {
    revision: i32,
    read_only: bool,
    path: QString,
    bindings: Keybindings,
    file: Option<PathBuf>,
    saves: SaveTracker,
    watcher: Option<FileWatcher>,
}

impl std::fmt::Debug for KeybindingsRust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeybindingsRust")
            .field("bindings", &self.bindings)
            .field("file", &self.file)
            .finish_non_exhaustive()
    }
}

impl qobject::Keybindings {
    fn bump(mut self: Pin<&mut Self>) {
        {
            let mut state = self.as_mut().rust_mut();
            state.revision = state.revision.wrapping_add(1);
            state.read_only = state.bindings.read_only;
        }
        self.as_mut().changed();
    }

    /// Queues a save (not in test runs: they keep changes in memory).
    fn save(mut self: Pin<&mut Self>) {
        let (Some(path), Some(services)) = (
            self.file.clone(),
            services::get().filter(|_| !is_test_run()),
        ) else {
            return;
        };
        if self.bindings.read_only {
            self.as_mut().problem(
                QString::from("read-only"),
                QString::from(&path.display().to_string()),
            );
            return;
        }
        let text = self.bindings.to_toml_string();
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
            tracing::warn!("could not save {KEYBINDINGS_FILE}: {detail}");
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
            .name("opensesh-keybindings".to_owned())
            .spawn(move || {
                let (bindings, warnings) = Keybindings::load_file(&path);
                for warning in &warnings {
                    tracing::warn!("{KEYBINDINGS_FILE}: {warning}");
                }
                let _ = qt_thread.queue(move |object| object.apply_disk(bindings));
            });
        if let Err(error) = spawned {
            tracing::warn!("could not reload {KEYBINDINGS_FILE}: {error}");
        }
    }

    fn apply_disk(mut self: Pin<&mut Self>, bindings: Keybindings) {
        if self.saves.pending() {
            self.as_mut().rust_mut().saves.reload_wanted = true;
            return;
        }
        if self.bindings != bindings {
            tracing::info!("{KEYBINDINGS_FILE} changed on disk; shortcuts reloaded");
            self.as_mut().rust_mut().bindings = bindings;
            self.bump();
        }
    }

    /// See the bridge declaration.
    pub fn shortcut(&self, action_id: &QString, default_sequence: &QString) -> QString {
        match self.bindings.get(&action_id.to_string()) {
            Some(sequence) => QString::from(sequence),
            None => default_sequence.clone(),
        }
    }

    /// See the bridge declaration.
    pub fn is_custom(&self, action_id: &QString) -> bool {
        self.bindings.get(&action_id.to_string()).is_some()
    }

    /// See the bridge declaration.
    pub fn set_shortcut(
        mut self: Pin<&mut Self>,
        action_id: &QString,
        sequence: &QString,
        default_sequence: &QString,
    ) {
        let changed = self.as_mut().rust_mut().bindings.set(
            &action_id.to_string(),
            &sequence.to_string(),
            &default_sequence.to_string(),
        );
        if changed {
            self.as_mut().bump();
            self.save();
        }
    }

    /// See the bridge declaration.
    pub fn reset(mut self: Pin<&mut Self>, action_id: &QString) {
        if self
            .as_mut()
            .rust_mut()
            .bindings
            .reset(&action_id.to_string())
        {
            self.as_mut().bump();
            self.save();
        }
    }

    /// See the bridge declaration.
    pub fn reset_all(mut self: Pin<&mut Self>) {
        if self.bindings.bindings.is_empty() {
            return;
        }
        self.as_mut().rust_mut().bindings.bindings.clear();
        self.as_mut().bump();
        self.save();
    }

    /// See the bridge declaration.
    pub fn takes_terminal_key(&self, sequence: &QString) -> bool {
        keybindings::takes_terminal_key(&sequence.to_string())
    }
}

impl cxx_qt::Initialize for qobject::Keybindings {
    fn initialize(mut self: Pin<&mut Self>) {
        let Some(services) = services::get() else {
            tracing::warn!("Keybindings created before services; shortcuts won't persist");
            return;
        };
        let path = services.paths.config_dir().join(KEYBINDINGS_FILE);
        // One small file read at startup, before the first frame.
        let (bindings, warnings) = Keybindings::load_file(&path);
        for warning in &warnings {
            tracing::warn!("{KEYBINDINGS_FILE}: {warning}");
        }
        {
            let mut state = self.as_mut().rust_mut();
            state.bindings = bindings;
            state.read_only = state.bindings.read_only;
            state.path = QString::from(&path.display().to_string());
            state.file = Some(path.clone());
        }
        if is_test_run() {
            self.bump();
            return;
        }
        let qt_thread = self.qt_thread();
        match FileWatcher::spawn(&path, RELOAD_DEBOUNCE, move || {
            let _ = qt_thread.queue(|object| object.reload_in_background());
        }) {
            Ok(watcher) => self.as_mut().rust_mut().watcher = Some(watcher),
            Err(error) => tracing::warn!("{KEYBINDINGS_FILE} won't hot-reload: {error}"),
        }
        self.bump();
    }
}
