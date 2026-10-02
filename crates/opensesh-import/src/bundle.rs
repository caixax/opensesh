//! OpenSesh bundles (Sprint 16): hosts and groups, terminal profiles, themes, snippets and,
//! optionally, the keychain, in one TOML file (`.opensesh`), to move to another computer or keep
//! as a backup.
//!
//! ```toml
//! format = "opensesh-bundle"
//! version = 1
//!
//! [hosts]          # hosts.toml's groups and hosts (linked ones left out)
//! [snippets]       # snippets.toml
//! [[file]]         # profiles/*.toml and themes/*.toml, as text
//! path = "profiles/dark.toml"
//! text = "..."
//! [keychain]       # identities and keys; their secrets sealed with an export password
//! ```
//!
//! The keychain part is opaque here: `opensesh_vault::transfer` makes and opens it.

use std::collections::HashMap;
use std::path::Path;

use opensesh_core::hosts::{HostsFile, new_id};
use opensesh_core::snippets::SnippetsFile;
use toml::{Table, Value};

use crate::common::{ImportWarning, Imported};

/// The `format` key's value.
pub const FORMAT: &str = "opensesh-bundle";

/// The newest version this OpenSesh writes and reads.
pub const VERSION: i64 = 1;

/// The usual file extension.
pub const EXTENSION: &str = "opensesh";

/// The folders of the settings folder whose files travel in a bundle.
pub const FOLDERS: [&str; 2] = ["profiles", "themes"];

/// Largest bundle read.
pub const MAX_SIZE: u64 = 64 * 1024 * 1024;

/// Why a bundle can't be read.
#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    /// Not TOML.
    #[error("not a bundle: {0}")]
    Toml(String),
    /// TOML, but not a bundle.
    #[error("not an OpenSesh bundle")]
    NotABundle,
    /// A newer OpenSesh wrote it.
    #[error("the bundle was written by a newer OpenSesh (version {0})")]
    Newer(i64),
    /// Its hosts can't be read.
    #[error("the bundle's hosts can't be read: {0}")]
    Hosts(String),
}

/// A bundle's contents.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bundle {
    /// Groups and hosts.
    pub hosts: HostsFile,
    /// Snippets.
    pub snippets: SnippetsFile,
    /// Profile and theme files: (path in the settings folder, text).
    pub files: Vec<(String, String)>,
    /// The keychain part, if it has one.
    pub keychain: Option<Table>,
}

/// The profile and theme files of `config_dir`, as (`profiles/x.toml`, text), sorted.
#[must_use]
pub fn collect_files(config_dir: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for folder in FOLDERS {
        let Ok(entries) = std::fs::read_dir(config_dir.join(folder)) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.ends_with(".toml"))
            .collect();
        names.sort();
        for name in names {
            // Small files the user asked to export.
            if let Ok(text) = std::fs::read_to_string(config_dir.join(folder).join(&name)) {
                files.push((format!("{folder}/{name}"), text));
            }
        }
    }
    files
}

/// Whether `path` is a file a bundle may write: `profiles/<name>.toml` or `themes/<name>.toml`
/// with a plain name (no other folders, no `..`).
#[must_use]
pub fn is_bundle_file(path: &str) -> bool {
    let Some((folder, name)) = path.split_once('/') else {
        return false;
    };
    FOLDERS.contains(&folder)
        && name.len() > ".toml".len()
        && name.ends_with(".toml")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !name.starts_with('.')
}

