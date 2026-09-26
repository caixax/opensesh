//! The keychain as the app uses it (Sprint 6): the vault and `keychain.toml` together, so that
//! a secret and the entry that refers to it change as one.
//!
//! Every call can be slow (Argon2id, RSA, the keyring) and must run off the GUI thread. With no
//! folders, everything stays in memory (test runs).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use opensesh_core::config::Warning;
use opensesh_core::fsutil::{self, DEFAULT_BACKUPS};
use ssh_key::PrivateKey;
use zeroize::Zeroizing;

use crate::backoff::ATTEMPTS_FILE;
use crate::crypto::KdfParams;
use crate::format::SecretKind;
use crate::keychain::{Identity, KEYCHAIN_FILE, KeyEntry, KeychainFile, new_id};
use crate::keys::{self, KeyError, KeyFormat, KeyType};
use crate::store::KeyStore;
use crate::vault::{VAULT_FILE, parse_ref, secret_ref};
use crate::{Status, Vault, VaultError};

/// Why a keychain operation failed. Messages never contain secrets.
#[derive(Debug, thiserror::Error)]
pub enum KeychainOpError {
    /// The vault refused.
    #[error(transparent)]
    Vault(#[from] VaultError),
    /// A key couldn't be read or made.
    #[error(transparent)]
    Key(#[from] KeyError),
    /// There is no vault and no keyring to create one: a master password is needed.
    #[error("a master password is needed to store secrets on this system")]
    NeedsVault,
    /// No identity or key with this id.
    #[error("no such entry: {0}")]
    NotFound(String),
    /// The key is in the keychain already (its id).
    #[error("this key is in the keychain already")]
    Duplicate(String),
    /// `keychain.toml` can't be written (unreadable, or from a newer OpenSesh).
    #[error("the keychain file is read-only")]
    ReadOnly,
    /// `keychain.toml` couldn't be written.
    #[error("could not write {path}")]
    Write {
        /// The file.
        path: String,
        /// Why.
        #[source]
        source: std::io::Error,
    },
}

impl KeychainOpError {
    /// A short code for the UI to word.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Vault(error) => match error {
                VaultError::WrongPassword { .. } => "wrong-password",
                VaultError::Wait(_) => "wait",
                VaultError::Locked => "locked",
                VaultError::Missing => "no-vault",
                VaultError::Keyring(_) => "keyring",
                VaultError::Write { .. } => "write",
                VaultError::Unreadable | VaultError::Newer(_) => "unreadable",
                _ => "vault",
            },
            Self::Key(error) => error.code(),
            Self::NeedsVault => "needs-vault",
            Self::NotFound(_) => "not-found",
            Self::Duplicate(_) => "duplicate",
            Self::ReadOnly => "read-only",
            Self::Write { .. } => "write",
        }
    }
}

/// How an identity's password changes when it is saved.
#[derive(Debug, Default)]
pub enum PasswordChange {
    /// As it was.
    #[default]
    Keep,
    /// No password any more.
    Clear,
    /// A new one.
    Set(Zeroizing<String>),
}

/// An identity as the editor saves it.
#[derive(Debug, Default)]
pub struct IdentityEdit {
    /// Empty for a new identity.
    pub id: String,
    /// Display name.
    pub name: String,
    /// User name.
    pub user: String,
    /// The password.
    pub password: PasswordChange,
    /// Key id, if any.
    pub key: Option<String>,
    /// Notes.
    pub notes: String,
}

