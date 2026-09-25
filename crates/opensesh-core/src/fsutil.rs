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

/// Replaces `path` with `contents` so that readers only ever see the old or the new file:
///
/// 1. the new contents go to a temporary file in the same directory, which is `fsync`ed;
/// 2. up to `backups` previous versions are kept as `<name>.bak.1` (newest) .. `<name>.bak.N`;
/// 3. the temporary file is renamed over `path` (atomic on POSIX; `MoveFileExW` with
///    `MOVEFILE_REPLACE_EXISTING` on Windows);
/// 4. on Unix the directory is `fsync`ed so the rename itself is durable.
///
/// Missing parent directories are created. On Unix the file is created with mode `0o600`.
/// Writing identical contents is a no-op, so repeated saves don't churn the backups.
///
/// # Errors
///
/// Returns the first I/O error; the temporary file is removed on failure.
pub fn atomic_write(path: &Path, contents: &[u8], backups: usize) -> io::Result<WriteOutcome> {
    if fs::read(path).is_ok_and(|current| current == contents) {
        return Ok(WriteOutcome::Unchanged);
    }
    let dir = parent_dir(path)?;
    fs::create_dir_all(&dir)?;

    let tmp = temp_path(&dir, path)?;
    let result = write_synced(&tmp, contents)
        .and_then(|()| rotate_backups(path, backups))
        .and_then(|()| fs::rename(&tmp, path))
        .and_then(|()| sync_dir(&dir));
    if result.is_err() {
        // Best effort: the original error is what matters.
        let _ = fs::remove_file(&tmp);
    }
    result.map(|()| WriteOutcome::Written)
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

/// A unique hidden temporary name next to `path`.
fn temp_path(dir: &Path, path: &Path) -> io::Result<PathBuf> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?
        .to_string_lossy();
    Ok(dir.join(format!(
        ".{name}.tmp-{}-{}",
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

/// Shifts `<name>.bak.k` to `<name>.bak.k+1` (dropping the oldest) and copies the current file
/// to `<name>.bak.1`. The current file stays in place until the final rename.
fn rotate_backups(path: &Path, backups: usize) -> io::Result<()> {
    if backups == 0 || !path.is_file() {
        return Ok(());
    }
    for index in (1..backups).rev() {
        let from = backup_path(path, index);
        if from.is_file() {
            fs::rename(&from, backup_path(path, index + 1))?;
        }
    }
    fs::copy(path, backup_path(path, 1)).map(|_| ())
}

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

    #[test]
    fn no_temporary_files_are_left_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        atomic_write(&path, b"one", 0).unwrap();
        atomic_write(&path, b"two", 0).unwrap();
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["state.toml".to_owned()]);
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