/// The bundle's text.
///
/// # Errors
///
/// When a part can't be written as TOML (not expected).
pub fn write(bundle: &Bundle) -> Result<String, String> {
    let mut root = Table::new();
    root.insert("format".to_owned(), Value::String(FORMAT.to_owned()));
    root.insert("version".to_owned(), Value::Integer(VERSION));
    let mut hosts = bundle.hosts.clone();
    hosts.sources.clear();
    hosts.hosts.retain(|host| !host.is_linked());
    root.insert(
        "hosts".to_owned(),
        Value::Table(parse(&hosts.to_toml_string()?)?),
    );
    if !bundle.snippets.snippets.is_empty() {
        root.insert(
            "snippets".to_owned(),
            Value::Table(parse(&bundle.snippets.to_toml_string())?),
        );
    }
    if !bundle.files.is_empty() {
        let files = bundle
            .files
            .iter()
            .map(|(path, text)| {
                let mut file = Table::new();
                file.insert("path".to_owned(), Value::String(path.clone()));
                file.insert("text".to_owned(), Value::String(text.clone()));
                Value::Table(file)
            })
            .collect();
        root.insert("file".to_owned(), Value::Array(files));
    }
    if let Some(keychain) = &bundle.keychain {
        root.insert("keychain".to_owned(), Value::Table(keychain.clone()));
    }
    let body = toml::to_string(&root).map_err(|error| error.to_string())?;
    Ok(format!(
        "# An OpenSesh bundle: hosts, profiles, themes and snippets. Import it from the Hosts view.\n{body}"
    ))
}

fn parse(text: &str) -> Result<Table, String> {
    text.parse::<Table>().map_err(|error| error.to_string())
}

/// Reads a bundle's text; the warnings are about parts that couldn't be used.
///
/// # Errors
///
/// When the text isn't a bundle, or a newer one.
pub fn read(text: &str) -> Result<(Bundle, Vec<String>), BundleError> {
    let mut root = parse(text).map_err(BundleError::Toml)?;
    if root.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(BundleError::NotABundle);
    }
    match root.get("version").and_then(Value::as_integer) {
        Some(version) if version > VERSION => return Err(BundleError::Newer(version)),
        _ => {}
    }
    let mut warnings = Vec::new();
    let mut bundle = Bundle::default();
    if let Some(Value::Table(hosts)) = root.remove("hosts") {
        let text =
            toml::to_string(&hosts).map_err(|error| BundleError::Hosts(error.to_string()))?;
        let (file, problems) = HostsFile::from_toml_str(&text).map_err(BundleError::Hosts)?;
        warnings.extend(problems.iter().map(|problem| format!("hosts: {problem}")));
        bundle.hosts = file;
    }
    if let Some(Value::Table(snippets)) = root.remove("snippets") {
        match toml::to_string(&snippets) {
            Ok(text) => {
                let (file, problems) = SnippetsFile::from_toml_str(&text);
                warnings.extend(
                    problems
                        .iter()
                        .map(|problem| format!("snippets: {problem}")),
                );
                bundle.snippets = file;
            }
            Err(error) => warnings.push(format!("snippets: {error}")),
        }
    }
    if let Some(Value::Array(files)) = root.remove("file") {
        for file in files {
            let path = file.get("path").and_then(Value::as_str).unwrap_or_default();
            let text = file.get("text").and_then(Value::as_str);
            match text {
                Some(text) if is_bundle_file(path) => {
                    bundle.files.push((path.to_owned(), text.to_owned()));
                }
                _ => warnings.push(format!("the file {path:?} was left out")),
            }
        }
    }
    if let Some(Value::Table(keychain)) = root.remove("keychain") {
        bundle.keychain = Some(keychain);
    }
    Ok((bundle, warnings))
}

/// Reads a bundle file.
///
/// # Errors
///
/// When it can't be read, is too large, or isn't a bundle.
pub fn load(path: &Path) -> Result<(Bundle, Vec<String>), BundleError> {
    let size = std::fs::metadata(path)
        .map_err(|error| BundleError::Toml(error.to_string()))?
        .len();
    if size > MAX_SIZE {
        return Err(BundleError::Toml("the file is too large".to_owned()));
    }
    let bytes = std::fs::read(path).map_err(|error| BundleError::Toml(error.to_string()))?;
    read(&crate::common::decode_text(&bytes))
}

