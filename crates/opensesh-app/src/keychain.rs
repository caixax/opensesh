//! The keychain's worker thread (Sprint 6): it owns the vault and `keychain.toml`
//! ([`opensesh_vault::manager::Keychain`]) and runs every operation, since any of them can be
//! slow (Argon2id, RSA, the system keyring) and the GUI thread must never wait.
//!
//! The `Keychain` QML singleton sends [`Job`]s and gets back an [`Outcome`]: an error code and a
//! [`Snapshot`] of everything it shows, all public. Secrets only enter this module as what the
//! user typed (passwords, passphrases) and never leave it: exports are written to files here.
//!
//! The identities are also published for other singletons (`Hosts` asks for an identity's user
//! name when it builds an `ssh` command) through [`identity_user`].

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, LazyLock, PoisonError, RwLock};
use std::time::Duration;

use opensesh_core::fsutil;
use opensesh_vault::agent::{self, AgentError};
use opensesh_vault::crypto::KdfParams;
use opensesh_vault::keychain::KeychainFile;
use opensesh_vault::keys::{self, KeyType};
use opensesh_vault::known_hosts::{self, KNOWN_HOSTS_FILE};
use opensesh_vault::manager::{IdentityEdit, Keychain, KeychainOpError};
use opensesh_vault::{KeyStore, MemoryKeyStore, Protection, Status, SystemKeyring, VaultError};
use serde_json::{Value as Json, json};
use zeroize::Zeroizing;

/// How long an agent may take to list its keys.
const AGENT_TIMEOUT: Duration = Duration::from_secs(3);

/// Largest key file read for an import.
const MAX_KEY_FILE: u64 = 1024 * 1024;

/// Something for the worker to do.
pub enum Job {
    /// `keychain.toml` changed on disk.
    Reload,
    /// Unlock with the key in the system keyring, when there is one.
    UnlockWithKeyring,
    /// Create the vault: with a password, protected by it; without, held by the keyring.
    CreateVault {
        /// The master password, if any.
        password: Option<Zeroizing<String>>,
        /// Keep the key in the keyring too.
        remember: bool,
    },
    /// Unlock with the master password.
    Unlock(Zeroizing<String>),
    /// Lock.
    Lock,
    /// Protect a keyring vault with a master password.
    SetPassword {
        /// The new master password.
        password: Zeroizing<String>,
        /// Keep the key in the keyring too.
        remember: bool,
    },
    /// Change the master password.
    ChangePassword {
        /// The current one.
        current: Zeroizing<String>,
        /// The new one.
        new: Zeroizing<String>,
    },
    /// Let the keyring hold the key again.
    RemovePassword(Zeroizing<String>),
    /// Remember (or forget) the key on this computer.
    SetRemember(bool),
    /// Delete the vault.
    ResetVault,
    /// Save an identity.
    SaveIdentity(IdentityEdit),
    /// Delete an identity.
    DeleteIdentity(String),
    /// Generate a key.
    GenerateKey {
        /// Its type.
        kind: KeyType,
        /// Display name.
        name: String,
        /// Comment in the public key.
        comment: String,
    },
    /// Import a key file or pasted text.
    ImportKey {
        /// A file to read, or the text itself.
        source: KeySource,
        /// Its passphrase, if it has one.
        passphrase: Option<Zeroizing<String>>,
        /// Display name.
        name: String,
    },
    /// Write a private key to a file.
    ExportPrivate {
        /// Key id.
        id: String,
        /// Where.
        path: PathBuf,
        /// A passphrase for the file, if any.
        passphrase: Option<Zeroizing<String>>,
    },
    /// Write a public key to a file.
    ExportPublic {
        /// Key id.
        id: String,
        /// Where.
        path: PathBuf,
    },
    /// Rename a key.
    RenameKey {
        /// Key id.
        id: String,
        /// New name.
        name: String,
    },
    /// Delete a key.
    DeleteKey(String),
    /// List the keys of the SSH agents.
    ListAgents,
    /// Read the known hosts files.
    ReadKnownHosts,
    /// Test runs: fill the keychain with sample entries.
    LoadSample,
}

/// Where an imported key comes from.
pub enum KeySource {
    /// A file.
    File(PathBuf),
    /// Pasted text.
    Text(Zeroizing<String>),
}

