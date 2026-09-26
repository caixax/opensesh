//! `keychain.toml` in the config folder (Sprint 6): identities and SSH keys, without their
//! secrets. Passwords and private keys are `vault:<ulid>` references into the vault; the rest
//! (names, users, public keys, fingerprints) stays readable, like `hosts.toml`.
//!
//! Loading is lenient per entry and fixes what it can with a warning. Anything that isn't a
//! `vault:` reference where a secret belongs is dropped (and never written back), and warnings
//! never quote values, because this is the file where a secret might be pasted by mistake.

use std::collections::HashSet;
use std::path::Path;

use opensesh_core::config::Warning;
use serde::{Deserialize, Serialize};
use ssh_key::PublicKey;
use toml::{Table, Value};
use ulid::Ulid;

use crate::keys;
use crate::vault::parse_ref;

/// File name in the config folder.
pub const KEYCHAIN_FILE: &str = "keychain.toml";

/// Current layout of `keychain.toml`.
pub const SCHEMA_VERSION: i64 = 1;

const HEADER: &str = "# OpenSesh identities and SSH keys. Secrets are not here: passwords and private keys\n\
                      # live in the encrypted vault and are referenced as vault:<id>.\n";

/// Longest id accepted.
const MAX_ID_LEN: usize = 64;

/// A user name with a password and/or a key, given to hosts and groups.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    /// Stable id (a ULID).
    #[serde(default)]
    pub id: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// User name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user: String,
    /// `vault:<ulid>` of the password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Id of the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Notes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// Keys this version doesn't know, written back unchanged.
    #[serde(flatten)]
    pub extra: Table,
}

fn is_zero(value: &i64) -> bool {
    *value == 0
}

/// An SSH key: its public half and a reference to the private one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyEntry {
    /// Stable id (a ULID).
    #[serde(default)]
    pub id: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// SSH algorithm name (from the public key).
    #[serde(default)]
    pub algorithm: String,
    /// Size in bits (from the public key).
    #[serde(default)]
    pub bits: u32,
    /// The OpenSSH public key line.
    #[serde(default)]
    pub public: String,
    /// `SHA256:...` (from the public key).
    #[serde(default)]
    pub fingerprint: String,
    /// `vault:<ulid>` of the private key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private: Option<String>,
    /// `generated`, `openssh` or `ppk`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
    /// When it was added (seconds since the Unix epoch).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub created: i64,
    /// Keys this version doesn't know, written back unchanged.
    #[serde(flatten)]
    pub extra: Table,
}

impl KeyEntry {
    /// Fills the algorithm, size and fingerprint from the public key; `false` when the public key
    /// doesn't parse.
    pub fn refresh_from_public(&mut self) -> bool {
        let Ok(public) = PublicKey::from_openssh(self.public.trim()) else {
            return false;
        };
        let info = keys::public_info(&public);
        self.algorithm = info.algorithm;
        self.bits = info.bits;
        self.fingerprint = info.fingerprint;
        self.public = info.public;
        true
    }
}

/// Why `keychain.toml` can't be read.
#[derive(Debug, thiserror::Error)]
pub enum KeychainError {
    /// The file exists but can't be read.
    #[error("could not read {path}")]
    Read {
        /// The file.
        path: String,
        /// Why.
        #[source]
        source: std::io::Error,
    },
    /// Not valid TOML.
    #[error("{path} is not valid TOML ({message})")]
    Syntax {
        /// The file.
        path: String,
        /// Where (never the text itself).
        message: String,
    },
}

/// The contents of `keychain.toml`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeychainFile {
    /// Identities, in file order.
    pub identities: Vec<Identity>,
    /// Keys, in file order.
    pub keys: Vec<KeyEntry>,
    /// Written by a newer OpenSesh: shown, never saved over.
    pub read_only: bool,
    /// Top-level keys this version doesn't know.
    pub extra: Table,
}

