//! Terminal profiles in `profiles/*.toml` (PLAN §4.2, §6.2) and the inheritance chain.
//!
//! A profile is a named layer of [`TerminalOverrides`]. `profiles/default.toml` is the **global**
//! level that every terminal starts from (it always exists, even without the file); any other
//! profile only sets what differs from it. The chain is global, then group, then host, then tab:
//! each of those levels may pick a profile and add its own overrides ([`Level`]).
//!
//! The file name (without `.toml`) is the profile's id; the display name is inside:
//!
//! ```toml
//! schema_version = 1
//! name = "Production"
//! [terminal]
//! theme_dark = "gruvbox-dark"
//! bell = "notification"
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml::{Table, Value};

use super::settings::{TerminalOverrides, TerminalSettings, valid_id};
use crate::config::Warning;

/// Id of the global profile.
pub const DEFAULT_PROFILE: &str = "default";

/// Directory of the profiles inside the config directory.
pub const PROFILES_DIR: &str = "profiles";

/// Current layout of a profile file.
pub const SCHEMA_VERSION: i64 = 1;

const HEADER: &str = "# OpenSesh terminal profile. Options left out are inherited from the default\n\
                      # profile (default.toml). OpenSesh rewrites this file when the profile is\n\
                      # changed in the app: unknown options are kept, comments are not.\n";

/// One profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    /// File name without `.toml` ([`valid_id`]).
    pub id: String,
    /// Display name.
    pub name: String,
    /// The options it sets.
    pub terminal: TerminalOverrides,
    /// Unknown keys and tables, written back unchanged.
    pub extra: Table,
    /// The file comes from a newer OpenSesh: never overwrite it.
    pub read_only: bool,
}

impl Profile {
    /// An empty profile.
    #[must_use]
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            terminal: TerminalOverrides::default(),
            extra: Table::new(),
            read_only: false,
        }
    }

    /// The built-in global profile, before any file changes it.
    #[must_use]
    pub fn default_profile() -> Self {
        Self::new(DEFAULT_PROFILE, "Default")
    }

    /// Whether this is the global profile.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.id == DEFAULT_PROFILE
    }

    /// Parses a profile file's text. Returns the profile and what was ignored.
    ///
    /// # Errors
    ///
    /// The TOML parser's message if the text isn't valid TOML.
    pub fn from_toml_str(id: &str, text: &str) -> Result<(Self, Vec<Warning>), String> {
        let mut root: Table = text
            .parse()
            .map_err(|error: toml::de::Error| error.to_string())?;
        let mut warnings = Vec::new();
        let mut warn = |key: &str, message: String| {
            warnings.push(Warning {
                key: key.to_owned(),
                message,
            });
        };
        let version = match root.remove("schema_version") {
            None => SCHEMA_VERSION,
            Some(Value::Integer(version)) => version,
            Some(other) => {
                warn(
                    "schema_version",
                    format!("expected an integer, found {}", other.type_str()),
                );
                SCHEMA_VERSION
            }
        };
        let read_only = version > SCHEMA_VERSION;
        if read_only {
            warn(
                "schema_version",
                format!(
                    "version {version} is newer than this OpenSesh supports ({SCHEMA_VERSION}); \
                     the profile is read-only until you upgrade"
                ),
            );
        }
        let name = match root.remove("name") {
            Some(Value::String(name)) if !name.trim().is_empty() => name.trim().to_owned(),
            Some(_) => {
                warn("name", "expected a non-empty string".to_owned());
                id.to_owned()
            }
            None => id.to_owned(),
        };
        let terminal = match root.remove("terminal") {
            None => TerminalOverrides::default(),
            Some(Value::Table(table)) => {
                let (terminal, found, unknown) = TerminalOverrides::from_table(&table, "terminal");
                warnings.extend(found);
                if !unknown.is_empty() {
                    root.insert("terminal".to_owned(), Value::Table(unknown));
                }
                terminal
            }
            Some(other) => {
                warnings.push(Warning {
                    key: "terminal".to_owned(),
                    message: format!("expected a table, found {}", other.type_str()),
                });
                TerminalOverrides::default()
            }
        };
        Ok((
            Self {
                id: id.to_owned(),
                name,
                terminal,
                extra: root,
                read_only,
            },
            warnings,
        ))
    }

    /// The file's text (deterministic order, header comment included).
    #[must_use]
    pub fn to_toml_string(&self) -> String {
        let mut terminal = self.terminal.to_table();
        let mut root = Table::new();
        for (key, value) in &self.extra {
            match (key.as_str(), value) {
                ("terminal", Value::Table(unknown)) => {
                    for (key, value) in unknown {
                        terminal.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }
                ("schema_version" | "name" | "terminal", _) => {}
                _ => {
                    root.insert(key.clone(), value.clone());
                }
            }
        }
        root.insert("schema_version".into(), Value::Integer(SCHEMA_VERSION));
        root.insert("name".into(), Value::String(self.name.clone()));
        root.insert("terminal".into(), Value::Table(terminal));
        format!("{HEADER}\n{root}")
    }
}

