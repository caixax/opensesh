//! Watches a single file for external changes, for hot reload (PLAN §4.2).
//!
//! The **parent directory** is watched, not the file: editors and our own [`crate::fsutil`]
//! replace files by renaming a temporary file over them, which would silently end a watch on
//! the old file. Events are debounced so a burst (write + rename + chmod) gives one callback.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

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
    /// or removed, once no new event arrived for `debounce`.
    ///
    /// # Errors
    ///
    /// Fails if the directory can't be watched or the callback thread can't start.
    pub fn spawn(
        path: &Path,
        debounce: Duration,
        on_change: impl Fn() + Send + 'static,
    ) -> Result<Self, WatchError> {
        let dir = path
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .ok_or(WatchError::NoParent)?
            .to_path_buf();
        let file_name = path.file_name().ok_or(WatchError::NoParent)?.to_os_string();

        let (sender, receiver) = mpsc::channel::<()>();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else { return };
                if matches!(event.kind, EventKind::Access(_)) {
                    return;
                }
                let concerns_file = event
                    .paths
                    .iter()
                    .any(|changed: &PathBuf| changed.file_name() == Some(file_name.as_os_str()));
                if concerns_file {
                    // The receiver only disappears when the watcher is being dropped.
                    let _ = sender.send(());
                }
            })?;
        watcher.watch(&dir, RecursiveMode::NonRecursive)?;

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
    fn paths_without_a_parent_are_rejected() {
        assert!(matches!(
            FileWatcher::spawn(Path::new("config.toml"), Duration::from_millis(1), || {}),
            Err(WatchError::NoParent)
        ));
    }
}
