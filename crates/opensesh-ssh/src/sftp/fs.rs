//! Either file system, for the views and the transfers.

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};

use super::FsError;
use super::entry::Entry;
use super::local::Local;
use super::path::Style;
use super::remote::Remote;

/// A stream a transfer reads.
pub type Reader = Box<dyn AsyncRead + Send + Unpin>;
/// A stream a transfer writes.
pub type Writer = Box<dyn AsyncWrite + Send + Unpin>;

/// This computer's files or a server's.
#[derive(Debug, Clone)]
pub enum Fs {
    /// This computer.
    Local(Arc<Local>),
    /// A server, over SFTP.
    Remote(Arc<Remote>),
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
            Self::Remote(_) => Style::Posix,
        }
    }

    /// The server's session, if it is one.
    #[must_use]
    pub fn remote(&self) -> Option<&Arc<Remote>> {
        match self {
            Self::Remote(remote) => Some(remote),
            Self::Local(_) => None,
        }
    }

    /// Whether both are the same file system (a rename or a copy can stay inside it).
    #[must_use]
    pub fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Local(_), Self::Local(_)) => true,
            (Self::Remote(a), Self::Remote(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// Where a listing starts: the home folder.
    #[must_use]
    pub fn home(&self) -> String {
        match self {
            Self::Local(local) => local.home(),
            Self::Remote(remote) => remote.home().to_owned(),
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
        })
    }
}
