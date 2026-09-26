//! Recent connections (the Hosts view's Recent list and the quick-connect suggestions), kept in
//! `recent.toml` in the data directory: it is state, not a setting.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// File name in the data directory.
pub const RECENT_FILE: &str = "recent.toml";

/// Entries kept.
pub const MAX_RECENT: usize = 30;

/// One connection: a saved host (by id) or a quick-connect target (its text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentEntry {
    /// Saved host id.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host: String,
    /// Quick-connect text, for a target that isn't saved.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target: String,
    /// When, in seconds since the Unix epoch.
    #[serde(default)]
    pub at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentFile {
    #[serde(default)]
    recent: Vec<RecentEntry>,
}

/// The recent connections, the newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecentList {
    entries: Vec<RecentEntry>,
}

impl RecentList {
    /// Parses the file's text; anything unreadable gives an empty list.
    #[must_use]
    pub fn from_toml_str(text: &str) -> Self {
        let file: RecentFile = toml::from_str(text).unwrap_or_default();
        let mut list = Self::default();
        for entry in file.recent {
            if (!entry.host.is_empty() || !entry.target.is_empty()) && !list.contains(&entry) {
                list.entries.push(entry);
            }
        }
        list.entries.sort_by(|a, b| b.at.cmp(&a.at));
        list.entries.truncate(MAX_RECENT);
        list
    }

    /// Reads `path`; a missing or broken file is an empty list.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|text| Self::from_toml_str(&text))
            .unwrap_or_default()
    }

    /// The file's text.
    #[must_use]
    pub fn to_toml_string(&self) -> String {
        let file = RecentFile {
            recent: self.entries.clone(),
        };
        let body = toml::to_string(&file).unwrap_or_default();
        format!("# Recent connections (OpenSesh keeps the last {MAX_RECENT}).\n\n{body}")
    }

    fn contains(&self, entry: &RecentEntry) -> bool {
        self.entries.iter().any(|seen| same(seen, entry))
    }

    /// The entries, the newest first.
    #[must_use]
    pub fn entries(&self) -> &[RecentEntry] {
        &self.entries
    }

    /// Records a connection to saved host `id` at `now`.
    pub fn touch_host(&mut self, id: &str, now: i64) {
        self.touch(RecentEntry {
            host: id.to_owned(),
            target: String::new(),
            at: now,
        });
    }

    /// Records a connection to a quick-connect target (its canonical text) at `now`.
    pub fn touch_target(&mut self, target: &str, now: i64) {
        self.touch(RecentEntry {
            host: String::new(),
            target: target.trim().to_owned(),
            at: now,
        });
    }

    fn touch(&mut self, entry: RecentEntry) {
        self.entries.retain(|seen| !same(seen, &entry));
        self.entries.insert(0, entry);
        self.entries.truncate(MAX_RECENT);
    }

    /// Forgets a saved host (it was deleted).
    pub fn remove_host(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.host != id);
        self.entries.len() != before
    }

    /// When saved host `id` was last connected to.
    #[must_use]
    pub fn last_used(&self, id: &str) -> Option<i64> {
        self.entries
            .iter()
            .find(|entry| entry.host == id)
            .map(|entry| entry.at)
    }
}

fn same(a: &RecentEntry, b: &RecentEntry) -> bool {
    if a.host.is_empty() {
        !a.target.is_empty() && a.target == b.target
    } else {
        a.host == b.host
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_connections_move_to_the_top_and_survive_a_file() {
        let mut list = RecentList::default();
        list.touch_host("A", 10);
        list.touch_target("deploy@web:22", 20);
        list.touch_host("B", 30);
        list.touch_host("A", 40);
        let ids: Vec<(&str, &str)> = list
            .entries()
            .iter()
            .map(|entry| (entry.host.as_str(), entry.target.as_str()))
            .collect();
        assert_eq!(ids, vec![("A", ""), ("B", ""), ("", "deploy@web:22")]);
        assert_eq!(list.last_used("A"), Some(40));
        let back = RecentList::from_toml_str(&list.to_toml_string());
        assert_eq!(back, list);
        assert!(list.remove_host("B"));
        assert_eq!(list.entries().len(), 2);
        for n in 0..50 {
            list.touch_host(&n.to_string(), 100 + n);
        }
        assert_eq!(list.entries().len(), MAX_RECENT);
        assert_eq!(
            RecentList::from_toml_str("recent = 3"),
            RecentList::default()
        );
    }
}
