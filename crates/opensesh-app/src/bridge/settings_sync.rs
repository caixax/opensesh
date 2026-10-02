//! `SettingsSync` QML singleton (Sprint 16): where the settings live, the conflicts that
//! Syncthing or Git left in them, and the Git helper ([`crate::git`]).
//!
//! Moving the settings writes a pointer in the default folder ([`opensesh_core::sync`]), used
//! from the next start; copying them there first never overwrites a file. Conflicts are found
//! at start and whenever a settings file changes. Every file and Git operation runs on a worker
//! thread and ends with `finished`. Test runs never move the settings or run Git, and can show
//! sample conflicts.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// The settings folder, its conflicts and its Git repository.
        ///
        /// `conflicts`: JSON `[{path, name, source}]` (`source`: `syncthing` or `git`). `git`:
        /// JSON `{repo, branch, upstream, ahead, behind, changes}`.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, folder, READ, NOTIFY = changed)]
        #[qproperty(QString, default_folder, cxx_name = "defaultFolder", READ, NOTIFY = changed)]
        #[qproperty(QString, next_folder, cxx_name = "nextFolder", READ, NOTIFY = changed)]
        #[qproperty(bool, moved, READ, NOTIFY = changed)]
        #[qproperty(bool, restart_needed, cxx_name = "restartNeeded", READ, NOTIFY = changed)]
        #[qproperty(QString, conflicts, READ, NOTIFY = changed)]
        #[qproperty(i32, conflict_count, cxx_name = "conflictCount", READ, NOTIFY = changed)]
        #[qproperty(bool, git_available, cxx_name = "gitAvailable", READ, NOTIFY = changed)]
        #[qproperty(QString, git, READ, NOTIFY = changed)]
        #[qproperty(bool, busy, READ, NOTIFY = changed)]
        type SettingsSync = super::SettingsSyncRust;

        /// A property changed.
        #[qsignal]
        fn changed(self: Pin<&mut Self>);

        /// `action` (`folder`, `resolve`, `git-commit`, `git-pull`, `git-push`, `git-init`)
        /// ended: `code` is empty on success, else `test-run`, `busy`, `failed` or `nothing`;
        /// `detail` is Git's output or the error.
        #[qsignal]
        fn finished(self: Pin<&mut Self>, action: QString, code: QString, detail: QString);

        /// Looks for conflicts and reads the Git status again.
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);

        /// What is at `path`: `{exists, empty, settings, git, current}`.
        #[qinvokable]
        #[cxx_name = "folderInfo"]
        fn folder_info(self: &Self, path: &QString) -> QString;

        /// Uses `path` as the settings folder from the next start: `mode` `use` (its files as
        /// they are), `copy` (this computer's settings copied there first, nothing
        /// overwritten) or `default` (back to the default folder; `path` is ignored).
        #[qinvokable]
        #[cxx_name = "setFolder"]
        fn set_folder(self: Pin<&mut Self>, path: &QString, mode: &QString);

        /// The differences of conflict `index`: `{path, name, source, whole, differences:
        /// [{section, id, name, change}], error}`; `whole` means the file can't be compared by
        /// record (choose a whole side, key `*`).
        #[qinvokable]
        #[cxx_name = "conflictDetails"]
        fn conflict_details(self: &Self, index: i32) -> QString;

        /// Resolves conflict `index` with `choices`, JSON `{"<section>\t<id>": "here" | "there"}`
        /// (`*` for the whole file); differences without a choice keep what nothing loses.
        #[qinvokable]
        #[cxx_name = "resolveConflict"]
        fn resolve_conflict(self: Pin<&mut Self>, index: i32, choices: &QString);

        /// Commits every change of the folder with `message`.
        #[qinvokable]
        #[cxx_name = "gitCommit"]
        fn git_commit(self: Pin<&mut Self>, message: &QString);

        /// `git pull` (a merge; conflicts show up as conflicts).
        #[qinvokable]
        #[cxx_name = "gitPull"]
        fn git_pull(self: Pin<&mut Self>);

        /// `git push`.
        #[qinvokable]
        #[cxx_name = "gitPush"]
        fn git_push(self: Pin<&mut Self>);

        /// Makes the folder a Git repository (with a `.gitignore` for OpenSesh's own files).
        #[qinvokable]
        #[cxx_name = "gitInit"]
        fn git_init(self: Pin<&mut Self>);

        /// Test runs only: a sample conflict and repository state.
        #[qinvokable]
        #[cxx_name = "loadSample"]
        fn load_sample(self: Pin<&mut Self>);
    }

    impl cxx_qt::Initialize for SettingsSync {}
    impl cxx_qt::Threading for SettingsSync {}
}

