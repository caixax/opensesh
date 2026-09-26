//! The bytes of `vault.bin`, format version 1 (described for people in `docs/vault-format.md`).
//!
//! ```text
//! header
//!   magic            8   "OSVAULT\0"
//!   version          2   u16 LE, 1
//!   vault id        16   random; names the system keyring entry
//!   holder           1   1 = system keyring, 2 = master password
//!   when the holder is the master password:
//!     kdf            1   1 = Argon2id (version 0x13)
//!     memory KiB     4   u32 LE
//!     iterations     4   u32 LE
//!     lanes          4   u32 LE
//!     salt          16
//!     wrap nonce    24
//!     wrapped key   48   the vault key sealed with the derived key (32 + 16 tag);
//!                        associated data: the header bytes before the wrap nonce
//!   body nonce      24
//! body               the payload sealed with the vault key; associated data: the whole header
//! ```
//!
//! The payload is `count` (u32 LE) entries of `id` (16 bytes, a ULID), `kind` (1 byte),
//! `length` (u32 LE) and the bytes, then zeros up to a multiple of [`PAD_TO`] bytes so the file
//! size only hints at how much is stored.

use std::collections::BTreeMap;

use ulid::Ulid;
use zeroize::Zeroizing;

use crate::VaultError;
use crate::crypto::{self, KEY_LEN, KdfParams, Key, NONCE_LEN, SALT_LEN, TAG_LEN};

/// First bytes of every vault file.
pub const MAGIC: &[u8; 8] = b"OSVAULT\0";
/// The format this version writes and reads.
pub const FORMAT_VERSION: u16 = 1;
/// The payload is padded to a multiple of this many bytes.
pub const PAD_TO: usize = 256;

const HOLDER_KEYRING: u8 = 1;
const HOLDER_PASSWORD: u8 = 2;
const KDF_ARGON2ID: u8 = 1;
/// Largest payload accepted (the vault holds keys and passwords, not files).
const MAX_PAYLOAD: usize = 64 * 1024 * 1024;
/// Largest single secret accepted.
pub const MAX_SECRET: usize = 1024 * 1024;

/// What a secret is, so tools can tell them apart. Unknown kinds are kept as they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    /// A password (UTF-8).
    Password,
    /// A private key, the OpenSSH binary encoding without encryption.
    PrivateKey,
    /// A kind a newer version wrote.
    Other(u8),
}

impl SecretKind {
    fn to_byte(self) -> u8 {
        match self {
            Self::Password => 1,
            Self::PrivateKey => 2,
            Self::Other(byte) => byte,
        }
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            1 => Self::Password,
            2 => Self::PrivateKey,
            other => Self::Other(other),
        }
    }
}

/// One secret. `Debug` shows its kind and size only.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret {
    /// What it is.
    pub kind: SecretKind,
    /// The bytes, wiped on drop.
    pub data: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secret")
            .field("kind", &self.kind)
            .field("len", &self.data.len())
            .finish()
    }
}

/// The master password's key slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordSlot {
    /// Argon2id costs.
    pub params: KdfParams,
    /// Argon2id salt.
    pub salt: [u8; SALT_LEN],
    /// Nonce of the wrapped key.
    pub nonce: [u8; NONCE_LEN],
    /// The vault key sealed with the derived key.
    pub wrapped: [u8; KEY_LEN + TAG_LEN],
}

/// Who holds the vault key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Holder {
    /// The system keyring holds the vault key itself.
    Keyring,
    /// The master password: the key derived from it unwraps the vault key.
    Password(PasswordSlot),
}

/// The header of a vault file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Random id of this vault.
    pub vault_id: [u8; 16],
    /// Who holds the key.
    pub holder: Holder,
    /// Nonce of the body.
    pub body_nonce: [u8; NONCE_LEN],
}

