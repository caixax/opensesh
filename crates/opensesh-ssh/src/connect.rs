//! Establishing a connection (PLAN Sprint 7, §8): each hop is reached (directly, through the
//! proxy, or through a `direct-tcpip` channel of the hop before it), its host key is checked
//! against `known_hosts`, and the user is authenticated with the hop's methods in order.
//!
//! - **Host keys:** a known key passes; a new key asks the user (trust once, or trust and remember
//!   in OpenSesh's file); a changed key asks too, as a blocking warning with the old and new
//!   fingerprints; a revoked key is refused without asking.
//! - **Authentication:** after `none` (which tells which methods the server offers), the hop's
//!   methods run in order. A partial success (MFA: a key, then a code) starts over with what the
//!   server still wants. Public keys are the identity's vault keys, key files (with their
//!   passphrase asked, and a `-cert.pub` certificate next to them), then the agent's keys.
//! - **Forwardings the user didn't ask for** (agent, X11) are refused: russh's default accepts
//!   them.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use opensesh_vault::known_hosts::{self, HostKeyStatus, KnownHosts};
use russh::client::{self, AuthResult, Handle, Handler, KeyboardInteractiveAuthResponse, Msg};
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::AgentClient;
use russh::keys::{PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{Channel, ChannelOpenFailure, MethodKind, MethodSet};
use secrecy::{ExposeSecret, SecretString};

use crate::SshError;
use crate::algorithms;
use crate::prompt::{self, Answer, Asker, Field, HostKeyKind, HostKeyQuestion, Prompt};
use crate::proxy::{self, Transport};
use crate::spec::{AuthMethod, ConnectSpec, Hop, KnownHostsFiles, Proxy};
use crate::tunnel;

/// Progress of a connection, for the pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// Reaching hop `index` (0-based) of `count`.
    Connecting {
        /// Which hop.
        index: usize,
        /// How many.
        count: usize,
        /// `user@host:port`.
        label: String,
    },
    /// Authenticating to a hop.
    Authenticating {
        /// `user@host:port`.
        label: String,
    },
    /// The server's authentication banner.
    Banner(String),
}

/// Where a connection reports its progress.
pub type Notes = Arc<dyn Fn(Note) + Send + Sync>;

/// A note sink that drops everything.
#[must_use]
pub fn quiet() -> Notes {
    Arc::new(|_note| {})
}

/// The client side of one hop.
pub struct ClientHandler {
    host: String,
    port: u16,
    known_hosts: KnownHostsFiles,
    asker: Asker,
    notes: Notes,
    /// Why the host key was refused, for the error message.
    rejection: Arc<Mutex<Option<String>>>,
    agent_forwarding: bool,
    agent_socket: Option<String>,
    /// Where the server's `forwarded-tcpip` channels go (remote forwards; the target hop only).
    routes: tunnel::Routes,
}

impl std::fmt::Debug for ClientHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientHandler")
            .field("host", &self.host)
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

fn read_known(files: &KnownHostsFiles) -> Vec<(PathBuf, KnownHosts)> {
    let mut out = Vec::new();
    for path in std::iter::once(&files.own).chain(files.user.iter()) {
        match known_hosts::read(path) {
            Ok(known) => out.push((path.clone(), known)),
            Err(error) => {
                tracing::warn!(path = %path.display(), "could not read known hosts: {error}")
            }
        }
    }
    out
}

impl ClientHandler {
    fn reject(&self, reason: impl Into<String>) {
        *self
            .rejection
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(reason.into());
    }