/// What the QML singleton shows. Everything here is public.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// `missing`, `unreadable`, `locked` or `unlocked`.
    pub status: &'static str,
    /// `keyring`, `password`, or empty without a vault.
    pub protection: &'static str,
    /// With a master password: the keyring has the key too.
    pub remembered: bool,
    /// The system keyring works.
    pub keyring_available: bool,
    /// Why it doesn't.
    pub keyring_problem: String,
    /// Why the vault can't be read.
    pub vault_problem: String,
    /// Wrong master passwords in a row.
    pub failures: u32,
    /// Milliseconds since the epoch until the next attempt is allowed (0: now).
    pub wait_until_ms: f64,
    /// Identities as JSON.
    pub identities: String,
    /// Keys as JSON.
    pub keys: String,
    /// Problems reading `keychain.toml`, as a JSON list.
    pub problems: String,
    /// `keychain.toml` can't be changed.
    pub read_only: bool,
    /// Where `keychain.toml` is.
    pub file_path: String,
}

/// The result of a job.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// Error code (empty on success).
    pub code: String,
    /// Technical detail of the error (never a secret).
    pub detail: String,
    /// A result value (a new id, seconds to wait).
    pub value: String,
    /// The state after the job.
    pub snapshot: Snapshot,
    /// Agents as JSON, after [`Job::ListAgents`].
    pub agents: Option<String>,
    /// Known hosts as JSON, after [`Job::ReadKnownHosts`].
    pub known_hosts: Option<String>,
}

static LIBRARY: LazyLock<RwLock<Arc<KeychainFile>>> =
    LazyLock::new(|| RwLock::new(Arc::new(KeychainFile::default())));

/// The user name of identity `id`, if it has one.
#[must_use]
pub fn identity_user(id: &str) -> Option<String> {
    let library = Arc::clone(&LIBRARY.read().unwrap_or_else(PoisonError::into_inner));
    library
        .identity(id)
        .map(|identity| identity.user.clone())
        .filter(|user| !user.is_empty())
}

