//! SSH connections for the terminal panes (Sprint 7): a saved host (with what its groups give it
//! and its identity) or quick-connect text becomes an `opensesh_ssh` connection and session.
//!
//! The identity's password and key are not copied here: an [`IdentitySource`] asks the keychain
//! worker for them each time the connection authenticates, so a locked vault stays locked.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, LazyLock, PoisonError, RwLock};
use std::time::Duration;

use opensesh_core::hosts::target::{self, ProxyKind};
use opensesh_core::hosts::{Host, HostsFile, Protocol, SessionLog, SshBackend};
use opensesh_ssh::SshError;
use opensesh_ssh::backend::Options;
use opensesh_ssh::spec::{
    AuthMethod, AuthPlan, ConnectSpec, Hop, KnownHostsFiles, LogSpec, Proxy, Reconnect,
    SecretSource, Secrets, SessionSpec, key_from_openssh_bytes, locale_env,
};
use opensesh_term::backend::TermSize;
use opensesh_vault::known_hosts::KNOWN_HOSTS_FILE;

use crate::bridge::app_info::{is_smoke_test, is_test_run};
use crate::keychain::{self, Job};
use crate::services;

/// Everything a pane needs to start an SSH session.
#[derive(Debug, Clone)]
pub struct SshStart {
    /// Where to connect.
    pub connect: ConnectSpec,
    /// What to run.
    pub session: SessionSpec,
    /// OS detection.
    pub options: Options,
}

/// How long reaching a host may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// The port of the smoke test's SSH server (0 until it starts).
static TEST_SERVER: AtomicU16 = AtomicU16::new(0);

/// Where session logs go when Settings > SSH names a folder (else `logs/sessions` in the data
/// folder).
static LOGS_DIR: LazyLock<RwLock<Option<PathBuf>>> = LazyLock::new(|| RwLock::new(None));

/// Applies the `[ssh]` settings: the defaults of every host and the session logs folder.
pub fn apply_settings(settings: &opensesh_core::config::SshSettings) {
    crate::hosts::set_defaults(settings.host_defaults());
    let folder = settings.logs_dir.trim();
    let folder = (!folder.is_empty()).then(|| match opensesh_core::paths::home_dir() {
        Some(home) => opensesh_core::paths::expand_tilde(folder, &home),
        None => PathBuf::from(folder),
    });
    *LOGS_DIR.write().unwrap_or_else(PoisonError::into_inner) = folder;
}

/// The folder session logs go to.
#[must_use]
pub fn logs_dir(data: &Path) -> PathBuf {
    LOGS_DIR
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .unwrap_or_else(|| data.join("logs").join("sessions"))
}

/// Sends every SSH connection of the smoke test to its server on `port`.
pub fn set_test_server(port: u16) {
    TEST_SERVER.store(port, Ordering::Relaxed);
}

/// The smoke test never reaches the network: every hop goes to its test server, as its user.
fn hermetic(mut connect: ConnectSpec) -> Result<ConnectSpec, String> {
    if !is_smoke_test() {
        return Ok(connect);
    }
    let port = TEST_SERVER.load(Ordering::Relaxed);
    if port == 0 {
        return Err("the smoke test connects only to its own SSH server".to_owned());
    }
    connect.proxy = None;
    for hop in &mut connect.hops {
        "127.0.0.1".clone_into(&mut hop.host);
        hop.port = port;
        opensesh_ssh::testing::USER.clone_into(&mut hop.user);
    }
    Ok(connect)
}

/// An identity's secrets, fetched from the keychain worker when needed.
struct IdentitySource {
    identity: String,
}

impl SecretSource for IdentitySource {
    fn fetch(&self) -> Pin<Box<dyn Future<Output = Result<Secrets, SshError>> + Send + '_>> {
        Box::pin(async move {
            let (reply, answer) = tokio::sync::oneshot::channel();
            let unavailable = || SshError::Local {
                what: "the keychain".to_owned(),
                message: "it is not available".to_owned(),
            };
            if !keychain::request(Job::ConnectionSecrets {
                identity: self.identity.clone(),
                reply,
            }) {
                return Err(unavailable());
            }
            match answer.await.map_err(|_| unavailable())? {
                Ok(secrets) => {
                    let keys = match &secrets.key {
                        Some(bytes) => vec![Arc::new(key_from_openssh_bytes(bytes)?)],
                        None => Vec::new(),
                    };
                    Ok(Secrets {
                        password: secrets.password,
                        keys,
                    })
                }
                Err("locked") => Err(SshError::SecretsLocked),
                // A missing identity leaves the other methods to try.
                Err(_) => Ok(Secrets::default()),
            }
        })
    }
}

