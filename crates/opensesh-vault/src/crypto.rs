//! The primitives of the vault (PLAN §8): Argon2id to derive a key from the master password,
//! XChaCha20-Poly1305 to encrypt, and the system's random generator.
//!
//! Every buffer that holds a key or a plaintext is wiped when dropped ([`Zeroizing`]).

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand_core::{OsRng, RngCore};
use zeroize::Zeroizing;

use crate::VaultError;

/// Bytes of a key (the vault key and the key derived from the password).
pub const KEY_LEN: usize = 32;
/// Bytes of an XChaCha20-Poly1305 nonce.
pub const NONCE_LEN: usize = 24;
/// Bytes of a Poly1305 tag.
pub const TAG_LEN: usize = 16;
/// Bytes of an Argon2 salt.
pub const SALT_LEN: usize = 16;

/// Largest memory cost accepted from a file (4 GiB in KiB), so a crafted header can't make
/// unlocking allocate without bound.
const MAX_MEMORY_KIB: u32 = 4 * 1024 * 1024;
/// Largest number of passes accepted from a file.
const MAX_ITERATIONS: u32 = 64;
/// Largest number of lanes accepted from a file.
const MAX_LANES: u32 = 64;

/// A 256-bit key, wiped on drop. `Debug` never shows it.
#[derive(Clone)]
pub struct Key(Zeroizing<[u8; KEY_LEN]>);

impl Key {
    /// A new random key.
    ///
    /// # Errors
    ///
    /// Fails when the system's random generator does.
    pub fn random() -> Result<Self, VaultError> {
        let mut key = Zeroizing::new([0_u8; KEY_LEN]);
        fill_random(key.as_mut_slice())?;
        Ok(Self(key))
    }

    /// The key in `bytes`, which must be [`KEY_LEN`] long.
    #[must_use]
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        let array: [u8; KEY_LEN] = bytes.try_into().ok()?;
        Some(Self(Zeroizing::new(array)))
    }

    /// The raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(..)")
    }
}

/// Argon2id costs, stored in the vault's header so they can grow without a format change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory in KiB.
    pub memory_kib: u32,
    /// Passes over the memory.
    pub iterations: u32,
    /// Lanes.
    pub lanes: u32,
}

impl KdfParams {
    /// The costs for new vaults: RFC 9106's second recommended option (64 MiB, 3 passes,
    /// 4 lanes), for machines that can't spare 2 GiB.
    pub const RECOMMENDED: Self = Self {
        memory_kib: 64 * 1024,
        iterations: 3,
        lanes: 4,
    };

    /// Tiny costs for tests only (they would be far too weak for a real vault).
    pub const INSECURE_FOR_TESTS: Self = Self {
        memory_kib: 64,
        iterations: 1,
        lanes: 1,
    };

    /// Whether the costs are within the bounds this version accepts.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        (1..=MAX_LANES).contains(&self.lanes)
            && (1..=MAX_ITERATIONS).contains(&self.iterations)
            && self.memory_kib >= 8 * self.lanes
            && self.memory_kib <= MAX_MEMORY_KIB
    }
}

/// Fills `buffer` from the operating system's random generator.
///
/// # Errors
///
/// Fails when the generator does (it never returns weak bytes instead).
pub fn fill_random(buffer: &mut [u8]) -> Result<(), VaultError> {
    OsRng.try_fill_bytes(buffer).map_err(|_| VaultError::Random)
}

/// `N` random bytes.
///
/// # Errors
///
/// Fails when the system's random generator does.
pub fn random_array<const N: usize>() -> Result<[u8; N], VaultError> {
    let mut out = [0_u8; N];
    fill_random(&mut out)?;
    Ok(out)
}

/// Argon2id (version 0x13) of `password` with `salt`, [`KEY_LEN`] bytes long.
///
/// # Errors
///
/// Fails for costs outside [`KdfParams::is_valid`].
pub fn derive_key(password: &[u8], salt: &[u8], params: KdfParams) -> Result<Key, VaultError> {
    let mut out = Zeroizing::new([0_u8; KEY_LEN]);
    argon2(
        Algorithm::Argon2id,
        password,
        salt,
        params,
        out.as_mut_slice(),
    )?;
    Ok(Key(out))
}

/// Argon2 of any flavour into `out` (PuTTY keys use Argon2i, Argon2d or Argon2id).
///
/// # Errors
///
/// Fails for costs outside [`KdfParams::is_valid`] or a salt Argon2 refuses.
pub fn argon2(
    algorithm: Algorithm,
    password: &[u8],
    salt: &[u8],
    params: KdfParams,
    out: &mut [u8],
) -> Result<(), VaultError> {
    if !params.is_valid() {
        return Err(VaultError::Format("key derivation costs out of range"));
    }
    let params = Params::new(
        params.memory_kib,
        params.iterations,
        params.lanes,
        Some(out.len()),
    )
    .map_err(|_| VaultError::Format("key derivation costs out of range"))?;
    Argon2::new(algorithm, Version::V0x13, params)
        .hash_password_into(password, salt, out)
        .map_err(|_| VaultError::Format("key derivation refused its input"))
}

