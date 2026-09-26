//! OpenSesh's secrets (PLAN §8, Sprint 6). This crate never depends on Qt.
//!
//! - [`vault`]: `vault.bin`, the encrypted secrets, and who holds its key (ADR 0023);
//!   [`format`] is its byte layout, [`crypto`] the primitives, [`store`] the system keyring and
//!   [`backoff`] the waits after wrong master passwords.
//!
//! Secrets are never logged, never put in error messages and never written in clear: types that
//! hold them wipe their memory on drop and print nothing in `Debug`.

pub mod backoff;
pub mod crypto;
pub mod format;
pub mod store;
pub mod vault;

pub use store::{KeyStore, MemoryKeyStore, StoreError, SystemKeyring};
pub use vault::{Protection, Status, Vault};

/// Why a vault operation failed. Messages never contain secrets.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    /// The system's random generator failed.
    #[error("the system's random generator failed")]
    Random,
    /// The file (or a secret in it) doesn't have the expected shape.
    #[error("the vault is damaged: {0}")]
    Format(&'static str),
    /// A newer OpenSesh wrote the file.
    #[error("the vault was written by a newer OpenSesh (format {0})")]
    Newer(u16),
    /// The key doesn't open the data (a wrong password, or a changed file).
    #[error("the key doesn't open the vault")]
    Decrypt,
    /// The master password is wrong; `wait` seconds before the next try.
    #[error("wrong master password")]
    WrongPassword {
        /// Seconds before the next attempt is allowed.
        wait: u64,
    },
    /// Too many wrong passwords: nothing was tried, wait this many seconds.
    #[error("too many wrong passwords: wait {0} s")]
    Wait(u64),
    /// The vault must be unlocked first.
    #[error("the vault is locked")]
    Locked,
    /// There is no vault yet.
    #[error("there is no vault yet")]
    Missing,
    /// There is a vault already.
    #[error("there is a vault already")]
    Exists,
    /// The vault file can't be read.
    #[error("the vault can't be read")]
    Unreadable,
    /// Not possible with the current protection (a master password operation on a vault the
    /// keyring holds, or the other way round).
    #[error("not possible with the vault's current protection")]
    WrongMode,
    /// No secret with this id.
    #[error("no secret {0}")]
    NotFound(String),
    /// The system keyring failed.
    #[error(transparent)]
    Keyring(#[from] StoreError),
    /// The vault file couldn't be written.
    #[error("could not write {path}")]
    Write {
        /// The file.
        path: String,
        /// Why.
        #[source]
        source: std::io::Error,
    },
}