/// The vault and `keychain.toml`.
#[derive(Debug)]
pub struct Keychain {
    /// `keychain.toml`, when on disk.
    file_path: Option<PathBuf>,
    /// The vault.
    pub vault: Vault,
    /// Identities and keys.
    pub file: KeychainFile,
    /// Problems reading `keychain.toml`.
    pub warnings: Vec<Warning>,
    /// `keychain.toml` couldn't be read: never saved over.
    locked_file: bool,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

impl Keychain {
    /// The keychain of `config_dir` and `data_dir` (`None`: in memory only), with `store` as the
    /// system keyring.
    #[must_use]
    pub fn open(dirs: Option<(&Path, &Path)>, store: Arc<dyn KeyStore>) -> Self {
        let (file_path, vault) = match dirs {
            Some((config, data)) => (
                Some(config.join(KEYCHAIN_FILE)),
                Vault::open(
                    Some(data.join(VAULT_FILE)),
                    Some(data.join(ATTEMPTS_FILE)),
                    store,
                ),
            ),
            None => (None, Vault::in_memory(store)),
        };
        let mut keychain = Self {
            file_path,
            vault,
            file: KeychainFile::default(),
            warnings: Vec::new(),
            locked_file: false,
        };
        keychain.reload();
        keychain
    }

    /// Reads `keychain.toml` again (after it changed on disk).
    pub fn reload(&mut self) {
        let Some(path) = &self.file_path else {
            return;
        };
        match KeychainFile::load(path) {
            Ok((file, warnings)) => {
                self.locked_file = file.read_only;
                self.file = file;
                self.warnings = warnings;
            }
            Err(error) => {
                self.locked_file = true;
                self.file = KeychainFile::default();
                self.warnings = vec![Warning {
                    key: KEYCHAIN_FILE.to_owned(),
                    message: error.to_string(),
                }];
            }
        }
    }

    /// Whether `keychain.toml` can't be changed.
    #[must_use]
    pub fn read_only(&self) -> bool {
        self.locked_file
    }

    /// Where `keychain.toml` is.
    #[must_use]
    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    fn save_file(&self, file: &KeychainFile) -> Result<(), KeychainOpError> {
        if self.locked_file {
            return Err(KeychainOpError::ReadOnly);
        }
        let Some(path) = &self.file_path else {
            return Ok(());
        };
        let text = file
            .to_toml_string()
            .map_err(|message| KeychainOpError::Write {
                path: path.display().to_string(),
                source: std::io::Error::other(message),
            })?;
        fsutil::atomic_write(path, text.as_bytes(), DEFAULT_BACKUPS).map_err(|source| {
            KeychainOpError::Write {
                path: path.display().to_string(),
                source,
            }
        })?;
        Ok(())
    }

    /// Commits `file`: written first, kept in memory only if that worked.
    fn commit(&mut self, file: KeychainFile) -> Result<(), KeychainOpError> {
        self.save_file(&file)?;
        self.file = file;
        Ok(())
    }

    /// Makes sure a vault exists: without one, a vault held by the system keyring is created.
    ///
    /// # Errors
    ///
    /// [`KeychainOpError::NeedsVault`] when there is no keyring to hold its key.
    pub fn ensure_vault(&mut self) -> Result<(), KeychainOpError> {
        if self.vault.status() != Status::Missing {
            return Ok(());
        }
        if self.vault.store().check().is_err() {
            return Err(KeychainOpError::NeedsVault);
        }
        self.vault.create(None, KdfParams::RECOMMENDED)?;
        Ok(())
    }

    /// Creates the vault protected by `password` (when there is none yet).
    ///
    /// # Errors
    ///
    /// When there is a vault already, or it can't be written.
    pub fn create_with_password(
        &mut self,
        password: &[u8],
        params: KdfParams,
    ) -> Result<(), KeychainOpError> {
        self.vault.create(Some(password), params)?;
        Ok(())
    }

    /// Removes vault secrets nothing refers to (left by entries deleted while it was locked).
    /// Does nothing unless `keychain.toml` was read without trouble.
    ///
    /// # Errors
    ///
    /// When the vault is locked or can't be written.
    pub fn collect_garbage(&mut self) -> Result<usize, KeychainOpError> {
        if self.locked_file || self.vault.status() != Status::Unlocked {
            return Ok(0);
        }
        let used = self.file.secret_ids();
        Ok(self.vault.retain(|id| used.contains(id))?)
    }

