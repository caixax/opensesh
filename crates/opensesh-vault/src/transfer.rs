//! The keychain in an OpenSesh bundle (Sprint 16): identities and keys, with their passwords and
//! private keys sealed under an export password.
//!
//! The secrets travel as a small file in the vault's own format ([`crate::format`]) whose key is
//! held by the export password (Argon2id, then XChaCha20-Poly1305): the code that protects
//! `vault.bin` protects the export too. The identities' and keys' public parts (names, user
//! names, public keys) stay readable, as in `keychain.toml`.

use std::collections::{BTreeMap, HashMap};

use base64ct::{Base64, Encoding};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::VaultError;
use crate::crypto::{self, KdfParams, Key};
use crate::format::{self, Header, Holder, Secret, SecretKind};
use crate::keychain::{Identity, KeyEntry};
use crate::keys;
use crate::manager::{IdentityEdit, Keychain, KeychainOpError, PasswordChange};
use crate::vault::parse_ref;

/// The keychain's part of a bundle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KeychainBundle {
    /// The identities; their `password` points into `sealed`.
    #[serde(default, rename = "identity", skip_serializing_if = "Vec::is_empty")]
    pub identities: Vec<Identity>,
    /// The keys; their `private` points into `sealed`.
    #[serde(default, rename = "key", skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<KeyEntry>,
    /// The secrets: a vault file held by the export password, in Base64.
    #[serde(default)]
    pub sealed: String,
}

/// What importing a bundle's keychain did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BundleImport {
    /// The bundle's identity ids and the ids they have here.
    pub identities: HashMap<String, String>,
    /// Identities added.
    pub identities_added: usize,
    /// Keys added.
    pub keys_added: usize,
    /// Keys that were here already (same fingerprint).
    pub keys_known: usize,
}

impl Keychain {
    /// Every identity and key with their secrets, sealed under `password`. The vault must be
    /// unlocked.
    ///
    /// # Errors
    ///
    /// When the vault is locked, a secret is missing, or the random generator fails.
    pub fn export_bundle(
        &self,
        password: &SecretString,
        params: KdfParams,
    ) -> Result<KeychainBundle, KeychainOpError> {
        use secrecy::ExposeSecret;

        let mut secrets = BTreeMap::new();
        let references = self
            .file
            .identities
            .iter()
            .filter_map(|identity| identity.password.as_deref())
            .chain(
                self.file
                    .keys
                    .iter()
                    .filter_map(|key| key.private.as_deref()),
            );
        for reference in references {
            if let Some(id) = parse_ref(reference) {
                secrets.insert(id, self.vault.get(&id)?.clone());
            }
        }
        let key = Key::random()?;
        let vault_id = crypto::random_array::<16>()?;
        let slot =
            Header::password_slot(vault_id, &key, password.expose_secret().as_bytes(), params)?;
        let mut header = Header {
            vault_id,
            holder: Holder::Password(slot),
            body_nonce: [0; crypto::NONCE_LEN],
        };
        let sealed = format::seal_file(&mut header, &key, &secrets)?;
        Ok(KeychainBundle {
            identities: self.file.identities.clone(),
            keys: self.file.keys.clone(),
            sealed: Base64::encode_string(&sealed),
        })
    }

    /// Adds a bundle's keys and identities. Keys already here (same fingerprint) are kept, and
    /// an identity with the same name, user and key is reused.
    ///
    /// # Errors
    ///
    /// [`VaultError::Decrypt`] for a wrong password; when the vault can't take the secrets
    /// (locked), or a file can't be written.
    pub fn import_bundle(
        &mut self,
        bundle: &KeychainBundle,
        password: &SecretString,
    ) -> Result<BundleImport, KeychainOpError> {
        use secrecy::ExposeSecret;

        let secrets = open_sealed(&bundle.sealed, password.expose_secret().as_bytes())?;
        let secret = |reference: Option<&str>| -> Option<&Secret> {
            reference
                .and_then(parse_ref)
                .and_then(|id| secrets.get(&id))
        };
        let mut result = BundleImport::default();
        let mut key_ids = HashMap::new();
        for entry in &bundle.keys {
            if let Some(known) = self.file.key_by_fingerprint(&entry.fingerprint) {
                key_ids.insert(entry.id.clone(), known.id.clone());
                result.keys_known += 1;
                continue;
            }
            let Some(private) = secret(entry.private.as_deref())
                .filter(|secret| secret.kind == SecretKind::PrivateKey)
            else {
                continue;
            };
            let key = keys::from_vault(&private.data)?;
            let id = self.add_key(&key, &entry.name, &entry.origin)?;
            let id = self.keep_id(Record::Key, id, &entry.id)?;
            key_ids.insert(entry.id.clone(), id);
            result.keys_added += 1;
        }
        for identity in &bundle.identities {
            let key = identity
                .key
                .as_ref()
                .and_then(|key| key_ids.get(key))
                .cloned();
            if let Some(same) = self.file.identities.iter().find(|here| {
                here.name == identity.name && here.user == identity.user && here.key == key
            }) {
                result
                    .identities
                    .insert(identity.id.clone(), same.id.clone());
                continue;
            }
            let password = match secret(identity.password.as_deref())
                .filter(|secret| secret.kind == SecretKind::Password)
            {
                Some(secret) => PasswordChange::Set(SecretString::from(
                    String::from_utf8_lossy(&secret.data).into_owned(),
                )),
                None => PasswordChange::Keep,
            };
            let id = self.save_identity(IdentityEdit {
                id: String::new(),
                name: identity.name.clone(),
                user: identity.user.clone(),
                password,
                key,
                notes: identity.notes.clone(),
            })?;
            let id = self.keep_id(Record::Identity, id, &identity.id)?;
            result.identities.insert(identity.id.clone(), id);
            result.identities_added += 1;
        }
        Ok(result)
    }
}