/// A problem with one profile file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProblem {
    /// The file.
    pub path: PathBuf,
    /// What is wrong (a warning about one key, or why the file was skipped).
    pub message: String,
}

/// Every profile, by id. The global profile is always present.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileSet {
    profiles: BTreeMap<String, Profile>,
}

impl Default for ProfileSet {
    fn default() -> Self {
        let mut profiles = BTreeMap::new();
        profiles.insert(DEFAULT_PROFILE.to_owned(), Profile::default_profile());
        Self { profiles }
    }
}

/// One level of the chain below the global profile: a group, a host or a tab.
#[derive(Debug, Clone, Copy, Default)]
pub struct Level<'a> {
    /// The profile this level picks, by id (an unknown id is skipped).
    pub profile: Option<&'a str>,
    /// Its own overrides, applied after its profile.
    pub overrides: Option<&'a TerminalOverrides>,
}

impl ProfileSet {
    /// Reads every `*.toml` in `dir`. A missing directory gives just the global profile; a file
    /// that can't be read or parsed, or whose name isn't a valid id, is skipped and reported.
    #[must_use]
    pub fn load_dir(dir: &Path) -> (Self, Vec<FileProblem>) {
        let mut set = Self::default();
        let mut problems = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return (set, problems);
            }
            Err(error) => {
                problems.push(FileProblem {
                    path: dir.to_path_buf(),
                    message: format!("could not list the profiles: {error}"),
                });
                return (set, problems);
            }
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
            .collect();
        paths.sort();
        for path in paths {
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if !valid_id(id) {
                problems.push(FileProblem {
                    path: path.clone(),
                    message:
                        "skipped: a profile file name uses lowercase letters, digits, - _ and ."
                            .to_owned(),
                });
                continue;
            }
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) => {
                    problems.push(FileProblem {
                        path: path.clone(),
                        message: format!("could not read: {error}"),
                    });
                    continue;
                }
            };
            match Profile::from_toml_str(id, &text) {
                Ok((profile, warnings)) => {
                    problems.extend(warnings.into_iter().map(|warning| FileProblem {
                        path: path.clone(),
                        message: warning.to_string(),
                    }));
                    set.insert(profile);
                }
                Err(message) => problems.push(FileProblem {
                    path: path.clone(),
                    message: format!("skipped: {message}"),
                }),
            }
        }
        (set, problems)
    }

    /// The profile with `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.profiles.get(id)
    }

    /// The global profile.
    #[must_use]
    pub fn default_profile(&self) -> &Profile {
        // The map always holds it: `Default` inserts it and `remove` refuses to take it out.
        self.profiles
            .get(DEFAULT_PROFILE)
            .unwrap_or_else(|| default_profile_ref())
    }

    /// Adds or replaces a profile.
    pub fn insert(&mut self, profile: Profile) {
        self.profiles.insert(profile.id.clone(), profile);
    }

    /// Removes a profile; the global one can't be removed.
    pub fn remove(&mut self, id: &str) -> Option<Profile> {
        if id == DEFAULT_PROFILE {
            return None;
        }
        self.profiles.remove(id)
    }

    /// Every profile: the global one first, then the others by name.
    #[must_use]
    pub fn list(&self) -> Vec<&Profile> {
        let mut list: Vec<&Profile> = self.profiles.values().collect();
        list.sort_by(|a, b| {
            b.is_default()
                .cmp(&a.is_default())
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        });
        list
    }

    /// A new id for a profile called `name` that no profile uses yet.
    #[must_use]
    pub fn unique_id(&self, name: &str) -> String {
        let base = slug(name);
        let base = if base.is_empty() {
            "profile".to_owned()
        } else {
            base
        };
        let mut id = base.clone();
        let mut n = 2;
        while self.profiles.contains_key(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }

    /// The layers for a terminal: the global profile, then each level's profile and overrides.
    /// A level that picks the global profile adds nothing for it (it is already first).
    #[must_use]
    pub fn layers<'a>(&'a self, levels: &[Level<'a>]) -> Vec<&'a TerminalOverrides> {
        let mut layers = vec![&self.default_profile().terminal];
        for level in levels {
            if let Some(profile) = level
                .profile
                .filter(|id| *id != DEFAULT_PROFILE)
                .and_then(|id| self.profiles.get(id))
            {
                layers.push(&profile.terminal);
            }
            if let Some(overrides) = level.overrides {
                layers.push(overrides);
            }
        }
        layers
    }

    /// The settings of a terminal whose group, host and tab are `levels`, in that order.
    #[must_use]
    pub fn resolve(&self, levels: &[Level<'_>]) -> TerminalSettings {
        super::settings::resolve(self.layers(levels))
    }

    /// The settings of profile `id` on its own (the global profile, then it).
    #[must_use]
    pub fn resolve_profile(&self, id: &str) -> TerminalSettings {
        self.resolve(&[Level {
            profile: Some(id),
            overrides: None,
        }])
    }
}

fn default_profile_ref() -> &'static Profile {
    static DEFAULT: std::sync::OnceLock<Profile> = std::sync::OnceLock::new();
    DEFAULT.get_or_init(Profile::default_profile)
}

/// A profile id made from a display name: lowercase ASCII letters and digits, other runs
/// turned into `-`, at most 48 characters.
#[must_use]
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 48 {
            break;
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// Path of profile `id` inside `config_dir`.
#[must_use]
pub fn profile_path(config_dir: &Path, id: &str) -> PathBuf {
    config_dir.join(PROFILES_DIR).join(format!("{id}.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::settings::{BellStyle, CursorStyle};

    fn profile(id: &str, text: &str) -> Profile {
        let (profile, warnings) = Profile::from_toml_str(id, text).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        profile
    }

    #[test]
    fn a_profile_file_round_trips_with_unknown_keys() {
        let (profile, warnings) = Profile::from_toml_str(
            "work",
            r##"
            schema_version = 1
            name = "Work"
            color = "#FF0000"
            [terminal]
            font_size = 13
            bell = "sound"
            sparkle = true
            "##,
        )
        .unwrap();
        assert_eq!(profile.name, "Work");
        assert_eq!(profile.terminal.font_size, Some(13.0));
        assert_eq!(profile.terminal.bell, Some(BellStyle::Sound));
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].key.contains("sparkle"));

        let text = profile.to_toml_string();
        assert!(text.starts_with("# OpenSesh terminal profile."));
        let document: Table = text.parse().unwrap();
        assert_eq!(document["terminal"]["sparkle"].as_bool(), Some(true));
        assert_eq!(document["color"].as_str(), Some("#FF0000"));
        let (again, _) = Profile::from_toml_str("work", &text).unwrap();
        assert_eq!(again, profile);
        assert_eq!(again.to_toml_string(), text);
    }

    #[test]
    fn newer_files_are_read_only_and_bad_names_fall_back_to_the_id() {
        let (profile, warnings) =
            Profile::from_toml_str("x", "schema_version = 9\nname = \"\"\n").unwrap();
        assert!(profile.read_only);
        assert_eq!(profile.name, "x");
        assert_eq!(warnings.len(), 2);
        assert!(Profile::from_toml_str("x", "[terminal").is_err());
    }

    #[test]
    fn the_chain_is_global_group_host_tab() {
        let mut set = ProfileSet::default();
        set.insert(profile(
            DEFAULT_PROFILE,
            "name = \"Default\"\n[terminal]\nfont_size = 12\ncursor_shape = \"beam\"\n",
        ));
        set.insert(profile(
            "prod",
            "name = \"Production\"\n[terminal]\ntheme_dark = \"gruvbox-dark\"\nbell = \"notification\"\nfont_size = 14\n",
        ));
        set.insert(profile(
            "big",
            "name = \"Big\"\n[terminal]\nfont_size = 20\n",
        ));
        let host_overrides = TerminalOverrides {
            font_size: Some(13.0),
            ..TerminalOverrides::default()
        };
        let tab_overrides = TerminalOverrides {
            cursor_shape: Some(CursorStyle::Underline),
            ..TerminalOverrides::default()
        };

        let group = Level {
            profile: Some("prod"),
            overrides: None,
        };
        let host = Level {
            profile: None,
            overrides: Some(&host_overrides),
        };
        let tab = Level {
            profile: None,
            overrides: Some(&tab_overrides),
        };
        let settings = set.resolve(&[group, host, tab]);
        assert_eq!(settings.theme_dark, "gruvbox-dark", "group profile");
        assert_eq!(settings.bell, BellStyle::Notification, "group profile");
        assert_eq!(settings.font_size, 13.0, "the host overrides the group");
        assert_eq!(
            settings.cursor_shape,
            CursorStyle::Underline,
            "the tab wins"
        );

        // A host that picks its own profile: applied after the group's.
        let host_with_profile = Level {
            profile: Some("big"),
            overrides: None,
        };
        let settings = set.resolve(&[group, host_with_profile]);
        assert_eq!(settings.font_size, 20.0);
        assert_eq!(settings.theme_dark, "gruvbox-dark");
        assert_eq!(
            settings.cursor_shape,
            CursorStyle::Beam,
            "from the global profile"
        );

        // Unknown profiles are skipped; the global profile is never applied twice.
        let settings = set.resolve(&[
            Level {
                profile: Some("missing"),
                overrides: None,
            },
            Level {
                profile: Some(DEFAULT_PROFILE),
                overrides: None,
            },
        ]);
        assert_eq!(settings.font_size, 12.0);
        assert_eq!(
            set.layers(&[Level {
                profile: Some(DEFAULT_PROFILE),
                overrides: None
            }])
            .len(),
            1
        );
        assert_eq!(set.resolve_profile("big").cursor_shape, CursorStyle::Beam);
    }

    #[test]
    fn the_global_profile_always_exists() {
        let mut set = ProfileSet::default();
        assert!(set.remove(DEFAULT_PROFILE).is_none());
        assert_eq!(set.default_profile().id, DEFAULT_PROFILE);
        set.insert(Profile::new("zeta", "zeta"));
        set.insert(Profile::new("alpha", "Alpha"));
        let names: Vec<&str> = set.list().iter().map(|p| p.id.as_str()).collect();
        assert_eq!(names, ["default", "alpha", "zeta"]);
        assert_eq!(set.unique_id("Alpha"), "alpha-2");
        assert_eq!(set.unique_id("Mi perfil (SSH)!"), "mi-perfil-ssh");
        assert_eq!(set.unique_id("***"), "profile");
    }

    #[test]
    fn a_directory_loads_and_reports_bad_files() {
        let dir = tempfile::tempdir().unwrap();
        let (set, problems) = ProfileSet::load_dir(&dir.path().join("missing"));
        assert_eq!(set, ProfileSet::default());
        assert!(problems.is_empty());

        std::fs::write(
            dir.path().join("default.toml"),
            "name = \"Default\"\n[terminal]\nfont_size = 15\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("ops.toml"), "name = \"Ops\"\n").unwrap();
        std::fs::write(dir.path().join("Bad Name.toml"), "name = \"x\"\n").unwrap();
        std::fs::write(dir.path().join("broken.toml"), "name = ").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();
        let (set, problems) = ProfileSet::load_dir(dir.path());
        assert_eq!(set.list().len(), 2);
        assert_eq!(set.resolve_profile("ops").font_size, 15.0);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert_eq!(
            profile_path(dir.path(), "ops"),
            dir.path().join("profiles").join("ops.toml")
        );
    }
}