    /// Saves an identity; its id.
    ///
    /// # Errors
    ///
    /// When a password must be stored and the vault can't take it (locked, or none and no
    /// keyring), the key is unknown, or a file can't be written.
    pub fn save_identity(&mut self, edit: IdentityEdit) -> Result<String, KeychainOpError> {
        if self.locked_file {
            return Err(KeychainOpError::ReadOnly);
        }
        let mut file = self.file.clone();
        let existing = if edit.id.is_empty() {
            None
        } else {
            Some(
                file.identities
                    .iter()
                    .position(|identity| identity.id == edit.id)
                    .ok_or_else(|| KeychainOpError::NotFound(edit.id.clone()))?,
            )
        };
        if let Some(key) = &edit.key {
            if file.key(key).is_none() {
                return Err(KeychainOpError::NotFound(key.clone()));
            }
        }
        let mut identity = existing
            .and_then(|index| file.identities.get(index).cloned())
            .unwrap_or_else(|| Identity {
                id: new_id(),
                ..Identity::default()
            });
        let old_password = identity.password.as_deref().and_then(parse_ref);
        let mut doomed = None;
        let mut inserted = None;
        match edit.password {
            PasswordChange::Keep => {}
            PasswordChange::Clear => {
                identity.password = None;
                doomed = old_password;
            }
            PasswordChange::Set(password) => {
                self.ensure_vault()?;
                let data = Zeroizing::new(password.as_bytes().to_vec());
                let id = match old_password {
                    Some(id) => {
                        self.vault.replace(id, SecretKind::Password, data)?;
                        id
                    }
                    None => {
                        let id = self.vault.insert(SecretKind::Password, data)?;
                        inserted = Some(id);
                        id
                    }
                };
                identity.password = Some(secret_ref(id));
            }
        }
        identity.name = edit.name.trim().to_owned();
        identity.user = edit.user.trim().to_owned();
        if identity.name.is_empty() {
            identity.name = if identity.user.is_empty() {
                "Identity".to_owned()
            } else {
                identity.user.clone()
            };
        }
        identity.key = edit.key;
        identity.notes = edit.notes;
        let id = identity.id.clone();
        match existing {
            Some(index) => {
                if let Some(slot) = file.identities.get_mut(index) {
                    *slot = identity;
                }
            }
            None => file.identities.push(identity),
        }
        if let Err(error) = self.commit(file) {
            if let Some(inserted) = inserted {
                self.forget_secret(inserted);
            }
            return Err(error);
        }
        if let Some(doomed) = doomed {
            self.forget_secret(doomed);
        }
        Ok(id)
    }

    /// Removes a secret nothing refers to any more; while locked it waits for the next
    /// [`Keychain::collect_garbage`].
    fn forget_secret(&mut self, id: ulid::Ulid) {
        if self.vault.status() == Status::Unlocked {
            if let Err(error) = self.vault.remove(&[id]) {
                tracing::warn!("could not remove a secret from the vault: {error}");
            }
        }
    }

    /// Deletes an identity and its password.
    ///
    /// # Errors
    ///
    /// When it doesn't exist or `keychain.toml` can't be written.
    pub fn delete_identity(&mut self, id: &str) -> Result<(), KeychainOpError> {
        let mut file = self.file.clone();
        let index = file
            .identities
            .iter()
            .position(|identity| identity.id == id)
            .ok_or_else(|| KeychainOpError::NotFound(id.to_owned()))?;
        let removed = file.identities.remove(index);
        self.commit(file)?;
        if let Some(secret) = removed.password.as_deref().and_then(parse_ref) {
            self.forget_secret(secret);
        }
        Ok(())
    }

    /// The password of identity `id` (for the SSH client, or to copy it).
    ///
    /// # Errors
    ///
    /// When the identity or its password doesn't exist, or the vault is locked.
    pub fn identity_password(&self, id: &str) -> Result<Zeroizing<String>, KeychainOpError> {
        let identity = self
            .file
            .identity(id)
            .ok_or_else(|| KeychainOpError::NotFound(id.to_owned()))?;
        let secret = identity
            .password
            .as_deref()
            .and_then(parse_ref)
            .ok_or_else(|| KeychainOpError::NotFound(format!("{id} password")))?;
        let bytes = &self.vault.get(&secret)?.data;
        Ok(Zeroizing::new(String::from_utf8_lossy(bytes).into_owned()))
    }