    async fn decide(&self, key: &russh::keys::PublicKey) -> bool {
        let key_type = key.algorithm().as_str().to_owned();
        let Ok(blob) = key.to_bytes() else {
            self.reject("the key could not be encoded");
            return false;
        };
        let fingerprint = known_hosts::fingerprint(&blob);
        let files = read_known(&self.known_hosts);
        let kind = match known_hosts::check(&files, &self.host, self.port, &key_type, &blob) {
            HostKeyStatus::Known { .. } => return true,
            HostKeyStatus::Revoked { file, line } => {
                self.reject(format!(
                    "the key {fingerprint} is revoked ({}, line {line})",
                    file.display()
                ));
                return false;
            }
            _ if self
                .known_hosts
                .trusted_once
                .contains(&self.host, self.port, &blob) =>
            {
                return true;
            }
            HostKeyStatus::New { other_types } => HostKeyKind::New { other_types },
            HostKeyStatus::Changed {
                file,
                line,
                known_fingerprint,
            } => HostKeyKind::Changed {
                known_fingerprint,
                file: file.display().to_string(),
                line,
            },
        };
        let changed = matches!(kind, HostKeyKind::Changed { .. });
        let question = HostKeyQuestion {
            host: self.host.clone(),
            port: self.port,
            key_type: key_type.clone(),
            fingerprint: fingerprint.clone(),
            kind,
        };
        match prompt::ask(&self.asker, Prompt::HostKey(question)).await {
            Answer::TrustOnce => {
                self.known_hosts
                    .trusted_once
                    .add(&self.host, self.port, blob);
                true
            }
            Answer::TrustAndRemember => {
                let path = self.known_hosts.own.clone();
                let (host, port) = (self.host.clone(), self.port);
                let saved = tokio::task::spawn_blocking(move || {
                    known_hosts::remember(&path, &host, port, &key_type, &blob)
                })
                .await;
                match saved {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => tracing::warn!("could not remember the host key: {error}"),
                    Err(error) => tracing::warn!("could not remember the host key: {error}"),
                }
                true
            }
            _ => {
                self.reject(if changed {
                    format!("the key changed (now {fingerprint}) and was not accepted")
                } else {
                    format!("the key {fingerprint} was not trusted")
                });
                false
            }
        }
    }
}

impl Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // A host certificate is checked as its key: no certificate authority is trusted yet.
        Ok(self.decide(&server_public_key.public_key()).await)
    }

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        (self.notes)(Note::Banner(banner.to_owned()));
        Ok(())
    }

    async fn server_channel_open_agent_forward(
        &mut self,
        channel: Channel<Msg>,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        if !self.agent_forwarding {
            reply
                .reject(ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        }
        reply.accept().await;
        tokio::spawn(forward_to_agent(channel, self.agent_socket.clone()));
        Ok(())
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        _connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        // Only for a remote forward this client asked for.
        match self.routes.get(connected_port) {
            Some(route) => {
                reply.accept().await;
                tokio::spawn(tunnel::serve_forwarded(channel, route));
            }
            None => {
                reply
                    .reject(ChannelOpenFailure::AdministrativelyProhibited)
                    .await;
            }
        }
        Ok(())
    }

    async fn server_channel_open_x11(
        &mut self,
        _channel: Channel<Msg>,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        reply
            .reject(ChannelOpenFailure::AdministrativelyProhibited)
            .await;
        Ok(())
    }
}

/// Carries a forwarded agent channel to the local agent (the first one that answers).
async fn forward_to_agent(channel: Channel<Msg>, socket: Option<String>) {
    let mut stream = channel.into_stream();
    let result = match local_agent_streams(socket.as_deref()).await {
        Ok(mut agents) if !agents.is_empty() => {
            let mut agent = agents.swap_remove(0);
            tokio::io::copy_bidirectional(&mut stream, &mut agent)
                .await
                .map(|_| ())
        }
        Ok(_) => Err(std::io::Error::other("no SSH agent")),
        Err(error) => Err(std::io::Error::other(error.to_string())),
    };
    if let Err(error) = result {
        tracing::debug!("agent forwarding ended: {error}");
    }
}

