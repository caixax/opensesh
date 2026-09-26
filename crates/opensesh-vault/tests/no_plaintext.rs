//! Sprint 6 "done when": after every kind of secret has gone through the keychain (identity
//! passwords set, replaced and cleared; keys generated and imported with passphrases; the vault
//! switched from the keyring to a master password, changed and remembered; wrong passwords),
//! no file in the config and data folders holds any of those secrets in clear, as raw bytes,
//! hex, base64 or UTF-16. And the wait after wrong master passwords holds across a restart.

#![allow(clippy::unwrap_used, clippy::panic, reason = "test helpers")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64ct::{Base64Unpadded, Encoding as _};
use opensesh_vault::crypto::KdfParams;
use opensesh_vault::keys::{self, KeyType};
use opensesh_vault::manager::{IdentityEdit, Keychain, PasswordChange};
use opensesh_vault::{KeyStore, MemoryKeyStore, Status, VaultError};
use secrecy::ExposeSecret;
use ssh_key::PrivateKey;

const FAST: KdfParams = KdfParams::INSECURE_FOR_TESTS;
const IDENTITY_PASSWORDS: [&str; 3] = [
    "Identity-Password-One-7f3a9c1e5b",
    "Identity-Password-Two-2d8e40aa91",
    "Identity-Password-Three-c05b7713ee",
];
const MASTER_ONE: &str = "Master-Password-One-91be30f2";
const MASTER_TWO: &str = "Master-Password-Two-44cd8a17";
const IMPORT_PASSPHRASE: &str = "fixture passphrase";

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/keys")
            .join(name),
    )
    .unwrap()
}

fn set(text: &str) -> PasswordChange {
    PasswordChange::Set(secrecy::SecretString::from(text))
}

/// Every form in which `needle` could sit in a file.
fn forms(needle: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut out = vec![("raw".to_owned(), needle.to_vec())];
    let hex: String = needle.iter().map(|byte| format!("{byte:02x}")).collect();
    out.push(("hex".to_owned(), hex.clone().into_bytes()));
    out.push(("HEX".to_owned(), hex.to_uppercase().into_bytes()));
    // Inside a longer base64 text the needle starts at any offset modulo 3: encode it from
    // each of the three and keep whole groups.
    for shift in 0..3 {
        let rest = needle.get(shift..).unwrap_or_default();
        let whole = rest.len() / 3 * 3;
        if whole >= 12 {
            out.push((
                format!("base64+{shift}"),
                Base64Unpadded::encode_string(&rest[..whole]).into_bytes(),
            ));
        }
    }
    if let Ok(text) = std::str::from_utf8(needle) {
        let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        out.push(("utf-16".to_owned(), utf16));
    }
    out
}

/// The private parts of `key`, and its whole encoding. (Not the lines of its PEM file: the
/// first ones only encode the public key, which `keychain.toml` does hold.)
fn key_material(key: &PrivateKey) -> Vec<(String, Vec<u8>)> {
    let data = key.key_data();
    let mut out = Vec::new();
    if let Some(ed) = data.ed25519() {
        out.push(("ed25519 seed".to_owned(), ed.private.to_bytes().to_vec()));
    }
    if let Some(ecdsa) = data.ecdsa() {
        out.push((
            "ecdsa scalar".to_owned(),
            ecdsa.private_key_bytes().to_vec(),
        ));
    }
    if let Some(rsa) = data.rsa() {
        for (name, value) in [
            ("rsa d", &rsa.private.d),
            ("rsa p", &rsa.private.p),
            ("rsa q", &rsa.private.q),
            ("rsa iqmp", &rsa.private.iqmp),
        ] {
            out.push((name.to_owned(), value.as_positive_bytes().unwrap().to_vec()));
        }
    }
    out.push((
        "vault encoding".to_owned(),
        keys::to_vault(key).unwrap().to_vec(),
    ));
    out
}

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(files(&path));
        } else {
            out.push(path);
        }
    }
    out
}

fn assert_no_secrets(root: &Path, secrets: &[(String, Vec<u8>)]) -> usize {
    let files = files(root);
    assert!(!files.is_empty());
    for path in &files {
        let bytes = std::fs::read(path).unwrap();
        for (what, needle) in secrets {
            for (form, pattern) in forms(needle) {
                assert!(
                    !bytes
                        .windows(pattern.len())
                        .any(|window| window == pattern.as_slice()),
                    "{} holds {what} as {form}",
                    path.display()
                );
            }
        }
    }
    files.len()
}

