//! Any of the file systems, for the views and the transfers.

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};

use super::FsError;
use super::entry::Entry;
use super::local::Local;
use super::path::Style;
use super::path::normalize_posix;
use super::remote::Remote;
use super::s3::S3Fs;

/// A stream a transfer reads.
pub type Reader = Box<dyn AsyncRead + Send + Unpin>;
/// A stream a transfer writes.
pub type Writer = Box<dyn AsyncWrite + Send + Unpin>;

/// This computer's files, a server's, or S3 storage.
#[derive(Debug, Clone)]
pub enum Fs {
    /// This computer.
    Local(Arc<Local>),
    /// A server, over SFTP.
    Remote(Arc<Remote>),
    /// S3 storage: buckets and objects.
    S3(Arc<S3Fs>),
}

impl Fs {
    /// This computer's files.
    #[must_use]
    pub fn local() -> Self {
        Self::Local(Arc::new(Local))
    }

    /// How its paths are written.
    #[must_use]
    pub fn style(&self) -> Style {
        match self {
            Self::Local(_) => Style::Local,
            Self::Remote(_) | Self::S3(_) => Style::Posix,
        }
    }

    /// The server's session, if it is one.
    #[must_use]
    pub fn remote(&self) -> Option<&Arc<Remote>> {
        match self {
            Self::Remote(remote) => Some(remote),
            Self::Local(_) | Self::S3(_) => None,
        }
    }

    /// The S3 storage, if it is.
    #[must_use]
    pub fn s3(&self) -> Option<&Arc<S3Fs>> {
        match self {
            Self::S3(s3) => Some(s3),
            Self::Local(_) | Self::Remote(_) => None,
        }
    }

    /// Whether these are this computer's files.
    #[must_use]
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Whether a file can be written from an offset (a paused or partial copy goes on); S3
    /// starts over.
    #[must_use]
    pub fn can_resume(&self) -> bool {
        !matches!(self, Self::S3(_))
    }

    /// Whether files keep times and permissions set on them.
    #[must_use]
    pub fn keeps_metadata(&self) -> bool {
        !matches!(self, Self::S3(_))
    }