impl Bundle {
    /// The hosts and groups with new ids (so they never clash with the ones here), their
    /// references (groups, jump hosts by id) following. `identities` maps the bundle's identity
    /// ids to the ones they got here (after importing its keychain); other identities are kept
    /// when `known` says they exist here, and dropped otherwise.
    #[must_use]
    pub fn to_imported(
        &self,
        identities: &HashMap<String, String>,
        known: &dyn Fn(&str) -> bool,
        origin: &Path,
    ) -> Imported {
        let mut imported = Imported::default();
        let groups: HashMap<&str, String> = self
            .hosts
            .groups
            .iter()
            .map(|group| (group.id.as_str(), new_id()))
            .collect();
        let hosts: HashMap<&str, String> = self
            .hosts
            .hosts
            .iter()
            .map(|host| (host.id.as_str(), new_id()))
            .collect();
        let mut dropped = 0usize;
        let mut identity = |id: &mut Option<String>| {
            if let Some(old) = id.take() {
                if old.is_empty() {
                    // "None, even if the group has one."
                    *id = Some(old);
                } else if let Some(new) = identities.get(&old) {
                    *id = Some(new.clone());
                } else if known(&old) {
                    *id = Some(old);
                } else {
                    dropped += 1;
                }
            }
        };
        let jumps = |list: &mut Option<Vec<String>>| {
            if let Some(list) = list {
                for jump in list.iter_mut() {
                    if let Some(new) = hosts.get(jump.as_str()) {
                        jump.clone_from(new);
                    }
                }
            }
        };
        for group in &self.hosts.groups {
            let mut group = group.clone();
            group.id = groups
                .get(group.id.as_str())
                .cloned()
                .unwrap_or_else(new_id);
            group.parent = group
                .parent
                .as_deref()
                .and_then(|parent| groups.get(parent))
                .cloned();
            identity(&mut group.defaults.identity);
            jumps(&mut group.defaults.jump);
            imported.groups.push(group);
        }
        for host in &self.hosts.hosts {
            let mut host = host.clone();
            host.id = hosts.get(host.id.as_str()).cloned().unwrap_or_else(new_id);
            host.group = host
                .group
                .as_deref()
                .and_then(|group| groups.get(group))
                .cloned();
            identity(&mut host.identity);
            jumps(&mut host.jump);
            imported.hosts.push(host);
        }
        if dropped > 0 {
            imported.warnings.push(ImportWarning::new(
                origin,
                format!(
                    "{dropped} keychain identities aren't on this computer; import the bundle with its keychain to bring them"
                ),
            ));
        }
        // Parents before children, as `Imported` promises.
        imported.groups = parents_first(imported.groups);
        imported
    }
}

fn parents_first(mut groups: Vec<opensesh_core::hosts::Group>) -> Vec<opensesh_core::hosts::Group> {
    let mut ordered = Vec::with_capacity(groups.len());
    let mut placed: std::collections::HashSet<String> = std::collections::HashSet::new();
    while !groups.is_empty() {
        let before = groups.len();
        groups.retain(|group| {
            let ready = group
                .parent
                .as_ref()
                .is_none_or(|parent| placed.contains(parent));
            if ready {
                placed.insert(group.id.clone());
                ordered.push(group.clone());
            }
            !ready
        });
        if groups.len() == before {
            // A cycle (never written by OpenSesh): cut it.
            for mut group in groups.drain(..) {
                group.parent = None;
                ordered.push(group);
            }
        }
    }
    ordered
}

#[cfg(test)]
mod tests {
    use opensesh_core::hosts::{Group, Host};
    use opensesh_core::snippets::Snippet;

    use super::*;