/// Streams to the local agents, in the order they are tried: `socket` when given, else
/// SSH_AUTH_SOCK on Unix; on Windows SSH_AUTH_SOCK's named pipe, or else the OpenSSH agent's pipe
/// and Pageant (each one that is running). An error when none can be reached.
async fn local_agent_streams(socket: Option<&str>) -> Result<Vec<Box<dyn Transport>>, SshError> {
    let local = |message: String| SshError::Local {
        what: "the SSH agent".to_owned(),
        message,
    };
    #[cfg(unix)]
    {
        let path = match socket {
            Some(socket) => std::ffi::OsString::from(socket),
            None => std::env::var_os("SSH_AUTH_SOCK")
                .filter(|value| !value.is_empty())
                .ok_or_else(|| local("SSH_AUTH_SOCK is not set".to_owned()))?,
        };
        let stream = tokio::net::UnixStream::connect(path)
            .await
            .map_err(|error| local(error.to_string()))?;
        Ok(vec![Box::new(stream)])
    }
    #[cfg(windows)]
    {
        let named = socket
            .map(str::to_owned)
            .or_else(|| std::env::var("SSH_AUTH_SOCK").ok())
            .filter(|value| value.starts_with(r"\\.\pipe\") || value.starts_with("//./pipe/"))
            .map(|value| value.replace('/', r"\"));
        let open = |pipe: &str| tokio::net::windows::named_pipe::ClientOptions::new().open(pipe);
        if let Some(pipe) = named {
            let stream = open(&pipe).map_err(|error| local(error.to_string()))?;
            return Ok(vec![Box::new(stream)]);
        }
        let mut streams: Vec<Box<dyn Transport>> = Vec::new();
        let mut problems = Vec::new();
        match open(opensesh_vault::agent::OPENSSH_PIPE) {
            Ok(stream) => streams.push(Box::new(stream)),
            Err(error) => problems.push(format!("OpenSSH agent: {error}")),
        }
        match pageant::PageantStream::new().await {
            Ok(stream) => streams.push(Box::new(stream)),
            Err(error) => problems.push(format!("Pageant: {error}")),
        }
        if streams.is_empty() {
            return Err(local(problems.join("; ")));
        }
        Ok(streams)
    }
}

/// How a method went.
enum Step {
    Success,
    /// Accepted, but the server wants more (these methods).
    Partial(MethodSet),
    /// Refused (the server still offers these methods).
    Failed(MethodSet),
}

impl From<AuthResult> for Step {
    fn from(result: AuthResult) -> Self {
        match result {
            AuthResult::Success => Self::Success,
            AuthResult::Failure {
                remaining_methods,
                partial_success: true,
            } => Self::Partial(remaining_methods),
            AuthResult::Failure {
                remaining_methods, ..
            } => Self::Failed(remaining_methods),
        }
    }
}

fn kind(method: AuthMethod) -> MethodKind {
    match method {
        AuthMethod::PublicKey => MethodKind::PublicKey,
        AuthMethod::KeyboardInteractive => MethodKind::KeyboardInteractive,
        AuthMethod::Password => MethodKind::Password,
    }
}

fn offers(remaining: &MethodSet, method: AuthMethod) -> bool {
    // An empty list says nothing: try.
    remaining.is_empty() || remaining.contains(&kind(method))
}

/// Authenticates `hop` on `handle`.
///
/// # Errors
///
/// [`SshError::Auth`] when every method failed, [`SshError::Cancelled`] when the user cancelled a
/// prompt, and protocol errors.
pub async fn authenticate(
    handle: &mut Handle<ClientHandler>,
    hop: &Hop,
    asker: &Asker,
) -> Result<(), SshError> {
    // Secrets from the source join the plan for this attempt only.
    let fetched;
    let hop = match &hop.auth.source {
        Some(source) => {
            let secrets = source.fetch().await?;
            let mut with = hop.clone();
            with.auth.source = None;
            if with.auth.password.is_none() {
                with.auth.password = secrets.password;
            }
            let mut keys = secrets.keys;
            keys.append(&mut with.auth.keys);
            with.auth.keys = keys;
            fetched = with;
            &fetched
        }
        None => hop,
    };
    let mut remaining = match handle.authenticate_none(&hop.user).await? {
        AuthResult::Success => return Ok(()),
        AuthResult::Failure {
            remaining_methods, ..
        } => remaining_methods,
    };
    let methods = hop.auth.methods();
    let mut done: Vec<AuthMethod> = Vec::new();
    let mut tried: Vec<&str> = Vec::new();
    let mut session = AuthSession::new(hop);
    while let Some(method) = methods
        .iter()
        .copied()
        .find(|method| !done.contains(method) && offers(&remaining, *method))
    {
        done.push(method);
        tried.push(method.as_str());
        let step = match method {
            AuthMethod::PublicKey => session.public_keys(handle, asker).await?,
            AuthMethod::KeyboardInteractive => session.keyboard(handle, asker).await?,
            AuthMethod::Password => session.password(handle, asker).await?,
        };
        match step {
            Step::Success => return Ok(()),
            Step::Partial(next) => {
                // Accepted: the server wants another method (MFA). Start over with what it asks.
                remaining = next;
                done.retain(|other| *other == method);
            }
            Step::Failed(next) => remaining = next,
        }
    }
    Err(SshError::Auth {
        target: hop.label(),
        tried: if tried.is_empty() {
            format!("the server offers {}", describe(&remaining))
        } else {
            tried.join(", ")
        },
    })
}

fn describe(methods: &MethodSet) -> String {
    let names: Vec<&str> = methods.iter().map(<&str>::from).collect();
    if names.is_empty() {
        "no method".to_owned()
    } else {
        names.join(", ")
    }
}

/// State of the authentication of one hop.
struct AuthSession<'a> {
    hop: &'a Hop,
    /// The identity's password was already tried (by password or keyboard-interactive).
    password_used: bool,
}

impl<'a> AuthSession<'a> {
    fn new(hop: &'a Hop) -> Self {
        Self {
            hop,
            password_used: false,
        }
    }

    async fn rsa_hash(
        handle: &Handle<ClientHandler>,
        key: &PrivateKey,
    ) -> Result<Option<russh::keys::HashAlg>, SshError> {
        if key.algorithm().is_rsa() {
            Ok(handle.best_supported_rsa_hash().await?.flatten())
        } else {
            Ok(None)
        }
    }

    async fn public_keys(
        &mut self,
        handle: &mut Handle<ClientHandler>,
        asker: &Asker,
    ) -> Result<Step, SshError> {
        let user = self.hop.user.clone();
        let mut last = Step::Failed(MethodSet::empty());
        for key in &self.hop.auth.keys {
            let hash = Self::rsa_hash(handle, key).await?;
            let result = handle
                .authenticate_publickey(&user, PrivateKeyWithHashAlg::new(Arc::clone(key), hash))
                .await?;
            match Step::from(result) {
                Step::Failed(next) => last = Step::Failed(next),
                other => return Ok(other),
            }
        }
        for file in &self.hop.auth.key_files {
            let Some(key) = load_key_file(file, asker).await? else {
                continue;
            };
            let key = Arc::new(key);
            let certificate = certificate_for(file);
            let result = match certificate {
                Some(cert) => {
                    handle
                        .authenticate_openssh_cert(&user, Arc::clone(&key), cert)
                        .await?
                }
                None => {
                    let hash = Self::rsa_hash(handle, &key).await?;
                    handle
                        .authenticate_publickey(&user, PrivateKeyWithHashAlg::new(key, hash))
                        .await?
                }
            };
            match Step::from(result) {
                Step::Failed(next) => last = Step::Failed(next),
                other => return Ok(other),
            }
        }
        if self.hop.auth.agent {
            match agent_keys(handle, &user, self.hop.auth.agent_socket.as_deref()).await {
                Ok(Some(Step::Failed(next))) => last = Step::Failed(next),
                Ok(Some(step)) => return Ok(step),
                Ok(None) => {}
                Err(error) => tracing::info!("the SSH agent could not be used: {error}"),
            }
        }
        for file in &self.hop.auth.fallback_key_files {
            if !file.is_file() {
                continue;
            }
            let Some(key) = load_key_file(file, asker).await? else {
                continue;
            };
            let key = Arc::new(key);
            let hash = Self::rsa_hash(handle, &key).await?;
            let result = handle
                .authenticate_publickey(&user, PrivateKeyWithHashAlg::new(key, hash))
                .await?;
            match Step::from(result) {
                Step::Failed(next) => last = Step::Failed(next),
                other => return Ok(other),
            }
        }
        Ok(last)
    }

    async fn keyboard(
        &mut self,
        handle: &mut Handle<ClientHandler>,
        asker: &Asker,
    ) -> Result<Step, SshError> {
        let mut response = handle
            .authenticate_keyboard_interactive_start(&self.hop.user, None::<String>)
            .await?;
        loop {
            match response {
                KeyboardInteractiveAuthResponse::Success => return Ok(Step::Success),
                KeyboardInteractiveAuthResponse::Failure {
                    remaining_methods,
                    partial_success,
                } => {
                    return Ok(if partial_success {
                        Step::Partial(remaining_methods)
                    } else {
                        Step::Failed(remaining_methods)
                    });
                }
                KeyboardInteractiveAuthResponse::InfoRequest {
                    name,
                    instructions,
                    prompts,
                } => {
                    let answers: Vec<String> = if prompts.is_empty() {
                        Vec::new()
                    } else if let Some(password) = self.password_for(&prompts) {
                        vec![password]
                    } else {
                        let fields = prompts
                            .iter()
                            .map(|prompt| Field {
                                label: prompt.prompt.clone(),
                                echo: prompt.echo,
                            })
                            .collect();
                        let question = Prompt::KeyboardInteractive {
                            target: self.hop.label(),
                            name,
                            instructions,
                            fields,
                        };
                        match prompt::ask(asker, question).await {
                            Answer::Secrets(secrets) => secrets
                                .iter()
                                .map(|secret| secret.expose_secret().to_owned())
                                .collect(),
                            _ => return Err(SshError::Cancelled),
                        }
                    };
                    response = handle
                        .authenticate_keyboard_interactive_respond(answers)
                        .await?;
                }
            }
        }
    }

    /// The identity's password, once, for a single masked prompt that asks for a password.
    fn password_for(&mut self, prompts: &[client::Prompt]) -> Option<String> {
        let [prompt] = prompts else {
            return None;
        };
        let password = self.hop.auth.password.as_ref()?;
        if self.password_used || prompt.echo || !prompt.prompt.to_lowercase().contains("password") {
            return None;
        }
        self.password_used = true;
        Some(password.expose_secret().to_owned())
    }

    async fn password(
        &mut self,
        handle: &mut Handle<ClientHandler>,
        asker: &Asker,
    ) -> Result<Step, SshError> {
        let mut last = Step::Failed(MethodSet::empty());
        for attempt in 0..3 {
            let password: SecretString = match (&self.hop.auth.password, self.password_used) {
                (Some(password), false) => {
                    self.password_used = true;
                    password.clone()
                }
                _ => {
                    let question = Prompt::Password {
                        target: self.hop.label(),
                        retry: attempt > 0,
                    };
                    match prompt::ask(asker, question).await {
                        Answer::Secrets(mut secrets) if !secrets.is_empty() => secrets.remove(0),
                        _ => return Err(SshError::Cancelled),
                    }
                }
            };
            let result = handle
                .authenticate_password(&self.hop.user, password.expose_secret())
                .await?;
            match Step::from(result) {
                Step::Failed(next) => {
                    let still_offered = offers(&next, AuthMethod::Password);
                    last = Step::Failed(next);
                    if !still_offered {
                        break;
                    }
                }
                other => return Ok(other),
            }
        }
        Ok(last)
    }
}

/// A key file, asking for its passphrase (three tries) when it has one. `None` when the file is
/// missing or the user skipped it.
async fn load_key_file(path: &Path, asker: &Asker) -> Result<Option<PrivateKey>, SshError> {
    let text = match tokio::fs::read_to_string(path).await {
        Ok(text) => zeroize::Zeroizing::new(text),
        Err(error) => {
            tracing::info!(path = %path.display(), "key file not used: {error}");
            return Ok(None);
        }
    };
    match russh::keys::decode_secret_key(&text, None) {
        Ok(key) => return Ok(Some(key)),
        Err(russh::keys::Error::KeyIsEncrypted) => {}
        Err(error) => {
            tracing::info!(path = %path.display(), "key file not used: {error}");
            return Ok(None);
        }
    }
    for attempt in 0..3 {
        let question = Prompt::Passphrase {
            key: path.display().to_string(),
            retry: attempt > 0,
        };
        let passphrase = match prompt::ask(asker, question).await {
            Answer::Secrets(mut secrets) if !secrets.is_empty() => secrets.remove(0),
            // Skipping one key is not cancelling the connection: the next method may work.
            _ => return Ok(None),
        };
        if let Ok(key) = russh::keys::decode_secret_key(&text, Some(passphrase.expose_secret())) {
            return Ok(Some(key));
        }
    }
    Ok(None)
}

/// `<key>-cert.pub` next to a key file, if there is one.
fn certificate_for(path: &Path) -> Option<russh::keys::Certificate> {
    let mut name = path.file_name()?.to_os_string();
    name.push("-cert.pub");
    let cert = path.with_file_name(name);
    if !cert.is_file() {
        return None;
    }
    match russh::keys::load_openssh_certificate(&cert) {
        Ok(cert) => Some(cert),
        Err(error) => {
            tracing::info!(path = %cert.display(), "certificate not used: {error}");
            None
        }
    }
}

/// Tries every key of the local agent. `None` when there is no agent or no key worked.
async fn agent_keys(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    socket: Option<&str>,
) -> Result<Option<Step>, SshError> {
    let agent_error = |error: russh::keys::Error| SshError::Local {
        what: "the SSH agent".to_owned(),
        message: error.to_string(),
    };
    let mut last = None;
    for stream in local_agent_streams(socket).await? {
        let mut agent = AgentClient::connect(stream);
        let identities = agent.request_identities().await.map_err(agent_error)?;
        for identity in identities {
            let result = match identity {
                AgentIdentity::PublicKey { key, .. } => {
                    let hash = if key.algorithm().is_rsa() {
                        handle.best_supported_rsa_hash().await?.flatten()
                    } else {
                        None
                    };
                    handle
                        .authenticate_publickey_with(user, key, hash, &mut agent)
                        .await
                }
                AgentIdentity::Certificate { certificate, .. } => {
                    handle
                        .authenticate_certificate_with(user, certificate, None, &mut agent)
                        .await
                }
            };
            let result = result.map_err(|error| SshError::Local {
                what: "signing with the SSH agent".to_owned(),
                message: error.to_string(),
            })?;
            match Step::from(result) {
                Step::Failed(next) => last = Some(Step::Failed(next)),
                other => return Ok(Some(other)),
            }
        }
    }
    Ok(last)
}

/// An established chain: every hop's handle, the target last. Dropping it disconnects.
pub struct Connection {
    handles: Vec<Handle<ClientHandler>>,
    routes: tunnel::Routes,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("hops", &self.handles.len())
            .finish()
    }
}

impl Connection {
    /// The target's handle.
    ///
    /// # Errors
    ///
    /// Never in practice (a connection has at least one hop).
    pub fn target(&self) -> Result<&Handle<ClientHandler>, SshError> {
        self.handles
            .last()
            .ok_or_else(|| SshError::Protocol("no connection".to_owned()))
    }

    /// The target's remote forwards.
    pub(crate) fn routes(&self) -> &tunnel::Routes {
        &self.routes
    }

    /// Whether the target's connection has ended.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.handles.iter().any(Handle::is_closed)
    }

    /// Ends every hop, the target first.
    pub async fn close(&self) {
        for handle in self.handles.iter().rev() {
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "", "en")
                .await;
        }
    }
}