    /// Adds `key` under `name`; its id. A key already here (same fingerprint) is refused.
    ///
    /// # Errors
    ///
    /// [`KeychainOpError::Duplicate`], or when the vault can't take it.
    pub fn add_key(
        &mut self,
        key: &PrivateKey,
        name: &str,
        origin: &str,
    ) -> Result<String, KeychainOpError> {
        if self.locked_file {
            return Err(KeychainOpError::ReadOnly);
        }
        let info = keys::info(key);
        if let Some(existing) = self.file.key_by_fingerprint(&info.fingerprint) {
            return Err(KeychainOpError::Duplicate(existing.id.clone()));
        }
        self.ensure_vault()?;
        let secret = self
            .vault
            .insert(SecretKind::PrivateKey, keys::to_vault(key)?)?;
        let mut file = self.file.clone();
        let name = name.trim();
        let entry = KeyEntry {
            id: new_id(),
            name: if name.is_empty() {
                if info.comment.is_empty() {
                    info.label.clone()
                } else {
                    info.comment.clone()
                }
            } else {
                name.to_owned()
            },
            algorithm: info.algorithm,
            bits: info.bits,
            public: info.public,
            fingerprint: info.fingerprint,
            private: Some(secret_ref(secret)),
            origin: origin.to_owned(),
            created: now_secs(),
            extra: toml::Table::new(),
        };
        let id = entry.id.clone();
        file.keys.push(entry);
        if let Err(error) = self.commit(file) {
            self.forget_secret(secret);
            return Err(error);
        }
        Ok(id)
    }

    /// Generates a key of `kind` and adds it; its id.
    ///
    /// # Errors
    ///
    /// As [`keys::generate`] and [`Keychain::add_key`].
    pub fn generate_key(
        &mut self,
        kind: KeyType,
        name: &str,
        comment: &str,
    ) -> Result<String, KeychainOpError> {
        // Check before spending seconds on RSA.
        if self.locked_file {
            return Err(KeychainOpError::ReadOnly);
        }
        self.ensure_vault()?;
        if self.vault.status() != Status::Unlocked {
            return Err(VaultError::Locked.into());
        }
        let key = keys::generate(kind, comment)?;
        self.add_key(&key, name, "generated")
    }

    /// Imports the key file in `bytes` (with `passphrase` when it has one); its id.
    ///
    /// # Errors
    ///
    /// As [`keys::import`] and [`Keychain::add_key`].
    pub fn import_key(
        &mut self,
        bytes: &[u8],
        passphrase: Option<&[u8]>,
        name: &str,
    ) -> Result<String, KeychainOpError> {
        let imported = keys::import(bytes, passphrase)?;
        let origin = match imported.format {
            KeyFormat::OpenSsh => "openssh",
            KeyFormat::Ppk => "ppk",
        };
        self.add_key(&imported.key, name, origin)
    }

    /// The private key `id`.
    ///
    /// # Errors
    ///
    /// When it doesn't exist, has no private part, or the vault is locked.
    pub fn private_key(&self, id: &str) -> Result<PrivateKey, KeychainOpError> {
        let entry = self
            .file
            .key(id)
            .ok_or_else(|| KeychainOpError::NotFound(id.to_owned()))?;
        let secret = entry
            .private
            .as_deref()
            .and_then(parse_ref)
            .ok_or_else(|| KeychainOpError::NotFound(format!("{id} private key")))?;
        Ok(keys::from_vault(&self.vault.get(&secret)?.data)?)
    }

