//! Keyboard shortcuts changed by the user, in `keybindings.toml` (PLAN §4.2, §6.4).
//!
//! The defaults live with the actions (the app's action registry); this file only holds what the
//! user changed: an action id and its new key sequence, in Qt's portable text ("Ctrl+Shift+P"),
//! or an empty string for an action the user left without a shortcut.
//!
//! ```toml
//! schema_version = 1
//! [bindings]
//! "tab.close" = "Ctrl+Shift+W"
//! "app.quickConnect" = ""
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use toml::{Table, Value};

use crate::config::Warning;

/// Name of the file inside the config directory.
pub const KEYBINDINGS_FILE: &str = "keybindings.toml";

/// Current layout of the file.
pub const SCHEMA_VERSION: i64 = 1;

const HEADER: &str = "# OpenSesh keyboard shortcuts that differ from the defaults. An empty value\n\
                      # removes an action's shortcut. OpenSesh rewrites this file when a shortcut\n\
                      # is changed in the app: comments are not kept.\n";

/// Whether `id` can name an action: ASCII letters, digits, `.`, `_` and `-`, 1 to 64 characters.
#[must_use]
pub fn valid_action_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// Whether `sequence` can be stored: at most 64 characters, no control characters. (Qt decides
/// whether it names real keys; an unknown one simply never fires.)
#[must_use]
pub fn valid_sequence(sequence: &str) -> bool {
    sequence.chars().count() <= 64 && !sequence.chars().any(char::is_control)
}

/// The user's shortcut changes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Keybindings {
    /// Action id to key sequence (`""` for none).
    pub bindings: BTreeMap<String, String>,
    /// Unknown top-level keys, written back unchanged.
    pub extra: Table,
    /// The file comes from a newer OpenSesh, or couldn't be parsed: never overwrite it.
    pub read_only: bool,
}

impl Keybindings {
    /// Reads the file (a missing file means no changes).
    #[must_use]
    pub fn load_file(path: &Path) -> (Self, Vec<Warning>) {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_toml_str(&text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (Self::default(), Vec::new())
            }
            Err(error) => (
                Self {
                    read_only: true,
                    ..Self::default()
                },
                vec![Warning {
                    key: KEYBINDINGS_FILE.to_owned(),
                    message: format!("could not read the file: {error}"),
                }],
            ),
        }
    }

    /// Parses the file; invalid entries are skipped with a warning.
    #[must_use]
    pub fn from_toml_str(text: &str) -> (Self, Vec<Warning>) {
        let mut warnings = Vec::new();
        let mut root: Table = match text.parse() {
            Ok(root) => root,
            Err(error) => {
                let error: toml::de::Error = error;
                warnings.push(Warning {
                    key: KEYBINDINGS_FILE.to_owned(),
                    message: format!("not valid TOML: {error}"),
                });
                return (
                    Self {
                        read_only: true,
                        ..Self::default()
                    },
                    warnings,
                );
            }
        };
        let version = root
            .remove("schema_version")
            .and_then(|value| value.as_integer())
            .unwrap_or(SCHEMA_VERSION);
        let mut bindings = BTreeMap::new();
        match root.remove("bindings") {
            None => {}
            Some(Value::Table(table)) => {
                for (id, value) in table {
                    let key = format!("bindings.{id}");
                    match value {
                        Value::String(sequence)
                            if valid_action_id(&id) && valid_sequence(&sequence) =>
                        {
                            bindings.insert(id, sequence.trim().to_owned());
                        }
                        Value::String(_) => warnings.push(Warning {
                            key,
                            message: "not a valid action id or key sequence; skipped".to_owned(),
                        }),
                        other => warnings.push(Warning {
                            key,
                            message: format!("expected a string, found {}", other.type_str()),
                        }),
                    }
                }
            }
            Some(other) => warnings.push(Warning {
                key: "bindings".to_owned(),
                message: format!("expected a table, found {}", other.type_str()),
            }),
        }
        (
            Self {
                bindings,
                extra: root,
                read_only: version > SCHEMA_VERSION,
            },
            warnings,
        )
    }

    /// The file's text.
    #[must_use]
    pub fn to_toml_string(&self) -> String {
        let mut root = self.extra.clone();
        root.insert("schema_version".into(), Value::Integer(SCHEMA_VERSION));
        let bindings: Table = self
            .bindings
            .iter()
            .map(|(id, sequence)| (id.clone(), Value::String(sequence.clone())))
            .collect();
        root.insert("bindings".into(), Value::Table(bindings));
        format!("{HEADER}\n{root}")
    }

    /// The user's sequence for `id`, if changed.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&str> {
        self.bindings.get(id).map(String::as_str)
    }

    /// Sets `id` to `sequence`, or removes the change when `sequence` equals `default`.
    /// Returns whether anything changed; invalid input changes nothing.
    pub fn set(&mut self, id: &str, sequence: &str, default: &str) -> bool {
        let sequence = sequence.trim();
        if !valid_action_id(id) || !valid_sequence(sequence) {
            return false;
        }
        if sequence == default {
            return self.bindings.remove(id).is_some();
        }
        self.bindings.insert(id.to_owned(), sequence.to_owned()) != Some(sequence.to_owned())
    }

    /// Back to the default for `id`. Returns whether it was changed before.
    pub fn reset(&mut self, id: &str) -> bool {
        self.bindings.remove(id).is_some()
    }
}