fn publish(file: &KeychainFile) {
    *LIBRARY.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(file.clone());
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Starts the worker. `dirs` are the config and data folders (`None` in test runs, which keep
/// everything in memory and never touch the system keyring, agents or files). `send` receives
/// each outcome with the job's token.
///
/// # Errors
///
/// When the thread can't start.
pub fn spawn(
    dirs: Option<(PathBuf, PathBuf)>,
    jobs: Receiver<(i32, Job)>,
    send: impl Fn(i32, Outcome) + Send + 'static,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("keychain".to_owned())
        .spawn(move || {
            let hermetic = dirs.is_none();
            let store: Arc<dyn KeyStore> = if hermetic {
                Arc::new(MemoryKeyStore::new())
            } else {
                Arc::new(SystemKeyring::default())
            };
            let keychain = match &dirs {
                Some((config, data)) => Keychain::open(Some((config, data)), Arc::clone(&store)),
                None => Keychain::open(None, Arc::clone(&store)),
            };
            let mut worker = Worker {
                keychain,
                store,
                config_dir: dirs.map(|(config, _)| config),
                sample: false,
            };
            for warning in &worker.keychain.warnings {
                tracing::warn!("keychain.toml: {warning}");
            }
            publish(&worker.keychain.file);
            while let Ok((token, job)) = jobs.recv() {
                let outcome = worker.run(job);
                publish(&worker.keychain.file);
                send(token, outcome);
            }
        })
        .map(|_| ())
}

struct Worker {
    keychain: Keychain,
    store: Arc<dyn KeyStore>,
    /// `None` in test runs.
    config_dir: Option<PathBuf>,
    /// Test runs: sample agents and known hosts instead of none.
    sample: bool,
}

fn error_outcome(error: &KeychainOpError) -> (String, String) {
    let detail = match error {
        KeychainOpError::Vault(VaultError::Wait(secs))
        | KeychainOpError::Vault(VaultError::WrongPassword { wait: secs }) => secs.to_string(),
        other => other.to_string(),
    };
    (error.code().to_owned(), detail)
}

impl Worker {
    fn run(&mut self, job: Job) -> Outcome {
        let mut outcome = Outcome::default();
        let result: Result<String, KeychainOpError> = match job {
            Job::Reload => {
                self.keychain.reload();
                Ok(String::new())
            }
            Job::UnlockWithKeyring => self
                .keychain
                .vault
                .unlock_with_keyring()
                .map(|opened| opened.to_string())
                .map_err(KeychainOpError::from),
            Job::CreateVault { password, remember } => match password {
                Some(password) => self
                    .keychain
                    .create_with_password(password.as_bytes(), KdfParams::RECOMMENDED)
                    .and_then(|()| {
                        if remember {
                            self.keychain.vault.set_remember(true)?;
                        }
                        Ok(String::new())
                    }),
                None => self.keychain.ensure_vault().map(|()| String::new()),
            },
            Job::Unlock(password) => self
                .keychain
                .vault
                .unlock(password.as_bytes(), now_secs())
                .map(|()| String::new())
                .map_err(KeychainOpError::from),
            Job::Lock => {
                self.keychain.vault.lock();
                Ok(String::new())
            }
            Job::SetPassword { password, remember } => self
                .keychain
                .vault
                .set_password(password.as_bytes(), KdfParams::RECOMMENDED, remember)
                .map(|()| String::new())
                .map_err(KeychainOpError::from),
            Job::ChangePassword { current, new } => self
                .keychain
                .vault
                .change_password(
                    current.as_bytes(),
                    new.as_bytes(),
                    KdfParams::RECOMMENDED,
                    now_secs(),
                )
                .map(|()| String::new())
                .map_err(KeychainOpError::from),
            Job::RemovePassword(current) => self
                .keychain
                .vault
                .remove_password(current.as_bytes(), now_secs())
                .map(|()| String::new())
                .map_err(KeychainOpError::from),
            Job::SetRemember(remember) => self
                .keychain
                .vault
                .set_remember(remember)
                .map(|()| String::new())
                .map_err(KeychainOpError::from),
            Job::ResetVault => self.keychain.reset_vault().map(|()| String::new()),
            Job::SaveIdentity(edit) => self.keychain.save_identity(edit),
            Job::DeleteIdentity(id) => self.keychain.delete_identity(&id).map(|()| String::new()),
            Job::GenerateKey {
                kind,
                name,
                comment,
            } => self.keychain.generate_key(kind, &name, &comment),
            Job::ImportKey {
                source,
                passphrase,
                name,
            } => self.import(source, passphrase.as_ref().map(|text| text.as_bytes()), &name),
            Job::ExportPrivate {
                id,
                path,
                passphrase,
            } => self
                .keychain
                .export_private(&id, passphrase.as_ref().map(|text| text.as_bytes()))
                .and_then(|text| write_file(&path, text.as_bytes()))
                .map(|()| String::new()),
            Job::ExportPublic { id, path } => match self.keychain.file.key(&id) {
                Some(key) => {
                    let line = format!("{}\n", key.public);
                    write_file(&path, line.as_bytes()).map(|()| String::new())
                }
                None => Err(KeychainOpError::NotFound(id)),
            },
            Job::RenameKey { id, name } => {
                self.keychain.rename_key(&id, &name).map(|()| String::new())
            }
            Job::DeleteKey(id) => self.keychain.delete_key(&id).map(|()| String::new()),
            Job::ListAgents => {
                outcome.agents = Some(self.agents());
                Ok(String::new())
            }
            Job::ReadKnownHosts => {
                outcome.known_hosts = Some(self.known_hosts());
                Ok(String::new())
            }
            Job::LoadSample => {
                self.load_sample();
                Ok(String::new())
            }
        };
        // Whatever unlocked the vault, secrets nothing refers to any more can go now.
        if self.keychain.vault.status() == Status::Unlocked {
            if let Err(error) = self.keychain.collect_garbage() {
                tracing::warn!("could not tidy the vault: {error}");
            }
        }
        match result {
            Ok(value) => outcome.value = value,
            Err(error) => {
                tracing::info!(code = error.code(), "keychain: {error}");
                let (code, detail) = error_outcome(&error);
                outcome.code = code;
                outcome.detail = detail;
            }
        }
        outcome.snapshot = self.snapshot();
        outcome
    }

    fn import(
        &mut self,
        source: KeySource,
        passphrase: Option<&[u8]>,
        name: &str,
    ) -> Result<String, KeychainOpError> {
        let bytes = match source {
            KeySource::Text(text) => Zeroizing::new(text.as_bytes().to_vec()),
            KeySource::File(path) => Zeroizing::new(read_key_file(&path)?),
        };
        self.keychain.import_key(&bytes, passphrase, name)
    }

    fn snapshot(&mut self) -> Snapshot {
        let vault = &mut self.keychain.vault;
        let wait = vault.wait(now_secs());
        let wait_until_ms = if wait > 0 {
            #[allow(clippy::cast_precision_loss)] // Milliseconds since 1970 fit f64 exactly.
            let until = (now_secs() + wait) as f64 * 1000.0;
            until
        } else {
            0.0
        };
        let keyring = self.store.check();
        Snapshot {
            status: match vault.status() {
                Status::Missing => "missing",
                Status::Unreadable => "unreadable",
                Status::Locked => "locked",
                Status::Unlocked => "unlocked",
            },
            protection: match vault.protection() {
                Some(Protection::Keyring) => "keyring",
                Some(Protection::Password) => "password",
                None => "",
            },
            remembered: vault.remembered(),
            keyring_available: keyring.is_ok(),
            keyring_problem: keyring.err().map(|error| error.to_string()).unwrap_or_default(),
            vault_problem: vault.problem().unwrap_or_default().to_owned(),
            failures: vault.failures(),
            wait_until_ms,
            identities: self.identities_json().to_string(),
            keys: self.keys_json().to_string(),
            problems: Json::from(
                self.keychain
                    .warnings
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            )
            .to_string(),
            read_only: self.keychain.read_only(),
            file_path: self
                .keychain
                .file_path()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        }
    }

    fn identities_json(&self) -> Json {
        let file = &self.keychain.file;
        Json::Array(
            file.identities
                .iter()
                .map(|identity| {
                    let key = identity.key.as_deref().and_then(|id| file.key(id));
                    json!({
                        "id": identity.id,
                        "name": identity.name,
                        "user": identity.user,
                        "hasPassword": identity.password.is_some(),
                        "key": identity.key.clone().unwrap_or_default(),
                        "keyName": key.map(|key| key.name.clone()).unwrap_or_default(),
                        "notes": identity.notes,
                    })
                })
                .collect(),
        )
    }

    fn keys_json(&self) -> Json {
        let file = &self.keychain.file;
        Json::Array(
            file.keys
                .iter()
                .map(|key| {
                    let info = keys::public_line_info(&key.public, None);
                    let label = info
                        .as_ref()
                        .map_or_else(|| key.algorithm.clone(), |info| info.label.clone());
                    let comment = info.map(|info| info.comment).unwrap_or_default();
                    let used_by: Vec<&str> = file
                        .identities_using(&key.id)
                        .iter()
                        .map(|identity| identity.name.as_str())
                        .collect();
                    json!({
                        "id": key.id,
                        "name": key.name,
                        "algorithm": key.algorithm,
                        "label": label,
                        "bits": key.bits,
                        "publicKey": key.public,
                        "fingerprint": key.fingerprint,
                        "comment": comment,
                        "origin": key.origin,
                        "created": key.created,
                        "hasPrivate": key.private.is_some(),
                        "usedBy": used_by,
                    })
                })
                .collect(),
        )
    }

    fn agents(&self) -> String {
        if self.config_dir.is_none() && !self.sample {
            return "[]".to_owned();
        }
        let listings = if self.sample {
            sample_agents()
        } else {
            agent::list_all(AGENT_TIMEOUT)
                .into_iter()
                .map(|listing| {
                    let (keys, error) = match listing.keys {
                        Ok(keys) => (keys, ""),
                        Err(AgentError::NotRunning) => (Vec::new(), "not-running"),
                        Err(AgentError::Timeout) => (Vec::new(), "timeout"),
                        Err(AgentError::Refused) => (Vec::new(), "refused"),
                        Err(error) => {
                            tracing::info!("agent {}: {error}", listing.agent.location());
                            (Vec::new(), "failed")
                        }
                    };
                    json!({
                        "kind": listing.agent.code(),
                        "location": listing.agent.location(),
                        "error": error,
                        "keys": keys.iter().map(|key| json!({
                            "algorithm": key.algorithm,
                            "label": key.label,
                            "bits": key.bits,
                            "fingerprint": key.fingerprint,
                            "comment": key.comment,
                            "publicKey": key.public,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect()
        };
        Json::Array(listings).to_string()
    }

    fn known_hosts(&self) -> String {
        let mut files = Vec::new();
        if self.sample {
            files.push(("~/.ssh/known_hosts".to_owned(), known_hosts::parse(SAMPLE_KNOWN_HOSTS), String::new()));
        } else if let Some(config) = &self.config_dir {
            let mut paths = Vec::new();
            if let Some(home) = opensesh_core::paths::home_dir() {
                paths.push(home.join(".ssh").join("known_hosts"));
            }
            paths.push(config.join(KNOWN_HOSTS_FILE));
            for path in paths {
                let (parsed, problem) = match known_hosts::read(&path) {
                    Ok(parsed) => (parsed, String::new()),
                    Err(error) => (known_hosts::KnownHosts::default(), error.to_string()),
                };
                files.push((path.display().to_string(), parsed, problem));
            }
        }
        Json::Array(
            files
                .into_iter()
                .map(|(path, parsed, problem)| {
                    json!({
                        "path": path,
                        "problem": problem,
                        "badLines": parsed.bad_lines,
                        "entries": parsed.entries.iter().map(|entry| json!({
                            "line": entry.line,
                            "marker": entry.marker.clone().unwrap_or_default(),
                            "hosts": entry.hosts,
                            "hashed": entry.hashed,
                            "keyType": entry.key_type,
                            "fingerprint": entry.fingerprint,
                            "comment": entry.comment,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect(),
        )
        .to_string()
    }

    /// Test runs: a few identities and keys (in memory), sample agents and known hosts.
    fn load_sample(&mut self) {
        if self.config_dir.is_some() {
            return;
        }
        self.sample = true;
        let steps: [(&str, KeyType, &str); 3] = [
            ("Laptop", KeyType::Ed25519, "me@laptop"),
            ("Deploy (CI)", KeyType::EcdsaP256, "ci@build"),
            ("Old server key", KeyType::EcdsaP384, "admin@legacy"),
        ];
        let mut first_key = None;
        for (name, kind, comment) in steps {
            match self.keychain.generate_key(kind, name, comment) {
                Ok(id) => {
                    first_key.get_or_insert(id);
                }
                Err(error) => tracing::warn!("sample key: {error}"),
            }
        }
        for (name, user, password, key) in [
            ("deploy", "deploy", true, first_key.clone()),
            ("root on lab", "root", true, None),
            ("Personal", "me", false, first_key),
        ] {
            let edit = IdentityEdit {
                name: name.to_owned(),
                user: user.to_owned(),
                password: if password {
                    opensesh_vault::manager::PasswordChange::Set(Zeroizing::new(
                        "sample password".to_owned(),
                    ))
                } else {
                    opensesh_vault::manager::PasswordChange::Keep
                },
                key,
                ..IdentityEdit::default()
            };
            if let Err(error) = self.keychain.save_identity(edit) {
                tracing::warn!("sample identity: {error}");
            }
        }
    }
}

const SAMPLE_KNOWN_HOSTS: &str = "\
git.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC6tmVU1VE59P7TYx6UJYcZkhy7FRiLjhH6gdK9Sayyd\n\
|1|F1E1KeoE/eEWhi10WpGv4OdiO6Y=|3988QV0VE8wmZL7suNrYQLITLCg= ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC6tmVU1VE59P7TYx6UJYcZkhy7FRiLjhH6gdK9Sayyd\n\
10.0.1.21,web-01 ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBHqfppNX5vS4eX9GvOQ7/uKZTWCq4n8c+F08fjstuFzIG8eYZpH/MeS1Hy6h1tNPNIaUvc04G0Ks90zScCvDXpk=\n\
@cert-authority *.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC6tmVU1VE59P7TYx6UJYcZkhy7FRiLjhH6gdK9Sayyd\n";

fn sample_agents() -> Vec<Json> {
    let key = |public: &str, comment: &str| {
        keys::public_line_info(public, Some(comment)).map(|info| {
            json!({
                "algorithm": info.algorithm,
                "label": info.label,
                "bits": info.bits,
                "fingerprint": info.fingerprint,
                "comment": info.comment,
                "publicKey": info.public,
            })
        })
    };
    let agent_keys: Vec<Json> = [
        key(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC6tmVU1VE59P7TYx6UJYcZkhy7FRiLjhH6gdK9Sayyd",
            "me@laptop",
        ),
        key(
            "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBHqfppNX5vS4eX9GvOQ7/uKZTWCq4n8c+F08fjstuFzIG8eYZpH/MeS1Hy6h1tNPNIaUvc04G0Ks90zScCvDXpk=",
            "yubikey",
        ),
    ]
    .into_iter()
    .flatten()
    .collect();
    vec![
        json!({"kind": "openssh", "location": agent::OPENSSH_PIPE, "error": "", "keys": agent_keys}),
        json!({"kind": "pageant", "location": "Pageant", "error": "not-running", "keys": []}),
    ]
}

/// Reads a key file for an import (a key is small; anything big isn't one).
fn read_key_file(path: &Path) -> Result<Vec<u8>, KeychainOpError> {
    let io = |source: std::io::Error| KeychainOpError::Read {
        path: path.display().to_string(),
        source,
    };
    let meta = std::fs::metadata(path).map_err(io)?;
    if meta.len() > MAX_KEY_FILE {
        return Err(keys::KeyError::NotAKey.into());
    }
    std::fs::read(path).map_err(io)
}

/// Writes an exported key: atomically, private to the user on Unix, no backups.
fn write_file(path: &Path, contents: &[u8]) -> Result<(), KeychainOpError> {
    fsutil::atomic_write(path, contents, 0)
        .map(|_| ())
        .map_err(|source| KeychainOpError::Write {
            path: path.display().to_string(),
            source,
        })
}