/// Connects every hop of `spec`, asking `asker` what needs a decision and telling `notes` how it
/// goes.
///
/// # Errors
///
/// The first hop that fails: unreachable, host key refused, authentication failed, cancelled.
pub async fn connect(
    spec: &ConnectSpec,
    asker: &Asker,
    notes: &Notes,
) -> Result<Connection, SshError> {
    let count = spec.hops.len();
    let mut handles: Vec<Handle<ClientHandler>> = Vec::with_capacity(count);
    let routes = tunnel::Routes::default();
    for (index, hop) in spec.hops.iter().enumerate() {
        let label = hop.label();
        notes(Note::Connecting {
            index,
            count,
            label: label.clone(),
        });
        let config = Arc::new(client::Config {
            preferred: algorithms::preferred(spec.legacy, spec.compression),
            keepalive_interval: spec.keepalive,
            keepalive_max: 3,
            nodelay: true,
            ..client::Config::default()
        });
        let rejection = Arc::new(Mutex::new(None));
        let handler = ClientHandler {
            host: hop.host.clone(),
            port: hop.port,
            known_hosts: spec.known_hosts.clone(),
            asker: Arc::clone(asker),
            notes: Arc::clone(notes),
            rejection: Arc::clone(&rejection),
            agent_forwarding: spec.agent_forwarding && index + 1 == count,
            agent_socket: spec.agent_socket.clone(),
            routes: if index + 1 == count {
                routes.clone()
            } else {
                tunnel::Routes::default()
            },
        };
        let transport: Box<dyn Transport> = match handles.last() {
            None => first_transport(spec, hop).await?,
            Some(previous) => {
                let channel = previous
                    .channel_open_direct_tcpip(
                        hop.host.clone(),
                        u32::from(hop.port),
                        "127.0.0.1",
                        0,
                    )
                    .await
                    .map_err(|error| SshError::Refused {
                        what: format!("a tunnel to {label} ({error})"),
                    })?;
                Box::new(channel.into_stream())
            }
        };
        let handshake = client::connect_stream(config, transport, handler).await;
        let mut handle = match handshake {
            Ok(handle) => handle,
            Err(error) => {
                let reason = rejection
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take();
                return Err(match reason {
                    Some(reason) => SshError::HostKey {
                        host: hop.host.clone(),
                        reason,
                    },
                    None => SshError::Network {
                        target: proxy::address(&hop.host, hop.port),
                        message: error.to_string(),
                    },
                });
            }
        };
        notes(Note::Authenticating {
            label: label.clone(),
        });
        authenticate(&mut handle, hop, asker).await?;
        handles.push(handle);
    }
    Ok(Connection { handles, routes })
}