    /// Key `id` as an OpenSSH private key file, encrypted with `passphrase` when given.
    ///
    /// # Errors
    ///
    /// As [`Keychain::private_key`] and [`keys::export_openssh`].
    pub fn export_private(
        &self,
        id: &str,
        passphrase: Option<&[u8]>,
    ) -> Result<Zeroizing<String>, KeychainOpError> {
        let key = self.private_key(id)?;
        Ok(keys::export_openssh(&key, passphrase)?)
    }

    /// Renames key `id`.
    ///
    /// # Errors
    ///
    /// When it doesn't exist or the file can't be written.
    pub fn rename_key(&mut self, id: &str, name: &str) -> Result<(), KeychainOpError> {
        let mut file = self.file.clone();
        let entry = file
            .keys
            .iter_mut()
            .find(|key| key.id == id)
            .ok_or_else(|| KeychainOpError::NotFound(id.to_owned()))?;
        let name = name.trim();
        if !name.is_empty() {
            entry.name = name.to_owned();
        }
        self.commit(file)
    }

    /// Deletes key `id` and its private part; identities that used it keep their password.
    ///
    /// # Errors
    ///
    /// When it doesn't exist or the file can't be written.
    pub fn delete_key(&mut self, id: &str) -> Result<(), KeychainOpError> {
        let mut file = self.file.clone();
        let index = file
            .keys
            .iter()
            .position(|key| key.id == id)
            .ok_or_else(|| KeychainOpError::NotFound(id.to_owned()))?;
        let removed = file.keys.remove(index);
        for identity in &mut file.identities {
            if identity.key.as_deref() == Some(id) {
                identity.key = None;
            }
        }
        self.commit(file)?;
        if let Some(secret) = removed.private.as_deref().and_then(parse_ref) {
            self.forget_secret(secret);
        }
        Ok(())
    }

    /// Deletes the vault (a lost master password): every password and private key is gone, and
    /// the entries that referred to them say so.
    ///
    /// # Errors
    ///
    /// When the vault or `keychain.toml` can't be written.
    pub fn reset_vault(&mut self) -> Result<(), KeychainOpError> {
        self.vault.reset()?;
        if self.locked_file {
            return Ok(());
        }
        let mut file = self.file.clone();
        for identity in &mut file.identities {
            identity.password = None;
        }
        for key in &mut file.keys {
            key.private = None;
        }
        self.commit(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryKeyStore;

    fn set(text: &str) -> PasswordChange {
        PasswordChange::Set(Zeroizing::new(text.to_owned()))
    }

    #[test]
    fn identities_keep_their_secret_in_the_vault() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryKeyStore::new());
        let mut keychain = Keychain::open(
            Some((dir.path(), dir.path())),
            Arc::clone(&store) as Arc<dyn KeyStore>,
        );
        assert_eq!(keychain.vault.status(), Status::Missing);
        let id = keychain
            .save_identity(IdentityEdit {
                name: "deploy".into(),
                user: "deploy".into(),
                password: set("pw-one"),
                ..IdentityEdit::default()
            })
            .unwrap();
        // The first secret created a vault held by the keyring.
        assert_eq!(keychain.vault.status(), Status::Unlocked);
        assert_eq!(keychain.identity_password(&id).unwrap().as_str(), "pw-one");
        // Replacing keeps one secret; clearing removes it.
        keychain
            .save_identity(IdentityEdit {
                id: id.clone(),
                name: "deploy".into(),
                password: set("pw-two"),
                ..IdentityEdit::default()
            })
            .unwrap();
        assert_eq!(keychain.vault.len(), 1);
        assert_eq!(keychain.identity_password(&id).unwrap().as_str(), "pw-two");
        keychain
            .save_identity(IdentityEdit {
                id: id.clone(),
                name: "deploy".into(),
                password: PasswordChange::Clear,
                ..IdentityEdit::default()
            })
            .unwrap();
        assert_eq!(keychain.vault.len(), 0);

        // Reopened from disk.
        let again = Keychain::open(Some((dir.path(), dir.path())), store);
        assert_eq!(again.file.identities.len(), 1);
        assert_eq!(again.file.identities[0].user, "");
    }

