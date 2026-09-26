//! The vault (PLAN §8, ADR 0023): the secrets of `vault.bin`, locked or unlocked, and who holds
//! its key (the system keyring, or the master password with an optional copy in the keyring).
//!
//! Every change is written at once, atomically and without backups: a backup would keep old
//! secrets, and a copy sealed under a master password that was since changed. Nothing here is
//! written while the vault is in memory only (test runs). Calls can be slow (Argon2id, the
//! keyring): keep them off the GUI thread.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use opensesh_core::fsutil;
use ulid::Ulid;
use zeroize::Zeroizing;

use crate::VaultError;
use crate::backoff::Backoff;
use crate::crypto::{KdfParams, Key, NONCE_LEN};
use crate::format::{self, Header, Holder, MAX_SECRET, Secret, SecretKind};
use crate::store::KeyStore;

/// File name in the data folder.
pub const VAULT_FILE: &str = "vault.bin";

/// References to vault secrets in other files look like `vault:<ulid>`.
pub const REF_PREFIX: &str = "vault:";

/// `vault:<id>`.
#[must_use]
pub fn secret_ref(id: Ulid) -> String {
    format!("{REF_PREFIX}{id}")
}

/// The id in a `vault:<id>` reference.
#[must_use]
pub fn parse_ref(text: &str) -> Option<Ulid> {
    Ulid::from_string(text.strip_prefix(REF_PREFIX)?).ok()
}

/// Where the vault is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// There is no vault yet.
    Missing,
    /// `vault.bin` can't be read (damaged, or written by a newer OpenSesh); it is never
    /// overwritten, only reset on request.
    Unreadable,
    /// The secrets are sealed.
    Locked,
    /// The secrets are in memory.
    Unlocked,
}

/// Who holds the vault key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protection {
    /// The system keyring.
    Keyring,
    /// The master password.
    Password,
}

struct Unlocked {
    header: Header,
    key: Key,
    entries: BTreeMap<Ulid, Secret>,
    /// The file as last written.
    file: Vec<u8>,
    /// With a master password: the keyring has a copy of the key too.
    remembered: bool,
}

enum State {
    Missing,
    Unreadable(String),
    Locked { header: Header, file: Vec<u8> },
    Unlocked(Box<Unlocked>),
}

/// The vault. `Debug` shows its state, never a secret.
pub struct Vault {
    /// `None`: in memory only.
    path: Option<PathBuf>,
    store: Arc<dyn KeyStore>,
    backoff: Backoff,
    state: State,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("path", &self.path)
            .field("status", &self.status())
            .field("protection", &self.protection())
            .finish_non_exhaustive()
    }
}

impl Vault {
    /// The vault in `path` (not unlocked yet), with the wrong-password waits kept in
    /// `attempts`. `None` for both keeps everything in memory.
    #[must_use]
    pub fn open(
        path: Option<PathBuf>,
        attempts: Option<PathBuf>,
        store: Arc<dyn KeyStore>,
    ) -> Self {
        let state = match &path {
            None => State::Missing,
            Some(path) => match std::fs::read(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => State::Missing,
                Err(error) => {
                    State::Unreadable(format!("could not read {}: {error}", path.display()))
                }
                Ok(file) => match format::read_header(&file) {
                    Ok((header, _)) => State::Locked { header, file },
                    Err(error) => State::Unreadable(error.to_string()),
                },
            },
        };
        Self {
            path,
            store,
            backoff: Backoff::load(attempts),
            state,
        }
    }

    /// A vault that lives in memory only (test runs).
    #[must_use]
    pub fn in_memory(store: Arc<dyn KeyStore>) -> Self {
        Self::open(None, None, store)
    }

    /// The keyring store it uses.
    #[must_use]
    pub fn store(&self) -> &Arc<dyn KeyStore> {
        &self.store
    }

    /// Where the vault is.
    #[must_use]
    pub fn status(&self) -> Status {
        match self.state {
            State::Missing => Status::Missing,
            State::Unreadable(_) => Status::Unreadable,
            State::Locked { .. } => Status::Locked,
            State::Unlocked(_) => Status::Unlocked,
        }
    }