/// The local user name, which SSH uses when no user is set.
fn local_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|user| !user.trim().is_empty())
        .unwrap_or_else(|| "root".to_owned())
}

/// OpenSSH's default keys (`~/.ssh/id_ed25519`, `id_ecdsa`, `id_rsa`), tried after the agent.
/// None in test runs, which leave the user's keys alone.
fn default_keys(home: &Path) -> Vec<PathBuf> {
    if is_test_run() {
        return Vec::new();
    }
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .iter()
        .map(|name| home.join(".ssh").join(name))
        .collect()
}

/// OpenSesh's `known_hosts` and the user's. Test runs read neither (and a key they remember goes
/// to the temporary folder).
fn known_hosts(config: &Path, home: Option<&Path>) -> KnownHostsFiles {
    if is_test_run() {
        return KnownHostsFiles {
            own: std::env::temp_dir().join("opensesh-test-known_hosts"),
            ..KnownHostsFiles::default()
        };
    }
    KnownHostsFiles {
        own: config.join(KNOWN_HOSTS_FILE),
        user: home.map(|home| home.join(".ssh").join("known_hosts")),
        ..KnownHostsFiles::default()
    }
}

/// A hop for a saved host: its address, what it and its groups say, its identity.
fn host_hop(file: &HostsFile, host: &Host, home: Option<&Path>) -> Hop {
    let resolved = file.resolve(host);
    let identity = resolved.identity().map(str::to_owned);
    let user = resolved
        .user()
        .map(str::to_owned)
        .or_else(|| identity.as_deref().and_then(keychain::identity_user))
        .unwrap_or_else(local_user);
    let order: Vec<AuthMethod> = resolved
        .list("ssh.auth_order")
        .iter()
        .filter_map(|name| AuthMethod::parse(name))
        .collect();
    let key_files = resolved
        .string("identity_file")
        .map(|file| match home {
            Some(home) => opensesh_core::paths::expand_tilde(file, home),
            None => PathBuf::from(file),
        })
        .into_iter()
        .collect();
    Hop {
        host: host.address.trim().to_owned(),
        port: resolved.port().unwrap_or(22),
        user,
        auth: AuthPlan {
            order,
            key_files,
            agent: !is_test_run(),
            agent_socket: resolved.string("ssh.agent_socket").map(str::to_owned),
            fallback_key_files: home.map(default_keys).unwrap_or_default(),
            source: identity
                .map(|identity| Arc::new(IdentitySource { identity }) as Arc<dyn SecretSource>),
            ..AuthPlan::default()
        },
    }
}

/// A hop for `[user@]host[:port]` text: the defaults.
fn endpoint_hop(text: &str, home: Option<&Path>) -> Result<Hop, String> {
    let parsed = target::parse(text).map_err(|error| error.to_string())?;
    Ok(Hop {
        host: parsed.host,
        port: parsed.port.unwrap_or(22),
        user: parsed.user.unwrap_or_else(local_user),
        auth: AuthPlan {
            agent: !is_test_run(),
            fallback_key_files: home.map(default_keys).unwrap_or_default(),
            ..AuthPlan::default()
        },
    })
}

/// A jump reference: a saved host (by id or name), else `[user@]host[:port]`.
fn jump_hop(file: &HostsFile, reference: &str, home: Option<&Path>) -> Result<Hop, String> {
    match file.find_host(reference) {
        Some(host) if !host.address.trim().is_empty() => Ok(host_hop(file, host, home)),
        _ => endpoint_hop(reference, home),
    }
}

fn proxy_of(text: Option<&str>, command: Option<&str>) -> Result<Option<Proxy>, String> {
    if let Some(command) = command {
        return Ok(Some(Proxy::Command(command.to_owned())));
    }
    let Some(text) = text else {
        return Ok(None);
    };
    let url = target::parse_proxy(text).map_err(|error| error.to_string())?;
    // A proxy password isn't asked for yet: a user name alone is sent as written.
    Ok(Some(match url.kind {
        ProxyKind::Socks5 => Proxy::Socks5 {
            host: url.host,
            port: url.port,
            login: None,
        },
        ProxyKind::Http => Proxy::Http {
            host: url.host,
            port: url.port,
            login: None,
        },
    }))
}

