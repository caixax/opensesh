//! One running instance (PLAN Sprint 5, ADR 0021): the app listens on a local socket (a Unix
//! socket in the user's runtime or data directory, a named pipe on Windows); a second start of
//! the app and the `opensesh` CLI hand their request to it instead of opening another window.
//!
//! The protocol is one JSON object per line each way: a [`Request`], then a [`Reply`]. The
//! endpoint name depends on the config directory, so a portable copy and an installed one don't
//! talk to each other.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, ListenerOptions, Name, Stream,
};
use serde::{Deserialize, Serialize};

use crate::AppPaths;

/// Longest request line accepted.
const MAX_LINE: u64 = 64 * 1024;

/// What a caller asks the running instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Bring the main window to the front.
    Activate,
    /// Connect to a saved host (by id or name).
    Connect {
        /// Id or name.
        host: String,
    },
    /// Connect to a quick-connect target or URL (asks the user first, PLAN §8).
    Open {
        /// The text or URL.
        url: String,
    },
}

/// The instance's answer: the request was taken (the app does the rest), or why not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Reply {
    /// Taken.
    Ok,
    /// Refused.
    Error {
        /// Why.
        message: String,
    },
}

/// Why talking to the instance failed.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    /// No instance listens (or it doesn't answer).
    #[error("no running OpenSesh answered")]
    NotRunning(#[source] std::io::Error),
    /// The endpoint name can't be used on this system.
    #[error("the instance endpoint can't be used: {0}")]
    Name(#[source] std::io::Error),
    /// Reading or writing failed after connecting.
    #[error("talking to the running OpenSesh failed")]
    Io(#[source] std::io::Error),
    /// The other side sent something that isn't a message.
    #[error("the running OpenSesh sent an unexpected answer: {0}")]
    Protocol(String),
}

/// Where the instance listens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// A Unix socket file.
    Path(PathBuf),
    /// A named pipe (Windows) or another namespaced name.
    Namespaced(String),
}

/// FNV-1a, 64 bits: a short, stable tag of the config directory.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

impl Endpoint {
    /// The endpoint of the instance that uses `paths`.
    #[must_use]
    pub fn for_paths(paths: &AppPaths) -> Self {
        let tag = format!(
            "{:012x}",
            fnv1a(paths.config_dir().to_string_lossy().as_bytes()) & 0xffff_ffff_ffff
        );
        if cfg!(windows) {
            let user: String = std::env::var("USERNAME")
                .unwrap_or_default()
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .take(32)
                .collect();
            return Self::Namespaced(format!("opensesh-{user}-{tag}"));
        }
        // The runtime directory is private to the user (XDG); else the data directory, which
        // OpenSesh keeps private too.
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute() && dir.is_dir())
            .unwrap_or_else(|| paths.data_dir().to_path_buf());
        Self::Path(dir.join(format!("opensesh-{tag}.sock")))
    }

    fn name(&self) -> Result<Name<'_>, IpcError> {
        match self {
            Self::Path(path) => path.as_path().to_fs_name::<GenericFilePath>(),
            Self::Namespaced(name) => name.as_str().to_ns_name::<GenericNamespaced>(),
        }
        .map_err(IpcError::Name)
    }

    /// The socket file, for an endpoint that has one.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Path(path) => Some(path),
            Self::Namespaced(_) => None,
        }
    }
}

fn read_message<T: for<'de> Deserialize<'de>>(stream: &mut impl Read) -> Result<T, IpcError> {
    let mut line = String::new();
    BufReader::new(stream.take(MAX_LINE))
        .read_line(&mut line)
        .map_err(IpcError::Io)?;
    serde_json::from_str(line.trim()).map_err(|error| IpcError::Protocol(error.to_string()))
}

fn write_message<T: Serialize>(stream: &mut impl Write, message: &T) -> Result<(), IpcError> {
    let mut text =
        serde_json::to_string(message).map_err(|error| IpcError::Protocol(error.to_string()))?;
    text.push('\n');
    stream.write_all(text.as_bytes()).map_err(IpcError::Io)?;
    stream.flush().map_err(IpcError::Io)
}

/// Sends `request` to the running instance and waits up to `timeout` for its reply.
///
/// # Errors
///
/// [`IpcError::NotRunning`] when nothing listens; other variants when the exchange fails.
pub fn send(endpoint: &Endpoint, request: &Request, timeout: Duration) -> Result<Reply, IpcError> {
    let mut stream = Stream::connect(endpoint.name()?).map_err(IpcError::NotRunning)?;
    // Named pipes have no I/O timeouts: the exchange runs in a helper thread instead, and an
    // instance that doesn't answer in time is left behind.
    let request = request.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("opensesh-instance-client".to_owned())
        .spawn(move || {
            let result =
                write_message(&mut stream, &request).and_then(|()| read_message(&mut stream));
            let _ = sender.send(result);
        })
        .map_err(IpcError::Io)?;
    receiver.recv_timeout(timeout).unwrap_or_else(|_| {
        Err(IpcError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "the running OpenSesh did not answer in time",
        )))
    })
}