    /// Why the vault can't be read.
    #[must_use]
    pub fn problem(&self) -> Option<&str> {
        match &self.state {
            State::Unreadable(problem) => Some(problem),
            _ => None,
        }
    }

    fn header(&self) -> Option<&Header> {
        match &self.state {
            State::Locked { header, .. } => Some(header),
            State::Unlocked(open) => Some(&open.header),
            State::Missing | State::Unreadable(_) => None,
        }
    }

    /// Who holds the key (none without a readable vault).
    #[must_use]
    pub fn protection(&self) -> Option<Protection> {
        self.header().map(|header| match header.holder {
            Holder::Keyring => Protection::Keyring,
            Holder::Password(_) => Protection::Password,
        })
    }

    /// With a master password: whether the keyring has a copy of the key ("remember on this
    /// computer"). Known once unlocked.
    #[must_use]
    pub fn remembered(&self) -> bool {
        matches!(&self.state, State::Unlocked(open) if open.remembered)
    }

    /// The Argon2id costs of the master password.
    #[must_use]
    pub fn kdf_params(&self) -> Option<KdfParams> {
        match &self.header()?.holder {
            Holder::Password(slot) => Some(slot.params),
            Holder::Keyring => None,
        }
    }

    /// The name of this vault's keyring entry.
    #[must_use]
    pub fn keyring_name(&self) -> Option<String> {
        self.header().map(|header| entry_name(&header.vault_id))
    }

    /// Seconds before a master password may be tried again at `now` (seconds since the epoch).
    pub fn wait(&mut self, now: u64) -> u64 {
        self.backoff.wait(now)
    }

    /// Wrong master passwords in a row.
    #[must_use]
    pub fn failures(&self) -> u32 {
        self.backoff.failures()
    }

    fn unlocked(&self) -> Result<&Unlocked, VaultError> {
        match &self.state {
            State::Unlocked(open) => Ok(open),
            State::Locked { .. } => Err(VaultError::Locked),
            State::Missing => Err(VaultError::Missing),
            State::Unreadable(_) => Err(VaultError::Unreadable),
        }
    }

    /// Writes `entries` under `header` and `key`; the new file on success.
    fn write(
        &self,
        header: &mut Header,
        key: &Key,
        entries: &BTreeMap<Ulid, Secret>,
    ) -> Result<Vec<u8>, VaultError> {
        let file = format::seal_file(header, key, entries)?;
        if let Some(path) = &self.path {
            fsutil::atomic_write(path, &file, 0).map_err(|source| VaultError::Write {
                path: path.display().to_string(),
                source,
            })?;
        }
        Ok(file)
    }

    /// Creates an empty vault, unlocked: with `password`, protected by it; without, its key goes
    /// to the keyring.
    ///
    /// # Errors
    ///
    /// [`VaultError::Exists`] when there is a vault already (even an unreadable one), a keyring
    /// error, or the write's.
    pub fn create(&mut self, password: Option<&[u8]>, params: KdfParams) -> Result<(), VaultError> {
        if !matches!(self.state, State::Missing) {
            return Err(VaultError::Exists);
        }
        let vault_id = crate::crypto::random_array::<16>()?;
        let key = Key::random()?;
        let holder = match password {
            Some(password) => {
                Holder::Password(Header::password_slot(vault_id, &key, password, params)?)
            }
            None => {
                self.store.set(&entry_name(&vault_id), key.as_bytes())?;
                Holder::Keyring
            }
        };
        let mut header = Header {
            vault_id,
            holder,
            body_nonce: [0; NONCE_LEN],
        };
        let entries = BTreeMap::new();
        let file = match self.write(&mut header, &key, &entries) {
            Ok(file) => file,
            Err(error) => {
                if password.is_none() {
                    let _ = self.store.delete(&entry_name(&vault_id));
                }
                return Err(error);
            }
        };
        self.state = State::Unlocked(Box::new(Unlocked {
            header,
            key,
            entries,
            file,
            remembered: false,
        }));
        Ok(())
    }

