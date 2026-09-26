//! `Keychain` QML singleton (Sprint 6): the vault, identities, SSH keys, agents and known hosts.
//!
//! Every operation runs on the keychain's worker thread (`crate::keychain`): an invokable
//! queues it and returns a token, and `finished(token, code, detail, value)` reports how it
//! went (`code` empty on success; `detail` is technical text, or the seconds to wait for
//! `wait` and `wrong-password`). The properties are a snapshot of the state after the last
//! operation. Lists go to QML as JSON text.
//!
//! Passwords and passphrases arrive as the text the user typed; they are wrapped in memory that
//! is wiped on drop as soon as they reach Rust, and nothing secret comes back. Test runs keep
//! everything in memory and never touch the system keyring, agents or files.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Identities, keys and the vault.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, revision, READ, NOTIFY = changed)]
        #[qproperty(bool, busy, READ, NOTIFY = changed)]
        #[qproperty(QString, vault_status, cxx_name = "vaultStatus", READ, NOTIFY = changed)]
        #[qproperty(QString, protection, READ, NOTIFY = changed)]
        #[qproperty(bool, remembered, READ, NOTIFY = changed)]
        #[qproperty(bool, keyring_available, cxx_name = "keyringAvailable", READ, NOTIFY = changed)]
        #[qproperty(QString, keyring_problem, cxx_name = "keyringProblem", READ, NOTIFY = changed)]
        #[qproperty(QString, vault_problem, cxx_name = "vaultProblem", READ, NOTIFY = changed)]
        #[qproperty(i32, failures, READ, NOTIFY = changed)]
        #[qproperty(f64, wait_until, cxx_name = "waitUntil", READ, NOTIFY = changed)]
        #[qproperty(QString, identities, READ, NOTIFY = changed)]
        #[qproperty(QString, keys, READ, NOTIFY = changed)]
        #[qproperty(QString, agents, READ, NOTIFY = changed)]
        #[qproperty(QString, known_hosts, cxx_name = "knownHosts", READ, NOTIFY = changed)]
        #[qproperty(QString, problems, READ, NOTIFY = changed)]
        #[qproperty(bool, read_only, cxx_name = "readOnly", READ, NOTIFY = changed)]
        #[qproperty(QString, file_path, cxx_name = "filePath", READ, NOTIFY = changed)]
        type Keychain = super::KeychainRust;

        /// The state changed.
        #[qsignal]
        fn changed(self: Pin<&mut Self>);

        /// Operation `token` ended: `code` is empty on success.
        #[qsignal]
        fn finished(
            self: Pin<&mut Self>,
            token: i32,
            code: QString,
            detail: QString,
            value: QString,
        );

        /// Creates the vault: with a `password`, protected by it (and remembered on this computer
        /// with `remember`); with an empty one, held by the system keyring.
        #[qinvokable]
        #[cxx_name = "createVault"]
        fn create_vault(self: Pin<&mut Self>, password: &QString, remember: bool) -> i32;

        /// Unlocks with the master password.
        #[qinvokable]
        fn unlock(self: Pin<&mut Self>, password: &QString) -> i32;

        /// Unlocks with the key in the system keyring (the vault's own, or a remembered one).
        #[qinvokable]
        #[cxx_name = "unlockWithKeyring"]
        fn unlock_with_keyring(self: Pin<&mut Self>) -> i32;

        /// Locks the vault.
        #[qinvokable]
        fn lock(self: Pin<&mut Self>) -> i32;

        /// Protects a keyring vault with a master password.
        #[qinvokable]
        #[cxx_name = "setMasterPassword"]
        fn set_master_password(self: Pin<&mut Self>, password: &QString, remember: bool) -> i32;

        /// Changes the master password.
        #[qinvokable]
        #[cxx_name = "changeMasterPassword"]
        fn change_master_password(
            self: Pin<&mut Self>,
            current: &QString,
            replacement: &QString,
        ) -> i32;

        /// Removes the master password: the system keyring holds the key again.
        #[qinvokable]
        #[cxx_name = "removeMasterPassword"]
        fn remove_master_password(self: Pin<&mut Self>, current: &QString) -> i32;

        /// Remembers the vault key on this computer (or forgets it).
        #[qinvokable]
        #[cxx_name = "setRemember"]
        fn set_remember(self: Pin<&mut Self>, remember: bool) -> i32;

        /// Deletes the vault and every secret in it.
        #[qinvokable]
        #[cxx_name = "resetVault"]
        fn reset_vault(self: Pin<&mut Self>) -> i32;

        /// Saves an identity (`{id, name, user, key, notes}`; an empty id adds one). The password
        /// is kept (`keep`), removed (`clear`) or replaced by `password` (`set`). The new id is
        /// the `finished` value.
        #[qinvokable]
        #[cxx_name = "saveIdentity"]
        fn save_identity(
            self: Pin<&mut Self>,
            identity: &QString,
            password_mode: &QString,
            password: &QString,
        ) -> i32;

        /// Deletes an identity and its password.
        #[qinvokable]
        #[cxx_name = "deleteIdentity"]
        fn delete_identity(self: Pin<&mut Self>, id: &QString) -> i32;

        /// Generates a key of `kind` (`ed25519`, `ecdsa-p256`, `ecdsa-p384`, `ecdsa-p521`,
        /// `rsa-4096`).
        #[qinvokable]
        #[cxx_name = "generateKey"]
        fn generate_key(
            self: Pin<&mut Self>,
            kind: &QString,
            name: &QString,
            comment: &QString,
        ) -> i32;

        /// Imports the key file at `path` (OpenSSH or PuTTY) with `passphrase` (empty: none).
        #[qinvokable]
        #[cxx_name = "importKeyFile"]
        fn import_key_file(
            self: Pin<&mut Self>,
            path: &QString,
            passphrase: &QString,
            name: &QString,
        ) -> i32;

        /// Imports a pasted key.
        #[qinvokable]
        #[cxx_name = "importKeyText"]
        fn import_key_text(
            self: Pin<&mut Self>,
            text: &QString,
            passphrase: &QString,
            name: &QString,
        ) -> i32;

        /// Writes private key `id` to `path` as an OpenSSH key, encrypted with `passphrase`
        /// unless it is empty.
        #[qinvokable]
        #[cxx_name = "exportPrivateKey"]
        fn export_private_key(
            self: Pin<&mut Self>,
            id: &QString,
            path: &QString,
            passphrase: &QString,
        ) -> i32;

        /// Writes the public key of `id` to `path`.
        #[qinvokable]
        #[cxx_name = "exportPublicKey"]
        fn export_public_key(self: Pin<&mut Self>, id: &QString, path: &QString) -> i32;

        /// Renames a key.
        #[qinvokable]
        #[cxx_name = "renameKey"]
        fn rename_key(self: Pin<&mut Self>, id: &QString, name: &QString) -> i32;

        /// Deletes a key and its private part.
        #[qinvokable]
        #[cxx_name = "deleteKey"]
        fn delete_key(self: Pin<&mut Self>, id: &QString) -> i32;

        /// Lists the keys of the SSH agents (`agents`).
        #[qinvokable]
        #[cxx_name = "refreshAgents"]
        fn refresh_agents(self: Pin<&mut Self>) -> i32;

        /// Reads the known hosts files (`knownHosts`).
        #[qinvokable]
        #[cxx_name = "refreshKnownHosts"]
        fn refresh_known_hosts(self: Pin<&mut Self>) -> i32;

        /// Test runs only: sample identities, keys, agents and known hosts.
        #[qinvokable]
        #[cxx_name = "loadSample"]
        fn load_sample(self: Pin<&mut Self>) -> i32;
    }

    impl cxx_qt::Initialize for Keychain {}
    impl cxx_qt::Threading for Keychain {}
}