use core::pin::Pin;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use opensesh_core::sync::{self, Change, ConflictFile, ConflictSource, Side};
use opensesh_core::watch::FileWatcher;
use serde_json::{Value as Json, json};

use crate::bridge::app_info::is_test_run;
use crate::git;
use crate::services;

/// The settings files and folders copied to a new settings folder.
const SETTINGS: [&str; 12] = [
    "config.toml",
    "hosts.toml",
    "snippets.toml",
    "tunnels.toml",
    "keybindings.toml",
    "highlights.toml",
    "known_hosts",
    "trusted_certificates.toml",
    "profiles",
    "themes",
    "workspaces",
    "location.toml",
];

/// Rust state behind `SettingsSync`.
#[derive(Default)]
pub struct SettingsSyncRust {
    folder: QString,
    default_folder: QString,
    next_folder: QString,
    moved: bool,
    restart_needed: bool,
    conflicts: QString,
    conflict_count: i32,
    git_available: bool,
    git: QString,
    busy: bool,
    dir: Option<PathBuf>,
    local: Option<PathBuf>,
    found: Vec<ConflictFile>,
    sample: bool,
    watcher: Option<FileWatcher>,
}

fn git_json(status: &git::Status) -> String {
    json!({
        "repo": status.repo,
        "branch": status.branch,
        "upstream": status.upstream,
        "ahead": status.ahead,
        "behind": status.behind,
        "changes": status.changes,
    })
    .to_string()
}

fn conflicts_json(dir: &Path, found: &[ConflictFile]) -> String {
    let list: Vec<Json> = found
        .iter()
        .map(|conflict| {
            json!({
                "path": conflict.path.display().to_string(),
                "name": conflict
                    .path
                    .strip_prefix(dir)
                    .unwrap_or(&conflict.path)
                    .display()
                    .to_string()
                    .replace('\\', "/"),
                "source": match conflict.source {
                    ConflictSource::Syncthing(_) => "syncthing",
                    ConflictSource::Git => "git",
                },
            })
        })
        .collect();
    Json::Array(list).to_string()
}

/// What a scan found.
struct Scan {
    found: Vec<ConflictFile>,
    git_available: bool,
    git: git::Status,
}

fn scan(dir: &Path) -> Scan {
    Scan {
        found: sync::find_conflicts(dir),
        git_available: git::available(),
        git: git::status(dir),
    }
}

/// Copies the settings of `from` into `to`, never over a file that is there.
fn copy_settings(from: &Path, to: &Path) -> std::io::Result<usize> {
    fn copy(from: &Path, to: &Path, copied: &mut usize) -> std::io::Result<()> {
        if from.is_dir() {
            std::fs::create_dir_all(to)?;
            for entry in std::fs::read_dir(from)? {
                let entry = entry?;
                copy(&entry.path(), &to.join(entry.file_name()), copied)?;
            }
        } else if from.is_file() && !to.exists() {
            std::fs::copy(from, to)?;
            *copied += 1;
        }
        Ok(())
    }
    std::fs::create_dir_all(to)?;
    let mut copied = 0;
    // The pointer itself stays behind.
    for name in SETTINGS.iter().filter(|name| **name != sync::LOCATION_FILE) {
        copy(&from.join(name), &to.join(name), &mut copied)?;
    }
    Ok(copied)
}

/// The two sides of a conflict as tables, or `None` when one isn't TOML.
fn tables(sides: &sync::Sides) -> Option<(toml::Table, toml::Table)> {
    Some((sides.here.parse().ok()?, sides.there.parse().ok()?))
}

fn change_code(change: Change) -> &'static str {
    match change {
        Change::OnlyHere => "only-here",
        Change::OnlyThere => "only-there",
        Change::Different => "different",
    }
}

/// The text a conflict resolves to.
fn resolved_text(
    conflict: &ConflictFile,
    choices: &HashMap<String, String>,
) -> Result<String, String> {
    let sides = sync::sides(conflict).map_err(|error| error.to_string())?;
    let side = |text: Option<&String>| match text.map(String::as_str) {
        Some("there") => Some(Side::There),
        Some("here") => Some(Side::Here),
        _ => None,
    };
    match tables(&sides) {
        Some((here, there)) => {
            let choices: HashMap<(String, String), Side> = choices
                .iter()
                .filter_map(|(key, value)| {
                    let (section, id) = key.split_once('\t').unwrap_or((key.as_str(), ""));
                    Some(((section.to_owned(), id.to_owned()), side(Some(value))?))
                })
                .collect();
            let table = sync::resolve(&here, &there, &choices);
            toml::to_string(&table).map_err(|error| error.to_string())
        }
        None => Ok(match side(choices.get("*")) {
            Some(Side::There) => sides.there,
            _ => sides.here,
        }),
    }
}

