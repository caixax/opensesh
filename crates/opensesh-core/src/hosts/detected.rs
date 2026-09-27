//! The operating system each saved host runs, as found by the SSH client (Sprint 7), kept in
//! `detected-os.toml` in the data directory: it is state, not a setting. A host whose icon is
//! `auto` shows it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// File name in the data directory.
pub const DETECTED_FILE: &str = "detected-os.toml";

#[derive(Debug, Default, Serialize, Deserialize)]
struct DetectedFile {
    #[serde(default)]
    hosts: BTreeMap<String, String>,
}

/// Host id to icon name (`os-debian`, `server`...).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DetectedOs {
    icons: BTreeMap<String, String>,
}

/// Whether `icon` can be an icon name from the file: `os-` names and `server`, short and plain.
fn is_icon_name(icon: &str) -> bool {
    (icon == "server" || icon.starts_with("os-"))
        && icon.len() <= 40
        && icon
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl DetectedOs {
    /// Parses the file's text; anything unreadable gives an empty map, and odd names are
    /// dropped.
    #[must_use]
    pub fn from_toml_str(text: &str) -> Self {
        let file: DetectedFile = toml::from_str(text).unwrap_or_default();
        Self {
            icons: file
                .hosts
                .into_iter()
                .filter(|(id, icon)| !id.is_empty() && is_icon_name(icon))
                .collect(),
        }
    }

    /// Reads `path`; a missing or broken file is an empty map.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|text| Self::from_toml_str(&text))
            .unwrap_or_default()
    }

    /// The file's text.
    #[must_use]
    pub fn to_toml_string(&self) -> String {
        let file = DetectedFile {
            hosts: self.icons.clone(),
        };
        let body = toml::to_string(&file).unwrap_or_default();
        format!(
            "# The operating system of each host, as OpenSesh found it (host id = icon).\n\n{body}"
        )
    }

    /// The icon found for host `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&str> {
        self.icons.get(id).map(String::as_str)
    }

    /// Records `icon` for host `id`. Returns whether anything changed (an odd name changes
    /// nothing).
    pub fn set(&mut self, id: &str, icon: &str) -> bool {
        if id.is_empty() || !is_icon_name(icon) || self.get(id) == Some(icon) {
            return false;
        }
        self.icons.insert(id.to_owned(), icon.to_owned());
        true
    }

    /// Forgets the hosts `keep` doesn't accept. Returns whether anything changed.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) -> bool {
        let before = self.icons.len();
        self.icons.retain(|id, _| keep(id));
        self.icons.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut detected = DetectedOs::default();
        assert!(detected.set("H1", "os-debian"));
        assert!(!detected.set("H1", "os-debian"));
        assert!(!detected.set("H2", "../../etc/passwd"));
        assert!(!detected.set("", "server"));
        assert!(detected.set("H2", "server"));
        let text = detected.to_toml_string();
        assert_eq!(DetectedOs::from_toml_str(&text), detected);
        assert_eq!(detected.get("H1"), Some("os-debian"));
        assert!(detected.retain(|id| id == "H2"));
        assert_eq!(detected.get("H1"), None);
        assert!(!detected.retain(|_| true));
    }

    #[test]
    fn odd_files() {
        assert_eq!(
            DetectedOs::from_toml_str("hosts = 3"),
            DetectedOs::default()
        );
        let parsed = DetectedOs::from_toml_str("[hosts]\nH1 = \"os-arch\"\nH2 = \"Os Arch\"\n");
        assert_eq!(parsed.get("H1"), Some("os-arch"));
        assert_eq!(parsed.get("H2"), None);
    }
}