fn warning(key: impl Into<String>, message: impl Into<String>) -> Warning {
    Warning {
        key: key.into(),
        message: message.into(),
    }
}

/// A new id for an identity or a key.
#[must_use]
pub fn new_id() -> String {
    Ulid::generate().to_string()
}

fn valid_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= MAX_ID_LEN && !id.chars().any(char::is_control)
}

/// Where a TOML error is, without quoting the text.
fn position(text: &str, error: &toml::de::Error) -> String {
    match error.span() {
        Some(span) => {
            let line = text
                .get(..span.start)
                .map_or(0, |before| before.matches('\n').count())
                + 1;
            format!("line {line}")
        }
        None => "somewhere".to_owned(),
    }
}

/// Each table of the array `key`, skipping (with a warning) what doesn't fit.
fn entries<T: for<'de> Deserialize<'de>>(
    root: &mut Table,
    key: &str,
    warnings: &mut Vec<Warning>,
) -> Vec<T> {
    let Some(value) = root.remove(key) else {
        return Vec::new();
    };
    let Value::Array(items) = value else {
        warnings.push(warning(key, format!("expected [[{key}]] tables; skipped")));
        return Vec::new();
    };
    items
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| match item.try_into::<T>() {
            Ok(entry) => Some(entry),
            Err(_) => {
                warnings.push(warning(
                    format!("{key}[{index}]"),
                    "skipped: a value has the wrong type",
                ));
                None
            }
        })
        .collect()
}

fn to_table<T: Serialize>(value: &T) -> Table {
    match Value::try_from(value) {
        Ok(Value::Table(table)) => table,
        _ => Table::new(),
    }
}

impl KeychainFile {
    /// Parses the text of `keychain.toml`, fixing what it can and reporting it.
    ///
    /// # Errors
    ///
    /// Where the text isn't valid TOML (the position only).
    pub fn from_toml_str(text: &str) -> Result<(Self, Vec<Warning>), String> {
        let mut root: Table = text
            .parse()
            .map_err(|error: toml::de::Error| position(text, &error))?;
        let mut warnings = Vec::new();
        let version = match root.remove("schema_version") {
            None => SCHEMA_VERSION,
            Some(Value::Integer(version)) => version,
            Some(_) => {
                warnings.push(warning("schema_version", "expected an integer"));
                SCHEMA_VERSION
            }
        };
        let read_only = version > SCHEMA_VERSION;
        if read_only {
            warnings.push(warning(
                "schema_version",
                format!(
                    "version {version} is newer than this OpenSesh supports ({SCHEMA_VERSION}); \
                     the keychain is read-only until you upgrade"
                ),
            ));
        }
        let identities = entries(&mut root, "identity", &mut warnings);
        let keys = entries(&mut root, "key", &mut warnings);
        let mut file = Self {
            identities,
            keys,
            read_only,
            extra: root,
        };
        file.check(&mut warnings);
        Ok((file, warnings))
    }