    /// Whether both are the same file system (a rename or a copy can stay inside it).
    #[must_use]
    pub fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Local(_), Self::Local(_)) => true,
            (Self::Remote(a), Self::Remote(b)) => Arc::ptr_eq(a, b),
            (Self::S3(a), Self::S3(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// Where a listing starts: the home folder.
    #[must_use]
    pub fn home(&self) -> String {
        match self {
            Self::Local(local) => local.home(),
            Self::Remote(remote) => remote.home().to_owned(),
            Self::S3(s3) => s3.home(),
        }
    }

    /// See [`Remote::list`].
    ///
    /// # Errors
    ///
    /// [`FsError`] about `dir`.
    pub async fn list(&self, dir: &str) -> Result<Vec<Entry>, FsError> {
        match self {
            Self::Local(fs) => fs.list(dir).await,
            Self::Remote(fs) => fs.list(dir).await,
            Self::S3(fs) => fs.list(dir).await,
        }
    }

    /// See [`Remote::stat`].
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn stat(&self, path: &str) -> Result<Entry, FsError> {
        match self {
            Self::Local(fs) => fs.stat(path).await,
            Self::Remote(fs) => fs.stat(path).await,
            Self::S3(fs) => fs.stat(path).await,
        }
    }

    /// See [`Remote::lstat`].
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn lstat(&self, path: &str) -> Result<Entry, FsError> {
        match self {
            Self::Local(fs) => fs.lstat(path).await,
            Self::Remote(fs) => fs.lstat(path).await,
            // No links: an entry is itself.
            Self::S3(fs) => fs.stat(path).await,
        }
    }

    /// `path` itself, or `None` when nothing is there.
    ///
    /// # Errors
    ///
    /// [`FsError`] other than "not found".
    pub async fn try_lstat(&self, path: &str) -> Result<Option<Entry>, FsError> {
        match self.lstat(path).await {
            Ok(entry) => Ok(Some(entry)),
            Err(FsError::NotFound { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// See [`Remote::mkdir`].
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn mkdir(&self, path: &str) -> Result<(), FsError> {
        match self {
            Self::Local(fs) => fs.mkdir(path).await,
            Self::Remote(fs) => fs.mkdir(path).await,
            Self::S3(fs) => fs.mkdir(path).await,
        }
    }

    /// See [`Remote::create_file`].
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn create_file(&self, path: &str) -> Result<(), FsError> {
        match self {
            Self::Local(fs) => fs.create_file(path).await,
            Self::Remote(fs) => fs.create_file(path).await,
            Self::S3(fs) => fs.create_file(path).await,
        }
    }

    /// See [`Remote::rename`].
    ///
    /// # Errors
    ///
    /// [`FsError`] about the paths.
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), FsError> {
        match self {
            Self::Local(fs) => fs.rename(from, to).await,
            Self::Remote(fs) => fs.rename(from, to).await,
            Self::S3(fs) => fs.rename(from, to).await,
        }
    }

    /// See [`Remote::remove`].
    ///
    /// # Errors
    ///
    /// The first [`FsError`] met.
    pub async fn remove(&self, path: &str, recursive: bool) -> Result<(), FsError> {
        match self {
            Self::Local(fs) => fs.remove(path, recursive).await,
            Self::Remote(fs) => fs.remove(path, recursive).await,
            Self::S3(fs) => fs.remove(path, recursive).await,
        }
    }

    /// See [`Remote::chmod`].
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn chmod(&self, path: &str, mode: u32) -> Result<(), FsError> {
        match self {
            Self::Local(fs) => fs.chmod(path, mode).await,
            Self::Remote(fs) => fs.chmod(path, mode).await,
            Self::S3(_) => Err(FsError::Unsupported {
                what: "permissions in S3".to_owned(),
            }),
        }
    }

    /// See [`Remote::set_times`].
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn set_times(&self, path: &str, accessed: i64, modified: i64) -> Result<(), FsError> {
        match self {
            Self::Local(fs) => fs.set_times(path, accessed, modified).await,
            Self::Remote(fs) => fs.set_times(path, accessed, modified).await,
            Self::S3(_) => Err(FsError::Unsupported {
                what: "setting times in S3".to_owned(),
            }),
        }
    }

    /// See [`Remote::symlink`].
    ///
    /// # Errors
    ///
    /// [`FsError`] about `link`.
    pub async fn symlink(&self, link: &str, target: &str) -> Result<(), FsError> {
        match self {
            Self::Local(fs) => fs.symlink(link, target).await,
            Self::Remote(fs) => fs.symlink(link, target).await,
            Self::S3(_) => Err(FsError::Unsupported {
                what: "links in S3".to_owned(),
            }),
        }
    }

    /// See [`Remote::read_link`].
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn read_link(&self, path: &str) -> Result<String, FsError> {
        match self {
            Self::Local(fs) => fs.read_link(path).await,
            Self::Remote(fs) => fs.read_link(path).await,
            Self::S3(_) => Err(FsError::Unsupported {
                what: "links in S3".to_owned(),
            }),
        }
    }

    /// See [`Remote::canonicalize`].
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn canonicalize(&self, path: &str) -> Result<String, FsError> {
        match self {
            Self::Local(fs) => fs.canonicalize(path).await,
            Self::Remote(fs) => fs.canonicalize(path).await,
            // `~` is the top, where the buckets are.
            Self::S3(_) => Ok(normalize_posix(&format!(
                "/{}",
                path.strip_prefix('~').unwrap_or(path)
            ))),
        }
    }

    /// `path` opened for reading at `offset`.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn open_read(&self, path: &str, offset: u64) -> Result<Reader, FsError> {
        Ok(match self {
            Self::Local(fs) => Box::new(fs.open_read(path, offset).await?),
            Self::Remote(fs) => Box::new(fs.open_read(path, offset).await?),
            Self::S3(fs) => fs.open_read(path, offset).await?,
        })
    }

    /// `path` opened for writing (truncated, or at `offset` to resume).
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn open_write(&self, path: &str, offset: Option<u64>) -> Result<Writer, FsError> {
        Ok(match self {
            Self::Local(fs) => Box::new(fs.open_write(path, offset).await?),
            Self::Remote(fs) => Box::new(fs.open_write(path, offset).await?),
            Self::S3(fs) => fs.open_write(path, offset).await?,
        })
    }
}
