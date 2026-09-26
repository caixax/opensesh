//! Watches a single file for external changes, for hot reload (PLAN §4.2).
//!
//! The **parent directory** is watched, not the file: editors and our own [`crate::fsutil`]
//! replace files by renaming a temporary file over them, which would silently end a watch on
//! the old file. Events are debounced so a burst (write + rename + chmod) gives one callback.
//!
//! When the file is a symbolic link (a dotfiles setup), the directory of the file it points to
//! is watched as well, since edits there (a `git pull` in the dotfiles repository) don't touch
//! the link's directory. The link is resolved once, when the watch starts.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

use crate::fsutil;

/// Keeps the watch alive; dropping it stops watching (the callback thread exits too).
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

impl std::fmt::Debug for FileWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileWatcher").finish_non_exhaustive()
    }
}

impl FileWatcher {
    /// Calls `on_change` (on a background thread) after `path` was created, modified, renamed
    /// or removed, once no new event arrived for `debounce`. If `path` is a symbolic link,
    /// changes to the file it points to are reported too.
    ///
    /// # Errors
    ///
    /// Fails if the directory of `path` can't be watched or the callback thread can't start. A
    /// link target's directory that can't be watched is only logged.
    pub fn spawn(
        path: &Path,
        debounce: Duration,
        on_change: impl Fn() + Send + 'static,
    ) -> Result<Self, WatchError> {
        let (dir, file_name) = split(path).ok_or(WatchError::NoParent)?;
        let mut names = vec![file_name];
        let mut target_dir = None;
        if let Some((link_dir, link_name)) = split(&fsutil::resolve_links(path)) {
            if !names.contains(&link_name) {
                names.push(link_name);
            }
            if link_dir != dir {
                target_dir = Some(link_dir);
            }
        }

        let (sender, receiver) = mpsc::channel::<()>();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else { return };
                if matches!(event.kind, EventKind::Access(_)) {
                    return;
                }
                // File names only: event paths may be spelled differently from the watched ones
                // (e.g. canonical paths on macOS). A same-named file in the other directory only
                // costs a harmless extra reload.
                let concerns_file = event.paths.iter().any(|changed: &PathBuf| {
                    changed
                        .file_name()
                        .is_some_and(|name| names.iter().any(|watched| watched == name))
                });
                if concerns_file {
                    // The receiver only disappears when the watcher is being dropped.
                    let _ = sender.send(());
                }
            })?;
        watcher.watch(&dir, RecursiveMode::NonRecursive)?;
        if let Some(target_dir) = target_dir {
            if let Err(error) = watcher.watch(&target_dir, RecursiveMode::NonRecursive) {
                tracing::warn!(
                    dir = %target_dir.display(),
                    "edits to the link target of {} won't be noticed: {error}",
                    path.display()
                );
            }
        }

        std::thread::Builder::new()
            .name("opensesh-watch".to_owned())
            .spawn(move || {
                while receiver.recv().is_ok() {
                    // Wait until the burst of events is over.
                    loop {
                        match receiver.recv_timeout(debounce) {
                            Ok(()) => {}
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    on_change();
                }
            })
            .map_err(WatchError::Thread)?;

        Ok(Self { _watcher: watcher })
    }
}

impl FileWatcher {
    /// Calls `on_change` (on a background thread) after any file directly inside `dir` whose
    /// name ends with `suffix` was created, modified, renamed or removed, once no new event
    /// arrived for `debounce`. `dir` must exist.
    ///
    /// # Errors
    ///
    /// Fails if `dir` can't be watched or the callback thread can't start.
    pub fn spawn_dir(
        dir: &Path,
        suffix: &str,
        debounce: Duration,
        on_change: impl Fn() + Send + 'static,
    ) -> Result<Self, WatchError> {
        let (sender, receiver) = mpsc::channel::<()>();
        let suffix = suffix.to_owned();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else { return };
                if matches!(event.kind, EventKind::Access(_)) {
                    return;
                }
                let concerns = event.paths.iter().any(|changed: &PathBuf| {
                    changed
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.ends_with(&suffix))
                });
                if concerns {
                    let _ = sender.send(());
                }
            })?;
        watcher.watch(dir, RecursiveMode::NonRecursive)?;
        std::thread::Builder::new()
            .name("opensesh-watch".to_owned())
            .spawn(move || {
                while receiver.recv().is_ok() {
                    loop {
                        match receiver.recv_timeout(debounce) {
                            Ok(()) => {}
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    on_change();
                }
            })
            .map_err(WatchError::Thread)?;
        Ok(Self { _watcher: watcher })
    }
}