    fn check(&mut self, warnings: &mut Vec<Warning>) {
        let mut ids = HashSet::new();
        let mut keys = Vec::with_capacity(self.keys.len());
        for (index, mut key) in std::mem::take(&mut self.keys).into_iter().enumerate() {
            let label = format!("key[{index}]");
            if !valid_id(&key.id) {
                key.id = new_id();
                warnings.push(warning(
                    &label,
                    "no valid id; a new one is saved with the next change",
                ));
            }
            if !ids.insert(key.id.clone()) {
                warnings.push(warning(&label, "its id is used twice; skipped"));
                continue;
            }
            if !key.refresh_from_public() {
                warnings.push(warning(
                    format!("{label}.public"),
                    "not an OpenSSH public key",
                ));
            }
            if key
                .private
                .as_deref()
                .is_some_and(|text| parse_ref(text).is_none())
            {
                warnings.push(warning(
                    format!("{label}.private"),
                    "not a vault reference; dropped",
                ));
                key.private = None;
            }
            key.name = key.name.trim().to_owned();
            if key.name.is_empty() {
                key.name = if key.fingerprint.is_empty() {
                    "Key".to_owned()
                } else {
                    key.fingerprint.clone()
                };
            }
            keys.push(key);
        }
        self.keys = keys;

        let key_ids: HashSet<String> = self.keys.iter().map(|key| key.id.clone()).collect();
        let mut identities = Vec::with_capacity(self.identities.len());
        for (index, mut identity) in std::mem::take(&mut self.identities).into_iter().enumerate() {
            let label = format!("identity[{index}]");
            if !valid_id(&identity.id) {
                identity.id = new_id();
                warnings.push(warning(
                    &label,
                    "no valid id; a new one is saved with the next change",
                ));
            }
            if !ids.insert(identity.id.clone()) {
                warnings.push(warning(&label, "its id is used twice; skipped"));
                continue;
            }
            if identity
                .password
                .as_deref()
                .is_some_and(|text| parse_ref(text).is_none())
            {
                warnings.push(warning(
                    format!("{label}.password"),
                    "not a vault reference; dropped",
                ));
                identity.password = None;
            }
            if identity
                .key
                .as_ref()
                .is_some_and(|key| !key_ids.contains(key))
            {
                warnings.push(warning(format!("{label}.key"), "unknown key; dropped"));
                identity.key = None;
            }
            identity.user = identity.user.trim().to_owned();
            identity.name = identity.name.trim().to_owned();
            if identity.name.is_empty() {
                identity.name = if identity.user.is_empty() {
                    "Identity".to_owned()
                } else {
                    identity.user.clone()
                };
            }
            identities.push(identity);
        }
        self.identities = identities;
    }

    /// The file's text.
    ///
    /// # Errors
    ///
    /// Only if a value can't be written as TOML (not expected).
    pub fn to_toml_string(&self) -> Result<String, String> {
        let mut root = self.extra.clone();
        root.insert("schema_version".to_owned(), Value::Integer(SCHEMA_VERSION));
        let array = |items: Vec<Table>| Value::Array(items.into_iter().map(Value::Table).collect());
        if !self.identities.is_empty() {
            root.insert(
                "identity".to_owned(),
                array(self.identities.iter().map(to_table).collect()),
            );
        }
        if !self.keys.is_empty() {
            root.insert(
                "key".to_owned(),
                array(self.keys.iter().map(to_table).collect()),
            );
        }
        let body = toml::to_string(&root).map_err(|error| error.to_string())?;
        Ok(format!("{HEADER}\n{body}"))
    }

