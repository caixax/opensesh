//! This computer's files, with the operations of [`Remote`](super::remote::Remote). Listings
//! and walks run on the blocking pool; file contents stream through tokio.

use std::path::{Path, PathBuf};

use tokio::io::AsyncSeekExt;

use super::FsError;
use super::entry::{Entry, Kind};
use super::path::{self, Style};

/// This computer's files.
#[derive(Debug, Default)]
pub struct Local;

fn modified(meta: &std::fs::Metadata) -> Option<i64> {
    let time = meta.modified().ok()?;
    let secs = time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(secs).ok()
}

#[cfg(unix)]
fn mode_and_owner(meta: &std::fs::Metadata) -> (u32, Option<u32>, Option<u32>) {
    use std::os::unix::fs::MetadataExt;
    (meta.mode() & 0o7777, Some(meta.uid()), Some(meta.gid()))
}

#[cfg(not(unix))]
fn mode_and_owner(meta: &std::fs::Metadata) -> (u32, Option<u32>, Option<u32>) {
    // Windows has no POSIX bits: read-only and folders are what they show.
    let base = if meta.permissions().readonly() {
        0o444
    } else {
        0o644
    };
    let mode = if meta.is_dir() { base | 0o111 } else { base };
    (mode, None, None)
}

fn kind(file_type: std::fs::FileType) -> Kind {
    if file_type.is_symlink() {
        Kind::Symlink
    } else if file_type.is_dir() {
        Kind::Dir
    } else if file_type.is_file() {
        Kind::File
    } else {
        Kind::Other
    }
}

/// An entry for `path` (not following a link), with its link resolved.
fn entry_of(path: &Path, name: String) -> std::io::Result<Entry> {
    let meta = std::fs::symlink_metadata(path)?;
    let (mode, uid, gid) = mode_and_owner(&meta);
    let kind = kind(meta.file_type());
    let (link_target, target_kind) = if kind == Kind::Symlink {
        (
            std::fs::read_link(path)
                .ok()
                .map(|target| target.display().to_string()),
            std::fs::metadata(path)
                .ok()
                .map(|target| self::kind(target.file_type())),
        )
    } else {
        (None, None)
    };
    Ok(Entry {
        name,
        kind,
        size: meta.len(),
        modified: modified(&meta),
        mode,
        uid,
        gid,
        link_target,
        target_kind,
    })
}

/// The drives of this computer (Windows), as folders named `C:`.
#[cfg(windows)]
fn drives() -> Vec<Entry> {
    (b'A'..=b'Z')
        .map(|letter| format!("{}:", char::from(letter)))
        .filter(|drive| Path::new(&format!("{drive}\\")).exists())
        .map(|name| Entry {
            name,
            kind: Kind::Dir,
            size: 0,
            modified: None,
            mode: 0o755,
            uid: None,
            gid: None,
            link_target: None,
            target_kind: None,
        })
        .collect()
}

fn list_blocking(dir: &str) -> Result<Vec<Entry>, FsError> {
    #[cfg(windows)]
    if dir.is_empty() {
        return Ok(drives());
    }
    let read = std::fs::read_dir(dir).map_err(|error| FsError::io(dir, &error))?;
    let mut entries = Vec::new();
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        // A file that vanished or can't be read in the meantime is left out.
        if let Ok(entry) = entry_of(&item.path(), name) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

async fn blocking<T: Send + 'static>(
    what: &str,
    job: impl FnOnce() -> Result<T, FsError> + Send + 'static,
) -> Result<T, FsError> {
    tokio::task::spawn_blocking(job)
        .await
        .map_err(|error| FsError::Failed {
            path: what.to_owned(),
            message: error.to_string(),
        })?
}

impl Local {
    /// The user's home folder.
    #[must_use]
    pub fn home(&self) -> String {
        opensesh_core::paths::home_dir()
            .map(|home| home.display().to_string())
            .unwrap_or_default()
    }

    /// The files of `dir`; on Windows the empty path lists the drives.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `dir`.
    pub async fn list(&self, dir: &str) -> Result<Vec<Entry>, FsError> {
        let owned = dir.to_owned();
        blocking(dir, move || list_blocking(&owned)).await
    }

