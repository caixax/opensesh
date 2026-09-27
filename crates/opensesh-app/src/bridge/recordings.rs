//! `Recordings` QML singleton (Sprint 10): session recordings (`crate::recordings`). Starts and
//! stops recording a pane, says which panes record, and lists the recordings for the History
//! view (read off the GUI thread), which plays, shows and deletes them.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// The session recordings.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, list, READ, NOTIFY = changed)]
        #[qproperty(i32, count, READ, NOTIFY = changed)]
        #[qproperty(QString, folder, READ, NOTIFY = changed)]
        #[qproperty(QString, panes, READ, NOTIFY = panes_changed)]
        type Recordings = super::RecordingsRust;

        /// The list changed.
        #[qsignal]
        fn changed(self: Pin<&mut Self>);

        /// A pane started or stopped recording.
        #[qsignal]
        #[cxx_name = "panesChanged"]
        fn panes_changed(self: Pin<&mut Self>);

        /// A recording didn't start or a file wasn't deleted: technical detail (paths, never
        /// secrets).
        #[qsignal]
        fn problem(self: Pin<&mut Self>, detail: QString);

        /// Reads the folder again (in the background; `changed` follows).
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);

        /// Starts recording pane `pane`, titled `title`. The file's path, empty when it didn't
        /// start (`problem` says why).
        #[qinvokable]
        fn start(self: Pin<&mut Self>, pane: i32, title: &QString) -> QString;

        /// Stops recording pane `pane`: the file's path, empty when it wasn't recording.
        #[qinvokable]
        fn stop(self: Pin<&mut Self>, pane: i32) -> QString;

        /// Deletes recording `path` (a file of the list; not while it records), in the
        /// background.
        #[qinvokable]
        fn remove(self: Pin<&mut Self>, path: &QString);

        /// Test runs only: a few sample recordings in the list (the History view's screenshot).
        #[qinvokable]
        #[cxx_name = "loadFixture"]
        fn load_fixture(self: Pin<&mut Self>);
    }

    impl cxx_qt::Initialize for Recordings {}
    impl cxx_qt::Threading for Recordings {}
}

use core::pin::Pin;
use std::path::PathBuf;
use std::sync::Arc;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use serde_json::{Value as Json, json};

use crate::bridge::app_info::is_test_run;
use crate::recordings::{self, Entry};

/// Rust state behind `Recordings`.
#[derive(Debug, Default)]
pub struct RecordingsRust {
    list: QString,
    count: i32,
    folder: QString,
    panes: QString,
    /// A fixture is shown: the folder isn't read.
    fixture: bool,
}

fn entry_json(entry: &Entry, recording: &[PathBuf]) -> Json {
    json!({
        "path": entry.path.display().to_string(),
        "name": entry.name,
        "title": entry.title,
        "size": entry.size,
        "modified": entry.modified,
        "recording": recording.contains(&entry.path),
    })
}

impl qobject::Recordings {
    fn publish(mut self: Pin<&mut Self>, entries: &[Entry]) {
        let recording = recordings::files();
        let list = Json::Array(
            entries
                .iter()
                .map(|entry| entry_json(entry, &recording))
                .collect(),
        )
        .to_string();
        {
            let mut state = self.as_mut().rust_mut();
            state.list = QString::from(&list);
            state.count = i32::try_from(entries.len()).unwrap_or(i32::MAX);
        }
        self.changed();
    }

    fn publish_panes(mut self: Pin<&mut Self>) {
        let mut panes = recordings::panes();
        panes.sort_unstable();
        self.as_mut().rust_mut().panes = QString::from(&json!(panes).to_string());
        self.panes_changed();
    }

    /// See the bridge declaration.
    pub fn refresh(self: Pin<&mut Self>) {
        if self.fixture {
            return;
        }
        let Some(folder) = recordings::folder() else {
            return;
        };
        let qt_thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("opensesh-recordings".to_owned())
            .spawn(move || {
                let entries = recordings::list(&folder);
                let _ = qt_thread.queue(move |object| object.publish(&entries));
            });
        if let Err(error) = spawned {
            tracing::warn!("could not list the recordings: {error}");
        }
    }

    /// See the bridge declaration.
    pub fn start(mut self: Pin<&mut Self>, pane: i32, title: &QString) -> QString {
        let term = opensesh_core::terminal::settings::DEFAULT_TERM;
        match recordings::start(pane, &title.to_string(), term) {
            Ok(path) => {
                // Now, not only when the queued notice arrives: QML reads it right after.
                self.as_mut().publish_panes();
                self.as_mut().refresh();
                QString::from(&path.display().to_string())
            }
            Err(detail) => {
                tracing::warn!(pane, "a recording didn't start: {detail}");
                self.problem(QString::from(&detail));
                QString::default()
            }
        }
    }

    /// See the bridge declaration.
    pub fn stop(mut self: Pin<&mut Self>, pane: i32) -> QString {
        let path = recordings::stop(pane);
        self.as_mut().publish_panes();
        self.as_mut().refresh();
        path.map(|path| QString::from(&path.display().to_string()))
            .unwrap_or_default()
    }

    /// See the bridge declaration.
    pub fn remove(self: Pin<&mut Self>, path: &QString) {
        let Some(folder) = recordings::folder() else {
            return;
        };
        let path = PathBuf::from(path.to_string());
        let qt_thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("opensesh-recordings".to_owned())
            .spawn(move || {
                let result = recordings::remove_in(&folder, &path);
                let entries = recordings::list(&folder);
                let _ = qt_thread.queue(move |mut object| {
                    if let Err(detail) = result {
                        tracing::warn!("a recording wasn't deleted: {detail}");
                        object.as_mut().problem(QString::from(&detail));
                    }
                    object.publish(&entries);
                });
            });
        if let Err(error) = spawned {
            tracing::warn!("could not delete a recording: {error}");
        }
    }

    /// See the bridge declaration.
    pub fn load_fixture(mut self: Pin<&mut Self>) {
        if !is_test_run() {
            return;
        }
        let folder = recordings::folder().unwrap_or_default();
        let at = |name: &str| folder.join(name);
        // Named when they started, changed when they ended (UTC).
        let entries = [
            (
                "2026-09-27_09-12-40_web-01.cast",
                "web-01",
                48_213,
                1_790_501_465,
            ),
            (
                "2026-09-26_16-03-11_db-02.cast",
                "db-02",
                1_204_877,
                1_790_441_320,
            ),
            (
                "2026-09-24_11-45-02_Local_terminal.cast",
                "Local terminal",
                9_120,
                1_790_250_739,
            ),
        ]
        .map(|(name, title, size, modified)| Entry {
            path: at(name),
            name: name.to_owned(),
            title: title.to_owned(),
            size,
            modified,
        });
        self.as_mut().rust_mut().fixture = true;
        self.publish(&entries);
    }
}

impl cxx_qt::Initialize for qobject::Recordings {
    fn initialize(mut self: Pin<&mut Self>) {
        let folder = recordings::folder()
            .map(|folder| folder.display().to_string())
            .unwrap_or_default();
        self.as_mut().rust_mut().folder = QString::from(&folder);
        let qt_thread = self.qt_thread();
        recordings::set_sink(Arc::new(move || {
            let _ = qt_thread.queue(|mut object| {
                object.as_mut().publish_panes();
                object.refresh();
            });
        }));
        self.as_mut().publish_panes();
        self.refresh();
    }
}