    /// Unlocks with the key in the keyring: the vault's own key, or a remembered one. `false`
    /// when the keyring has none (or a stale one).
    ///
    /// # Errors
    ///
    /// When the keyring can't be read, or the vault is damaged.
    pub fn unlock_with_keyring(&mut self) -> Result<bool, VaultError> {
        let (header, file) = match &self.state {
            State::Unlocked(_) => return Ok(true),
            State::Locked { header, file } => (header, file),
            State::Missing | State::Unreadable(_) => return Ok(false),
        };
        let Some(stored) = self.store.get(&entry_name(&header.vault_id))? else {
            return Ok(false);
        };
        let Some(key) = Key::from_slice(&stored) else {
            tracing::warn!("the keyring entry of the vault has the wrong size; ignored");
            return Ok(false);
        };
        let entries = match format::open_file(file, &key) {
            Ok((_, entries)) => entries,
            Err(VaultError::Decrypt) => {
                tracing::warn!("the keyring entry of the vault doesn't open it; ignored");
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        let remembered = matches!(header.holder, Holder::Password(_));
        self.state = State::Unlocked(Box::new(Unlocked {
            header: header.clone(),
            key,
            entries,
            file: file.clone(),
            remembered,
        }));
        Ok(true)
    }

    /// Runs `password` against the master password slot at `now`, with the waits.
    fn try_password(&mut self, password: &[u8], now: u64) -> Result<Key, VaultError> {
        let header = self.header().cloned().ok_or(VaultError::Missing)?;
        if !matches!(header.holder, Holder::Password(_)) {
            return Err(VaultError::WrongMode);
        }
        let wait = self.backoff.wait(now);
        if wait > 0 {
            return Err(VaultError::Wait(wait));
        }
        match header.unwrap_key(password) {
            Ok(key) => {
                self.backoff.succeeded();
                Ok(key)
            }
            Err(VaultError::Decrypt) => {
                self.backoff.failed(now);
                Err(VaultError::WrongPassword {
                    wait: self.backoff.wait(now),
                })
            }
            Err(error) => Err(error),
        }
    }

    /// Unlocks with the master password at `now` (seconds since the epoch).
    ///
    /// # Errors
    ///
    /// [`VaultError::Wait`] while waiting after wrong passwords (nothing is tried),
    /// [`VaultError::WrongPassword`] with the next wait, [`VaultError::WrongMode`] when the
    /// keyring holds the key, [`VaultError::Format`] for a damaged file.
    pub fn unlock(&mut self, password: &[u8], now: u64) -> Result<(), VaultError> {
        let file = match &self.state {
            State::Unlocked(_) => return Ok(()),
            State::Locked { file, .. } => file.clone(),
            State::Missing => return Err(VaultError::Missing),
            State::Unreadable(_) => return Err(VaultError::Unreadable),
        };
        let key = self.try_password(password, now)?;
        let (header, entries) = format::open_file(&file, &key).map_err(|error| match error {
            VaultError::Decrypt => VaultError::Format("the secrets don't match their key"),
            other => other,
        })?;
        let remembered = self
            .store
            .get(&entry_name(&header.vault_id))
            .ok()
            .flatten()
            .is_some_and(|stored| stored.as_slice() == key.as_bytes());
        self.state = State::Unlocked(Box::new(Unlocked {
            header,
            key,
            entries,
            file,
            remembered,
        }));
        Ok(())
    }

    /// Forgets the secrets and the key until the next unlock.
    pub fn lock(&mut self) {
        if let State::Unlocked(open) = &self.state {
            self.state = State::Locked {
                header: open.header.clone(),
                file: open.file.clone(),
            };
        }
    }

    /// Checks `password` against the master password of an unlocked vault (before changing
    /// security settings), with the same waits as unlocking.
    ///
    /// # Errors
    ///
    /// As [`Vault::unlock`].
    pub fn verify_password(&mut self, password: &[u8], now: u64) -> Result<(), VaultError> {
        let current = self.unlocked()?.key.clone();
        let key = self.try_password(password, now)?;
        if key.as_bytes() == current.as_bytes() {
            Ok(())
        } else {
            Err(VaultError::Format("the master password opens another key"))
        }
    }

    /// How many secrets it holds (0 while locked).
    #[must_use]
    pub fn len(&self) -> usize {
        self.unlocked().map_or(0, |open| open.entries.len())
    }

    /// Whether it holds no secret (or is locked).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The ids of every secret.
    ///
    /// # Errors
    ///
    /// When the vault isn't unlocked.
    pub fn ids(&self) -> Result<Vec<Ulid>, VaultError> {
        Ok(self.unlocked()?.entries.keys().copied().collect())
    }

    /// Secret `id`.
    ///
    /// # Errors
    ///
    /// When the vault isn't unlocked or there is no such secret.
    pub fn get(&self, id: &Ulid) -> Result<&Secret, VaultError> {
        self.unlocked()?
            .entries
            .get(id)
            .ok_or_else(|| VaultError::NotFound(id.to_string()))
    }

    /// Applies `change` to a copy of the secrets and writes them; the vault changes only if the
    /// write succeeds.
    fn change<T>(
        &mut self,
        change: impl FnOnce(&mut BTreeMap<Ulid, Secret>) -> Result<T, VaultError>,
    ) -> Result<T, VaultError> {
        let open = self.unlocked()?;
        let mut entries = open.entries.clone();
        let mut header = open.header.clone();
        let key = open.key.clone();
        let result = change(&mut entries)?;
        let file = self.write(&mut header, &key, &entries)?;
        if let State::Unlocked(open) = &mut self.state {
            open.entries = entries;
            open.header = header;
            open.file = file;
        }
        Ok(result)
    }

    /// Stores a new secret; its id.
    ///
    /// # Errors
    ///
    /// When locked, too large, or the write fails.
    pub fn insert(
        &mut self,
        kind: SecretKind,
        data: Zeroizing<Vec<u8>>,
    ) -> Result<Ulid, VaultError> {
        if data.len() > MAX_SECRET {
            return Err(VaultError::Format("a secret is too large"));
        }
        self.change(|entries| {
            let id = Ulid::generate();
            entries.insert(id, Secret { kind, data });
            Ok(id)
        })
    }

    /// Replaces secret `id` (or stores it under that id).
    ///
    /// # Errors
    ///
    /// When locked, too large, or the write fails.
    pub fn replace(
        &mut self,
        id: Ulid,
        kind: SecretKind,
        data: Zeroizing<Vec<u8>>,
    ) -> Result<(), VaultError> {
        if data.len() > MAX_SECRET {
            return Err(VaultError::Format("a secret is too large"));
        }
        self.change(|entries| {
            entries.insert(id, Secret { kind, data });
            Ok(())
        })
    }

    /// Removes the secrets `ids` names; how many there were. Nothing is written when none.
    ///
    /// # Errors
    ///
    /// When locked or the write fails.
    pub fn remove(&mut self, ids: &[Ulid]) -> Result<usize, VaultError> {
        self.retain(|id| !ids.contains(id))
    }

    /// Keeps only the secrets `keep` accepts; how many went. Nothing is written when none.
    ///
    /// # Errors
    ///
    /// When locked or the write fails.
    pub fn retain(&mut self, keep: impl Fn(&Ulid) -> bool) -> Result<usize, VaultError> {
        let doomed = self
            .unlocked()?
            .entries
            .keys()
            .filter(|id| !keep(id))
            .count();
        if doomed == 0 {
            return Ok(0);
        }
        self.change(|entries| {
            entries.retain(|id, _| keep(id));
            Ok(doomed)
        })
    }

    /// Protects the unlocked vault with `password` (from the keyring, or a new password after
    /// [`Vault::verify_password`]). With `remember`, the keyring keeps a copy of the key;
    /// without, any copy there is removed.
    ///
    /// # Errors
    ///
    /// When locked, the keyring or the write fails. If only removing the old keyring copy
    /// fails, the password is set, the vault reports itself as remembered, and the keyring
    /// error is returned.
    pub fn set_password(
        &mut self,
        password: &[u8],
        params: KdfParams,
        remember: bool,
    ) -> Result<(), VaultError> {
        let open = self.unlocked()?;
        let name = entry_name(&open.header.vault_id);
        let key = open.key.clone();
        let entries = open.entries.clone();
        let mut header = Header {
            vault_id: open.header.vault_id,
            holder: Holder::Password(Header::password_slot(
                open.header.vault_id,
                &key,
                password,
                params,
            )?),
            body_nonce: [0; NONCE_LEN],
        };
        if remember {
            self.store.set(&name, key.as_bytes())?;
        }
        let file = self.write(&mut header, &key, &entries)?;
        let forgotten = if remember {
            Ok(())
        } else {
            self.store.delete(&name)
        };
        if let State::Unlocked(open) = &mut self.state {
            open.header = header;
            open.file = file;
            open.remembered = remember || forgotten.is_err();
        }
        forgotten.map_err(VaultError::from)
    }

    /// Changes the master password after checking the current one; "remember" stays as it was.
    ///
    /// # Errors
    ///
    /// As [`Vault::verify_password`] and [`Vault::set_password`].
    pub fn change_password(
        &mut self,
        current: &[u8],
        new: &[u8],
        params: KdfParams,
        now: u64,
    ) -> Result<(), VaultError> {
        self.verify_password(current, now)?;
        let remember = self.remembered();
        self.set_password(new, params, remember)
    }

    /// Removes the master password after checking it: the keyring holds the key from now on.
    ///
    /// # Errors
    ///
    /// As [`Vault::verify_password`], or when the keyring or the write fails (nothing changes).
    pub fn remove_password(&mut self, current: &[u8], now: u64) -> Result<(), VaultError> {
        self.verify_password(current, now)?;
        let open = self.unlocked()?;
        let name = entry_name(&open.header.vault_id);
        let key = open.key.clone();
        let entries = open.entries.clone();
        let mut header = Header {
            vault_id: open.header.vault_id,
            holder: Holder::Keyring,
            body_nonce: [0; NONCE_LEN],
        };
        self.store.set(&name, key.as_bytes())?;
        let file = self.write(&mut header, &key, &entries)?;
        if let State::Unlocked(open) = &mut self.state {
            open.header = header;
            open.file = file;
            open.remembered = false;
        }
        Ok(())
    }

    /// With a master password: keeps a copy of the key in the keyring, or removes it.
    ///
    /// # Errors
    ///
    /// When locked, without a master password, or when the keyring fails.
    pub fn set_remember(&mut self, remember: bool) -> Result<(), VaultError> {
        let open = self.unlocked()?;
        if !matches!(open.header.holder, Holder::Password(_)) {
            return Err(VaultError::WrongMode);
        }
        let name = entry_name(&open.header.vault_id);
        if remember {
            self.store.set(&name, open.key.as_bytes())?;
        } else {
            self.store.delete(&name)?;
        }
        if let State::Unlocked(open) = &mut self.state {
            open.remembered = remember;
        }
        Ok(())
    }

    /// Deletes the vault, its keyring entry and the wrong-password waits: every secret is gone.
    ///
    /// # Errors
    ///
    /// When the file can't be removed (the keyring entry is removed on a best-effort basis).
    pub fn reset(&mut self) -> Result<(), VaultError> {
        if let Some(name) = self.keyring_name() {
            if let Err(error) = self.store.delete(&name) {
                tracing::warn!("could not remove the vault key from the keyring: {error}");
            }
        }
        if let Some(path) = &self.path {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(VaultError::Write {
                        path: path.display().to_string(),
                        source,
                    });
                }
            }
        }
        self.backoff.succeeded();
        self.state = State::Missing;
        Ok(())
    }
}

