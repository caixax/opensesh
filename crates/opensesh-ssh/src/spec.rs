//! What to connect to and what to run there. The app builds these from a saved host (with what
//! its groups give it and its identity) or from quick-connect text.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use opensesh_term::backend::TermSize;
use russh::keys::PrivateKey;
use secrecy::SecretString;

use crate::SshError;

/// An authentication method, in the order a host tries them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    /// Public keys: the identity's key, key files, then the agent's keys.
    PublicKey,
    /// Keyboard-interactive (MFA prompts, PAM).
    KeyboardInteractive,
    /// A password.
    Password,
}

impl AuthMethod {
    /// The order when a host doesn't set one.
    pub const DEFAULT_ORDER: [Self; 3] =
        [Self::PublicKey, Self::KeyboardInteractive, Self::Password];

    /// The name in files (`publickey`, `keyboard-interactive`, `password`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PublicKey => "publickey",
            Self::KeyboardInteractive => "keyboard-interactive",
            Self::Password => "password",
        }
    }

    /// The method called `text`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::DEFAULT_ORDER
            .into_iter()
            .find(|method| method.as_str() == text.trim())
    }
}

/// Secrets fetched when a connection needs them (the identity's password and key, from the
/// vault): never held by the connection between attempts.
#[derive(Default)]
pub struct Secrets {
    /// The password.
    pub password: Option<SecretString>,
    /// Decrypted keys.
    pub keys: Vec<Arc<PrivateKey>>,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secrets")
            .field("password", &self.password.is_some())
            .field("keys", &self.keys.len())
            .finish()
    }
}

/// Where a hop's secrets come from at connection time.
pub trait SecretSource: Send + Sync {
    /// The secrets, or why they aren't available (for example a locked vault).
    fn fetch(&self) -> Pin<Box<dyn Future<Output = Result<Secrets, SshError>> + Send + '_>>;
}

/// How to authenticate to one server. `Debug` shows no secret.
#[derive(Clone, Default)]
pub struct AuthPlan {
    /// Methods, in order (empty: [`AuthMethod::DEFAULT_ORDER`]).
    pub order: Vec<AuthMethod>,
    /// The identity's password, tried before asking.
    pub password: Option<SecretString>,
    /// Keys already decrypted (the identity's key from the vault).
    pub keys: Vec<Arc<PrivateKey>>,
    /// Key files (their passphrase is asked when they have one; a `<file>-cert.pub` next to one
    /// is used as its certificate).
    pub key_files: Vec<PathBuf>,
    /// Try the SSH agent's keys.
    pub agent: bool,
    /// Key files tried after the agent, when present (OpenSSH's default `~/.ssh/id_*`).
    pub fallback_key_files: Vec<PathBuf>,
    /// More secrets, fetched at connection time (the identity's password and key).
    pub source: Option<Arc<dyn SecretSource>>,
    /// The agent to use instead of the usual one (like OpenSSH's `IdentityAgent`): a Unix socket
    /// path, or a named pipe (`\\.\pipe\...`) on Windows.
    pub agent_socket: Option<String>,
}

impl std::fmt::Debug for AuthPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthPlan")
            .field("order", &self.order)
            .field("password", &self.password.is_some())
            .field("keys", &self.keys.len())
            .field("key_files", &self.key_files)
            .field("agent", &self.agent)
            .field("fallback_key_files", &self.fallback_key_files)
            .field("source", &self.source.is_some())
            .field("agent_socket", &self.agent_socket)
            .finish()
    }
}

impl AuthPlan {
    /// The methods to try, in order.
    #[must_use]
    pub fn methods(&self) -> Vec<AuthMethod> {
        if self.order.is_empty() {
            AuthMethod::DEFAULT_ORDER.to_vec()
        } else {
            self.order.clone()
        }
    }
}

/// One SSH server on the way.
#[derive(Debug, Clone)]
pub struct Hop {
    /// Host name or address.
    pub host: String,
    /// Port.
    pub port: u16,
    /// User name.
    pub user: String,
    /// How to authenticate.
    pub auth: AuthPlan,
}