/// The stream to the first hop: TCP, or through the proxy.
async fn first_transport(spec: &ConnectSpec, hop: &Hop) -> Result<Box<dyn Transport>, SshError> {
    let timeout = spec.connect_timeout;
    let target = proxy::address(&hop.host, hop.port);
    let timed_out = || SshError::Timeout {
        what: format!("connecting to {target}"),
    };
    let reach = async {
        match &spec.proxy {
            None => Ok(Box::new(proxy::tcp(&hop.host, hop.port).await?) as Box<dyn Transport>),
            Some(Proxy::Socks5 { host, port, login }) => {
                let mut stream = proxy::tcp(host, *port).await?;
                proxy::socks5(&mut stream, &hop.host, hop.port, login.as_ref()).await?;
                Ok(Box::new(stream) as Box<dyn Transport>)
            }
            Some(Proxy::Http { host, port, login }) => {
                let mut stream = proxy::tcp(host, *port).await?;
                proxy::http_connect(&mut stream, &hop.host, hop.port, login.as_ref()).await?;
                Ok(Box::new(stream) as Box<dyn Transport>)
            }
            Some(Proxy::Command(command)) => {
                let command = proxy::expand_command(command, &hop.host, hop.port, &hop.user);
                Ok(Box::new(proxy::spawn_command(&command)?) as Box<dyn Transport>)
            }
        }
    };
    tokio::time::timeout(timeout, reach)
        .await
        .map_err(|_| timed_out())?
}