/// The directory to watch and the file name to look for.
fn split(path: &Path) -> Option<(PathBuf, OsString)> {
    let dir = path.parent().filter(|dir| !dir.as_os_str().is_empty())?;
    Some((dir.to_path_buf(), path.file_name()?.to_os_string()))
}

/// Errors from [`FileWatcher::spawn`].
#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    /// The path has no parent directory to watch.
    #[error("the watched path has no parent directory")]
    NoParent,
    /// The OS watcher failed.
    #[error("could not watch the directory")]
    Notify(#[from] notify::Error),
    /// The callback thread could not start.
    #[error("could not start the watcher thread")]
    Thread(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Receiver;

    use super::*;
    use crate::fsutil;

    fn watch(path: &Path) -> (FileWatcher, Receiver<()>) {
        let (tx, rx) = mpsc::channel();
        let watcher = FileWatcher::spawn(path, Duration::from_millis(50), move || {
            let _ = tx.send(());
        })
        .unwrap();
        (watcher, rx)
    }

    #[test]
    fn atomic_replacements_are_reported_once_per_burst() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "a = 1").unwrap();
        let (_watcher, changes) = watch(&path);

        fsutil::atomic_write(&path, b"a = 2", 5).unwrap();
        changes
            .recv_timeout(Duration::from_secs(10))
            .expect("a change notification");
        // The burst (temp write, backup copy, rename) produced a single callback.
        assert!(changes.recv_timeout(Duration::from_millis(300)).is_err());
    }

    #[test]
    fn other_files_in_the_directory_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let (_watcher, changes) = watch(&path);
        std::fs::write(dir.path().join("hosts.toml"), "x").unwrap();
        assert!(changes.recv_timeout(Duration::from_millis(400)).is_err());
    }

    #[test]
    fn a_directory_watch_reports_matching_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let _watcher =
            FileWatcher::spawn_dir(dir.path(), ".toml", Duration::from_millis(50), move || {
                let _ = tx.send(());
            })
            .unwrap();
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
        std::fs::write(dir.path().join("work.toml"), "x").unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(5)).is_ok());
        std::fs::remove_file(dir.path().join("work.toml")).unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(5)).is_ok());
    }

    #[test]
    fn a_file_created_later_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let (_watcher, changes) = watch(&path);
        std::fs::write(&path, "created").unwrap();
        changes
            .recv_timeout(Duration::from_secs(10))
            .expect("a change notification");
    }

    #[test]
    fn edits_to_a_link_target_in_another_directory_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let dotfiles = dir.path().join("dotfiles");
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&dotfiles).unwrap();
        std::fs::create_dir_all(&config_dir).unwrap();
        let target = dotfiles.join("opensesh.toml");
        std::fs::write(&target, "a = 1").unwrap();
        let link = config_dir.join("config.toml");
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&target, &link);
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&target, &link);
        if let Err(error) = linked {
            eprintln!("skipped: can't create a symbolic link here: {error}");
            return;
        }
        let (_watcher, changes) = watch(&link);

        // E.g. a `git pull` in the dotfiles repository.
        std::fs::write(&target, "a = 2").unwrap();
        changes
            .recv_timeout(Duration::from_secs(10))
            .expect("a change notification");
        // Other files next to the target are still ignored.
        std::fs::write(dotfiles.join("README.md"), "x").unwrap();
        assert!(changes.recv_timeout(Duration::from_millis(400)).is_err());
    }

    #[test]
    fn paths_without_a_parent_are_rejected() {
        assert!(matches!(
            FileWatcher::spawn(Path::new("config.toml"), Duration::from_millis(1), || {}),
            Err(WatchError::NoParent)
        ));
    }
}