impl Hop {
    /// `user@host:port`, for messages.
    #[must_use]
    pub fn label(&self) -> String {
        if self.port == 22 {
            format!("{}@{}", self.user, self.host)
        } else {
            format!("{}@{}:{}", self.user, self.host, self.port)
        }
    }
}

/// How the first hop is reached. `Debug` shows no password.
#[derive(Clone)]
pub enum Proxy {
    /// A SOCKS5 proxy, optionally with a user name and password.
    Socks5 {
        /// Proxy host.
        host: String,
        /// Proxy port.
        port: u16,
        /// User and password, if the proxy asks for them.
        login: Option<(String, SecretString)>,
    },
    /// An HTTP proxy that supports CONNECT, optionally with Basic authentication.
    Http {
        /// Proxy host.
        host: String,
        /// Proxy port.
        port: u16,
        /// User and password, if the proxy asks for them.
        login: Option<(String, SecretString)>,
    },
    /// A command whose standard input and output carry the connection (`%h`, `%p`, `%r` are
    /// the host, the port and the user).
    Command(String),
}

impl std::fmt::Debug for Proxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Socks5 { host, port, login } => f
                .debug_struct("Socks5")
                .field("host", host)
                .field("port", port)
                .field("login", &login.as_ref().map(|(user, _)| user))
                .finish(),
            Self::Http { host, port, login } => f
                .debug_struct("Http")
                .field("host", host)
                .field("port", port)
                .field("login", &login.as_ref().map(|(user, _)| user))
                .finish(),
            Self::Command(command) => f.debug_tuple("Command").field(command).finish(),
        }
    }
}

/// The `known_hosts` files: OpenSesh's own (read and written) and the user's (read only).
#[derive(Debug, Clone, Default)]
pub struct KnownHostsFiles {
    /// OpenSesh's file, where accepted keys go.
    pub own: PathBuf,
    /// `~/.ssh/known_hosts`, if there is a home folder.
    pub user: Option<PathBuf>,
    /// Keys trusted once, for this connection's reconnections.
    pub trusted_once: TrustedOnce,
}

/// A host, its port and a key's wire encoding.
type TrustedKey = (String, u16, Vec<u8>);

/// Host keys the user trusted once: kept in memory while the connection (and its reconnections)
/// lives, never written. Clones share the list.
#[derive(Debug, Clone, Default)]
pub struct TrustedOnce(Arc<Mutex<Vec<TrustedKey>>>);

impl TrustedOnce {
    /// Whether `key` (its wire encoding) was trusted for `host`:`port`.
    #[must_use]
    pub fn contains(&self, host: &str, port: u16, key: &[u8]) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|(known, known_port, blob)| known == host && *known_port == port && blob == key)
    }

    /// Trusts `key` for `host`:`port` from now on.
    pub fn add(&self, host: &str, port: u16, key: Vec<u8>) {
        if !self.contains(host, port, &key) {
            self.0.lock().unwrap_or_else(PoisonError::into_inner).push((
                host.to_owned(),
                port,
                key,
            ));
        }
    }
}

/// A connection: its hops (jump hosts first, the target last) and the transport options.
#[derive(Debug, Clone)]
pub struct ConnectSpec {
    /// Jump hosts in order, then the target. Never empty.
    pub hops: Vec<Hop>,
    /// How the first hop is reached.
    pub proxy: Option<Proxy>,
    /// Offer the legacy algorithms too.
    pub legacy: bool,
    /// Offer compression.
    pub compression: bool,
    /// Keepalive interval (none: off).
    pub keepalive: Option<Duration>,
    /// How long establishing each hop may take.
    pub connect_timeout: Duration,
    /// Where host keys are checked and remembered.
    pub known_hosts: KnownHostsFiles,
    /// Forward the local agent to the target (never to jump hosts).
    pub agent_forwarding: bool,
    /// The agent to forward instead of the usual one (see [`AuthPlan::agent_socket`]).
    pub agent_socket: Option<String>,
    /// X11 forwarding to the target (never to jump hosts).
    pub x11: Option<X11Spec>,
}

