//! SSH keys (Sprint 6): generate, import (OpenSSH and PuTTY), export, and describe them.
//!
//! Private keys go into the vault as OpenSSH's binary encoding without encryption (the vault
//! encrypts them); an imported key's passphrase is only needed to import it. Exporting can set a
//! new passphrase.

use ssh_key::{Algorithm, EcdsaCurve, HashAlg, LineEnding, PrivateKey, PublicKey};
use zeroize::Zeroizing;

use crate::ppk;

/// Why a key couldn't be read or made. Messages never contain key material or passphrases.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// The key is encrypted: ask for its passphrase.
    #[error("the key is protected by a passphrase")]
    NeedsPassphrase,
    /// The passphrase doesn't open the key.
    #[error("wrong passphrase")]
    WrongPassphrase,
    /// The old PEM format (`BEGIN RSA PRIVATE KEY` and similar).
    #[error("the key is in the old PEM format")]
    LegacyPem,
    /// A key this version can't use.
    #[error("not supported: {0}")]
    Unsupported(String),
    /// Not a private key at all.
    #[error("not a private key")]
    NotAKey,
    /// Looks like a key, but something in it is wrong.
    #[error("the key is damaged ({0})")]
    Damaged(String),
    /// Generating failed.
    #[error("could not generate the key: {0}")]
    Generate(String),
}

impl KeyError {
    /// A short code for the UI to word.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NeedsPassphrase => "needs-passphrase",
            Self::WrongPassphrase => "wrong-passphrase",
            Self::LegacyPem => "legacy-pem",
            Self::Unsupported(_) => "unsupported",
            Self::NotAKey => "not-a-key",
            Self::Damaged(_) => "damaged",
            Self::Generate(_) => "generate",
        }
    }
}

/// Keys OpenSesh can generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyType {
    /// Ed25519 (the default).
    #[default]
    Ed25519,
    /// ECDSA on NIST P-256.
    EcdsaP256,
    /// ECDSA on NIST P-384.
    EcdsaP384,
    /// ECDSA on NIST P-521.
    EcdsaP521,
    /// RSA with 4096 bits.
    Rsa4096,
}

impl KeyType {
    /// Every type, in menu order.
    pub const ALL: [Self; 5] = [
        Self::Ed25519,
        Self::EcdsaP256,
        Self::EcdsaP384,
        Self::EcdsaP521,
        Self::Rsa4096,
    ];

    /// The name in the UI and in files.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ed25519 => "ed25519",
            Self::EcdsaP256 => "ecdsa-p256",
            Self::EcdsaP384 => "ecdsa-p384",
            Self::EcdsaP521 => "ecdsa-p521",
            Self::Rsa4096 => "rsa-4096",
        }
    }

    /// The type called `text`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    fn algorithm(self) -> Algorithm {
        match self {
            Self::Ed25519 => Algorithm::Ed25519,
            Self::EcdsaP256 => Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP256,
            },
            Self::EcdsaP384 => Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP384,
            },
            Self::EcdsaP521 => Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP521,
            },
            // ssh-key generates 4096-bit RSA keys.
            Self::Rsa4096 => Algorithm::Rsa { hash: None },
        }
    }
}

/// Where a key came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFormat {
    /// OpenSSH's own format.
    OpenSsh,
    /// PuTTY's `.ppk`.
    Ppk,
}

impl KeyFormat {
    /// The name in files.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenSsh => "openssh",
            Self::Ppk => "ppk",
        }
    }
}

/// A private key read from a file.
#[derive(Debug)]
pub struct Imported {
    /// The key, decrypted.
    pub key: PrivateKey,
    /// Its format.
    pub format: KeyFormat,
    /// Whether it had a passphrase.
    pub was_encrypted: bool,
}

/// What can be shown about a key (all public).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    /// SSH algorithm name (`ssh-ed25519`, `ecdsa-sha2-nistp256`, `ssh-rsa`, ...).
    pub algorithm: String,
    /// Words for people (`Ed25519`, `ECDSA P-256`, `RSA`).
    pub label: String,
    /// Key size in bits.
    pub bits: u32,
    /// The OpenSSH public key line, with the comment.
    pub public: String,
    /// `SHA256:...`.
    pub fingerprint: String,
    /// The comment.
    pub comment: String,
}

/// A new key of `kind` with `comment`. RSA-4096 can take seconds: never on the GUI thread.
///
/// # Errors
///
/// When the random generator or the key generation fails.
pub fn generate(kind: KeyType, comment: &str) -> Result<PrivateKey, KeyError> {
    let mut key = PrivateKey::random(&mut rand_core::OsRng, kind.algorithm())
        .map_err(|error| KeyError::Generate(error.to_string()))?;
    key.set_comment(comment);
    Ok(key)
}