    /// Reads `path`; a missing file is an empty keychain.
    ///
    /// # Errors
    ///
    /// When the file can't be read or isn't TOML.
    pub fn load(path: &Path) -> Result<(Self, Vec<Warning>), KeychainError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Self::default(), Vec::new()));
            }
            Err(source) => {
                return Err(KeychainError::Read {
                    path: path.display().to_string(),
                    source,
                });
            }
        };
        Self::from_toml_str(&text).map_err(|message| KeychainError::Syntax {
            path: path.display().to_string(),
            message,
        })
    }

    /// Identity `id`.
    #[must_use]
    pub fn identity(&self, id: &str) -> Option<&Identity> {
        self.identities.iter().find(|identity| identity.id == id)
    }

    /// Key `id`.
    #[must_use]
    pub fn key(&self, id: &str) -> Option<&KeyEntry> {
        self.keys.iter().find(|key| key.id == id)
    }

    /// The key with this fingerprint, if it is here already.
    #[must_use]
    pub fn key_by_fingerprint(&self, fingerprint: &str) -> Option<&KeyEntry> {
        self.keys.iter().find(|key| key.fingerprint == fingerprint)
    }

    /// Every vault secret the file refers to.
    #[must_use]
    pub fn secret_ids(&self) -> HashSet<Ulid> {
        self.identities
            .iter()
            .filter_map(|identity| identity.password.as_deref())
            .chain(self.keys.iter().filter_map(|key| key.private.as_deref()))
            .filter_map(parse_ref)
            .collect()
    }

    /// Identities that use key `id`.
    #[must_use]
    pub fn identities_using(&self, key: &str) -> Vec<&Identity> {
        self.identities
            .iter()
            .filter(|identity| identity.key.as_deref() == Some(key))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PUBLIC: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC6tmVU1VE59P7TYx6UJYcZkhy7FRiLjhH6gdK9Sayyd fixture-ed25519";

    #[test]
    fn round_trip() {
        let id = Ulid::generate();
        let text = format!(
            r#"
schema_version = 1
future = "kept"

[[identity]]
id = "01J9ZM0000000000000000IDDE"
name = "deploy"
user = "deploy"
password = "vault:{id}"
key = "01J9ZM0000000000000000KEY1"
shoe_size = 42

[[key]]
id = "01J9ZM0000000000000000KEY1"
name = "laptop"
public = "{PUBLIC}"
private = "vault:{id}"
origin = "openssh"
"#
        );
        let (file, warnings) = KeychainFile::from_toml_str(&text).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let key = file.key("01J9ZM0000000000000000KEY1").unwrap();
        assert_eq!(key.algorithm, "ssh-ed25519");
        assert_eq!(key.bits, 256);
        assert!(key.fingerprint.starts_with("SHA256:"));
        assert_eq!(file.secret_ids(), HashSet::from([id]));
        assert_eq!(file.identities_using("01J9ZM0000000000000000KEY1").len(), 1);

        let saved = file.to_toml_string().unwrap();
        assert!(saved.contains("future = \"kept\""));
        assert!(saved.contains("shoe_size = 42"));
        let (again, _) = KeychainFile::from_toml_str(&saved).unwrap();
        assert_eq!(again, file);
    }

    #[test]
    fn secrets_in_clear_are_dropped_without_being_quoted() {
        let text = format!(
            r#"
[[identity]]
id = "A"
name = "oops"
password = "hunter2"
key = "missing"

[[key]]
id = "B"
public = "{PUBLIC}"
private = "-----BEGIN OPENSSH PRIVATE KEY-----"

[[key]]
id = "C"
bits = "hunter3"
"#
        );
        let (file, warnings) = KeychainFile::from_toml_str(&text).unwrap();
        assert_eq!(file.identities[0].password, None);
        assert_eq!(file.identities[0].key, None);
        assert_eq!(file.keys.len(), 1);
        assert_eq!(file.keys[0].private, None);
        let all = format!("{warnings:?}");
        assert!(!all.contains("hunter"), "{all}");
        assert!(!all.contains("BEGIN"), "{all}");
        let saved = file.to_toml_string().unwrap();
        assert!(!saved.contains("hunter2"));
        assert!(!saved.contains("BEGIN"));
    }

    #[test]
    fn syntax_errors_name_the_line_only() {
        let error = KeychainFile::from_toml_str("[[identity]]\nname = \"a\"\npassword = hunter2\n")
            .unwrap_err();
        assert_eq!(error, "line 3");
    }

    #[test]
    fn ids_and_names_are_fixed() {
        let text = r#"
[[identity]]
user = "root"

[[identity]]
id = "X"

[[identity]]
id = "X"
"#;
        let (file, warnings) = KeychainFile::from_toml_str(text).unwrap();
        assert_eq!(file.identities.len(), 2);
        assert_eq!(file.identities[0].name, "root");
        assert_eq!(file.identities[1].name, "Identity");
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn newer_files_are_read_only() {
        let (file, warnings) = KeychainFile::from_toml_str("schema_version = 9").unwrap();
        assert!(file.read_only);
        assert_eq!(warnings.len(), 1);
    }
}