/// X11 forwarding: the local display and how much the remote programs are trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X11Spec {
    /// The local display (`DISPLAY`).
    pub display: String,
    /// Trusted: the display's own cookie; untrusted: one made with the X SECURITY extension's
    /// limits (`xauth generate ... untrusted`).
    pub trusted: bool,
}

impl ConnectSpec {
    /// The target (the last hop).
    ///
    /// # Errors
    ///
    /// When there are no hops.
    pub fn target(&self) -> Result<&Hop, SshError> {
        self.hops.last().ok_or_else(|| SshError::Local {
            what: "the connection".to_owned(),
            message: "no host to connect to".to_owned(),
        })
    }
}

/// Automatic reconnection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reconnect {
    /// Reconnect by itself after the connection is lost.
    pub automatic: bool,
    /// Give up after this many attempts in a row (then Enter still works).
    pub max_attempts: u32,
}

impl Default for Reconnect {
    fn default() -> Self {
        Self {
            automatic: false,
            max_attempts: 5,
        }
    }
}

/// A session log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogSpec {
    /// The file (appended to).
    pub path: PathBuf,
    /// Keep escape sequences as they came; else clean text.
    pub raw: bool,
}

/// What runs over the connection.
#[derive(Debug, Clone)]
pub struct SessionSpec {
    /// `TERM` for the PTY.
    pub term: String,
    /// The pane's size.
    pub size: TermSize,
    /// Environment variables to send (the server may refuse them).
    pub env: Vec<(String, String)>,
    /// A command instead of the login shell.
    pub command: Option<String>,
    /// Text typed into the shell once it is ready.
    pub startup: Option<String>,
    /// Reconnection.
    pub reconnect: Reconnect,
    /// A session log.
    pub log: Option<LogSpec>,
    /// Show the server's Wayland programs here through Waypipe (Sprint 15).
    pub waypipe: bool,
}

impl Default for SessionSpec {
    fn default() -> Self {
        Self {
            term: "xterm-256color".to_owned(),
            size: TermSize::default(),
            env: Vec::new(),
            command: None,
            startup: None,
            reconnect: Reconnect::default(),
            log: None,
            waypipe: false,
        }
    }
}

/// `LANG` and `LC_*` of this process, the variables OpenSSH clients usually send.
#[must_use]
pub fn locale_env() -> Vec<(String, String)> {
    let mut vars: Vec<(String, String)> = std::env::vars()
        .filter(|(name, value)| (name == "LANG" || name.starts_with("LC_")) && !value.is_empty())
        .collect();
    vars.sort();
    vars
}

/// A private key from its OpenSSH binary encoding (how the vault stores it), for `russh`.
///
/// # Errors
///
/// When the bytes aren't a key.
pub fn key_from_openssh_bytes(bytes: &[u8]) -> Result<PrivateKey, SshError> {
    PrivateKey::from_bytes(bytes).map_err(|error| SshError::Local {
        what: "reading a key from the vault".to_owned(),
        message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods() {
        assert_eq!(AuthPlan::default().methods(), AuthMethod::DEFAULT_ORDER);
        for method in AuthMethod::DEFAULT_ORDER {
            assert_eq!(AuthMethod::parse(method.as_str()), Some(method));
        }
        assert_eq!(AuthMethod::parse("hostbased"), None);
    }

    #[test]
    fn debug_hides_secrets() {
        let plan = AuthPlan {
            password: Some(SecretString::from("hunter2")),
            ..AuthPlan::default()
        };
        let proxy = Proxy::Socks5 {
            host: "p".into(),
            port: 1080,
            login: Some(("u".into(), SecretString::from("hunter3"))),
        };
        let text = format!("{plan:?} {proxy:?}");
        assert!(!text.contains("hunter"), "{text}");
    }

    #[test]
    fn labels() {
        let hop = Hop {
            host: "web".into(),
            port: 2222,
            user: "deploy".into(),
            auth: AuthPlan::default(),
        };
        assert_eq!(hop.label(), "deploy@web:2222");
    }
}
