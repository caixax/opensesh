//! Files over SSH (PLAN Sprint 8, ADR 0028), without Qt.
//!
//! - [`remote`]: a server's files through the SFTP subsystem (`russh-sftp`), on a new channel of
//!   a [`Connection`](crate::connect::Connection).
//! - [`local`]: this computer's files, behind the same operations.
//! - [`fs`]: either of them ([`Fs`]), which the views and the transfers use.
//! - [`entry`]: what a listing holds, sorting, permissions text.
//! - [`path`]: POSIX paths on servers, native paths here.
//! - [`transfer`]: the transfer queue (parallel limit, progress, pause, resume, the overwrite
//!   policy).
//!
//! Errors name paths, never file contents.

pub mod entry;
pub mod fs;
pub mod local;
pub mod path;
pub mod remote;
pub mod transfer;

pub use entry::{Entry, Kind};
pub use fs::Fs;

use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::StatusCode;

/// Why a file operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FsError {
    /// The file or folder doesn't exist.
    #[error("{path}: no such file or folder")]
    NotFound {
        /// The path.
        path: String,
    },
    /// Not allowed.
    #[error("{path}: permission denied")]
    PermissionDenied {
        /// The path.
        path: String,
    },
    /// Something is in the way.
    #[error("{path}: already exists")]
    Exists {
        /// The path.
        path: String,
    },
    /// The operation failed for another reason.
    #[error("{path}: {message}")]
    Failed {
        /// The path.
        path: String,
        /// Why, as the server or the system said it.
        message: String,
    },
    /// The connection is gone.
    #[error("the connection was lost")]
    Disconnected,
    /// The server didn't answer in time.
    #[error("the server didn't answer in time")]
    Timeout,
    /// The server can't do this.
    #[error("the server doesn't support {what}")]
    Unsupported {
        /// What was asked.
        what: String,
    },
    /// The user stopped it.
    #[error("cancelled")]
    Cancelled,
}

impl FsError {
    /// A short code for the UI to word.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "not-found",
            Self::PermissionDenied { .. } => "permission",
            Self::Exists { .. } => "exists",
            Self::Failed { .. } => "failed",
            Self::Disconnected => "disconnected",
            Self::Timeout => "timeout",
            Self::Unsupported { .. } => "unsupported",
            Self::Cancelled => "cancelled",
        }
    }

    /// Whether trying again later can work (the connection, not the file, was the problem).
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Disconnected | Self::Timeout)
    }

    /// An SFTP error about `path`.
    pub(crate) fn sftp(path: &str, error: SftpError) -> Self {
        let path = path.to_owned();
        match error {
            SftpError::Status(status) => match status.status_code {
                StatusCode::NoSuchFile => Self::NotFound { path },
                StatusCode::PermissionDenied => Self::PermissionDenied { path },
                StatusCode::NoConnection | StatusCode::ConnectionLost => Self::Disconnected,
                StatusCode::OpUnsupported => Self::Unsupported {
                    what: format!("this operation on {path}"),
                },
                _ => Self::Failed {
                    path,
                    message: if status.error_message.trim().is_empty() {
                        status.status_code.to_string()
                    } else {
                        status.error_message
                    },
                },
            },
            SftpError::Timeout => Self::Timeout,
            SftpError::IO(_) => Self::Disconnected,
            other => Self::Failed {
                path,
                message: other.to_string(),
            },
        }
    }

    /// A local I/O error about `path`.
    pub(crate) fn io(path: &str, error: &std::io::Error) -> Self {
        let path = path.to_owned();
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound { path },
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied { path },
            std::io::ErrorKind::AlreadyExists => Self::Exists { path },
            std::io::ErrorKind::TimedOut => Self::Timeout,
            _ => Self::Failed {
                path,
                message: error.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh_sftp::protocol::Status;

    fn status(code: StatusCode, message: &str) -> SftpError {
        SftpError::Status(Status {
            id: 1,
            status_code: code,
            error_message: message.to_owned(),
            language_tag: "en-US".to_owned(),
        })
    }

    #[test]
    fn errors_map_to_codes() {
        assert_eq!(
            FsError::sftp("/a", status(StatusCode::NoSuchFile, "")).code(),
            "not-found"
        );
        assert_eq!(
            FsError::sftp("/a", status(StatusCode::PermissionDenied, "")).code(),
            "permission"
        );
        assert_eq!(
            FsError::sftp("/a", status(StatusCode::Failure, "disk full")),
            FsError::Failed {
                path: "/a".into(),
                message: "disk full".into()
            }
        );
        assert!(FsError::sftp("/a", SftpError::Timeout).is_transient());
        assert!(
            !FsError::io("x", &std::io::Error::from(std::io::ErrorKind::NotFound)).is_transient()
        );
    }
}