    /// `path`, following links.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn stat(&self, path: &str) -> Result<Entry, FsError> {
        let owned = path.to_owned();
        blocking(path, move || {
            let meta = std::fs::metadata(&owned).map_err(|error| FsError::io(&owned, &error))?;
            let (mode, uid, gid) = mode_and_owner(&meta);
            Ok(Entry {
                name: path::file_name(Style::Local, &owned),
                kind: kind(meta.file_type()),
                size: meta.len(),
                modified: modified(&meta),
                mode,
                uid,
                gid,
                link_target: None,
                target_kind: None,
            })
        })
        .await
    }

    /// `path` itself (a link is a link).
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn lstat(&self, path: &str) -> Result<Entry, FsError> {
        let owned = path.to_owned();
        blocking(path, move || {
            entry_of(Path::new(&owned), path::file_name(Style::Local, &owned))
                .map_err(|error| FsError::io(&owned, &error))
        })
        .await
    }

    /// Whether something is at `path`.
    ///
    /// # Errors
    ///
    /// Never, today; the signature matches the remote one.
    pub async fn exists(&self, path: &str) -> Result<bool, FsError> {
        Ok(tokio::fs::symlink_metadata(path).await.is_ok())
    }

    /// A new folder.
    ///
    /// # Errors
    ///
    /// [`FsError::Exists`] when something is there.
    pub async fn mkdir(&self, path: &str) -> Result<(), FsError> {
        tokio::fs::create_dir(path)
            .await
            .map_err(|error| FsError::io(path, &error))
    }

    /// A new empty file.
    ///
    /// # Errors
    ///
    /// [`FsError::Exists`] when something is there.
    pub async fn create_file(&self, path: &str) -> Result<(), FsError> {
        tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .await
            .map(drop)
            .map_err(|error| FsError::io(path, &error))
    }

    /// Renames or moves `from` to `to`, which must not exist.
    ///
    /// # Errors
    ///
    /// [`FsError::Exists`] when `to` exists.
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), FsError> {
        if self.exists(to).await? {
            return Err(FsError::Exists {
                path: to.to_owned(),
            });
        }
        tokio::fs::rename(from, to)
            .await
            .map_err(|error| FsError::io(from, &error))
    }

    /// Deletes `path`; a folder with its contents when `recursive`. Links are removed, not
    /// followed.
    ///
    /// # Errors
    ///
    /// The first [`FsError`] met.
    pub async fn remove(&self, path: &str, recursive: bool) -> Result<(), FsError> {
        let owned = path.to_owned();
        blocking(path, move || {
            let meta =
                std::fs::symlink_metadata(&owned).map_err(|error| FsError::io(&owned, &error))?;
            let result = if meta.is_dir() {
                if recursive {
                    std::fs::remove_dir_all(&owned)
                } else {
                    std::fs::remove_dir(&owned)
                }
            } else if meta.file_type().is_symlink()
                && cfg!(windows)
                && std::fs::metadata(&owned).is_ok_and(|target| target.is_dir())
            {
                // A directory symlink on Windows is removed as a folder.
                std::fs::remove_dir(&owned)
            } else {
                std::fs::remove_file(&owned)
            };
            result.map_err(|error| FsError::io(&owned, &error))
        })
        .await
    }

    /// Sets the permission bits of `path` (on Windows, only read-only follows the owner's `w`).
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn chmod(&self, path: &str, mode: u32) -> Result<(), FsError> {
        let owned = path.to_owned();
        blocking(path, move || {
            #[cfg(unix)]
            let permissions = {
                use std::os::unix::fs::PermissionsExt;
                std::fs::Permissions::from_mode(mode & 0o7777)
            };
            #[cfg(not(unix))]
            let permissions = {
                let mut permissions = std::fs::metadata(&owned)
                    .map_err(|error| FsError::io(&owned, &error))?
                    .permissions();
                permissions.set_readonly(mode & 0o200 == 0);
                permissions
            };
            std::fs::set_permissions(&owned, permissions)
                .map_err(|error| FsError::io(&owned, &error))
        })
        .await
    }

    /// Sets the access and modification times of `path` (seconds since the Unix epoch).
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn set_times(&self, path: &str, accessed: i64, modified: i64) -> Result<(), FsError> {
        let owned = path.to_owned();
        blocking(path, move || {
            let at = |secs: i64| {
                std::time::UNIX_EPOCH
                    + std::time::Duration::from_secs(u64::try_from(secs.max(0)).unwrap_or(0))
            };
            let times = std::fs::FileTimes::new()
                .set_accessed(at(accessed))
                .set_modified(at(modified));
            std::fs::File::options()
                .write(true)
                .open(&owned)
                .and_then(|file| file.set_times(times))
                .map_err(|error| FsError::io(&owned, &error))
        })
        .await
    }

    /// A symbolic link at `link` pointing to `target` (on Windows it may need Developer Mode).
    ///
    /// # Errors
    ///
    /// [`FsError`] about `link`.
    pub async fn symlink(&self, link: &str, target: &str) -> Result<(), FsError> {
        let (link, target) = (link.to_owned(), target.to_owned());
        let what = link.clone();
        blocking(&what, move || {
            #[cfg(unix)]
            let result = std::os::unix::fs::symlink(&target, &link);
            #[cfg(windows)]
            let result = {
                let resolved = Path::new(&link)
                    .parent()
                    .map_or_else(|| PathBuf::from(&target), |dir| dir.join(&target));
                if resolved.is_dir() {
                    std::os::windows::fs::symlink_dir(&target, &link)
                } else {
                    std::os::windows::fs::symlink_file(&target, &link)
                }
            };
            result.map_err(|error| FsError::io(&link, &error))
        })
        .await
    }

    /// Where the link at `path` points.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn read_link(&self, path: &str) -> Result<String, FsError> {
        tokio::fs::read_link(path)
            .await
            .map(|target| target.display().to_string())
            .map_err(|error| FsError::io(path, &error))
    }

    /// The absolute form of `path` (`~` is the home folder).
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn canonicalize(&self, path: &str) -> Result<String, FsError> {
        let expanded = match opensesh_core::paths::home_dir() {
            Some(home) if path == "~" || path.starts_with("~/") || path.starts_with("~\\") => {
                opensesh_core::paths::expand_tilde(path, &home)
            }
            _ => PathBuf::from(path),
        };
        if cfg!(windows) && path.is_empty() {
            return Ok(String::new());
        }
        let canonical = tokio::fs::canonicalize(&expanded)
            .await
            .map_err(|error| FsError::io(path, &error))?;
        Ok(strip_verbatim(&canonical.display().to_string()))
    }

    /// `path` opened for reading at `offset`.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn open_read(&self, path: &str, offset: u64) -> Result<tokio::fs::File, FsError> {
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|error| FsError::io(path, &error))?;
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|error| FsError::io(path, &error))?;
        }
        Ok(file)
    }

    /// `path` opened for writing: from the start, truncated (`offset` `None`), or at `offset`.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn open_write(
        &self,
        path: &str,
        offset: Option<u64>,
    ) -> Result<tokio::fs::File, FsError> {
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create(true);
        if offset.is_none() {
            options.truncate(true);
        }
        let mut file = options
            .open(path)
            .await
            .map_err(|error| FsError::io(path, &error))?;
        if let Some(offset) = offset.filter(|offset| *offset > 0) {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|error| FsError::io(path, &error))?;
        }
        Ok(file)
    }
}