/// Encrypts `plaintext` with `key` and `nonce`, authenticating `aad` too. Returns the ciphertext
/// followed by the tag.
///
/// # Errors
///
/// Fails only if the cipher does (the buffer can't grow).
pub fn seal(
    key: &Key,
    nonce: &[u8; NONCE_LEN],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, VaultError> {
    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    // Room for the tag up front: a reallocation would leave a copy of the plaintext behind.
    let mut buffer = Vec::with_capacity(plaintext.len() + TAG_LEN);
    buffer.extend_from_slice(plaintext);
    cipher
        .encrypt_in_place(XNonce::from_slice(nonce), aad, &mut buffer)
        .map_err(|_| VaultError::Format("could not encrypt"))?;
    Ok(buffer)
}

/// Decrypts what [`seal`] made. Fails the same way for a wrong key and for tampered data.
///
/// # Errors
///
/// [`VaultError::Decrypt`] when the tag doesn't match.
pub fn open(
    key: &Key,
    nonce: &[u8; NONCE_LEN],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    let mut buffer = Zeroizing::new(ciphertext.to_vec());
    cipher
        .decrypt_in_place(XNonce::from_slice(nonce), aad, &mut *buffer)
        .map_err(|_| VaultError::Decrypt)?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_and_open() {
        let key = Key::random().unwrap();
        let nonce = random_array::<NONCE_LEN>().unwrap();
        let sealed = seal(&key, &nonce, b"header", b"secret").unwrap();
        assert_eq!(sealed.len(), 6 + TAG_LEN);
        assert!(!sealed.windows(6).any(|window| window == b"secret"));
        assert_eq!(
            open(&key, &nonce, b"header", &sealed).unwrap().as_slice(),
            b"secret"
        );
        // Another key, another header or a flipped bit all fail.
        let other = Key::random().unwrap();
        assert!(matches!(
            open(&other, &nonce, b"header", &sealed),
            Err(VaultError::Decrypt)
        ));
        assert!(open(&key, &nonce, b"Header", &sealed).is_err());
        let mut flipped = sealed.clone();
        flipped[0] ^= 1;
        assert!(open(&key, &nonce, b"header", &flipped).is_err());
    }

    #[test]
    fn derivation_is_deterministic_and_salted() {
        let params = KdfParams::INSECURE_FOR_TESTS;
        let a = derive_key(b"password", b"0123456789abcdef", params).unwrap();
        let b = derive_key(b"password", b"0123456789abcdef", params).unwrap();
        let c = derive_key(b"password", b"fedcba9876543210", params).unwrap();
        assert_eq!(a.as_bytes(), b.as_bytes());
        assert_ne!(a.as_bytes(), c.as_bytes());
        assert_eq!(format!("{a:?}"), "Key(..)");
    }

    #[test]
    fn argon2id_matches_the_reference_implementation() {
        // Computed with the reference C implementation (through argon2-cffi): Argon2id,
        // version 0x13, "password", salt "somesalt", 2 passes, 64 MiB, 1 lane, 32 bytes.
        let mut out = [0_u8; 32];
        argon2(
            Algorithm::Argon2id,
            b"password",
            b"somesalt",
            KdfParams {
                memory_kib: 65536,
                iterations: 2,
                lanes: 1,
            },
            &mut out,
        )
        .unwrap();
        let hex: String = out.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(
            hex,
            "09316115d5cf24ed5a15a31a3ba326e5cf32edc24702987c02b6566f61913cf7"
        );
    }

    #[test]
    fn costs_are_bounded() {
        assert!(KdfParams::RECOMMENDED.is_valid());
        assert!(KdfParams::INSECURE_FOR_TESTS.is_valid());
        for bad in [
            KdfParams {
                memory_kib: 4,
                iterations: 1,
                lanes: 1,
            },
            KdfParams {
                memory_kib: u32::MAX,
                iterations: 1,
                lanes: 1,
            },
            KdfParams {
                memory_kib: 64,
                iterations: 0,
                lanes: 1,
            },
            KdfParams {
                memory_kib: 1024,
                iterations: 1,
                lanes: 0,
            },
        ] {
            assert!(!bad.is_valid());
            assert!(derive_key(b"x", b"0123456789abcdef", bad).is_err());
        }
    }
}