impl qobject::SettingsSync {
    fn apply_scan(mut self: Pin<&mut Self>, scan: Scan) {
        {
            let mut state = self.as_mut().rust_mut();
            if state.sample {
                return;
            }
            let dir = state.dir.clone().unwrap_or_default();
            state.conflicts = QString::from(&conflicts_json(&dir, &scan.found));
            state.conflict_count = i32::try_from(scan.found.len()).unwrap_or(i32::MAX);
            state.found = scan.found;
            state.git_available = scan.git_available;
            state.git = QString::from(&git_json(&scan.git));
        }
        self.changed();
    }

    /// See the bridge declaration.
    pub fn refresh(self: Pin<&mut Self>) {
        let Some(dir) = self.dir.clone() else {
            return;
        };
        if self.sample {
            return;
        }
        let qt_thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("opensesh-sync".to_owned())
            .spawn(move || {
                let found = scan(&dir);
                let _ = qt_thread.queue(move |object| object.apply_scan(found));
            });
        if let Err(error) = spawned {
            tracing::warn!("could not look for sync conflicts: {error}");
        }
    }

    /// See the bridge declaration.
    pub fn folder_info(&self, path: &QString) -> QString {
        let path = PathBuf::from(path.to_string().trim());
        let exists = path.is_dir();
        let empty = std::fs::read_dir(&path).map_or(true, |mut entries| entries.next().is_none());
        let settings = ["hosts.toml", "config.toml", "snippets.toml"]
            .iter()
            .any(|name| path.join(name).is_file());
        let current = self.dir.as_ref().is_some_and(|dir| *dir == path);
        QString::from(
            &json!({
                "exists": exists,
                "empty": empty,
                "settings": settings,
                "git": path.join(".git").exists(),
                "current": current,
            })
            .to_string(),
        )
    }

    /// Runs `work` on a worker thread, then reports `action` and refreshes.
    fn run(
        mut self: Pin<&mut Self>,
        action: &'static str,
        work: impl FnOnce() -> Result<String, (String, String)> + Send + 'static,
    ) {
        if is_test_run() {
            self.finished(
                QString::from(action),
                QString::from("test-run"),
                QString::default(),
            );
            return;
        }
        if self.busy {
            self.finished(
                QString::from(action),
                QString::from("busy"),
                QString::default(),
            );
            return;
        }
        self.as_mut().rust_mut().busy = true;
        self.as_mut().changed();
        let qt_thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("opensesh-sync".to_owned())
            .spawn(move || {
                let result = work();
                let _ = qt_thread.queue(move |mut object| {
                    object.as_mut().rust_mut().busy = false;
                    let (code, detail) = match result {
                        Ok(detail) => (String::new(), detail),
                        Err((code, detail)) => (code, detail),
                    };
                    object.as_mut().update_folders();
                    object.as_mut().changed();
                    object.as_mut().finished(
                        QString::from(action),
                        QString::from(&code),
                        QString::from(&detail),
                    );
                    object.refresh();
                });
            });
        if let Err(error) = spawned {
            tracing::warn!("could not start a sync job: {error}");
            self.as_mut().rust_mut().busy = false;
            self.changed();
        }
    }

    /// Reads the pointer again (`nextFolder`, `restartNeeded`).
    fn update_folders(mut self: Pin<&mut Self>) {
        let (Some(dir), Some(local)) = (self.dir.clone(), self.local.clone()) else {
            return;
        };
        let next = sync::location(&local).unwrap_or_else(|| local.clone());
        let mut state = self.as_mut().rust_mut();
        state.next_folder = QString::from(&next.display().to_string());
        state.restart_needed = next != dir;
    }

    /// See the bridge declaration.
    pub fn set_folder(self: Pin<&mut Self>, path: &QString, mode: &QString) {
        let (Some(dir), Some(local)) = (self.dir.clone(), self.local.clone()) else {
            return;
        };
        let target = PathBuf::from(path.to_string().trim());
        let mode = mode.to_string();
        self.run("folder", move || {
            let failed = |error: std::io::Error| ("failed".to_owned(), error.to_string());
            if mode == "default" {
                sync::set_location(&local, None).map_err(failed)?;
                return Ok(String::new());
            }
            if !target.is_absolute() {
                return Err(("failed".to_owned(), "not an absolute path".to_owned()));
            }
            let copied = if mode == "copy" {
                copy_settings(&dir, &target).map_err(failed)?
            } else {
                std::fs::create_dir_all(&target).map_err(failed)?;
                0
            };
            let pointer = (target != local).then_some(target.as_path());
            sync::set_location(&local, pointer).map_err(failed)?;
            Ok(copied.to_string())
        });
    }