#[test]
fn no_secret_is_written_in_clear() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config");
    let data = root.path().join("data");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    let store = Arc::new(MemoryKeyStore::new());
    let open = || {
        Keychain::open(
            Some((&config, &data)),
            Arc::clone(&store) as Arc<dyn KeyStore>,
        )
    };
    let mut keychain = open();

    // Identities: set, replace, clear, and one that stays.
    let first = keychain
        .save_identity(IdentityEdit {
            name: "deploy".into(),
            user: "deploy".into(),
            password: set(IDENTITY_PASSWORDS[0]),
            ..IdentityEdit::default()
        })
        .unwrap();
    keychain
        .save_identity(IdentityEdit {
            id: first.clone(),
            name: "deploy".into(),
            user: "deploy".into(),
            password: set(IDENTITY_PASSWORDS[1]),
            ..IdentityEdit::default()
        })
        .unwrap();
    let second = keychain
        .save_identity(IdentityEdit {
            name: "root".into(),
            user: "root".into(),
            password: set(IDENTITY_PASSWORDS[2]),
            ..IdentityEdit::default()
        })
        .unwrap();

    // Keys: every generated type, and imports with passphrases.
    let mut key_ids = Vec::new();
    for kind in KeyType::ALL {
        key_ids.push(keychain.generate_key(kind, kind.as_str(), "test").unwrap());
    }
    for (file, name) in [
        ("rsa-enc", "imported rsa"),
        ("p384-enc", "imported p384"),
        ("ed25519-v3-enc.ppk", "imported ppk"),
        ("rsa-v2-enc.ppk", "imported ppk v2"),
    ] {
        let bytes = fixture(file);
        match keychain.import_key(&bytes, Some(IMPORT_PASSPHRASE.as_bytes()), name) {
            Ok(id) => key_ids.push(id),
            // The PPK RSA key is the same key as `rsa-enc`.
            Err(opensesh_vault::manager::KeychainOpError::Duplicate(_)) => {}
            Err(error) => panic!("{file}: {error}"),
        }
    }
    keychain
        .save_identity(IdentityEdit {
            id: second.clone(),
            name: "root".into(),
            user: "root".into(),
            key: Some(key_ids[0].clone()),
            ..IdentityEdit::default()
        })
        .unwrap();

    // Collect the key material while the vault is open.
    let mut secrets: Vec<(String, Vec<u8>)> = Vec::new();
    for id in &key_ids {
        let key = keychain.private_key(id).unwrap();
        for (what, bytes) in key_material(&key) {
            secrets.push((format!("key {id} {what}"), bytes));
        }
    }
    for (index, password) in IDENTITY_PASSWORDS.iter().enumerate() {
        secrets.push((
            format!("identity password {index}"),
            password.as_bytes().to_vec(),
        ));
    }
    for (name, text) in [
        ("master password one", MASTER_ONE),
        ("master password two", MASTER_TWO),
        ("import passphrase", IMPORT_PASSPHRASE),
    ] {
        secrets.push((name.to_owned(), text.as_bytes().to_vec()));
    }
    // The vault key itself, while the keyring holds it.
    let vault_key = store
        .get(&keychain.vault.keyring_name().unwrap())
        .unwrap()
        .unwrap();
    secrets.push(("vault key".to_owned(), vault_key.to_vec()));

    assert_no_secrets(root.path(), &secrets);

    // A master password, changed, remembered and forgotten; wrong guesses on the way.
    keychain
        .vault
        .set_password(MASTER_ONE.as_bytes(), FAST, false)
        .unwrap();
    assert!(store.is_empty());
    keychain
        .vault
        .change_password(MASTER_ONE.as_bytes(), MASTER_TWO.as_bytes(), FAST, 1_000)
        .unwrap();
    keychain.vault.set_remember(true).unwrap();
    keychain.vault.set_remember(false).unwrap();
    keychain.vault.lock();
    for _ in 0..3 {
        assert!(matches!(
            keychain.vault.unlock(MASTER_ONE.as_bytes(), 2_000),
            Err(VaultError::WrongPassword { .. })
        ));
    }

    let checked = assert_no_secrets(root.path(), &secrets);
    // keychain.toml (and its backups), vault.bin and vault-attempts.toml at least.
    assert!(checked >= 3, "only {checked} files");

    // The wait holds across a restart, and the right password works once it's over.
    let mut restarted = open();
    assert_eq!(restarted.vault.status(), Status::Locked);
    assert!(matches!(
        restarted.vault.unlock(MASTER_TWO.as_bytes(), 2_001),
        Err(VaultError::Wait(4))
    ));
    restarted
        .vault
        .unlock(MASTER_TWO.as_bytes(), 2_005)
        .unwrap();
    assert_eq!(
        restarted.identity_password(&first).unwrap().expose_secret(),
        IDENTITY_PASSWORDS[1]
    );
    assert_eq!(restarted.file.identities.len(), 2);
    assert_eq!(restarted.file.keys.len(), key_ids.len());
    assert_eq!(restarted.collect_garbage().unwrap(), 0);
}

/// The search itself finds a secret in each form, at any offset.
#[test]
fn the_search_finds_what_it_looks_for() {
    let secret = b"Planted-Secret-6b1f09d2c4".to_vec();
    let mut embedded = b"xy".to_vec();
    embedded.extend_from_slice(&secret);
    embedded.extend_from_slice(b"tail");
    let hex: String = secret.iter().map(|byte| format!("{byte:02X}")).collect();
    let utf16: Vec<u8> = String::from_utf8(secret.clone())
        .unwrap()
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    for (name, contents) in [
        ("raw", embedded.clone()),
        (
            "base64",
            Base64Unpadded::encode_string(&embedded).into_bytes(),
        ),
        ("hex", hex.into_bytes()),
        ("utf16", utf16),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(name), contents).unwrap();
        let found = std::panic::catch_unwind(|| {
            assert_no_secrets(dir.path(), &[("planted".to_owned(), secret.clone())]);
        });
        assert!(found.is_err(), "{name} was not found");
    }
}