use core::pin::Pin;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use opensesh_core::watch::FileWatcher;
use opensesh_vault::keychain::KEYCHAIN_FILE;
use opensesh_vault::keys::KeyType;
use opensesh_vault::manager::{IdentityEdit, PasswordChange};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value as Json;

use crate::bridge::app_info::is_test_run;
use crate::keychain::{Job, KeySource, Outcome};
use crate::services;

/// Rust state behind `Keychain`.
#[derive(Default)]
pub struct KeychainRust {
    revision: i32,
    busy: bool,
    vault_status: QString,
    protection: QString,
    remembered: bool,
    keyring_available: bool,
    keyring_problem: QString,
    vault_problem: QString,
    failures: i32,
    wait_until: f64,
    identities: QString,
    keys: QString,
    agents: QString,
    known_hosts: QString,
    problems: QString,
    read_only: bool,
    file_path: QString,
    jobs: Option<Sender<(i32, Job)>>,
    next_token: i32,
    in_flight: i32,
    watcher: Option<FileWatcher>,
}

/// Secret text from QML, moved into memory that is wiped on drop (and never printed).
fn secret(text: &QString) -> SecretString {
    SecretString::from(text.to_string())
}

/// `None` for empty text.
fn optional_secret(text: &QString) -> Option<SecretString> {
    let value = secret(text);
    (!value.expose_secret().is_empty()).then_some(value)
}

