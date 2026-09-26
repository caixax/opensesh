//! PuTTY private keys (`.ppk`, versions 2 and 3), read without PuTTY (ADR 0024).
//!
//! The format (PuTTY's manual, appendix C): `Key: value` lines, the public key blob and the
//! private key blob in base64, and a MAC over both.
//!
//! - **Version 3:** with a passphrase, Argon2 (i, d or id; costs and salt in the file) makes 80
//!   bytes: an AES-256 key, a CBC IV and an HMAC-SHA-256 key. Without, the MAC key is empty.
//! - **Version 2:** the AES-256 key is SHA-1(0,passphrase) followed by SHA-1(1,passphrase), the IV
//!   is zero, and the MAC is HMAC-SHA-1 keyed with SHA-1("putty-private-key-file-mac-key" +
//!   passphrase) (an empty passphrase without encryption).
//!
//! The MAC covers the algorithm, the encryption, the comment, the public blob and the private
//! blob (decrypted, with its padding), each as an SSH string. A wrong passphrase shows as a MAC
//! that doesn't match. Version 1 and DSA keys aren't read.

use aes::Aes256;
use base64ct::Encoding as _;
use cbc::cipher::block_padding::NoPadding;
use cbc::cipher::{BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use sha1::{Digest as _, Sha1};
use ssh_encoding::Decode;
use ssh_key::private::{Ed25519Keypair, KeypairData};
use ssh_key::sha2::Sha256;
use ssh_key::{PrivateKey, PublicKey};
use zeroize::Zeroizing;

use crate::crypto::{self, KdfParams};
use crate::keys::KeyError;

/// The first line of a version 2 or 3 key starts like this.
pub const MAGIC: &str = "PuTTY-User-Key-File-";

/// Longest base64 section accepted, in lines (a 16384-bit RSA key needs about 50).
const MAX_LINES: usize = 1000;

/// Whether `text` looks like a PuTTY key (of any version).
#[must_use]
pub fn is_ppk(text: &str) -> bool {
    text.trim_start().starts_with(MAGIC)
}

struct Fields<'a> {
    version: u32,
    algorithm: &'a str,
    encryption: &'a str,
    comment: &'a str,
    public: Vec<u8>,
    private: Vec<u8>,
    mac: Vec<u8>,
    kdf: Option<(argon2::Algorithm, KdfParams, Vec<u8>)>,
}

/// Whether the key in `text` is encrypted (so the user must be asked for its passphrase).
///
/// # Errors
///
/// When it isn't a PuTTY key this version reads.
pub fn is_encrypted(text: &str) -> Result<bool, KeyError> {
    Ok(parse_fields(text)?.encryption != "none")
}

