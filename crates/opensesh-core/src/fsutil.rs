//! Crash-safe file writes (PLAN §4.2): temporary file, `fsync`, `rename`, with rotated backups.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Backups kept for user-editable files (`config.toml`, `hosts.toml`, ...).
pub const DEFAULT_BACKUPS: usize = 5;

/// Result of [`atomic_write`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    /// The file was replaced.
    Written,
    /// The file already had exactly these contents; nothing was touched.
    Unchanged,
}

/// Longest chain of symbolic links [`resolve_links`] follows (Linux's own limit).
const MAX_LINK_HOPS: usize = 40;

/// Replaces `path` with `contents` so that readers only ever see the old or the new file:
///
/// 1. the new contents go to a temporary file next to the file, which is `fsync`ed;
/// 2. the current file, if any, is copied aside for the backups;
/// 3. the temporary file is renamed over the file (atomic on POSIX; `MoveFileExW` with
///    `MOVEFILE_REPLACE_EXISTING` on Windows);
/// 4. only then are the backups rotated: up to `backups` previous versions are kept next to
///    `path` as `<name>.bak.1` (newest) .. `<name>.bak.N`;
/// 5. on Unix the file's directory is `fsync`ed so the rename itself is durable.
///
/// If `path` is a symbolic link, the write goes **through** it: the file it points to is
/// replaced and the link stays (a dotfiles setup keeps working). Hard links are not kept: the
/// file gets a new inode, like with any editor that saves atomically.
///
/// A failed save leaves the backups as they were, and a problem with the backups (a read-only
/// or locked `.bak` file) is logged but never blocks the save itself.
///
/// Missing parent directories are created. On Unix the file is created with mode `0o600`.
/// Writing identical contents is a no-op, so repeated saves don't churn the backups.
///
/// # Errors
///
/// Returns the first I/O error of the save itself; the temporary file is removed on failure.
pub fn atomic_write(path: &Path, contents: &[u8], backups: usize) -> io::Result<WriteOutcome> {
    let target = resolve_links(path);
    if fs::read(&target).is_ok_and(|current| current == contents) {
        return Ok(WriteOutcome::Unchanged);
    }
    let dir = parent_dir(&target)?;
    fs::create_dir_all(&dir)?;

    let tmp = temp_path(&dir, &target, "tmp")?;
    if let Err(error) = write_synced(&tmp, contents) {
        // Best effort: the original error is what matters.
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    let previous = if backups > 0 {
        copy_aside(&target, path)
    } else {
        None
    };
    if let Err(error) = fs::rename(&tmp, &target) {
        let _ = fs::remove_file(&tmp);
        if let Some(previous) = previous {
            discard(&previous);
        }
        return Err(error);
    }
    if let Some(previous) = previous {
        if let Err(error) = rotate_backups(path, &previous, backups) {
            tracing::warn!(path = %path.display(), "saved, but could not rotate the backups: {error}");
            discard(&previous);
        }
    }
    sync_dir(&dir).map(|()| WriteOutcome::Written)
}

/// The file that `path` finally names: `path` itself, or the end of its chain of symbolic
/// links (relative links are resolved against the link's directory). The result may not exist
/// yet, like the target of a dangling link.
#[must_use]
pub fn resolve_links(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    for _ in 0..MAX_LINK_HOPS {
        // Fails for anything that isn't a symbolic link, which ends the chain.
        let Ok(target) = fs::read_link(&current) else {
            break;
        };
        current = match current.parent() {
            Some(parent) => parent.join(target),
            None => target,
        };
    }
    current
}

/// Path of the `index`-th backup of `path` (1 = newest).
#[must_use]
pub fn backup_path(path: &Path, index: usize) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".bak.{index}"));
    path.with_file_name(name)
}

fn parent_dir(path: &Path) -> io::Result<PathBuf> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => Ok(parent.to_path_buf()),
        Some(_) => Ok(PathBuf::from(".")),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path has no parent directory",
        )),
    }
}