/// `\\?\C:\x` (what `canonicalize` gives on Windows) as `C:\x`.
fn strip_verbatim(path: &str) -> String {
    path.strip_prefix(r"\\?\")
        .filter(|rest| rest.as_bytes().get(1) == Some(&b':'))
        .unwrap_or(path)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbatim_paths() {
        assert_eq!(strip_verbatim(r"\\?\C:\Users"), r"C:\Users");
        assert_eq!(
            strip_verbatim(r"\\?\UNC\server\share"),
            r"\\?\UNC\server\share"
        );
        assert_eq!(strip_verbatim("/home/me"), "/home/me");
    }

    #[tokio::test]
    async fn local_operations() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().display().to_string();
        let local = Local;
        let sub = path::join(Style::Local, &root, "sub");
        local.mkdir(&sub).await.unwrap();
        assert_eq!(local.mkdir(&sub).await.unwrap_err().code(), "exists");
        let file = path::join(Style::Local, &sub, "a.txt");
        local.create_file(&file).await.unwrap();
        assert_eq!(local.create_file(&file).await.unwrap_err().code(), "exists");
        local.set_times(&file, 1_000_000, 2_000_000).await.unwrap();
        assert_eq!(local.stat(&file).await.unwrap().modified, Some(2_000_000));
        let moved = path::join(Style::Local, &root, "b.txt");
        local.rename(&file, &moved).await.unwrap();
        let listing = local.list(&root).await.unwrap();
        let mut names: Vec<&str> = listing.iter().map(|entry| entry.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["b.txt", "sub"]);
        local.chmod(&moved, 0o444).await.unwrap();
        assert_eq!(local.stat(&moved).await.unwrap().mode & 0o200, 0);
        local.chmod(&moved, 0o644).await.unwrap();
        local.remove(&root, true).await.unwrap();
        assert!(!local.exists(&root).await.unwrap());
        assert_eq!(local.list(&root).await.unwrap_err().code(), "not-found");
    }
}
