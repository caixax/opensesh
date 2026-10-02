//! OpenSesh's SSH client (PLAN §3, Sprint 7, ADR 0027), on `russh`. This crate never depends on
//! Qt.
//!
//! - [`spec`]: what to connect to (hops, authentication, proxy, algorithms) and what to run.
//! - [`prompt`]: the questions a connection asks the user (host keys, passwords, MFA codes).
//! - [`algorithms`]: modern defaults and the per-host legacy set.
//! - [`proxy`]: SOCKS5, HTTP CONNECT and ProxyCommand for the first hop.
//! - [`connect`]: host key checks, authentication, and jump host chains.
//! - [`backend`]: a terminal backend over an SSH session channel, with reconnection.
//! - [`log`], [`osdetect`], [`copy_id`]: session logs, the remote OS, installing a public key.
//! - [`monitor`]: the remote monitor (CPU, memory, network, disks...) and the host info.
//! - [`mosh`]: starting a mosh server for `mosh-client`.
//! - [`sftp`]: files over SSH, and the transfer queue.
//! - [`testing`]: a tiny SSH server for tests and the app's smoke test.
//!
//! Everything runs on one tokio runtime ([`runtime`]) off the GUI thread. Secrets (passwords,
//! passphrases, answers to prompts, private keys) are never logged or put in error messages.

pub mod algorithms;
pub mod backend;
pub mod connect;
pub mod copy_id;
pub mod log;
pub mod monitor;
pub mod mosh;
pub mod osdetect;
pub mod prompt;
pub mod proxy;
pub mod sftp;
pub mod spec;
pub mod testing;
pub mod tunnel;
pub mod waypipe;
pub mod x11;

use std::sync::OnceLock;

/// Why a connection failed. Messages never contain secrets.
#[derive(Debug, thiserror::Error)]
pub enum SshError {
    /// A name didn't resolve, a TCP connection or the proxy failed.
    #[error("could not reach {target}: {message}")]
    Network {
        /// `host:port`.
        target: String,
        /// Why.
        message: String,
    },
    /// The user refused the server's key, or it is revoked or changed and wasn't accepted.
    #[error("the host key of {host} was not accepted: {reason}")]
    HostKey {
        /// The host.
        host: String,
        /// Why.
        reason: String,
    },
    /// No authentication method worked.
    #[error("authentication to {target} failed ({tried})")]
    Auth {
        /// `user@host`.
        target: String,
        /// The methods tried.
        tried: String,
    },
    /// The user cancelled a prompt.
    #[error("cancelled")]
    Cancelled,
    /// The saved secrets (the vault) are locked: unlock them and try again.
    #[error("the vault is locked: unlock it, then press Enter to connect")]
    SecretsLocked,
    /// The SSH protocol failed.
    #[error("SSH error: {0}")]
    Protocol(String),
    /// Opening or setting up a channel failed.
    #[error("the server refused {what}")]
    Refused {
        /// What was asked (a session, a jump, a PTY...).
        what: String,
    },
    /// A timeout.
    #[error("{what} timed out")]
    Timeout {
        /// What.
        what: String,
    },
    /// Local I/O (a key file, the agent, a proxy command).
    #[error("{what}: {message}")]
    Local {
        /// What was being done.
        what: String,
        /// Why.
        message: String,
    },
}

impl From<russh::Error> for SshError {
    fn from(error: russh::Error) -> Self {
        Self::Protocol(error.to_string())
    }
}

impl SshError {
    /// A short code for the UI to word.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Network { .. } => "network",
            Self::HostKey { .. } => "host-key",
            Self::Auth { .. } => "auth",
            Self::Cancelled => "cancelled",
            Self::SecretsLocked => "locked",
            Self::Protocol(_) => "protocol",
            Self::Refused { .. } => "refused",
            Self::Timeout { .. } => "timeout",
            Self::Local { .. } => "local",
        }
    }
}

static RUNTIME: OnceLock<Option<tokio::runtime::Runtime>> = OnceLock::new();

/// The runtime every SSH connection runs on (two worker threads; started on first use).
/// `None` if it could not be started (logged).
pub fn runtime() -> Option<&'static tokio::runtime::Runtime> {
    RUNTIME
        .get_or_init(|| {
            match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("ssh")
                .enable_all()
                .build()
            {
                Ok(runtime) => Some(runtime),
                Err(error) => {
                    tracing::error!("could not start the SSH runtime: {error}");
                    None
                }
            }
        })
        .as_ref()
}