/// A unique hidden temporary name in `dir` for `path`, e.g. `.config.toml.tmp-<pid>-<n>`.
fn temp_path(dir: &Path, path: &Path, kind: &str) -> io::Result<PathBuf> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?
        .to_string_lossy();
    Ok(dir.join(format!(
        ".{name}.{kind}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )))
}

fn write_synced(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.sync_all()
}

/// Copies `current` (the file about to be replaced) to a hidden name next to the backups of
/// `path`. Returns `None` when there is nothing to back up or the copy failed (logged: a missing
/// backup must not block the save).
fn copy_aside(current: &Path, path: &Path) -> Option<PathBuf> {
    if !current.is_file() {
        return None;
    }
    let copy = parent_dir(path)
        .and_then(|dir| temp_path(&dir, path, "bak"))
        .and_then(|copy| fs::copy(current, &copy).map(|_| copy));
    match copy {
        Ok(copy) => {
            // `fs::copy` carries a read-only attribute over, and on Windows a read-only backup
            // could never be replaced or deleted by a later rotation.
            clear_readonly(&copy);
            Some(copy)
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), "could not back up the previous version: {error}");
            None
        }
    }
}

/// Drops the oldest backup, shifts `<name>.bak.k` to `<name>.bak.k+1` and moves `previous` (the
/// replaced version) to `<name>.bak.1`. Every rename goes to a name that is free by then, so a
/// read-only backup can't block it.
fn rotate_backups(path: &Path, previous: &Path, backups: usize) -> io::Result<()> {
    remove_if_present(&backup_path(path, backups))?;
    for index in (1..backups).rev() {
        let from = backup_path(path, index);
        if from.is_file() {
            fs::rename(&from, backup_path(path, index + 1))?;
        }
    }
    fs::rename(previous, backup_path(path, 1))
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    clear_readonly(path);
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

/// Removes a leftover copy, best effort (logged).
fn discard(path: &Path) {
    clear_readonly(path);
    if let Err(error) = fs::remove_file(path) {
        tracing::warn!(path = %path.display(), "could not remove a temporary backup: {error}");
    }
}

/// Clears the read-only attribute, without which Windows refuses to replace or delete a file.
/// Best effort.
#[cfg(windows)]
fn clear_readonly(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    let mut permissions = metadata.permissions();
    if permissions.readonly() {
        // On Windows this only clears FILE_ATTRIBUTE_READONLY: no Unix mode bits get widened.
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        if let Err(error) = fs::set_permissions(path, permissions) {
            tracing::warn!(path = %path.display(), "could not clear the read-only attribute: {error}");
        }
    }
}

/// Unix renames and deletes depend on the directory's permissions, not the file's.
#[cfg(not(windows))]
fn clear_readonly(_path: &Path) {}

#[cfg(unix)]
fn sync_dir(dir: &Path) -> io::Result<()> {
    fs::File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)] // Same signature as the Unix version.
