//! Where the vault key is kept when the master password doesn't hold it: the system keyring
//! (Credential Manager on Windows, the Secret Service on Linux, which GNOME Keyring and KWallet
//! provide), or memory for tests and test runs (ADR 0023).
//!
//! Calls may block (a keyring can ask the user to unlock it): never make them on the GUI thread.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use zeroize::Zeroizing;

/// The keyring service name of every OpenSesh entry (the app id).
pub const SERVICE: &str = "cc.caixa.OpenSesh";

/// Why the keyring couldn't be used. Messages never contain secrets.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// There is no keyring on this system, or it can't be reached.
    #[error("the system keyring is not available: {0}")]
    Unavailable(String),
    /// The keyring is there but refused access (locked, or the user said no).
    #[error("the system keyring refused access: {0}")]
    Denied(String),
    /// Anything else.
    #[error("the system keyring failed: {0}")]
    Failed(String),
}

/// Somewhere to keep a few small secrets by name.
pub trait KeyStore: Send + Sync + std::fmt::Debug {
    /// Whether the store can be used at all.
    ///
    /// # Errors
    ///
    /// Why it can't.
    fn check(&self) -> Result<(), StoreError>;

    /// The secret called `name`, `None` when there is none.
    ///
    /// # Errors
    ///
    /// When the store can't be read.
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>, StoreError>;

    /// Keeps `secret` as `name`, replacing any previous one.
    ///
    /// # Errors
    ///
    /// When the store can't be written.
    fn set(&self, name: &str, secret: &[u8]) -> Result<(), StoreError>;

    /// Forgets `name` (fine when it isn't there).
    ///
    /// # Errors
    ///
    /// When the store can't be written.
    fn delete(&self, name: &str) -> Result<(), StoreError>;
}

/// The operating system's keyring.
#[derive(Debug, Clone)]
pub struct SystemKeyring {
    service: String,
}

impl SystemKeyring {
    /// The keyring, with entries under `service`.
    #[must_use]
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_owned(),
        }
    }

    fn entry(&self, name: &str) -> Result<keyring::Entry, StoreError> {
        keyring::Entry::new(&self.service, name).map_err(|error| map_error(&error))
    }
}

impl Default for SystemKeyring {
    fn default() -> Self {
        Self::new(SERVICE)
    }
}

/// A keyring error in words that never carry secret data.
fn map_error(error: &keyring::Error) -> StoreError {
    match error {
        keyring::Error::NoDefaultStore | keyring::Error::Invalid(..) => {
            StoreError::Unavailable(error.to_string())
        }
        keyring::Error::NoStorageAccess(_) => StoreError::Denied(error.to_string()),
        // These two carry the stored bytes: say what happened, not what was there.
        keyring::Error::BadEncoding(_) | keyring::Error::BadDataFormat(..) => {
            StoreError::Failed("the stored data has an unexpected format".to_owned())
        }
        _ => StoreError::Failed(error.to_string()),
    }
}

impl KeyStore for SystemKeyring {
    fn check(&self) -> Result<(), StoreError> {
        keyring::Entry::store_status()
            .as_ref()
            .map(|_| ())
            .map_err(|error| match map_error(error) {
                StoreError::Failed(message) | StoreError::Denied(message) => {
                    StoreError::Unavailable(message)
                }
                other => other,
            })
    }

    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>, StoreError> {
        match self.entry(name)?.get_secret() {
            // Moved, not copied: the buffer the keyring returned is the one wiped on drop.
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(map_error(&error)),
        }
    }

    fn set(&self, name: &str, secret: &[u8]) -> Result<(), StoreError> {
        self.entry(name)?
            .set_secret(secret)
            .map_err(|error| map_error(&error))
    }

    fn delete(&self, name: &str) -> Result<(), StoreError> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(map_error(&error)),
        }
    }
}

/// A keyring in memory: for tests and test runs, which must never touch the user's keyring.
#[derive(Debug, Default)]
pub struct MemoryKeyStore {
    entries: Mutex<HashMap<String, Zeroizing<Vec<u8>>>>,
    unavailable: bool,
}

impl MemoryKeyStore {
    /// An empty, working store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A store that behaves like a system without a keyring.
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            entries: Mutex::default(),
            unavailable: true,
        }
    }

    /// How many entries it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Whether it holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn ensure(&self) -> Result<(), StoreError> {
        if self.unavailable {
            Err(StoreError::Unavailable(
                "no keyring in this test".to_owned(),
            ))
        } else {
            Ok(())
        }
    }
}

impl KeyStore for MemoryKeyStore {
    fn check(&self) -> Result<(), StoreError> {
        self.ensure()
    }

    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>, StoreError> {
        self.ensure()?;
        Ok(self
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(name)
            .cloned())
    }

    fn set(&self, name: &str, secret: &[u8]) -> Result<(), StoreError> {
        self.ensure()?;
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(name.to_owned(), Zeroizing::new(secret.to_vec()));
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<(), StoreError> {
        self.ensure()?;
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store() {
        let store = MemoryKeyStore::new();
        assert!(store.check().is_ok());
        assert_eq!(store.get("a").unwrap(), None);
        store.set("a", b"one").unwrap();
        store.set("a", b"two").unwrap();
        assert_eq!(store.get("a").unwrap().unwrap().as_slice(), b"two");
        store.delete("a").unwrap();
        store.delete("a").unwrap();
        assert!(store.is_empty());

        let none = MemoryKeyStore::unavailable();
        assert!(matches!(none.check(), Err(StoreError::Unavailable(_))));
        assert!(none.set("a", b"x").is_err());
    }

    /// The real keyring, with a throwaway entry that is removed again. Run by hand once per
    /// platform (`cargo test -p opensesh-vault --lib system_keyring -- --ignored`, which also runs
    /// `manager::tests::system_keyring_vault`); CI and normal test runs never touch the user's
    /// keyring.
    #[test]
    #[ignore = "touches the system keyring"]
    fn system_keyring() {
        let store = SystemKeyring::new("cc.caixa.OpenSesh.selftest");
        store.check().unwrap();
        let name = format!("selftest-{}", std::process::id());
        let secret = [0_u8, 1, 2, 250, 255];
        store.set(&name, &secret).unwrap();
        assert_eq!(store.get(&name).unwrap().unwrap().as_slice(), secret);
        store.delete(&name).unwrap();
        assert_eq!(store.get(&name).unwrap(), None);
    }
}
