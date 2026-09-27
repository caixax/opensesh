//! Which algorithms to offer (PLAN Sprint 7): modern ones by default, and a per-host "legacy" set
//! for old equipment (Cisco and similar) that adds SHA-1 key exchange and signatures, CBC
//! ciphers and hmac-sha1.

use std::borrow::Cow;

use russh::keys::{Algorithm, EcdsaCurve, HashAlg};
use russh::{Preferred, cipher, compression, kex, mac};

/// Modern key exchanges, most preferred first (russh's safe list).
const MODERN_KEX: &[kex::Name] = &[
    kex::MLKEM768X25519_SHA256,
    kex::CURVE25519,
    kex::CURVE25519_PRE_RFC_8731,
    kex::DH_GEX_SHA256,
    kex::DH_G18_SHA512,
    kex::DH_G17_SHA512,
    kex::DH_G16_SHA512,
    kex::DH_G15_SHA512,
    kex::DH_G14_SHA256,
    kex::EXTENSION_SUPPORT_AS_CLIENT,
    kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT,
];

/// The modern ones, then SHA-1 and 1024-bit groups.
const LEGACY_KEX: &[kex::Name] = &[
    kex::MLKEM768X25519_SHA256,
    kex::CURVE25519,
    kex::CURVE25519_PRE_RFC_8731,
    kex::ECDH_SHA2_NISTP256,
    kex::ECDH_SHA2_NISTP384,
    kex::ECDH_SHA2_NISTP521,
    kex::DH_GEX_SHA256,
    kex::DH_G18_SHA512,
    kex::DH_G17_SHA512,
    kex::DH_G16_SHA512,
    kex::DH_G15_SHA512,
    kex::DH_G14_SHA256,
    kex::DH_GEX_SHA1,
    kex::DH_G14_SHA1,
    kex::DH_G1_SHA1,
    kex::EXTENSION_SUPPORT_AS_CLIENT,
    kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT,
];

/// Host key algorithms without `ssh-rsa` (RSA with SHA-1), which OpenSSH turned off in 8.8.
const MODERN_KEYS: &[Algorithm] = &[
    Algorithm::Ed25519,
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP256,
    },
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP384,
    },
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP521,
    },
    Algorithm::Rsa {
        hash: Some(HashAlg::Sha512),
    },
    Algorithm::Rsa {
        hash: Some(HashAlg::Sha256),
    },
];

const LEGACY_KEYS: &[Algorithm] = &[
    Algorithm::Ed25519,
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP256,
    },
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP384,
    },
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP521,
    },
    Algorithm::Rsa {
        hash: Some(HashAlg::Sha512),
    },
    Algorithm::Rsa {
        hash: Some(HashAlg::Sha256),
    },
    Algorithm::Rsa { hash: None },
];

const MODERN_CIPHERS: &[cipher::Name] = &[
    cipher::CHACHA20_POLY1305,
    cipher::AES_256_GCM,
    cipher::AES_128_GCM,
    cipher::AES_256_CTR,
    cipher::AES_192_CTR,
    cipher::AES_128_CTR,
];

const LEGACY_CIPHERS: &[cipher::Name] = &[
    cipher::CHACHA20_POLY1305,
    cipher::AES_256_GCM,
    cipher::AES_128_GCM,
    cipher::AES_256_CTR,
    cipher::AES_192_CTR,
    cipher::AES_128_CTR,
    cipher::AES_256_CBC,
    cipher::AES_192_CBC,
    cipher::AES_128_CBC,
    cipher::TRIPLE_DES_CBC,
];

const MODERN_MACS: &[mac::Name] = &[
    mac::HMAC_SHA512_ETM,
    mac::HMAC_SHA256_ETM,
    mac::HMAC_SHA512,
    mac::HMAC_SHA256,
];

const LEGACY_MACS: &[mac::Name] = &[
    mac::HMAC_SHA512_ETM,
    mac::HMAC_SHA256_ETM,
    mac::HMAC_SHA512,
    mac::HMAC_SHA256,
    mac::HMAC_SHA1_ETM,
    mac::HMAC_SHA1,
];

/// Compression off: offer only `none`.
const NO_COMPRESSION: &[compression::Name] = &[compression::NONE];

/// Compression on: `zlib@openssh.com` (what OpenSSH servers accept by default) and `zlib`,
/// still accepting `none`.
const COMPRESSION: &[compression::Name] = &[
    compression::ZLIB_LEGACY,
    compression::ZLIB,
    compression::NONE,
];

/// The algorithms to offer: modern or legacy, with compression or not.
#[must_use]
pub fn preferred(legacy: bool, compress: bool) -> Preferred {
    Preferred {
        kex: Cow::Borrowed(if legacy { LEGACY_KEX } else { MODERN_KEX }),
        key: Cow::Borrowed(if legacy { LEGACY_KEYS } else { MODERN_KEYS }),
        host_key_certificates: Cow::Borrowed(&[]),
        cipher: Cow::Borrowed(if legacy {
            LEGACY_CIPHERS
        } else {
            MODERN_CIPHERS
        }),
        mac: Cow::Borrowed(if legacy { LEGACY_MACS } else { MODERN_MACS }),
        compression: Cow::Borrowed(if compress {
            COMPRESSION
        } else {
            NO_COMPRESSION
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names<T: AsRef<str>>(list: &[T]) -> Vec<String> {
        list.iter().map(|name| name.as_ref().to_owned()).collect()
    }

    #[test]
    fn modern_has_no_sha1_or_cbc() {
        let modern = preferred(false, false);
        let all = [
            names(&modern.kex),
            names(&modern.cipher),
            names(&modern.mac),
            modern.key.iter().map(ToString::to_string).collect(),
        ]
        .concat();
        for name in &all {
            assert!(!name.contains("sha1"), "{name}");
            assert!(!name.contains("-cbc"), "{name}");
            assert_ne!(name, "ssh-rsa", "{name}");
        }
        assert_eq!(names(&modern.compression), ["none"]);
    }

    #[test]
    fn legacy_adds_the_old_ones_after_the_modern_ones() {
        let legacy = preferred(true, true);
        let kex = names(&legacy.kex);
        assert!(kex.contains(&"diffie-hellman-group14-sha1".to_owned()));
        assert!(kex.contains(&"diffie-hellman-group1-sha1".to_owned()));
        // Modern first: a server that supports both still gets a modern one.
        assert_eq!(kex[0], "mlkem768x25519-sha256");
        assert!(names(&legacy.cipher).contains(&"aes128-cbc".to_owned()));
        assert!(names(&legacy.cipher).contains(&"3des-cbc".to_owned()));
        assert!(names(&legacy.mac).contains(&"hmac-sha1".to_owned()));
        assert!(legacy.key.iter().any(|key| key.to_string() == "ssh-rsa"));
        assert_eq!(names(&legacy.compression)[0], "zlib@openssh.com");
    }
}