    /// See the bridge declaration.
    pub fn conflict_details(&self, index: i32) -> QString {
        if self.sample {
            return QString::from(&sample_details().to_string());
        }
        let Some(conflict) = usize::try_from(index)
            .ok()
            .and_then(|at| self.found.get(at))
        else {
            return QString::from(&json!({ "error": "gone" }).to_string());
        };
        let base = json!({
            "path": conflict.path.display().to_string(),
            "name": conflict.path.file_name().map(|name| name.to_string_lossy().into_owned()),
            "source": match conflict.source {
                ConflictSource::Syncthing(_) => "syncthing",
                ConflictSource::Git => "git",
            },
        });
        // Two small files, read when the user opens the dialog.
        let details = match sync::sides(conflict) {
            Err(error) => json!({ "whole": true, "differences": [], "error": error.to_string() }),
            Ok(sides) => match tables(&sides) {
                None => json!({ "whole": true, "differences": [], "error": "" }),
                Some((here, there)) => {
                    let differences: Vec<Json> = sync::differences(&here, &there)
                        .iter()
                        .map(|difference| {
                            json!({
                                "section": difference.section,
                                "id": difference.id,
                                "name": difference.name,
                                "change": change_code(difference.change),
                            })
                        })
                        .collect();
                    json!({ "whole": false, "differences": differences, "error": "" })
                }
            },
        };
        let mut out = base;
        if let (Some(out), Some(details)) = (out.as_object_mut(), details.as_object()) {
            out.extend(details.clone());
        }
        QString::from(&out.to_string())
    }

    /// See the bridge declaration.
    pub fn resolve_conflict(self: Pin<&mut Self>, index: i32, choices: &QString) {
        let Some(conflict) = usize::try_from(index)
            .ok()
            .and_then(|at| self.found.get(at))
            .cloned()
        else {
            return;
        };
        let choices: HashMap<String, String> =
            serde_json::from_str(&choices.to_string()).unwrap_or_default();
        self.run("resolve", move || {
            let text =
                resolved_text(&conflict, &choices).map_err(|error| ("failed".to_owned(), error))?;
            sync::finish(&conflict, &text)
                .map_err(|error| ("failed".to_owned(), error.to_string()))?;
            Ok(String::new())
        });
    }

    fn git_job(
        self: Pin<&mut Self>,
        action: &'static str,
        job: impl FnOnce(&Path) -> Result<String, String> + Send + 'static,
    ) {
        let Some(dir) = self.dir.clone() else {
            return;
        };
        self.run(action, move || {
            job(&dir).map_err(|error| {
                let code = if error == "nothing to commit" {
                    "nothing"
                } else {
                    "failed"
                };
                (code.to_owned(), error)
            })
        });
    }

    /// See the bridge declaration.
    pub fn git_commit(self: Pin<&mut Self>, message: &QString) {
        let message = message.to_string();
        let message = if message.trim().is_empty() {
            "OpenSesh settings".to_owned()
        } else {
            message
        };
        self.git_job("git-commit", move |dir| git::commit(dir, &message));
    }

    /// See the bridge declaration.
    pub fn git_pull(self: Pin<&mut Self>) {
        self.git_job("git-pull", git::pull);
    }

    /// See the bridge declaration.
    pub fn git_push(self: Pin<&mut Self>) {
        self.git_job("git-push", git::push);
    }

    /// See the bridge declaration.
    pub fn git_init(self: Pin<&mut Self>) {
        self.git_job("git-init", git::init);
    }

    /// See the bridge declaration.
    pub fn load_sample(mut self: Pin<&mut Self>) {
        if !is_test_run() {
            return;
        }
        {
            let mut state = self.as_mut().rust_mut();
            state.sample = true;
            state.conflicts = QString::from(
                &json!([
                    { "path": "hosts.toml", "name": "hosts.toml", "source": "syncthing" },
                    { "path": "snippets.toml", "name": "snippets.toml", "source": "git" },
                ])
                .to_string(),
            );
            state.conflict_count = 2;
            state.git_available = true;
            state.git = QString::from(&git_json(&git::Status {
                repo: true,
                branch: "main".to_owned(),
                upstream: "origin/main".to_owned(),
                ahead: 1,
                behind: 2,
                changes: 3,
            }));
        }
        self.changed();
    }
}