impl Header {
    /// The bytes before the wrap nonce (the associated data of the wrapped key).
    fn slot_prefix(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&self.vault_id);
        match &self.holder {
            Holder::Keyring => out.push(HOLDER_KEYRING),
            Holder::Password(slot) => {
                out.push(HOLDER_PASSWORD);
                out.push(KDF_ARGON2ID);
                out.extend_from_slice(&slot.params.memory_kib.to_le_bytes());
                out.extend_from_slice(&slot.params.iterations.to_le_bytes());
                out.extend_from_slice(&slot.params.lanes.to_le_bytes());
                out.extend_from_slice(&slot.salt);
            }
        }
        out
    }

    /// The header's bytes (the associated data of the body).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = self.slot_prefix();
        if let Holder::Password(slot) = &self.holder {
            out.extend_from_slice(&slot.nonce);
            out.extend_from_slice(&slot.wrapped);
        }
        out.extend_from_slice(&self.body_nonce);
        out
    }

    /// A password slot for `vault_key` under `password`, with a fresh salt and nonce.
    ///
    /// # Errors
    ///
    /// Fails when the random generator or the key derivation does.
    pub fn password_slot(
        vault_id: [u8; 16],
        vault_key: &Key,
        password: &[u8],
        params: KdfParams,
    ) -> Result<PasswordSlot, VaultError> {
        let salt = crypto::random_array::<SALT_LEN>()?;
        let nonce = crypto::random_array::<NONCE_LEN>()?;
        let mut slot = PasswordSlot {
            params,
            salt,
            nonce,
            wrapped: [0; KEY_LEN + TAG_LEN],
        };
        let prefix = Self {
            vault_id,
            holder: Holder::Password(slot.clone()),
            body_nonce: [0; NONCE_LEN],
        }
        .slot_prefix();
        let derived = crypto::derive_key(password, &salt, params)?;
        let sealed = crypto::seal(&derived, &nonce, &prefix, vault_key.as_bytes())?;
        slot.wrapped = sealed
            .as_slice()
            .try_into()
            .map_err(|_| VaultError::Format("wrapped key has the wrong size"))?;
        Ok(slot)
    }

    /// The vault key, unwrapped with `password`. [`VaultError::Decrypt`] for a wrong one.
    ///
    /// # Errors
    ///
    /// [`VaultError::Decrypt`] when the password is wrong or the header was changed;
    /// [`VaultError::WrongMode`] when the keyring holds the key.
    pub fn unwrap_key(&self, password: &[u8]) -> Result<Key, VaultError> {
        let Holder::Password(slot) = &self.holder else {
            return Err(VaultError::WrongMode);
        };
        let derived = crypto::derive_key(password, &slot.salt, slot.params)?;
        let key = crypto::open(&derived, &slot.nonce, &self.slot_prefix(), &slot.wrapped)?;
        Key::from_slice(&key).ok_or(VaultError::Format("wrapped key has the wrong size"))
    }
}

/// Seals `entries` under `key` with a fresh body nonce: the whole file.
///
/// # Errors
///
/// Fails when the random generator does or an entry is too large.
pub fn seal_file(
    header: &mut Header,
    key: &Key,
    entries: &BTreeMap<Ulid, Secret>,
) -> Result<Vec<u8>, VaultError> {
    header.body_nonce = crypto::random_array::<NONCE_LEN>()?;
    let aad = header.encode();
    let payload = encode_payload(entries)?;
    let body = crypto::seal(key, &header.body_nonce, &aad, &payload)?;
    let mut out = aad;
    out.extend_from_slice(&body);
    Ok(out)
}

/// The header of a vault file and where its body starts.
///
/// # Errors
///
/// [`VaultError::Format`] for anything that isn't a vault file of a known shape,
/// [`VaultError::Newer`] for a newer format.
pub fn read_header(bytes: &[u8]) -> Result<(Header, usize), VaultError> {
    let mut reader = Reader { bytes, pos: 0 };
    if reader.take(MAGIC.len())? != MAGIC {
        return Err(VaultError::Format("not an OpenSesh vault"));
    }
    let version = reader.u16()?;
    if version > FORMAT_VERSION {
        return Err(VaultError::Newer(version));
    }
    if version == 0 {
        return Err(VaultError::Format("unknown version"));
    }
    let vault_id = reader.array::<16>()?;
    let holder = match reader.u8()? {
        HOLDER_KEYRING => Holder::Keyring,
        HOLDER_PASSWORD => {
            if reader.u8()? != KDF_ARGON2ID {
                return Err(VaultError::Format("unknown key derivation"));
            }
            let params = KdfParams {
                memory_kib: reader.u32()?,
                iterations: reader.u32()?,
                lanes: reader.u32()?,
            };
            if !params.is_valid() {
                return Err(VaultError::Format("key derivation costs out of range"));
            }
            Holder::Password(PasswordSlot {
                params,
                salt: reader.array()?,
                nonce: reader.array()?,
                wrapped: reader.array()?,
            })
        }
        _ => return Err(VaultError::Format("unknown key holder")),
    };
    let body_nonce = reader.array()?;
    if bytes.len() - reader.pos < TAG_LEN {
        return Err(VaultError::Format("the file is cut short"));
    }
    Ok((
        Header {
            vault_id,
            holder,
            body_nonce,
        },
        reader.pos,
    ))
}