const PEM_HEADERS: [&str; 5] = [
    "-----BEGIN RSA PRIVATE KEY-----",
    "-----BEGIN EC PRIVATE KEY-----",
    "-----BEGIN DSA PRIVATE KEY-----",
    "-----BEGIN PRIVATE KEY-----",
    "-----BEGIN ENCRYPTED PRIVATE KEY-----",
];

const OPENSSH_HEADER: &str = "-----BEGIN OPENSSH PRIVATE KEY-----";

enum Kind {
    OpenSsh,
    Ppk,
}

fn kind_of(text: &str) -> Result<Kind, KeyError> {
    let text = text.trim_start();
    if text.starts_with(OPENSSH_HEADER) {
        Ok(Kind::OpenSsh)
    } else if ppk::is_ppk(text) {
        Ok(Kind::Ppk)
    } else if PEM_HEADERS.iter().any(|header| text.starts_with(header)) {
        Err(KeyError::LegacyPem)
    } else {
        Err(KeyError::NotAKey)
    }
}

fn as_text(bytes: &[u8]) -> Result<&str, KeyError> {
    std::str::from_utf8(bytes).map_err(|_| KeyError::NotAKey)
}

/// Whether the key in `bytes` needs a passphrase to be imported.
///
/// # Errors
///
/// When it isn't a key this version reads.
pub fn needs_passphrase(bytes: &[u8]) -> Result<bool, KeyError> {
    let text = as_text(bytes)?;
    match kind_of(text)? {
        Kind::OpenSsh => Ok(PrivateKey::from_openssh(text)
            .map_err(|error| KeyError::Damaged(error.to_string()))?
            .is_encrypted()),
        Kind::Ppk => ppk::is_encrypted(text),
    }
}

/// The private key in `bytes` (an OpenSSH or PuTTY key file), decrypted with `passphrase`
/// when it has one.
///
/// # Errors
///
/// [`KeyError::NeedsPassphrase`] (try again with one), [`KeyError::WrongPassphrase`],
/// [`KeyError::LegacyPem`], [`KeyError::Unsupported`], [`KeyError::NotAKey`] or
/// [`KeyError::Damaged`].
pub fn import(bytes: &[u8], passphrase: Option<&[u8]>) -> Result<Imported, KeyError> {
    let text = as_text(bytes)?;
    match kind_of(text)? {
        Kind::OpenSsh => {
            let key = PrivateKey::from_openssh(text)
                .map_err(|error| KeyError::Damaged(error.to_string()))?;
            let was_encrypted = key.is_encrypted();
            let key = if was_encrypted {
                let passphrase = passphrase.ok_or(KeyError::NeedsPassphrase)?;
                key.decrypt(passphrase).map_err(|error| match error {
                    ssh_key::Error::AlgorithmUnknown
                    | ssh_key::Error::AlgorithmUnsupported { .. } => {
                        KeyError::Unsupported(error.to_string())
                    }
                    _ => KeyError::WrongPassphrase,
                })?
            } else {
                key
            };
            check_usable(&key)?;
            Ok(Imported {
                key,
                format: KeyFormat::OpenSsh,
                was_encrypted,
            })
        }
        Kind::Ppk => {
            let was_encrypted = ppk::is_encrypted(text)?;
            let key = ppk::parse(text, passphrase)?;
            check_usable(&key)?;
            Ok(Imported {
                key,
                format: KeyFormat::Ppk,
                was_encrypted,
            })
        }
    }
}

/// Refuses keys the SSH client won't use (DSA, security keys, unknown algorithms).
fn check_usable(key: &PrivateKey) -> Result<(), KeyError> {
    match key.algorithm() {
        Algorithm::Ed25519 | Algorithm::Ecdsa { .. } | Algorithm::Rsa { .. } => Ok(()),
        other => Err(KeyError::Unsupported(format!("{} keys", other.as_str()))),
    }
}

/// The key as stored in the vault (OpenSSH's binary encoding, not encrypted).
///
/// # Errors
///
/// When the key can't be encoded.
pub fn to_vault(key: &PrivateKey) -> Result<Zeroizing<Vec<u8>>, KeyError> {
    key.to_bytes()
        .map_err(|error| KeyError::Damaged(error.to_string()))
}

/// A key as [`to_vault`] stored it.
///
/// # Errors
///
/// When the bytes aren't a key.
pub fn from_vault(bytes: &[u8]) -> Result<PrivateKey, KeyError> {
    PrivateKey::from_bytes(bytes).map_err(|error| KeyError::Damaged(error.to_string()))
}

/// The key as an OpenSSH private key file, encrypted with `passphrase` when given (AES-256-CTR
/// and bcrypt-pbkdf, like `ssh-keygen`).
///
/// # Errors
///
/// When encryption or encoding fails.
pub fn export_openssh(
    key: &PrivateKey,
    passphrase: Option<&[u8]>,
) -> Result<Zeroizing<String>, KeyError> {
    let encoded = match passphrase.filter(|passphrase| !passphrase.is_empty()) {
        Some(passphrase) => key
            .encrypt(&mut rand_core::OsRng, passphrase)
            .and_then(|encrypted| encrypted.to_openssh(LineEnding::LF)),
        None => key.to_openssh(LineEnding::LF),
    };
    encoded.map_err(|error| KeyError::Generate(error.to_string()))
}