/// The sample conflict's differences (test runs).
fn sample_details() -> Json {
    json!({
        "path": "hosts.toml",
        "name": "hosts.toml",
        "source": "syncthing",
        "whole": false,
        "error": "",
        "differences": [
            { "section": "host", "id": "01J0SAMPLE0000000000000001", "name": "web-01", "change": "different" },
            { "section": "host", "id": "01J0SAMPLE0000000000000002", "name": "db-02", "change": "only-there" },
            { "section": "host", "id": "01J0SAMPLE0000000000000003", "name": "old-box", "change": "only-here" },
            { "section": "group", "id": "01J0SAMPLE0000000000000004", "name": "Production", "change": "different" },
        ],
    })
}

impl cxx_qt::Initialize for qobject::SettingsSync {
    fn initialize(mut self: Pin<&mut Self>) {
        let Some(services) = services::get() else {
            return;
        };
        let dir = services.paths.config_dir().to_path_buf();
        let local = services.paths.local_config_dir().to_path_buf();
        {
            let mut state = self.as_mut().rust_mut();
            state.folder = QString::from(&dir.display().to_string());
            state.default_folder = QString::from(&local.display().to_string());
            state.moved = services.paths.settings_moved();
            state.conflicts = QString::from("[]");
            state.git = QString::from(&git_json(&git::Status::default()));
            state.dir = Some(dir.clone());
            state.local = Some(local);
        }
        self.as_mut().update_folders();
        // Settings files that change (Syncthing, a pull) may bring conflicts.
        let qt_thread = self.qt_thread();
        match FileWatcher::spawn_dir(&dir, ".toml", Duration::from_secs(1), move || {
            let _ = qt_thread.queue(|object| object.refresh());
        }) {
            Ok(watcher) => self.as_mut().rust_mut().watcher = Some(watcher),
            Err(error) => tracing::warn!("could not watch the settings folder: {error}"),
        }
        self.refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copying_never_overwrites() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        std::fs::write(from.path().join("hosts.toml"), "a").unwrap();
        std::fs::write(from.path().join("config.toml"), "b").unwrap();
        std::fs::write(from.path().join("location.toml"), "c").unwrap();
        std::fs::write(from.path().join("vault.bin"), "secret").unwrap();
        std::fs::create_dir(from.path().join("themes")).unwrap();
        std::fs::write(from.path().join("themes").join("x.toml"), "d").unwrap();
        std::fs::write(to.path().join("config.toml"), "theirs").unwrap();
        let copied = copy_settings(from.path(), to.path()).unwrap();
        assert_eq!(copied, 2);
        assert_eq!(
            std::fs::read_to_string(to.path().join("config.toml")).unwrap(),
            "theirs"
        );
        assert_eq!(
            std::fs::read_to_string(to.path().join("themes").join("x.toml")).unwrap(),
            "d"
        );
        assert!(!to.path().join("location.toml").exists());
        assert!(!to.path().join("vault.bin").exists());
    }

    #[test]
    fn conflicts_resolve_by_choice() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosts.toml");
        let copy = dir
            .path()
            .join("hosts.sync-conflict-20261002-101010-AAAAAAA.toml");
        std::fs::write(
            &path,
            "[[host]]\nid = \"A\"\nname = \"a\"\naddress = \"here\"\n",
        )
        .unwrap();
        std::fs::write(
            &copy,
            "[[host]]\nid = \"A\"\nname = \"a\"\naddress = \"there\"\n",
        )
        .unwrap();
        let conflict = ConflictFile {
            path: path.clone(),
            source: ConflictSource::Syncthing(copy),
        };
        let choices = HashMap::from([("host\tA".to_owned(), "there".to_owned())]);
        let text = resolved_text(&conflict, &choices).unwrap();
        assert!(text.contains("there"), "{text}");
        // known_hosts isn't TOML: a whole side.
        let hosts = dir.path().join("known_hosts");
        let hosts_copy = dir
            .path()
            .join("known_hosts.sync-conflict-20261002-101010-AAAAAAA");
        std::fs::write(&hosts, "here key\n").unwrap();
        std::fs::write(&hosts_copy, "there key\n").unwrap();
        let conflict = ConflictFile {
            path: hosts,
            source: ConflictSource::Syncthing(hosts_copy),
        };
        let whole = HashMap::from([("*".to_owned(), "there".to_owned())]);
        assert_eq!(resolved_text(&conflict, &whole).unwrap(), "there key\n");
        assert_eq!(
            resolved_text(&conflict, &HashMap::new()).unwrap(),
            "here key\n"
        );
    }
}