/// A file name part made of the host's name (letters, digits, `-` and `_`).
fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "session".to_owned()
    } else {
        cleaned
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Whether saved host `id` connects with the built-in client (SSH, not the OpenSSH backend).
#[must_use]
pub fn is_internal(id: &str) -> bool {
    let library = crate::hosts::current();
    library.file.host(id).is_some_and(|host| {
        host.protocol == Protocol::Ssh
            && library.file.resolve(host).string("ssh.backend")
                != Some(SshBackend::Openssh.as_str())
    })
}

/// The session of saved host `id` in a pane of `size` with `term`.
///
/// # Errors
///
/// A message when the host is unknown, not SSH, or has a bad jump or proxy.
pub fn for_host(id: &str, size: TermSize, term: &str) -> Result<SshStart, String> {
    let library = crate::hosts::current();
    let host = library.file.host(id).ok_or("the host no longer exists")?;
    session_for(&library.file, host, size, term)
}

/// The session of `host` (saved, or made from quick-connect text) with what it, its groups and
/// the app's defaults say.
fn session_for(
    file: &HostsFile,
    host: &Host,
    size: TermSize,
    term: &str,
) -> Result<SshStart, String> {
    let services = services::get().ok_or("the app isn't ready")?;
    let config = services.paths.config_dir();
    let data = services.paths.data_dir();
    let home = opensesh_core::paths::home_dir();
    let resolved = file.resolve(host);
    let mut hops = Vec::new();
    for reference in resolved.jump() {
        hops.push(jump_hop(file, &reference, home.as_deref())?);
    }
    hops.push(host_hop(file, host, home.as_deref()));
    let keepalive = u64::from(resolved.keepalive_secs());
    let connect = ConnectSpec {
        hops,
        proxy: proxy_of(
            resolved.string("ssh.proxy"),
            resolved.string("ssh.proxy_command"),
        )?,
        legacy: resolved.flag("ssh.legacy_algorithms"),
        compression: resolved.flag("ssh.compression"),
        keepalive: (keepalive > 0).then(|| Duration::from_secs(keepalive)),
        connect_timeout: CONNECT_TIMEOUT,
        known_hosts: known_hosts(config, home.as_deref()),
        agent_forwarding: resolved.flag("ssh.agent_forwarding"),
        agent_socket: resolved.string("ssh.agent_socket").map(str::to_owned),
    };
    let mut env: Vec<(String, String)> = if resolved.flag("ssh.send_locale") {
        locale_env()
    } else {
        Vec::new()
    };
    env.extend(resolved.string_map("ssh.env"));
    let log = match resolved.session_log() {
        SessionLog::Off => None,
        mode => Some(LogSpec {
            path: logs_dir(data).join(format!(
                "{}_{}.log",
                safe_name(&host.name),
                opensesh_ssh::log::timestamp(now_secs())
            )),
            raw: mode == SessionLog::Raw,
        }),
    };
    let session = SessionSpec {
        term: term.to_owned(),
        size,
        env,
        command: resolved.string("ssh.command").map(str::to_owned),
        startup: resolved.string("ssh.startup_snippet").map(str::to_owned),
        reconnect: Reconnect {
            automatic: resolved.flag("ssh.auto_reconnect"),
            ..Reconnect::default()
        },
        log,
    };
    // Only a saved host has an icon to show the OS with.
    let auto_icon = !host.id.is_empty() && (host.icon.is_empty() || host.icon == "auto");
    Ok(SshStart {
        connect: hermetic(connect)?,
        session,
        options: Options {
            detect_os: auto_icon && resolved.flag("ssh.detect_os"),
            install_key: None,
        },
    })
}

/// The session of quick-connect `text` (`user@host:port -J jump`).
///
/// # Errors
///
/// A message when the text isn't an SSH target.
pub fn for_target(text: &str, size: TermSize, term: &str) -> Result<SshStart, String> {
    let parsed = target::parse(text).map_err(|error| error.to_string())?;
    if parsed.protocol != Protocol::Ssh {
        return Err(format!("{} isn't an SSH target", parsed.protocol.as_str()));
    }
    let library = crate::hosts::current();
    // An unsaved host: no group, so the app's defaults apply.
    let host = Host {
        name: parsed.host.clone(),
        address: parsed.host,
        port: parsed.port,
        user: parsed.user,
        jump: Some(parsed.jump),
        ..Host::default()
    };
    session_for(&library.file, &host, size, term)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_for_log_files() {
        assert_eq!(safe_name("web-01 (prod)"), "web-01__prod_");
        assert_eq!(safe_name(""), "session");
    }

    #[test]
    fn proxies() {
        assert!(matches!(
            proxy_of(Some("socks5://p:1080"), None),
            Ok(Some(Proxy::Socks5 { port: 1080, .. }))
        ));
        assert!(matches!(
            proxy_of(Some("http://p"), Some("nc %h %p")),
            Ok(Some(Proxy::Command(_)))
        ));
        assert!(proxy_of(Some("ftp://p"), None).is_err());
        assert!(matches!(proxy_of(None, None), Ok(None)));
    }
}