/// The entries of a vault file, decrypted with `key`.
///
/// # Errors
///
/// [`VaultError::Decrypt`] for a wrong key or a changed file, [`VaultError::Format`] for a
/// payload that doesn't parse.
pub fn open_file(bytes: &[u8], key: &Key) -> Result<(Header, BTreeMap<Ulid, Secret>), VaultError> {
    let (header, start) = read_header(bytes)?;
    let aad = bytes
        .get(..start)
        .ok_or(VaultError::Format("the file is cut short"))?;
    let body = bytes
        .get(start..)
        .ok_or(VaultError::Format("the file is cut short"))?;
    let payload = crypto::open(key, &header.body_nonce, aad, body)?;
    let entries = decode_payload(&payload)?;
    Ok((header, entries))
}

fn encode_payload(entries: &BTreeMap<Ulid, Secret>) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let count = u32::try_from(entries.len()).map_err(|_| VaultError::Format("too many secrets"))?;
    let size: usize = 4 + entries
        .values()
        .map(|secret| 16 + 1 + 4 + secret.data.len())
        .sum::<usize>();
    let padded = size.div_ceil(PAD_TO).max(1) * PAD_TO;
    if padded > MAX_PAYLOAD {
        return Err(VaultError::Format("the vault would be too large"));
    }
    // Allocated once at its final size: growing would leave copies of secrets behind.
    let mut out = Zeroizing::new(Vec::with_capacity(padded + TAG_LEN));
    out.extend_from_slice(&count.to_le_bytes());
    for (id, secret) in entries {
        if secret.data.len() > MAX_SECRET {
            return Err(VaultError::Format("a secret is too large"));
        }
        let len = u32::try_from(secret.data.len())
            .map_err(|_| VaultError::Format("a secret is too large"))?;
        out.extend_from_slice(&id.to_bytes());
        out.push(secret.kind.to_byte());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&secret.data);
    }
    out.resize(padded, 0);
    Ok(out)
}