/// Whether a key sequence would take a key that terminal programs need: a Ctrl or Alt
/// combination without Shift on a letter, or a bare key other than a function key. PLAN §6.4:
/// app shortcuts use Shift or Alt combinations for this reason; the settings page warns about
/// the others.
#[must_use]
pub fn takes_terminal_key(sequence: &str) -> bool {
    let parts: Vec<&str> = sequence.split('+').map(str::trim).collect();
    let Some((key, modifiers)) = parts.split_last().filter(|(key, _)| !key.is_empty()) else {
        return false;
    };
    let has = |name: &str| modifiers.iter().any(|m| m.eq_ignore_ascii_case(name));
    let function_key = key.len() >= 2
        && key.starts_with(['F', 'f'])
        && key[1..].bytes().all(|b| b.is_ascii_digit());
    if modifiers.is_empty() {
        return !function_key;
    }
    let letter = key.len() == 1 && key.bytes().all(|b| b.is_ascii_alphabetic());
    (has("Ctrl") || has("Alt")) && !has("Shift") && !has("Meta") && letter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_unknown_keys_and_skipped_entries() {
        let (bindings, warnings) = Keybindings::from_toml_str(
            "schema_version = 1\nnote = \"kept\"\n[bindings]\n\"tab.close\" = \"Ctrl+Alt+W\"\n\"app.quickConnect\" = \"\"\n\"bad id!\" = \"F1\"\n\"x\" = 3\n",
        );
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert_eq!(bindings.get("tab.close"), Some("Ctrl+Alt+W"));
        assert_eq!(bindings.get("app.quickConnect"), Some(""));
        let text = bindings.to_toml_string();
        assert!(text.contains("note = \"kept\""));
        let (again, warnings) = Keybindings::from_toml_str(&text);
        assert!(warnings.is_empty());
        assert_eq!(again, bindings);
    }

    #[test]
    fn set_to_the_default_removes_the_change() {
        let mut bindings = Keybindings::default();
        assert!(bindings.set("tab.close", "Ctrl+Alt+W", "Ctrl+Shift+W"));
        assert!(!bindings.set("tab.close", "Ctrl+Alt+W", "Ctrl+Shift+W"));
        assert!(bindings.set("tab.close", "Ctrl+Shift+W", "Ctrl+Shift+W"));
        assert!(bindings.bindings.is_empty());
        assert!(bindings.set("tab.close", "", "Ctrl+Shift+W"), "no shortcut");
        assert!(bindings.reset("tab.close"));
        assert!(!bindings.set("bad id", "F2", ""));
        assert!(!bindings.set("a", "Ctrl+\u{1}X", ""));
    }

    #[test]
    fn broken_or_newer_files_are_read_only() {
        assert!(Keybindings::from_toml_str("[bindings").0.read_only);
        assert!(Keybindings::from_toml_str("schema_version = 7").0.read_only);
    }

    #[test]
    fn keys_terminal_programs_need() {
        for taken in [
            "Ctrl+R",
            "Ctrl+A",
            "Alt+B",
            "Ctrl+Alt+K",
            "A",
            "Tab",
            "Escape",
        ] {
            assert!(takes_terminal_key(taken), "{taken}");
        }
        for free in [
            "Ctrl+Shift+R",
            "Alt+Shift+=",
            "F11",
            "Ctrl+F6",
            "Alt+1",
            "Ctrl+,",
            "Ctrl+=",
            "",
        ] {
            assert!(!takes_terminal_key(free), "{free}");
        }
    }
}