    #[test]
    fn without_a_keyring_a_master_password_is_needed() {
        let store = Arc::new(MemoryKeyStore::unavailable());
        let mut keychain = Keychain::open(None, store);
        let error = keychain
            .save_identity(IdentityEdit {
                password: set("x"),
                ..IdentityEdit::default()
            })
            .unwrap_err();
        assert!(matches!(error, KeychainOpError::NeedsVault));
        assert_eq!(error.code(), "needs-vault");
        // Nothing was saved half-way.
        assert!(keychain.file.identities.is_empty());
        keychain
            .create_with_password(b"master", KdfParams::INSECURE_FOR_TESTS)
            .unwrap();
        keychain
            .save_identity(IdentityEdit {
                password: set("x"),
                ..IdentityEdit::default()
            })
            .unwrap();
    }

    #[test]
    fn keys_and_their_identities() {
        let store = Arc::new(MemoryKeyStore::new());
        let mut keychain = Keychain::open(None, store);
        let key = keychain
            .generate_key(KeyType::Ed25519, "laptop", "me@laptop")
            .unwrap();
        let entry = keychain.file.key(&key).unwrap().clone();
        assert_eq!(entry.name, "laptop");
        assert_eq!(entry.origin, "generated");
        assert!(entry.public.ends_with(" me@laptop"));
        let private = keychain.private_key(&key).unwrap();
        assert_eq!(keys::info(&private).fingerprint, entry.fingerprint);
        // The same key again is refused.
        let again = keychain.add_key(&private, "copy", "openssh").unwrap_err();
        assert!(matches!(again, KeychainOpError::Duplicate(ref id) if *id == key));

        let identity = keychain
            .save_identity(IdentityEdit {
                name: "me".into(),
                key: Some(key.clone()),
                ..IdentityEdit::default()
            })
            .unwrap();
        let exported = keychain.export_private(&key, Some(b"pp")).unwrap();
        assert!(keys::needs_passphrase(exported.as_bytes()).unwrap());
        keychain.delete_key(&key).unwrap();
        assert_eq!(keychain.file.identity(&identity).unwrap().key, None);
        assert_eq!(keychain.vault.len(), 0);
        assert!(matches!(
            keychain.save_identity(IdentityEdit {
                key: Some(key),
                ..IdentityEdit::default()
            }),
            Err(KeychainOpError::NotFound(_))
        ));
    }

    #[test]
    fn secrets_of_entries_deleted_while_locked_are_collected_later() {
        let store = Arc::new(MemoryKeyStore::new());
        let mut keychain = Keychain::open(None, store);
        keychain
            .create_with_password(b"m", KdfParams::INSECURE_FOR_TESTS)
            .unwrap();
        let id = keychain
            .save_identity(IdentityEdit {
                password: set("secret"),
                ..IdentityEdit::default()
            })
            .unwrap();
        keychain.vault.lock();
        keychain.delete_identity(&id).unwrap();
        keychain.vault.unlock(b"m", 0).unwrap();
        assert_eq!(keychain.vault.len(), 1);
        assert_eq!(keychain.collect_garbage().unwrap(), 1);
        assert_eq!(keychain.vault.len(), 0);
    }

    #[test]
    fn a_reset_clears_the_references() {
        let store = Arc::new(MemoryKeyStore::new());
        let mut keychain = Keychain::open(None, store);
        let key = keychain.generate_key(KeyType::EcdsaP256, "", "c").unwrap();
        let identity = keychain
            .save_identity(IdentityEdit {
                password: set("p"),
                key: Some(key.clone()),
                ..IdentityEdit::default()
            })
            .unwrap();
        keychain.reset_vault().unwrap();
        assert_eq!(keychain.vault.status(), Status::Missing);
        assert_eq!(keychain.file.identity(&identity).unwrap().password, None);
        assert_eq!(keychain.file.key(&key).unwrap().private, None);
        // The public half stays usable.
        assert!(!keychain.file.key(&key).unwrap().public.is_empty());
    }
}