impl qobject::Keychain {
    fn submit(mut self: Pin<&mut Self>, job: Job) -> i32 {
        let token = {
            let mut state = self.as_mut().rust_mut();
            state.next_token = state.next_token.wrapping_add(1).max(1);
            state.next_token
        };
        let sent = self
            .jobs
            .as_ref()
            .is_some_and(|jobs| jobs.send((token, job)).is_ok());
        if sent {
            {
                let mut state = self.as_mut().rust_mut();
                state.in_flight += 1;
                state.busy = true;
            }
            self.as_mut().changed();
        } else {
            tracing::warn!("the keychain worker isn't running");
            self.as_mut().finished(
                token,
                QString::from("unavailable"),
                QString::default(),
                QString::default(),
            );
        }
        token
    }

    fn apply(mut self: Pin<&mut Self>, token: i32, outcome: Outcome) {
        {
            let snapshot = &outcome.snapshot;
            let mut state = self.as_mut().rust_mut();
            // Token 0: a reload after `keychain.toml` changed on disk, not a request.
            if token != 0 {
                state.in_flight = (state.in_flight - 1).max(0);
            }
            state.busy = state.in_flight > 0;
            state.revision = state.revision.wrapping_add(1);
            state.vault_status = QString::from(snapshot.status);
            state.protection = QString::from(snapshot.protection);
            state.remembered = snapshot.remembered;
            state.keyring_available = snapshot.keyring_available;
            state.keyring_problem = QString::from(&snapshot.keyring_problem);
            state.vault_problem = QString::from(&snapshot.vault_problem);
            state.failures = i32::try_from(snapshot.failures).unwrap_or(i32::MAX);
            state.wait_until = snapshot.wait_until_ms;
            state.identities = QString::from(&snapshot.identities);
            state.keys = QString::from(&snapshot.keys);
            state.problems = QString::from(&snapshot.problems);
            state.read_only = snapshot.read_only;
            state.file_path = QString::from(&snapshot.file_path);
            if let Some(agents) = &outcome.agents {
                state.agents = QString::from(agents);
            }
            if let Some(known_hosts) = &outcome.known_hosts {
                state.known_hosts = QString::from(known_hosts);
            }
        }
        self.as_mut().changed();
        self.as_mut().finished(
            token,
            QString::from(&outcome.code),
            QString::from(&outcome.detail),
            QString::from(&outcome.value),
        );
    }

