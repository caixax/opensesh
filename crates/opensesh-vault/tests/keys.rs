//! Real key files made by `ssh-keygen` and `puttygen` (see `fixtures/keys/README.md`).

#![allow(clippy::unwrap_used, clippy::panic, reason = "test helpers")]

use std::path::PathBuf;

use opensesh_vault::keys::{self, KeyError, KeyFormat};

const PASSPHRASE: &[u8] = b"fixture passphrase";

fn fixture(name: &str) -> (Vec<u8>, String) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/keys");
    let key = std::fs::read(dir.join(name)).unwrap();
    let public = std::fs::read_to_string(dir.join(format!("{name}.pub"))).unwrap();
    (key, public.trim().to_owned())
}

/// Imports `name` and checks it against the public key the tool printed.
fn check(name: &str, format: KeyFormat) {
    let (bytes, public) = fixture(name);
    let encrypted = name.contains("enc");
    assert_eq!(keys::needs_passphrase(&bytes).unwrap(), encrypted, "{name}");
    if encrypted {
        assert_eq!(
            keys::import(&bytes, None).unwrap_err(),
            KeyError::NeedsPassphrase,
            "{name}"
        );
        assert_eq!(
            keys::import(&bytes, Some(b"not it")).unwrap_err(),
            KeyError::WrongPassphrase,
            "{name}"
        );
    }
    let imported =
        keys::import(&bytes, Some(PASSPHRASE)).unwrap_or_else(|error| panic!("{name}: {error}"));
    assert_eq!(imported.format, format, "{name}");
    assert_eq!(imported.was_encrypted, encrypted, "{name}");
    assert_eq!(keys::info(&imported.key).public, public, "{name}");
    // And it survives the vault's encoding and an export.
    let stored = keys::to_vault(&imported.key).unwrap();
    assert_eq!(keys::from_vault(&stored).unwrap(), imported.key, "{name}");
    let exported = keys::export_openssh(&imported.key, None).unwrap();
    assert_eq!(
        keys::import(exported.as_bytes(), None).unwrap().key,
        imported.key,
        "{name}"
    );
}

#[test]
fn openssh_keys() {
    for name in [
        "ed25519",
        "ed25519-enc",
        "p256",
        "p384-enc",
        "p521",
        "rsa-enc",
    ] {
        check(name, KeyFormat::OpenSsh);
    }
}

#[test]
fn ppk_version_3() {
    for name in [
        "ed25519-v3.ppk",
        "ed25519-v3-enc.ppk",
        "ed25519-v3-argon2i-enc.ppk",
        "p256-v3-enc.ppk",
        "p256-v3-argon2d-enc.ppk",
        "p521-v3.ppk",
        "rsa-v3-enc.ppk",
    ] {
        check(name, KeyFormat::Ppk);
    }
}

#[test]
fn ppk_version_2() {
    for name in ["ed25519-v2.ppk", "rsa-v2-enc.ppk", "p384-v2-enc.ppk"] {
        check(name, KeyFormat::Ppk);
    }
}

#[test]
fn same_key_in_every_format() {
    let openssh = keys::import(&fixture("ed25519").0, None).unwrap().key;
    for name in ["ed25519-v3.ppk", "ed25519-v3-enc.ppk", "ed25519-v2.ppk"] {
        let ppk = keys::import(&fixture(name).0, Some(PASSPHRASE))
            .unwrap()
            .key;
        assert_eq!(ppk, openssh, "{name}");
    }
}

#[test]
fn old_pem_keys_are_refused_with_a_reason() {
    let (bytes, _) = fixture("rsa-pem");
    assert_eq!(keys::import(&bytes, None).unwrap_err(), KeyError::LegacyPem);
}

#[test]
fn a_changed_ppk_is_refused() {
    let (bytes, _) = fixture("ed25519-v2.ppk");
    let text = String::from_utf8(bytes).unwrap();
    // Change the comment: the MAC covers it.
    let changed = text.replace("Comment: fixture-ed25519", "Comment: someone-else");
    assert!(matches!(
        keys::import(changed.as_bytes(), None),
        Err(KeyError::Damaged(_))
    ));
    // Windows line endings are fine.
    let crlf = text.replace('\n', "\r\n");
    assert!(keys::import(crlf.as_bytes(), None).is_ok());
}