/// The keyring entry of vault `id`.
fn entry_name(id: &[u8; 16]) -> String {
    format!("vault-{}", Ulid::from_bytes(*id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryKeyStore;

    const FAST: KdfParams = KdfParams::INSECURE_FOR_TESTS;

    fn password(text: &str) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(text.as_bytes().to_vec())
    }

    fn on_disk(dir: &std::path::Path, store: &Arc<MemoryKeyStore>) -> Vault {
        Vault::open(
            Some(dir.join(VAULT_FILE)),
            Some(dir.join(crate::backoff::ATTEMPTS_FILE)),
            Arc::clone(store) as Arc<dyn KeyStore>,
        )
    }

    #[test]
    fn keyring_vault_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryKeyStore::new());
        let mut vault = on_disk(dir.path(), &store);
        assert_eq!(vault.status(), Status::Missing);
        vault.create(None, FAST).unwrap();
        assert_eq!(vault.protection(), Some(Protection::Keyring));
        assert_eq!(store.len(), 1);
        let id = vault
            .insert(SecretKind::Password, password("s3cret"))
            .unwrap();

        // A restart opens it from the keyring.
        let mut again = on_disk(dir.path(), &store);
        assert_eq!(again.status(), Status::Locked);
        assert!(again.unlock_with_keyring().unwrap());
        assert_eq!(again.get(&id).unwrap().data.as_slice(), b"s3cret");
        assert_eq!(again.get(&id).unwrap().kind, SecretKind::Password);
        assert!(matches!(again.unlock(b"x", 0), Ok(())));

        // Without its keyring entry it stays locked.
        let empty = Arc::new(MemoryKeyStore::new());
        let mut elsewhere = on_disk(dir.path(), &empty);
        assert!(!elsewhere.unlock_with_keyring().unwrap());
        assert_eq!(elsewhere.status(), Status::Locked);
        assert!(matches!(
            elsewhere.unlock(b"x", 0),
            Err(VaultError::WrongMode)
        ));
    }

    #[test]
    fn password_vault_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryKeyStore::new());
        let mut vault = on_disk(dir.path(), &store);
        vault.create(Some(b"master"), FAST).unwrap();
        assert!(store.is_empty());
        let id = vault
            .insert(SecretKind::PrivateKey, password("key bytes"))
            .unwrap();
        vault.lock();
        assert_eq!(vault.status(), Status::Locked);
        assert!(matches!(vault.get(&id), Err(VaultError::Locked)));
        assert!(!vault.unlock_with_keyring().unwrap());
        assert!(matches!(
            vault.unlock(b"wrong", 100),
            Err(VaultError::WrongPassword { wait: 0 })
        ));
        vault.unlock(b"master", 100).unwrap();
        assert_eq!(vault.get(&id).unwrap().data.as_slice(), b"key bytes");
        assert!(!vault.remembered());

        // Remember on this computer: a restart opens it without the password.
        vault.set_remember(true).unwrap();
        let mut again = on_disk(dir.path(), &store);
        assert!(again.unlock_with_keyring().unwrap());
        assert!(again.remembered());
        again.set_remember(false).unwrap();
        assert!(store.is_empty());
        let mut third = on_disk(dir.path(), &store);
        assert!(!third.unlock_with_keyring().unwrap());
    }

    #[test]
    fn switching_protection() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryKeyStore::new());
        let mut vault = on_disk(dir.path(), &store);
        vault.create(None, FAST).unwrap();
        let id = vault.insert(SecretKind::Password, password("pw")).unwrap();

        // Keyring to password: the keyring entry goes.
        vault.set_password(b"one", FAST, false).unwrap();
        assert_eq!(vault.protection(), Some(Protection::Password));
        assert!(store.is_empty());
        // Change it; the old one no longer opens it.
        assert!(matches!(
            vault.change_password(b"bad", b"two", FAST, 0),
            Err(VaultError::WrongPassword { .. })
        ));
        vault.change_password(b"one", b"two", FAST, 0).unwrap();
        let mut again = on_disk(dir.path(), &store);
        assert!(matches!(
            again.unlock(b"one", 0),
            Err(VaultError::WrongPassword { .. })
        ));
        again.unlock(b"two", 0).unwrap();
        assert_eq!(again.get(&id).unwrap().data.as_slice(), b"pw");

        // Back to the keyring.
        again.remove_password(b"two", 0).unwrap();
        assert_eq!(again.protection(), Some(Protection::Keyring));
        let mut third = on_disk(dir.path(), &store);
        assert!(third.unlock_with_keyring().unwrap());
        assert_eq!(third.get(&id).unwrap().data.as_slice(), b"pw");
    }

    #[test]
    fn backoff_blocks_guessing() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryKeyStore::new());
        let mut vault = on_disk(dir.path(), &store);
        vault.create(Some(b"right"), FAST).unwrap();
        vault.lock();
        let now = 5_000;
        for _ in 0..2 {
            assert!(matches!(
                vault.unlock(b"guess", now),
                Err(VaultError::WrongPassword { wait: 0 })
            ));
        }
        assert!(matches!(
            vault.unlock(b"guess", now),
            Err(VaultError::WrongPassword { wait: 5 })
        ));
        // Even the right password isn't tried while waiting.
        assert!(matches!(
            vault.unlock(b"right", now + 1),
            Err(VaultError::Wait(4))
        ));
        assert_eq!(vault.status(), Status::Locked);
        // Restarting doesn't help.
        let mut again = on_disk(dir.path(), &store);
        assert!(matches!(
            again.unlock(b"right", now + 2),
            Err(VaultError::Wait(3))
        ));
        assert!(matches!(
            again.unlock(b"guess", now + 5),
            Err(VaultError::WrongPassword { wait: 10 })
        ));
        again.unlock(b"right", now + 15).unwrap();
        assert_eq!(again.failures(), 0);
    }

    #[test]
    fn failed_writes_change_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryKeyStore::new());
        let mut vault = on_disk(dir.path(), &store);
        vault.create(None, FAST).unwrap();
        let id = vault.insert(SecretKind::Password, password("a")).unwrap();
        // Make the file's folder unusable: a directory where the temporary file would go
        // doesn't help on every platform, so point the vault at a path under a file instead.
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"").unwrap();
        vault.path = Some(blocker.join(VAULT_FILE));
        assert!(matches!(
            vault.insert(SecretKind::Password, password("b")),
            Err(VaultError::Write { .. })
        ));
        assert_eq!(vault.ids().unwrap(), vec![id]);
    }

    #[test]
    fn retain_and_reset() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryKeyStore::new());
        let mut vault = on_disk(dir.path(), &store);
        vault.create(None, FAST).unwrap();
        let a = vault.insert(SecretKind::Password, password("a")).unwrap();
        let b = vault.insert(SecretKind::Password, password("b")).unwrap();
        assert_eq!(vault.retain(|id| *id == a).unwrap(), 1);
        assert_eq!(vault.ids().unwrap(), vec![a]);
        assert_eq!(vault.remove(&[b]).unwrap(), 0);
        vault.reset().unwrap();
        assert_eq!(vault.status(), Status::Missing);
        assert!(!dir.path().join(VAULT_FILE).exists());
        assert!(store.is_empty());
    }

    #[test]
    fn unreadable_files_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(VAULT_FILE);
        std::fs::write(&path, b"garbage that is not a vault").unwrap();
        let store = Arc::new(MemoryKeyStore::new());
        let mut vault = on_disk(dir.path(), &store);
        assert_eq!(vault.status(), Status::Unreadable);
        assert!(vault.problem().is_some());
        assert!(matches!(vault.create(None, FAST), Err(VaultError::Exists)));
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"garbage that is not a vault"
        );
        vault.reset().unwrap();
        vault.create(None, FAST).unwrap();
    }

    #[test]
    fn references() {
        let id = Ulid::generate();
        assert_eq!(parse_ref(&secret_ref(id)), Some(id));
        assert_eq!(parse_ref("vault:nope"), None);
        assert_eq!(parse_ref(&id.to_string()), None);
    }

    #[test]
    fn debug_hides_secrets() {
        let store = Arc::new(MemoryKeyStore::new());
        let mut vault = Vault::in_memory(Arc::clone(&store) as Arc<dyn KeyStore>);
        vault.create(Some(b"master-pw"), FAST).unwrap();
        let id = vault
            .insert(SecretKind::Password, password("visible?"))
            .unwrap();
        let text = format!("{vault:?} {:?}", vault.get(&id).unwrap());
        assert!(!text.contains("visible?"));
        assert!(!text.contains("master-pw"));
    }
}