/// What can be shown about a private key.
#[must_use]
pub fn info(key: &PrivateKey) -> KeyInfo {
    public_info(key.public_key())
}

/// What can be shown about a public key.
#[must_use]
pub fn public_info(key: &PublicKey) -> KeyInfo {
    let algorithm = key.algorithm();
    let (label, bits) = match &algorithm {
        Algorithm::Ed25519 => ("Ed25519".to_owned(), 256),
        Algorithm::Ecdsa { curve } => match curve {
            EcdsaCurve::NistP256 => ("ECDSA P-256".to_owned(), 256),
            EcdsaCurve::NistP384 => ("ECDSA P-384".to_owned(), 384),
            EcdsaCurve::NistP521 => ("ECDSA P-521".to_owned(), 521),
        },
        Algorithm::Rsa { .. } => ("RSA".to_owned(), rsa_bits(key)),
        Algorithm::Dsa => ("DSA".to_owned(), 1024),
        Algorithm::SkEd25519 => ("Ed25519 security key".to_owned(), 256),
        Algorithm::SkEcdsaSha2NistP256 => ("ECDSA security key".to_owned(), 256),
        other => (other.as_str().to_owned(), 0),
    };
    KeyInfo {
        algorithm: algorithm.as_str().to_owned(),
        label,
        bits,
        public: key.to_openssh().unwrap_or_default(),
        fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        comment: key.comment().to_owned(),
    }
}

/// What can be shown about an OpenSSH public key line (`None` when it doesn't parse); `comment`
/// replaces the line's own when given.
#[must_use]
pub fn public_line_info(line: &str, comment: Option<&str>) -> Option<KeyInfo> {
    let mut key = PublicKey::from_openssh(line.trim()).ok()?;
    if let Some(comment) = comment {
        key.set_comment(comment);
    }
    Some(public_info(&key))
}

fn rsa_bits(key: &PublicKey) -> u32 {
    let Some(rsa) = key.key_data().rsa() else {
        return 0;
    };
    let Some(bytes) = rsa.n.as_positive_bytes() else {
        return 0;
    };
    let Some(first) = bytes.first() else {
        return 0;
    };
    let len = u32::try_from(bytes.len()).unwrap_or(0);
    len.saturating_mul(8).saturating_sub(first.leading_zeros())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_round_trip_through_the_vault_encoding() {
        for kind in [
            KeyType::Ed25519,
            KeyType::EcdsaP256,
            KeyType::EcdsaP384,
            KeyType::EcdsaP521,
        ] {
            let key = generate(kind, "me@here").unwrap();
            let stored = to_vault(&key).unwrap();
            let back = from_vault(&stored).unwrap();
            assert_eq!(back, key);
            assert_eq!(info(&back).comment, "me@here");
            assert!(info(&back).fingerprint.starts_with("SHA256:"));
        }
        assert_eq!(info(&generate(KeyType::EcdsaP521, "").unwrap()).bits, 521);
    }

    #[test]
    fn rsa_4096() {
        let key = generate(KeyType::Rsa4096, "rsa").unwrap();
        let info = info(&key);
        assert_eq!(info.algorithm, "ssh-rsa");
        assert_eq!(info.bits, 4096);
        assert!(info.public.starts_with("ssh-rsa AAAA"));
        assert!(info.public.ends_with(" rsa"));
    }

    #[test]
    fn export_and_import_again() {
        let key = generate(KeyType::Ed25519, "exported").unwrap();
        let plain = export_openssh(&key, None).unwrap();
        assert!(!needs_passphrase(plain.as_bytes()).unwrap());
        assert_eq!(import(plain.as_bytes(), None).unwrap().key, key);

        let locked = export_openssh(&key, Some(b"new pass")).unwrap();
        assert!(needs_passphrase(locked.as_bytes()).unwrap());
        assert_eq!(
            import(locked.as_bytes(), None).unwrap_err(),
            KeyError::NeedsPassphrase
        );
        assert_eq!(
            import(locked.as_bytes(), Some(b"old pass")).unwrap_err(),
            KeyError::WrongPassphrase
        );
        let again = import(locked.as_bytes(), Some(b"new pass")).unwrap();
        assert_eq!(again.key, key);
        assert!(again.was_encrypted);
    }

    #[test]
    fn types_by_name() {
        for kind in KeyType::ALL {
            assert_eq!(KeyType::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(KeyType::parse("dsa"), None);
        assert_eq!(import(b"hello", None).unwrap_err(), KeyError::NotAKey);
        assert_eq!(import(&[0xff, 0xfe], None).unwrap_err(), KeyError::NotAKey);
    }
}