/// The private key in `text`, with `passphrase` when it is encrypted.
///
/// # Errors
///
/// [`KeyError::NeedsPassphrase`], [`KeyError::WrongPassphrase`], [`KeyError::Unsupported`]
/// (version 1, DSA, an unknown cipher) or [`KeyError::Damaged`].
pub fn parse(text: &str, passphrase: Option<&[u8]>) -> Result<PrivateKey, KeyError> {
    let fields = parse_fields(text)?;
    let encrypted = fields.encryption != "none";
    let passphrase: &[u8] = match (encrypted, passphrase) {
        (true, None) => return Err(KeyError::NeedsPassphrase),
        (true, Some(passphrase)) => passphrase,
        (false, _) => b"",
    };

    let mut private = Zeroizing::new(fields.private.clone());
    let mac_ok = if fields.version == 3 {
        let mut derived = Zeroizing::new([0_u8; 80]);
        if encrypted {
            let (flavour, params, salt) = fields
                .kdf
                .as_ref()
                .ok_or_else(|| damaged("no key derivation"))?;
            crypto::argon2(*flavour, passphrase, salt, *params, derived.as_mut_slice())
                .map_err(|_| KeyError::Unsupported("Argon2 settings".to_owned()))?;
            decrypt(&derived[..32], &derived[32..48], &mut private)?;
        }
        let mac_key: &[u8] = if encrypted { &derived[48..80] } else { &[] };
        let mut mac =
            <Hmac<Sha256> as Mac>::new_from_slice(mac_key).map_err(|_| damaged("MAC key"))?;
        mac_input(&fields, &private, |data| mac.update(data));
        mac.verify_slice(&fields.mac).is_ok()
    } else {
        if encrypted {
            let mut key = Zeroizing::new(Vec::with_capacity(40));
            for counter in [0_u32, 1] {
                let mut hash = Sha1::new();
                hash.update(counter.to_be_bytes());
                hash.update(passphrase);
                key.extend_from_slice(&hash.finalize());
            }
            decrypt(&key[..32], &[0; 16], &mut private)?;
        }
        let mut mac_key = Sha1::new();
        mac_key.update(b"putty-private-key-file-mac-key");
        mac_key.update(passphrase);
        let mac_key = Zeroizing::new(mac_key.finalize().to_vec());
        let mut mac =
            <Hmac<Sha1> as Mac>::new_from_slice(&mac_key).map_err(|_| damaged("MAC key"))?;
        mac_input(&fields, &private, |data| mac.update(data));
        mac.verify_slice(&fields.mac).is_ok()
    };
    if !mac_ok {
        return Err(if encrypted {
            KeyError::WrongPassphrase
        } else {
            damaged("the MAC doesn't match")
        });
    }
    build_key(&fields, &private)
}

fn damaged(what: &str) -> KeyError {
    KeyError::Damaged(format!("PuTTY key: {what}"))
}

fn decrypt(key: &[u8], iv: &[u8], data: &mut [u8]) -> Result<(), KeyError> {
    if data.len() % 16 != 0 {
        return Err(damaged("the private part isn't whole AES blocks"));
    }
    cbc::Decryptor::<Aes256>::new_from_slices(key, iv)
        .map_err(|_| damaged("cipher key"))?
        .decrypt_padded_mut::<NoPadding>(data)
        .map_err(|_| damaged("the private part isn't whole AES blocks"))?;
    Ok(())
}

fn mac_input(fields: &Fields<'_>, private: &[u8], mut update: impl FnMut(&[u8])) {
    for part in [
        fields.algorithm.as_bytes(),
        fields.encryption.as_bytes(),
        fields.comment.as_bytes(),
        &fields.public,
        private,
    ] {
        // Lengths beyond u32 can't come from a file this module accepted.
        let len = u32::try_from(part.len()).unwrap_or(u32::MAX);
        update(&len.to_be_bytes());
        update(part);
    }
}

/// The lines of a key file, read in order.
struct Lines<'a>(std::str::Lines<'a>);

impl<'a> Lines<'a> {
    fn line(&mut self) -> Result<&'a str, KeyError> {
        self.0
            .next()
            .map(str::trim_end)
            .ok_or_else(|| damaged("the file is cut short"))
    }

    /// The next `Key: value` line.
    fn pair(&mut self) -> Result<(&'a str, &'a str), KeyError> {
        self.line()?
            .split_once(": ")
            .ok_or_else(|| damaged("a line isn't `Key: value`"))
    }

    /// The value of the next line, which must be `name: value`.
    fn field(&mut self, name: &str) -> Result<&'a str, KeyError> {
        expect(self.pair()?, name)
    }

    /// `count` lines of base64, decoded.
    fn base64(&mut self, count: usize) -> Result<Vec<u8>, KeyError> {
        let mut text = String::new();
        for _ in 0..count {
            text.push_str(self.line()?.trim());
        }
        base64ct::Base64::decode_vec(&text).map_err(|_| damaged("bad base64"))
    }
}

fn expect<'a>((key, value): (&str, &'a str), name: &str) -> Result<&'a str, KeyError> {
    if key == name {
        Ok(value)
    } else {
        Err(damaged(&format!("expected {name}")))
    }
}