    fn sample() -> Bundle {
        let mut hosts = HostsFile::default();
        hosts.groups.push(Group {
            id: "G1".to_owned(),
            name: "Prod".to_owned(),
            ..Group::default()
        });
        hosts.groups.push(Group {
            id: "G2".to_owned(),
            name: "Web".to_owned(),
            parent: Some("G1".to_owned()),
            ..Group::default()
        });
        hosts.hosts.push(Host {
            id: "H1".to_owned(),
            name: "bastion".to_owned(),
            address: "bastion.example".to_owned(),
            group: Some("G1".to_owned()),
            identity: Some("I1".to_owned()),
            ..Host::default()
        });
        hosts.hosts.push(Host {
            id: "H2".to_owned(),
            name: "web".to_owned(),
            address: "web.internal".to_owned(),
            group: Some("G2".to_owned()),
            jump: Some(vec!["H1".to_owned(), "me@other:2222".to_owned()]),
            identity: Some("I2".to_owned()),
            ..Host::default()
        });
        hosts.hosts.push(Host {
            id: "ssh_config:linked".to_owned(),
            name: "linked".to_owned(),
            address: "linked.example".to_owned(),
            ..Host::default()
        });
        let mut snippets = SnippetsFile::default();
        snippets.snippets.push(Snippet {
            id: "S1".to_owned(),
            name: "uptime".to_owned(),
            text: "uptime".to_owned(),
            ..Snippet::default()
        });
        let mut keychain = Table::new();
        keychain.insert("sealed".to_owned(), Value::String("AAAA".to_owned()));
        Bundle {
            hosts,
            snippets,
            files: vec![(
                "profiles/dark.toml".to_owned(),
                "name = \"Dark\"\n".to_owned(),
            )],
            keychain: Some(keychain),
        }
    }

    #[test]
    fn written_and_read_back() {
        let bundle = sample();
        let text = write(&bundle).unwrap();
        assert!(text.starts_with("# An OpenSesh bundle"));
        let (back, warnings) = read(&text).unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        // The linked host stays behind.
        assert_eq!(back.hosts.hosts.len(), 2);
        assert_eq!(back.hosts.groups, bundle.hosts.groups);
        assert_eq!(back.hosts.hosts[1].jump, bundle.hosts.hosts[1].jump);
        assert_eq!(back.snippets.snippets[0].text, "uptime");
        assert_eq!(back.files, bundle.files);
        assert_eq!(back.keychain, bundle.keychain);
    }

    #[test]
    fn not_bundles() {
        assert!(matches!(read("x = 1"), Err(BundleError::NotABundle)));
        assert!(matches!(read("not toml ="), Err(BundleError::Toml(_))));
        assert!(matches!(
            read("format = \"opensesh-bundle\"\nversion = 9"),
            Err(BundleError::Newer(9))
        ));
        let (bundle, warnings) = read(
            "format = \"opensesh-bundle\"\nversion = 1\n[[file]]\npath = \"../evil.toml\"\ntext = \"\"\n",
        )
        .unwrap();
        assert!(bundle.files.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(is_bundle_file("themes/solarized-dark.toml"));
        for bad in [
            "profiles/../x.toml",
            "profiles/a/b.toml",
            "other/x.toml",
            "profiles/.toml",
            "profiles/x.txt",
        ] {
            assert!(!is_bundle_file(bad), "{bad}");
        }
    }

    #[test]
    fn new_ids_and_references() {
        let bundle = sample();
        let identities = HashMap::from([("I1".to_owned(), "NEW-I1".to_owned())]);
        let imported = bundle.to_imported(&identities, &|id| id == "I3", Path::new("b.opensesh"));
        assert_eq!(imported.groups.len(), 2);
        let prod = &imported.groups[0];
        let web_group = &imported.groups[1];
        assert_ne!(prod.id, "G1");
        assert_eq!(web_group.parent.as_ref(), Some(&prod.id));
        let bastion = &imported.hosts[0];
        let web = &imported.hosts[1];
        assert_ne!(bastion.id, "H1");
        assert_eq!(bastion.group.as_ref(), Some(&prod.id));
        assert_eq!(bastion.identity.as_deref(), Some("NEW-I1"));
        assert_eq!(web.identity, None);
        assert_eq!(
            web.jump,
            Some(vec![bastion.id.clone(), "me@other:2222".to_owned()])
        );
        assert_eq!(imported.warnings.len(), 1);
        // The linked one was in the file but not in the bundle's text.
        assert_eq!(imported.hosts.len(), 3);
    }
}