    /// See the bridge declaration.
    pub fn create_vault(self: Pin<&mut Self>, password: &QString, remember: bool) -> i32 {
        self.submit(Job::CreateVault {
            password: optional_secret(password),
            remember,
        })
    }

    /// See the bridge declaration.
    pub fn unlock(self: Pin<&mut Self>, password: &QString) -> i32 {
        self.submit(Job::Unlock(secret(password)))
    }

    /// See the bridge declaration.
    pub fn unlock_with_keyring(self: Pin<&mut Self>) -> i32 {
        self.submit(Job::UnlockWithKeyring)
    }

    /// See the bridge declaration.
    pub fn lock(self: Pin<&mut Self>) -> i32 {
        self.submit(Job::Lock)
    }

    /// See the bridge declaration.
    pub fn set_master_password(self: Pin<&mut Self>, password: &QString, remember: bool) -> i32 {
        self.submit(Job::SetPassword {
            password: secret(password),
            remember,
        })
    }

    /// See the bridge declaration.
    pub fn change_master_password(
        self: Pin<&mut Self>,
        current: &QString,
        replacement: &QString,
    ) -> i32 {
        self.submit(Job::ChangePassword {
            current: secret(current),
            new: secret(replacement),
        })
    }

    /// See the bridge declaration.
    pub fn remove_master_password(self: Pin<&mut Self>, current: &QString) -> i32 {
        self.submit(Job::RemovePassword(secret(current)))
    }

    /// See the bridge declaration.
    pub fn set_remember(self: Pin<&mut Self>, remember: bool) -> i32 {
        self.submit(Job::SetRemember(remember))
    }

    /// See the bridge declaration.
    pub fn reset_vault(self: Pin<&mut Self>) -> i32 {
        self.submit(Job::ResetVault)
    }