/// A running listener; it stops accepting when the process ends.
#[derive(Debug)]
pub struct Server {
    endpoint: Endpoint,
}

impl Server {
    /// Where it listens.
    #[must_use]
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

/// Starts listening on `endpoint` in a background thread; each request goes to `handler`, whose
/// reply is sent back. A stale socket file left by a crashed instance is replaced (only after a
/// connection attempt found nobody listening).
///
/// # Errors
///
/// [`IpcError`] when the endpoint is in use by a live instance or can't be created.
pub fn serve(
    endpoint: Endpoint,
    handler: impl Fn(Request) -> Reply + Send + Sync + 'static,
) -> Result<Server, IpcError> {
    if let Some(path) = endpoint.path() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(IpcError::Io)?;
        }
    }
    let create = |overwrite: bool| {
        ListenerOptions::new()
            .name(endpoint.name()?)
            .try_overwrite(overwrite)
            .create_sync()
            .map_err(IpcError::Io)
    };
    let listener = match create(false) {
        Ok(listener) => listener,
        Err(IpcError::Io(error)) if error.kind() == std::io::ErrorKind::AddrInUse => {
            // Someone owns the name: a live instance, or a socket file left behind.
            if send(&endpoint, &Request::Activate, Duration::from_millis(300)).is_ok() {
                return Err(IpcError::Io(error));
            }
            create(true)?
        }
        Err(error) => return Err(error),
    };
    let handler = std::sync::Arc::new(handler);
    std::thread::Builder::new()
        .name("opensesh-instance".to_owned())
        .spawn(move || {
            for connection in listener.incoming() {
                let Ok(mut stream) = connection else {
                    continue;
                };
                // One thread per client, so a client that stops talking holds only its own.
                let handler = std::sync::Arc::clone(&handler);
                let spawned = std::thread::Builder::new()
                    .name("opensesh-instance-client".to_owned())
                    .spawn(move || {
                        let reply = match read_message::<Request>(&mut stream) {
                            Ok(request) => handler(request),
                            Err(error) => Reply::Error {
                                message: error.to_string(),
                            },
                        };
                        if let Err(error) = write_message(&mut stream, &reply) {
                            tracing::debug!("instance reply not delivered: {error}");
                        }
                    });
                if let Err(error) = spawned {
                    tracing::warn!("could not serve an instance request: {error}");
                }
            }
        })
        .map_err(IpcError::Io)?;
    Ok(Server { endpoint })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn messages_are_one_json_line() {
        let mut buffer = Vec::new();
        write_message(
            &mut buffer,
            &Request::Connect {
                host: "web-01".into(),
            },
        )
        .unwrap();
        assert_eq!(buffer, b"{\"op\":\"connect\",\"host\":\"web-01\"}\n");
        let back: Request = read_message(&mut buffer.as_slice()).unwrap();
        assert_eq!(
            back,
            Request::Connect {
                host: "web-01".into()
            }
        );
        let mut buffer = Vec::new();
        write_message(
            &mut buffer,
            &Reply::Error {
                message: "no".into(),
            },
        )
        .unwrap();
        assert_eq!(buffer, b"{\"status\":\"error\",\"message\":\"no\"}\n");
        assert!(read_message::<Request>(&mut b"{\"op\":\"format_disk\"}\n".as_slice()).is_err());
    }

    #[test]
    fn a_request_reaches_the_listening_instance() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::portable(dir.path());
        let endpoint = Endpoint::for_paths(&paths);
        // Nobody listens yet.
        assert!(matches!(
            send(&endpoint, &Request::Activate, Duration::from_millis(200)),
            Err(IpcError::NotRunning(_))
        ));
        let (sender, received) = mpsc::channel();
        let server = serve(endpoint.clone(), move |request| {
            let _ = sender.send(request.clone());
            match request {
                Request::Open { .. } => Reply::Error {
                    message: "refused".into(),
                },
                _ => Reply::Ok,
            }
        })
        .unwrap();
        assert_eq!(server.endpoint(), &endpoint);
        let reply = send(
            &endpoint,
            &Request::Connect { host: "web".into() },
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(reply, Reply::Ok);
        assert_eq!(
            received.recv_timeout(Duration::from_secs(2)).unwrap(),
            Request::Connect { host: "web".into() }
        );
        let reply = send(
            &endpoint,
            &Request::Open {
                url: "ssh://x".into(),
            },
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(matches!(reply, Reply::Error { .. }));
        // A second listener on the same endpoint is refused while the first one answers.
        assert!(serve(endpoint, |_| Reply::Ok).is_err());
    }

    #[test]
    fn endpoints_differ_per_config_directory() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        assert_ne!(
            Endpoint::for_paths(&AppPaths::portable(a.path())),
            Endpoint::for_paths(&AppPaths::portable(b.path()))
        );
    }
}