fn parse_fields(text: &str) -> Result<Fields<'_>, KeyError> {
    let mut lines = Lines(text.trim_start().lines());
    let (first, algorithm) = lines.pair().map_err(|_| KeyError::NotAKey)?;
    let version = match first.strip_prefix(MAGIC) {
        Some("3") => 3,
        Some("2") => 2,
        Some(_) => return Err(KeyError::Unsupported("PuTTY key version".to_owned())),
        None => return Err(KeyError::NotAKey),
    };
    let encryption = lines.field("Encryption")?;
    if !matches!(encryption, "none" | "aes256-cbc") {
        return Err(KeyError::Unsupported(format!("PuTTY cipher {encryption}")));
    }
    let comment = lines.field("Comment")?;
    let public_lines = count(lines.field("Public-Lines")?)?;
    let public = lines.base64(public_lines)?;

    let mut kdf = None;
    let mut next = lines.pair()?;
    if version == 3 && encryption != "none" {
        let flavour = match expect(next, "Key-Derivation")? {
            "Argon2id" => argon2::Algorithm::Argon2id,
            "Argon2i" => argon2::Algorithm::Argon2i,
            "Argon2d" => argon2::Algorithm::Argon2d,
            other => {
                return Err(KeyError::Unsupported(format!(
                    "PuTTY key derivation {other}"
                )));
            }
        };
        let memory_kib = number(lines.field("Argon2-Memory")?)?;
        let iterations = number(lines.field("Argon2-Passes")?)?;
        let lanes = number(lines.field("Argon2-Parallelism")?)?;
        let salt = hex(lines.field("Argon2-Salt")?)?;
        kdf = Some((
            flavour,
            KdfParams {
                memory_kib,
                iterations,
                lanes,
            },
            salt,
        ));
        next = lines.pair()?;
    }
    let private_lines = count(expect(next, "Private-Lines")?)?;
    let private = lines.base64(private_lines)?;
    let mac = hex(lines.field("Private-MAC")?)?;
    Ok(Fields {
        version,
        algorithm,
        encryption,
        comment,
        public,
        private,
        mac,
        kdf,
    })
}

fn number(text: &str) -> Result<u32, KeyError> {
    text.trim()
        .parse()
        .map_err(|_| damaged("a number doesn't parse"))
}

fn count(text: &str) -> Result<usize, KeyError> {
    let lines = usize::try_from(number(text)?).map_err(|_| damaged("too many lines"))?;
    if lines > MAX_LINES {
        return Err(damaged("too many lines"));
    }
    Ok(lines)
}

fn hex(text: &str) -> Result<Vec<u8>, KeyError> {
    let text = text.trim();
    if text.len() % 2 != 0 || text.len() > 256 {
        return Err(damaged("bad hex"));
    }
    (0..text.len())
        .step_by(2)
        .map(|at| {
            text.get(at..at + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or_else(|| damaged("bad hex"))
        })
        .collect()
}

/// SSH strings (u32 big-endian length, then the bytes) one after the other.
struct Strings<'a>(&'a [u8]);

impl<'a> Strings<'a> {
    fn next(&mut self) -> Result<&'a [u8], KeyError> {
        let (len, rest) = self
            .0
            .split_first_chunk::<4>()
            .ok_or_else(|| damaged("a key field is cut short"))?;
        let len = usize::try_from(u32::from_be_bytes(*len))
            .map_err(|_| damaged("a key field is too long"))?;
        let value = rest
            .get(..len)
            .ok_or_else(|| damaged("a key field is cut short"))?;
        self.0 = rest.get(len..).unwrap_or_default();
        Ok(value)
    }
}