fn decode_payload(payload: &[u8]) -> Result<BTreeMap<Ulid, Secret>, VaultError> {
    let mut reader = Reader {
        bytes: payload,
        pos: 0,
    };
    let count = reader.u32()?;
    let mut entries = BTreeMap::new();
    for _ in 0..count {
        let id = Ulid::from_bytes(reader.array::<16>()?);
        let kind = SecretKind::from_byte(reader.u8()?);
        let len = usize::try_from(reader.u32()?)
            .map_err(|_| VaultError::Format("a secret is too large"))?;
        if len > MAX_SECRET {
            return Err(VaultError::Format("a secret is too large"));
        }
        let data = Zeroizing::new(reader.take(len)?.to_vec());
        if entries.insert(id, Secret { kind, data }).is_some() {
            return Err(VaultError::Format("a secret appears twice"));
        }
    }
    Ok(entries)
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], VaultError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(VaultError::Format("the file is cut short"))?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(VaultError::Format("the file is cut short"))?;
        self.pos = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], VaultError> {
        self.take(N)?
            .try_into()
            .map_err(|_| VaultError::Format("the file is cut short"))
    }

    fn u8(&mut self) -> Result<u8, VaultError> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, VaultError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, VaultError> {
        Ok(u32::from_le_bytes(self.array()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(kind: SecretKind, text: &str) -> Secret {
        Secret {
            kind,
            data: Zeroizing::new(text.as_bytes().to_vec()),
        }
    }

    fn sample() -> BTreeMap<Ulid, Secret> {
        let mut entries = BTreeMap::new();
        entries.insert(Ulid::generate(), secret(SecretKind::Password, "hunter2"));
        entries.insert(
            Ulid::generate(),
            secret(SecretKind::PrivateKey, "not really a key"),
        );
        entries.insert(Ulid::generate(), secret(SecretKind::Other(77), "future"));
        entries
    }

    #[test]
    fn keyring_vault_round_trip() {
        let key = Key::random().unwrap();
        let mut header = Header {
            vault_id: [7; 16],
            holder: Holder::Keyring,
            body_nonce: [0; NONCE_LEN],
        };
        let entries = sample();
        let file = seal_file(&mut header, &key, &entries).unwrap();
        assert!(file.starts_with(MAGIC));
        // Header (8 + 2 + 16 + 1 + 24), one padded block, the tag.
        assert_eq!(file.len(), 51 + PAD_TO + TAG_LEN);
        let (read, opened) = open_file(&file, &key).unwrap();
        assert_eq!(read, header);
        assert_eq!(opened, entries);
        assert!(!file.windows(7).any(|window| window == b"hunter2"));
    }

    #[test]
    fn password_vault_round_trip() {
        let key = Key::random().unwrap();
        let slot =
            Header::password_slot([1; 16], &key, b"master", KdfParams::INSECURE_FOR_TESTS).unwrap();
        let mut header = Header {
            vault_id: [1; 16],
            holder: Holder::Password(slot),
            body_nonce: [0; NONCE_LEN],
        };
        let entries = sample();
        let file = seal_file(&mut header, &key, &entries).unwrap();
        let (read, _) = read_header(&file).unwrap();
        let unwrapped = read.unwrap_key(b"master").unwrap();
        assert_eq!(unwrapped.as_bytes(), key.as_bytes());
        assert!(matches!(
            read.unwrap_key(b"Master"),
            Err(VaultError::Decrypt)
        ));
        assert_eq!(open_file(&file, &unwrapped).unwrap().1, entries);
    }

    #[test]
    fn any_changed_byte_is_refused() {
        let key = Key::random().unwrap();
        let slot =
            Header::password_slot([2; 16], &key, b"pw", KdfParams::INSECURE_FOR_TESTS).unwrap();
        let mut header = Header {
            vault_id: [2; 16],
            holder: Holder::Password(slot),
            body_nonce: [0; NONCE_LEN],
        };
        let file = seal_file(&mut header, &key, &sample()).unwrap();
        for index in 0..file.len() {
            let mut changed = file.clone();
            changed[index] ^= 0x01;
            let opened = read_header(&changed)
                .and_then(|(header, _)| header.unwrap_key(b"pw"))
                .and_then(|key| open_file(&changed, &key));
            assert!(opened.is_err(), "byte {index} was not authenticated");
        }
        // Cut short anywhere.
        for len in 0..file.len() {
            assert!(open_file(&file[..len], &key).is_err());
        }
    }

    #[test]
    fn newer_and_foreign_files() {
        let mut file = MAGIC.to_vec();
        file.extend_from_slice(&2_u16.to_le_bytes());
        file.resize(200, 0);
        assert!(matches!(read_header(&file), Err(VaultError::Newer(2))));
        assert!(matches!(
            read_header(b"hello world, this is not a vault at all"),
            Err(VaultError::Format(_))
        ));
    }

    #[test]
    fn crafted_costs_are_refused_before_deriving() {
        let key = Key::random().unwrap();
        let mut slot =
            Header::password_slot([3; 16], &key, b"pw", KdfParams::INSECURE_FOR_TESTS).unwrap();
        slot.params.memory_kib = u32::MAX;
        let mut header = Header {
            vault_id: [3; 16],
            holder: Holder::Password(slot),
            body_nonce: [0; NONCE_LEN],
        };
        let file = seal_file(&mut header, &key, &BTreeMap::new()).unwrap();
        assert!(matches!(read_header(&file), Err(VaultError::Format(_))));
    }

    #[test]
    fn padding_hides_small_sizes() {
        let key = Key::random().unwrap();
        let mut header = Header {
            vault_id: [4; 16],
            holder: Holder::Keyring,
            body_nonce: [0; NONCE_LEN],
        };
        let empty = seal_file(&mut header, &key, &BTreeMap::new()).unwrap();
        let mut one = BTreeMap::new();
        one.insert(Ulid::generate(), secret(SecretKind::Password, "short"));
        let small = seal_file(&mut header, &key, &one).unwrap();
        assert_eq!(empty.len(), small.len());
        assert_eq!(
            format!("{:?}", one.values().next().unwrap()).find("short"),
            None
        );
    }
}