/// What [`Keychain::keep_id`] renames.
#[derive(Clone, Copy)]
enum Record {
    Identity,
    Key,
}

impl Keychain {
    /// Gives the record just added as `new` the id it had in the bundle, when no record here
    /// has it: hosts synced from the other computer refer to identities by those ids.
    fn keep_id(
        &mut self,
        record: Record,
        new: String,
        wanted: &str,
    ) -> Result<String, KeychainOpError> {
        let valid = !wanted.trim().is_empty()
            && wanted.len() <= 64
            && !wanted.chars().any(char::is_control);
        let taken = match record {
            Record::Identity => self.file.identity(wanted).is_some(),
            Record::Key => self.file.key(wanted).is_some(),
        };
        if !valid || taken || new == wanted {
            return Ok(new);
        }
        let mut file = self.file.clone();
        match record {
            Record::Identity => {
                for identity in &mut file.identities {
                    if identity.id == new {
                        wanted.clone_into(&mut identity.id);
                    }
                }
            }
            Record::Key => {
                for key in &mut file.keys {
                    if key.id == new {
                        wanted.clone_into(&mut key.id);
                    }
                }
                for identity in &mut file.identities {
                    if identity.key.as_deref() == Some(new.as_str()) {
                        identity.key = Some(wanted.to_owned());
                    }
                }
            }
        }
        self.commit(file)?;
        Ok(wanted.to_owned())
    }
}

/// The secrets in a bundle's `sealed` text, opened with `password`.
///
/// # Errors
///
/// [`VaultError::Decrypt`] for a wrong password (or a changed bundle), [`VaultError::Format`]
/// for text that isn't a sealed keychain.
pub fn open_sealed(sealed: &str, password: &[u8]) -> Result<BTreeMap<Ulid, Secret>, VaultError> {
    let bytes = Base64::decode_vec(sealed.trim())
        .map_err(|_| VaultError::Format("the bundle's secrets aren't Base64"))?;
    let (header, _) = format::read_header(&bytes)?;
    let key = header.unwrap_key(password)?;
    let (_, secrets) = format::open_file(&bytes, &key)?;
    Ok(secrets)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::MemoryKeyStore;
    use crate::keys::KeyType;

    fn keychain() -> Keychain {
        let mut keychain = Keychain::open(None, Arc::new(MemoryKeyStore::default()));
        keychain.ensure_vault().unwrap();
        keychain
    }

    #[test]
    fn a_keychain_travels_sealed() {
        let mut here = keychain();
        let key = here
            .generate_key(KeyType::Ed25519, "laptop", "me@laptop")
            .unwrap();
        let identity = here
            .save_identity(IdentityEdit {
                name: "Deploy".to_owned(),
                user: "deploy".to_owned(),
                password: PasswordChange::Set(SecretString::from("hunter2".to_owned())),
                key: Some(key.clone()),
                ..IdentityEdit::default()
            })
            .unwrap();
        let password = SecretString::from("export pass".to_owned());
        let bundle = here
            .export_bundle(&password, KdfParams::INSECURE_FOR_TESTS)
            .unwrap();
        // Nothing secret in the clear.
        let text = toml::to_string(&bundle).unwrap();
        assert!(!text.contains("hunter2"), "{text}");
        assert!(text.contains("deploy"));

        let wrong = SecretString::from("nope".to_owned());
        let mut there = keychain();
        assert!(matches!(
            there.import_bundle(&bundle, &wrong),
            Err(KeychainOpError::Vault(VaultError::Decrypt))
        ));
        let imported = there.import_bundle(&bundle, &password).unwrap();
        assert_eq!(imported.keys_added, 1);
        assert_eq!(imported.identities_added, 1);
        let new_id = &imported.identities[&identity];
        // The ids hosts refer to are kept on a computer that doesn't have them.
        assert_eq!(new_id, &identity);
        {
            use secrecy::ExposeSecret;
            assert_eq!(
                there.identity_password(new_id).unwrap().expose_secret(),
                "hunter2"
            );
        }
        let new_key = there.file.identity(new_id).unwrap().key.clone().unwrap();
        assert_eq!(
            there.private_key(&new_key).unwrap().public_key(),
            here.private_key(&key).unwrap().public_key()
        );

        // Again: everything is known, nothing is added twice.
        let again = there.import_bundle(&bundle, &password).unwrap();
        assert_eq!(
            (again.keys_added, again.keys_known, again.identities_added),
            (0, 1, 0)
        );
        assert_eq!(&again.identities[&identity], new_id);
        assert_eq!(there.file.identities.len(), 1);
    }
}