fn put_string(out: &mut Vec<u8>, value: &[u8]) {
    let len = u32::try_from(value.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(value);
}

/// The key from the public blob and the decrypted private blob: rebuilt in OpenSSH's encoding
/// and read back by `ssh-key`, then checked against the public blob.
fn build_key(fields: &Fields<'_>, private: &[u8]) -> Result<PrivateKey, KeyError> {
    let public_key = PublicKey::from_bytes(&fields.public).map_err(|_| damaged("public key"))?;
    let mut public = Strings(&fields.public);
    let algorithm = public.next()?;
    if algorithm != fields.algorithm.as_bytes() {
        return Err(damaged("the algorithm names disagree"));
    }
    let mut secret = Strings(private);
    let keypair = match fields.algorithm {
        "ssh-ed25519" => {
            let public_point = public.next()?;
            let seed = ed25519_seed(secret.next()?)?;
            let keypair = Ed25519Keypair::from_seed(&seed);
            if keypair.public.0.as_slice() != public_point {
                return Err(damaged("the private key doesn't match the public key"));
            }
            KeypairData::Ed25519(keypair)
        }
        "ssh-rsa" => {
            let e = public.next()?;
            let n = public.next()?;
            let d = secret.next()?;
            let p = secret.next()?;
            let q = secret.next()?;
            let iqmp = secret.next()?;
            let mut encoded = Zeroizing::new(Vec::new());
            for part in [b"ssh-rsa".as_slice(), n, e, d, iqmp, p, q] {
                put_string(&mut encoded, part);
            }
            decode_keypair(&encoded)?
        }
        name if name.starts_with("ecdsa-sha2-") => {
            let curve = public.next()?;
            let point = public.next()?;
            let scalar = secret.next()?;
            let mut encoded = Zeroizing::new(Vec::new());
            for part in [name.as_bytes(), curve, point, scalar] {
                put_string(&mut encoded, part);
            }
            decode_keypair(&encoded)?
        }
        "ssh-dss" => return Err(KeyError::Unsupported("DSA keys".to_owned())),
        other => return Err(KeyError::Unsupported(format!("{other} keys"))),
    };
    let key = PrivateKey::new(keypair, fields.comment).map_err(|_| damaged("key"))?;
    if key.public_key().key_data() != public_key.key_data() {
        return Err(damaged("the private key doesn't match the public key"));
    }
    Ok(key)
}

fn decode_keypair(encoded: &[u8]) -> Result<KeypairData, KeyError> {
    let mut reader = encoded;
    KeypairData::decode(&mut reader).map_err(|_| damaged("private key"))
}

/// PuTTY writes the Ed25519 private key as its 32 bytes; older versions wrote an mpint of the
/// little-endian number instead (big-endian, maybe with a leading zero, maybe shorter).
fn ed25519_seed(field: &[u8]) -> Result<Zeroizing<[u8; 32]>, KeyError> {
    let mut seed = Zeroizing::new([0_u8; 32]);
    if field.len() == 32 {
        seed.copy_from_slice(field);
        return Ok(seed);
    }
    let digits = match field.split_first() {
        Some((0, rest)) => rest,
        _ => field,
    };
    if digits.len() > 32 {
        return Err(damaged("Ed25519 private key"));
    }
    // Big-endian digits of the little-endian seed.
    for (index, byte) in digits.iter().rev().enumerate() {
        seed[index] = *byte;
    }
    Ok(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_counts() {
        assert_eq!(hex("00ff10").unwrap(), [0, 255, 16]);
        assert!(hex("0").is_err());
        assert!(hex("zz").is_err());
        assert!(count("5000").is_err());
    }

    #[test]
    fn old_ed25519_mpint_form() {
        let seed = [7_u8; 32];
        let mut reversed = seed;
        reversed.reverse();
        let mut mpint = vec![0];
        mpint.extend_from_slice(&reversed);
        assert_eq!(*ed25519_seed(&mpint).unwrap(), seed);
        assert_eq!(*ed25519_seed(&seed).unwrap(), seed);
    }

    #[test]
    fn not_ppk() {
        assert!(!is_ppk("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(matches!(
            parse("hello: world", None),
            Err(KeyError::NotAKey)
        ));
        assert!(matches!(
            parse("PuTTY-User-Key-File-1: ssh-rsa\n", None),
            Err(KeyError::Unsupported(_))
        ));
    }
}