fn sync_dir(_dir: &Path) -> io::Result<()> {
    // Windows has no directory fsync; MoveFileExW is already durable enough for our needs.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_new_files_and_creates_parents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/config.toml");
        assert_eq!(
            atomic_write(&path, b"a = 1\n", 5).unwrap(),
            WriteOutcome::Written
        );
        assert_eq!(fs::read(&path).unwrap(), b"a = 1\n");
        assert!(!backup_path(&path, 1).exists(), "nothing to back up yet");
    }

    #[test]
    fn identical_contents_are_not_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        atomic_write(&path, b"same", 5).unwrap();
        assert_eq!(
            atomic_write(&path, b"same", 5).unwrap(),
            WriteOutcome::Unchanged
        );
        assert!(!backup_path(&path, 1).exists());
    }

    #[test]
    fn keeps_the_last_n_versions_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        for version in 0..8 {
            atomic_write(&path, format!("v{version}").as_bytes(), 5).unwrap();
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), "v7");
        for index in 1..=5 {
            assert_eq!(
                fs::read_to_string(backup_path(&path, index)).unwrap(),
                format!("v{}", 7 - index)
            );
        }
        assert!(!backup_path(&path, 6).exists());
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn set_readonly(path: &Path, readonly: bool) {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        // Test files only; on Unix this makes them writable by their owner again.
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(readonly);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[test]
    fn no_temporary_files_are_left_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        atomic_write(&path, b"one", 0).unwrap();
        atomic_write(&path, b"two", 0).unwrap();
        assert_eq!(names_in(dir.path()), vec!["state.toml".to_owned()]);

        let config = dir.path().join("config.toml");
        for version in 0..3 {
            atomic_write(&config, format!("v{version}").as_bytes(), 5).unwrap();
        }
        assert_eq!(
            names_in(dir.path()),
            [
                "config.toml",
                "config.toml.bak.1",
                "config.toml.bak.2",
                "state.toml"
            ]
        );
    }

    #[test]
    fn read_only_backups_never_block_a_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        for version in 0..6 {
            atomic_write(&path, format!("v{version}").as_bytes(), 5).unwrap();
        }
        // E.g. restored from read-only media, or copied from a read-only config.toml by an older
        // build (Windows refuses to replace or delete a read-only file).
        set_readonly(&backup_path(&path, 3), true);
        set_readonly(&backup_path(&path, 5), true);
        for version in 6..12 {
            atomic_write(&path, format!("v{version}").as_bytes(), 5).unwrap();
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), "v11");
        for index in 1..=5 {
            assert_eq!(
                fs::read_to_string(backup_path(&path, index)).unwrap(),
                format!("v{}", 11 - index)
            );
        }
        assert_eq!(names_in(dir.path()).len(), 6, "{:?}", names_in(dir.path()));
    }

    /// Windows refuses to rename over a read-only file, so the save fails. It must not touch
    /// the backups, and the read-only attribute must not spread to them.
    #[cfg(windows)]
    #[test]
    fn a_failed_save_leaves_the_backups_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        atomic_write(&path, b"v0", 5).unwrap();
        atomic_write(&path, b"v1", 5).unwrap();
        set_readonly(&path, true);
        for attempt in 0..3 {
            let error = atomic_write(&path, format!("failed {attempt}").as_bytes(), 5)
                .expect_err("a read-only file can't be replaced on Windows");
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        }
        assert_eq!(
            names_in(dir.path()),
            ["config.toml", "config.toml.bak.1"],
            "no rotation and no leftovers"
        );
        assert_eq!(fs::read_to_string(backup_path(&path, 1)).unwrap(), "v0");

        // Once the user clears the attribute, every later save works.
        set_readonly(&path, false);
        for version in 2..10 {
            atomic_write(&path, format!("v{version}").as_bytes(), 5).unwrap();
        }
        for index in 1..=5 {
            let backup = backup_path(&path, index);
            assert_eq!(
                fs::read_to_string(&backup).unwrap(),
                format!("v{}", 9 - index)
            );
            assert!(!fs::metadata(&backup).unwrap().permissions().readonly());
        }
    }

    /// Creates a symbolic link to a file; `false` if this system doesn't allow it (Windows
    /// without Developer Mode or admin rights).
    fn symlink_file(target: &Path, link: &Path) -> bool {
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(target, link);
        #[cfg(windows)]
        let result = std::os::windows::fs::symlink_file(target, link);
        match result {
            Ok(()) => true,
            Err(error) => {
                eprintln!("skipped: can't create a symbolic link here: {error}");
                false
            }
        }
    }

    #[test]
    fn writes_go_through_symbolic_links() {
        let dir = tempfile::tempdir().unwrap();
        let dotfiles = dir.path().join("dotfiles");
        let config_dir = dir.path().join("config");
        fs::create_dir_all(&dotfiles).unwrap();
        fs::create_dir_all(&config_dir).unwrap();
        let target = dotfiles.join("opensesh.toml");
        fs::write(&target, "v0").unwrap();
        let link = config_dir.join("config.toml");
        // A relative link, as GNU Stow makes them.
        if !symlink_file(Path::new("../dotfiles/opensesh.toml"), &link) {
            return;
        }
        assert_eq!(
            resolve_links(&link),
            config_dir.join("../dotfiles/opensesh.toml")
        );

        atomic_write(&link, b"v1", 5).unwrap();
        atomic_write(&link, b"v2", 5).unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "v2");
        assert_eq!(fs::read_to_string(&link).unwrap(), "v2");
        // Backups stay in the config directory, next to the link.
        assert_eq!(fs::read_to_string(backup_path(&link, 1)).unwrap(), "v1");
        assert_eq!(fs::read_to_string(backup_path(&link, 2)).unwrap(), "v0");
        assert_eq!(names_in(&dotfiles), ["opensesh.toml"]);
        assert_eq!(
            names_in(&config_dir),
            ["config.toml", "config.toml.bak.1", "config.toml.bak.2"]
        );
        assert_eq!(
            atomic_write(&link, b"v2", 5).unwrap(),
            WriteOutcome::Unchanged
        );
    }

    #[test]
    fn regular_files_resolve_to_themselves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert_eq!(resolve_links(&path), path, "missing file");
        fs::write(&path, "x").unwrap();
        assert_eq!(resolve_links(&path), path);
    }

    #[test]
    fn backup_names_append_a_suffix() {
        assert_eq!(
            backup_path(Path::new("/x/config.toml"), 2),
            PathBuf::from("/x/config.toml.bak.2")
        );
    }

    #[cfg(unix)]
    #[test]
    fn files_are_private_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        atomic_write(&path, b"secret-free but private", 0).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