    /// See the bridge declaration.
    pub fn save_identity(
        self: Pin<&mut Self>,
        identity: &QString,
        password_mode: &QString,
        password: &QString,
    ) -> i32 {
        let value: Json = serde_json::from_str(&identity.to_string()).unwrap_or(Json::Null);
        let text = |key: &str| {
            value
                .get(key)
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let key = text("key");
        let edit = IdentityEdit {
            id: text("id"),
            name: text("name"),
            user: text("user"),
            password: match password_mode.to_string().as_str() {
                "clear" => PasswordChange::Clear,
                "set" => PasswordChange::Set(secret(password)),
                _ => PasswordChange::Keep,
            },
            key: (!key.is_empty()).then_some(key),
            notes: text("notes"),
        };
        self.submit(Job::SaveIdentity(edit))
    }

    /// See the bridge declaration.
    pub fn delete_identity(self: Pin<&mut Self>, id: &QString) -> i32 {
        self.submit(Job::DeleteIdentity(id.to_string()))
    }

    /// See the bridge declaration.
    pub fn generate_key(
        self: Pin<&mut Self>,
        kind: &QString,
        name: &QString,
        comment: &QString,
    ) -> i32 {
        let kind = KeyType::parse(&kind.to_string()).unwrap_or_default();
        self.submit(Job::GenerateKey {
            kind,
            name: name.to_string(),
            comment: comment.to_string(),
        })
    }

    /// See the bridge declaration.
    pub fn import_key_file(
        self: Pin<&mut Self>,
        path: &QString,
        passphrase: &QString,
        name: &QString,
    ) -> i32 {
        self.submit(Job::ImportKey {
            source: KeySource::File(PathBuf::from(path.to_string())),
            passphrase: optional_secret(passphrase),
            name: name.to_string(),
        })
    }

    /// See the bridge declaration.
    pub fn import_key_text(
        self: Pin<&mut Self>,
        text: &QString,
        passphrase: &QString,
        name: &QString,
    ) -> i32 {
        self.submit(Job::ImportKey {
            source: KeySource::Text(secret(text)),
            passphrase: optional_secret(passphrase),
            name: name.to_string(),
        })
    }

    /// See the bridge declaration.
    pub fn export_private_key(
        self: Pin<&mut Self>,
        id: &QString,
        path: &QString,
        passphrase: &QString,
    ) -> i32 {
        if is_test_run() {
            return self.refuse_in_tests();
        }
        self.submit(Job::ExportPrivate {
            id: id.to_string(),
            path: PathBuf::from(path.to_string()),
            passphrase: optional_secret(passphrase),
        })
    }

    /// See the bridge declaration.
    pub fn export_public_key(self: Pin<&mut Self>, id: &QString, path: &QString) -> i32 {
        if is_test_run() {
            return self.refuse_in_tests();
        }
        self.submit(Job::ExportPublic {
            id: id.to_string(),
            path: PathBuf::from(path.to_string()),
        })
    }

    /// Test runs never write the user's files.
    fn refuse_in_tests(mut self: Pin<&mut Self>) -> i32 {
        let token = {
            let mut state = self.as_mut().rust_mut();
            state.next_token = state.next_token.wrapping_add(1).max(1);
            state.next_token
        };
        self.as_mut().finished(
            token,
            QString::from("test-run"),
            QString::default(),
            QString::default(),
        );
        token
    }

    /// See the bridge declaration.
    pub fn rename_key(self: Pin<&mut Self>, id: &QString, name: &QString) -> i32 {
        self.submit(Job::RenameKey {
            id: id.to_string(),
            name: name.to_string(),
        })
    }

    /// See the bridge declaration.
    pub fn delete_key(self: Pin<&mut Self>, id: &QString) -> i32 {
        self.submit(Job::DeleteKey(id.to_string()))
    }

    /// See the bridge declaration.
    pub fn refresh_agents(self: Pin<&mut Self>) -> i32 {
        self.submit(Job::ListAgents)
    }

    /// See the bridge declaration.
    pub fn refresh_known_hosts(self: Pin<&mut Self>) -> i32 {
        self.submit(Job::ReadKnownHosts)
    }

    /// See the bridge declaration.
    pub fn load_sample(self: Pin<&mut Self>) -> i32 {
        if !is_test_run() {
            return 0;
        }
        self.submit(Job::LoadSample)
    }
}

impl cxx_qt::Initialize for qobject::Keychain {
    fn initialize(mut self: Pin<&mut Self>) {
        {
            let mut state = self.as_mut().rust_mut();
            state.vault_status = QString::from("missing");
            state.identities = QString::from("[]");
            state.keys = QString::from("[]");
            state.agents = QString::from("[]");
            state.known_hosts = QString::from("[]");
            state.problems = QString::from("[]");
        }
        let dirs = if is_test_run() {
            None
        } else {
            services::get().map(|services| {
                (
                    services.paths.config_dir().to_path_buf(),
                    services.paths.data_dir().to_path_buf(),
                )
            })
        };
        let watched = dirs.as_ref().map(|(config, _)| config.join(KEYCHAIN_FILE));
        let (sender, receiver) = mpsc::channel();
        let qt_thread = self.qt_thread();
        let started = crate::keychain::spawn(dirs, receiver, move |token, outcome| {
            let _ = qt_thread.queue(move |object| object.apply(token, outcome));
        });
        if let Err(error) = started {
            tracing::error!("could not start the keychain worker: {error}");
            return;
        }
        if let Some(path) = watched {
            let jobs = sender.clone();
            match FileWatcher::spawn(&path, Duration::from_millis(300), move || {
                let _ = jobs.send((0, Job::Reload));
            }) {
                Ok(watcher) => self.as_mut().rust_mut().watcher = Some(watcher),
                Err(error) => tracing::warn!("keychain.toml won't hot-reload: {error}"),
            }
        }
        self.as_mut().rust_mut().jobs = Some(sender);
        // The vault opens by itself when the keyring holds its key (or remembers it).
        self.as_mut().submit(Job::UnlockWithKeyring);
    }
}
