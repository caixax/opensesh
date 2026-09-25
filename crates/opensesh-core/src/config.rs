//! Application settings stored in `config.toml` (PLAN §4.2, §6.1).
//!
//! Parsing is **lenient per field**: an invalid value falls back to its default and produces a
//! [`Warning`] that names the key, so one typo never discards the whole file.
//! Serialization is deterministic (sections and keys in alphabetical order, one value per line)
//! so the file diffs and merges well in Git or Syncthing folders.
//!
//! Sections and keys this version doesn't know (written by a newer OpenSesh sharing the folder,
//! for example) produce a warning too, but are kept in [`Config::extra`] and written back
//! unchanged. Comments and formatting are not kept: the file is regenerated on every save.
//!
//! A file whose `schema_version` is newer than [`SCHEMA_VERSION`] was written by a newer
//! OpenSesh: it is read as far as possible but must not be overwritten ([`Loaded::read_only`]).

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use toml::{Table, Value};

use crate::fsutil::{self, WriteOutcome};
use crate::theme::{Density, Rgba, ThemeMode, UI_SCALE_RANGE, UnknownValue};

/// Name of the settings file inside the config directory.
pub const CONFIG_FILE: &str = "config.toml";

/// Current layout of `config.toml`.
pub const SCHEMA_VERSION: i64 = 1;

/// Header written at the top of the file.
const HEADER: &str = "# OpenSesh settings. You can edit this file while OpenSesh is running:\n\
                      # changes are applied automatically. Invalid values fall back to defaults.\n\
                      # OpenSesh rewrites this file when a setting changes in the app: unknown\n\
                      # settings are kept, but comments and formatting are not.\n";

/// Declares a string-backed settings enum with its TOML/QML identifiers.
macro_rules! choice {
    (
        $(#[$meta:meta])*
        $name:ident { $($(#[$vmeta:meta])* $variant:ident => $text:literal),+ $(,)? }
        default $default:ident
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            /// All values, in UI order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// Stable identifier used in `config.toml` and QML.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::$default
            }
        }

        impl FromStr for $name {
            type Err = UnknownValue;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|value| value.as_str() == text)
                    .ok_or_else(|| UnknownValue(text.to_owned()))
            }
        }
    };
}

choice! {
    /// What happens when the last tab is closed (§6.1).
    LastTabAction {
        /// Keep the window open with the start page.
        KeepWindow => "keep_window",
        /// Quit the application.
        Quit => "quit",
    }
    default KeepWindow
}

choice! {
    /// Where the navigation rail goes (§5.3).
    RailPosition {
        /// Left edge.
        Left => "left",
        /// Right edge.
        Right => "right",
        /// Not shown (views stay reachable from the command palette).
        Hidden => "hidden",
    }
    default Left
}

choice! {
    /// Where the collapsible side panel goes (§5.3).
    SidePanelPosition {
        /// Right edge.
        Right => "right",
        /// Left edge.
        Left => "left",
    }
    default Right
}

choice! {
    /// Where the session tabs are drawn (§6.1).
    TabsPosition {
        /// In the title bar row.
        TitleBar => "title_bar",
        /// In a row below the title bar.
        BelowTitleBar => "below_title_bar",
    }
    default TitleBar
}

choice! {
    /// Window decoration mode (§5.3).
    Decorations {
        /// Custom title bar, without window buttons on tiling compositors.
        Auto => "auto",
        /// Always the custom title bar with window buttons.
        Custom => "custom",
        /// The system title bar.
        Native => "native",
        /// No title bar buttons and no system frame.
        None => "none",
    }
    default Auto
}

/// Accent color setting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Accent {
    /// The §5.2 default of the active scheme.
    #[default]
    Default,
    /// A user-chosen color.
    Custom(Rgba),
}

impl Accent {
    /// `"default"` or `#RRGGBB`.
    #[must_use]
    pub fn to_config_string(self) -> String {
        match self {
            Self::Default => "default".to_owned(),
            Self::Custom(color) => color.to_hex(),
        }
    }

    /// The custom color, if any.
    #[must_use]
    pub const fn color(self) -> Option<Rgba> {
        match self {
            Self::Default => None,
            Self::Custom(color) => Some(color),
        }
    }
}

impl FromStr for Accent {
    type Err = UnknownValue;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text == "default" {
            return Ok(Self::Default);
        }
        text.parse::<Rgba>()
            .map(Self::Custom)
            .map_err(|_| UnknownValue(text.to_owned()))
    }
}

/// `[general]` settings.
#[derive(Debug, Clone, PartialEq)]
pub struct General {
    /// `"system"` or a translation code (e.g. `es`, `pt_BR`, `pseudo`).
    pub language: String,
    /// Behaviour when the last tab closes.
    pub on_last_tab_closed: LastTabAction,
    /// Reopen the previous sessions at startup (implemented with sessions, Sprint 4).
    pub restore_sessions: bool,
    /// Ask before closing the window while sessions are active.
    pub confirm_close_with_sessions: bool,
    /// Opt-in update check (PLAN §0 rule 6: off by default; implemented in Sprint 18).
    pub check_for_updates: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            language: "system".to_owned(),
            on_last_tab_closed: LastTabAction::default(),
            restore_sessions: false,
            confirm_close_with_sessions: true,
            check_for_updates: false,
        }
    }
}

/// `[appearance]` settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Appearance {
    /// Light, dark or follow the system.
    pub theme: ThemeMode,
    /// Accent color.
    pub accent: Accent,
    /// Comfortable or compact.
    pub density: Density,
    /// Zoom for fonts and metrics.
    pub ui_scale: f64,
    /// UI font family; empty uses the bundled Inter.
    pub ui_font: String,
    /// Disable animations.
    pub reduce_motion: bool,
    /// Rail placement.
    pub rail_position: RailPosition,
    /// Show text labels under the rail icons.
    pub rail_labels: bool,
    /// Side panel placement.
    pub side_panel_position: SidePanelPosition,
    /// Tabs in the title bar or below it.
    pub tabs_position: TabsPosition,
    /// Show the status bar.
    pub show_status_bar: bool,
    /// Window decorations.
    pub window_decorations: Decorations,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: ThemeMode::default(),
            accent: Accent::default(),
            density: Density::default(),
            ui_scale: 1.0,
            ui_font: String::new(),
            reduce_motion: false,
            rail_position: RailPosition::default(),
            rail_labels: false,
            side_panel_position: SidePanelPosition::default(),
            tabs_position: TabsPosition::default(),
            show_status_bar: true,
            window_decorations: Decorations::default(),
        }
    }
}

/// The whole `config.toml`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Config {
    /// `[general]`.
    pub general: General,
    /// `[appearance]`.
    pub appearance: Appearance,
    /// What this version doesn't know, written back as it was read so a save never deletes a
    /// newer OpenSesh's settings: unknown top-level keys and tables as they are, and the unknown
    /// keys of `[general]` and `[appearance]` as tables under those names. Known keys always
    /// win over entries here.
    pub extra: Table,
}

/// A value that was ignored while loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// Dotted key, e.g. `appearance.density`.
    pub key: String,
    /// Human-readable reason.
    pub message: String,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.key, self.message)
    }
}

/// Errors that make the file unusable as a whole.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file could not be read.
    #[error("could not read {path}")]
    Read {
        /// File that failed.
        path: String,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The file is not valid TOML.
    #[error("{path} is not valid TOML: {message}")]
    Syntax {
        /// File that failed.
        path: String,
        /// Parser message (line and column included).
        message: String,
    },
    /// The file could not be written.
    #[error("could not write {path}")]
    Write {
        /// File that failed.
        path: String,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// Outcome of [`load_file`].
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    /// Settings (defaults for anything missing or invalid).
    pub config: Config,
    /// Values that were ignored.
    pub warnings: Vec<Warning>,
    /// The file comes from a newer OpenSesh: never overwrite it.
    pub read_only: bool,
    /// Whether the file existed.
    pub existed: bool,
}

/// Reads `path`. A missing file gives the defaults.
///
/// # Errors
///
/// Fails if the file exists but can't be read or isn't valid TOML.
pub fn load_file(path: &Path) -> Result<Loaded, ConfigError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Loaded {
                config: Config::default(),
                warnings: Vec::new(),
                read_only: false,
                existed: false,
            });
        }
        Err(source) => {
            return Err(ConfigError::Read {
                path: path.display().to_string(),
                source,
            });
        }
    };
    let (config, warnings, read_only) =
        Config::from_toml_str(&text).map_err(|message| ConfigError::Syntax {
            path: path.display().to_string(),
            message,
        })?;
    Ok(Loaded {
        config,
        warnings,
        read_only,
        existed: true,
    })
}

/// Writes `config` to `path` atomically, keeping [`fsutil::DEFAULT_BACKUPS`] backups.
///
/// # Errors
///
/// Fails if the file can't be written.
pub fn save_file(path: &Path, config: &Config) -> Result<WriteOutcome, ConfigError> {
    fsutil::atomic_write(
        path,
        config.to_toml_string().as_bytes(),
        fsutil::DEFAULT_BACKUPS,
    )
    .map_err(|source| ConfigError::Write {
        path: path.display().to_string(),
        source,
    })
}

impl Config {
    /// Parses a `config.toml` document. Returns the settings, the ignored values and whether
    /// the document comes from a newer schema.
    ///
    /// # Errors
    ///
    /// Returns the TOML parser message if the document isn't valid TOML.
    pub fn from_toml_str(text: &str) -> Result<(Self, Vec<Warning>, bool), String> {
        let mut root: Table = text
            .parse()
            .map_err(|error: toml::de::Error| error.to_string())?;
        let mut reader = Reader::default();

        let version = match root.remove("schema_version") {
            None => SCHEMA_VERSION,
            Some(Value::Integer(version)) => version,
            Some(other) => {
                reader.warn(
                    "schema_version",
                    format!("expected an integer, found {}", other.type_str()),
                );
                SCHEMA_VERSION
            }
        };
        let read_only = version > SCHEMA_VERSION;
        if read_only {
            reader.warn(
                "schema_version",
                format!(
                    "version {version} is newer than this OpenSesh supports ({SCHEMA_VERSION}); \
                     the file is read-only until you upgrade"
                ),
            );
        } else if version < 1 {
            reader.warn("schema_version", format!("invalid version {version}"));
        }
        // Migrations from older schemas go here, oldest first, once SCHEMA_VERSION > 1.

        let mut config = Self::default();
        let mut general = reader.section(&mut root, "general");
        {
            let g = &mut config.general;
            reader.language(&mut general, "general.language", &mut g.language);
            reader.choice(
                &mut general,
                "general.on_last_tab_closed",
                &mut g.on_last_tab_closed,
            );
            reader.boolean(
                &mut general,
                "general.restore_sessions",
                &mut g.restore_sessions,
            );
            reader.boolean(
                &mut general,
                "general.confirm_close_with_sessions",
                &mut g.confirm_close_with_sessions,
            );
            reader.boolean(
                &mut general,
                "general.check_for_updates",
                &mut g.check_for_updates,
            );
            reader.unknown(&general, "general");
        }
        let mut appearance = reader.section(&mut root, "appearance");
        {
            let a = &mut config.appearance;
            reader.choice(&mut appearance, "appearance.theme", &mut a.theme);
            reader.choice(&mut appearance, "appearance.accent", &mut a.accent);
            reader.choice(&mut appearance, "appearance.density", &mut a.density);
            reader.scale(&mut appearance, "appearance.ui_scale", &mut a.ui_scale);
            reader.string(&mut appearance, "appearance.ui_font", &mut a.ui_font);
            reader.boolean(
                &mut appearance,
                "appearance.reduce_motion",
                &mut a.reduce_motion,
            );
            reader.choice(
                &mut appearance,
                "appearance.rail_position",
                &mut a.rail_position,
            );
            reader.boolean(
                &mut appearance,
                "appearance.rail_labels",
                &mut a.rail_labels,
            );
            reader.choice(
                &mut appearance,
                "appearance.side_panel_position",
                &mut a.side_panel_position,
            );
            reader.choice(
                &mut appearance,
                "appearance.tabs_position",
                &mut a.tabs_position,
            );
            reader.boolean(
                &mut appearance,
                "appearance.show_status_bar",
                &mut a.show_status_bar,
            );
            reader.choice(
                &mut appearance,
                "appearance.window_decorations",
                &mut a.window_decorations,
            );
            reader.unknown(&appearance, "appearance");
        }
        reader.unknown(&root, "");

        // Every known key was taken out above (valid or not): what is left is kept as is.
        let mut extra = root;
        for (name, rest) in [("general", general), ("appearance", appearance)] {
            if !rest.is_empty() {
                extra.insert(name.to_owned(), Value::Table(rest));
            }
        }
        config.extra = extra;
        Ok((config, reader.warnings, read_only))
    }

    /// Serializes to a `config.toml` document (deterministic order, header comment included).
    #[must_use]
    pub fn to_toml_string(&self) -> String {
        let g = &self.general;
        let a = &self.appearance;
        let mut general = Table::new();
        general.insert("language".into(), Value::String(g.language.clone()));
        general.insert(
            "on_last_tab_closed".into(),
            Value::String(g.on_last_tab_closed.as_str().into()),
        );
        general.insert(
            "restore_sessions".into(),
            Value::Boolean(g.restore_sessions),
        );
        general.insert(
            "confirm_close_with_sessions".into(),
            Value::Boolean(g.confirm_close_with_sessions),
        );
        general.insert(
            "check_for_updates".into(),
            Value::Boolean(g.check_for_updates),
        );

        let mut appearance = Table::new();
        appearance.insert("theme".into(), Value::String(a.theme.as_str().into()));
        appearance.insert("accent".into(), Value::String(a.accent.to_config_string()));
        appearance.insert("density".into(), Value::String(a.density.as_str().into()));
        appearance.insert("ui_scale".into(), Value::Float(a.ui_scale));
        appearance.insert("ui_font".into(), Value::String(a.ui_font.clone()));
        appearance.insert("reduce_motion".into(), Value::Boolean(a.reduce_motion));
        appearance.insert(
            "rail_position".into(),
            Value::String(a.rail_position.as_str().into()),
        );
        appearance.insert("rail_labels".into(), Value::Boolean(a.rail_labels));
        appearance.insert(
            "side_panel_position".into(),
            Value::String(a.side_panel_position.as_str().into()),
        );
        appearance.insert(
            "tabs_position".into(),
            Value::String(a.tabs_position.as_str().into()),
        );
        appearance.insert("show_status_bar".into(), Value::Boolean(a.show_status_bar));
        appearance.insert(
            "window_decorations".into(),
            Value::String(a.window_decorations.as_str().into()),
        );

        // Unknown settings go back where they were read from; known keys take precedence.
        let keep_unknown = |known: &mut Table, unknown: &Table| {
            for (key, value) in unknown {
                known.entry(key.clone()).or_insert_with(|| value.clone());
            }
        };
        let mut root = Table::new();
        for (key, value) in &self.extra {
            match (key.as_str(), value) {
                ("general", Value::Table(unknown)) => keep_unknown(&mut general, unknown),
                ("appearance", Value::Table(unknown)) => keep_unknown(&mut appearance, unknown),
                ("schema_version" | "general" | "appearance", _) => {}
                _ => {
                    root.insert(key.clone(), value.clone());
                }
            }
        }
        root.insert("schema_version".into(), Value::Integer(SCHEMA_VERSION));
        root.insert("general".into(), Value::Table(general));
        root.insert("appearance".into(), Value::Table(appearance));
        format!("{HEADER}\n{root}")
    }
}

/// Parses `value` into a valid UI scale.
#[must_use]
pub fn valid_ui_scale(value: f64) -> Option<f64> {
    (value.is_finite() && UI_SCALE_RANGE.contains(&value)).then_some(value)
}

/// Whether `code` is an acceptable `general.language` value.
#[must_use]
pub fn valid_language(code: &str) -> bool {
    let is_lower = |part: &str| part.bytes().all(|b| b.is_ascii_lowercase());
    let is_upper = |part: &str| part.bytes().all(|b| b.is_ascii_uppercase());
    match code.split_once('_') {
        _ if code == "system" || code == "pseudo" => true,
        None => (2..=3).contains(&code.len()) && is_lower(code),
        Some((lang, region)) => {
            (2..=3).contains(&lang.len()) && is_lower(lang) && region.len() == 2 && is_upper(region)
        }
    }
}

/// Collects values from a parsed table, with a warning for anything unusable.
#[derive(Default)]
struct Reader {
    warnings: Vec<Warning>,
}

impl Reader {
    fn warn(&mut self, key: &str, message: String) {
        self.warnings.push(Warning {
            key: key.to_owned(),
            message,
        });
    }

    fn section(&mut self, root: &mut Table, name: &str) -> Table {
        match root.remove(name) {
            None => Table::new(),
            Some(Value::Table(table)) => table,
            Some(other) => {
                self.warn(
                    name,
                    format!("expected a table, found {}", other.type_str()),
                );
                Table::new()
            }
        }
    }

    /// Takes `key` from `table` (the last dotted segment is the table key).
    fn take(table: &mut Table, key: &str) -> Option<Value> {
        let field = key.rsplit('.').next().unwrap_or(key);
        table.remove(field)
    }

    fn boolean(&mut self, table: &mut Table, key: &str, target: &mut bool) {
        match Self::take(table, key) {
            None => {}
            Some(Value::Boolean(value)) => *target = value,
            Some(other) => self.warn(
                key,
                format!("expected true or false, found {}", other.type_str()),
            ),
        }
    }

    fn string(&mut self, table: &mut Table, key: &str, target: &mut String) {
        match Self::take(table, key) {
            None => {}
            Some(Value::String(value)) => *target = value,
            Some(other) => self.warn(
                key,
                format!("expected a string, found {}", other.type_str()),
            ),
        }
    }

    fn language(&mut self, table: &mut Table, key: &str, target: &mut String) {
        let mut value = target.clone();
        self.string(table, key, &mut value);
        if valid_language(&value) {
            *target = value;
        } else {
            self.warn(key, format!("unknown language `{value}`, using `system`"));
        }
    }

    fn choice<T: FromStr>(&mut self, table: &mut Table, key: &str, target: &mut T) {
        match Self::take(table, key) {
            None => {}
            Some(Value::String(text)) => match text.parse::<T>() {
                Ok(value) => *target = value,
                Err(_) => self.warn(key, format!("unknown value `{text}`, keeping the default")),
            },
            Some(other) => self.warn(
                key,
                format!("expected a string, found {}", other.type_str()),
            ),
        }
    }

    fn scale(&mut self, table: &mut Table, key: &str, target: &mut f64) {
        #[allow(clippy::cast_precision_loss)] // Small integers like 1 are exact in f64.
        let number = match Self::take(table, key) {
            None => return,
            Some(Value::Float(value)) => value,
            Some(Value::Integer(value)) => value as f64,
            Some(other) => {
                self.warn(
                    key,
                    format!("expected a number, found {}", other.type_str()),
                );
                return;
            }
        };
        match valid_ui_scale(number) {
            Some(value) => *target = value,
            None => self.warn(
                key,
                format!(
                    "{number} is outside {}..={}, keeping the default",
                    UI_SCALE_RANGE.start(),
                    UI_SCALE_RANGE.end()
                ),
            ),
        }
    }

    fn unknown(&mut self, table: &Table, section: &str) {
        for name in table.keys() {
            let key = if section.is_empty() {
                name.clone()
            } else {
                format!("{section}.{name}")
            };
            self.warn(
                &key,
                "unknown setting, not used by this version (kept in the file)".to_owned(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> (Config, Vec<String>, bool) {
        let (config, warnings, read_only) = Config::from_toml_str(text).unwrap();
        (
            config,
            warnings.iter().map(ToString::to_string).collect(),
            read_only,
        )
    }

    #[test]
    fn empty_document_gives_defaults() {
        let (config, warnings, read_only) = parse("");
        assert_eq!(config, Config::default());
        assert!(warnings.is_empty());
        assert!(!read_only);
    }

    #[test]
    fn defaults_match_the_plan() {
        let config = Config::default();
        assert_eq!(config.appearance.theme, ThemeMode::System);
        assert_eq!(config.appearance.window_decorations, Decorations::Auto);
        assert!(
            !config.general.check_for_updates,
            "updates are opt-in (PLAN rule 6)"
        );
        assert!(config.general.confirm_close_with_sessions);
        assert!(config.appearance.show_status_bar);
    }

    #[test]
    fn round_trip_preserves_every_field() {
        let mut config = Config::default();
        config.general.language = "pt_BR".into();
        config.general.on_last_tab_closed = LastTabAction::Quit;
        config.general.restore_sessions = true;
        config.appearance.theme = ThemeMode::Light;
        config.appearance.accent = Accent::Custom(Rgba::rgb(0x12, 0x34, 0x56));
        config.appearance.density = Density::Compact;
        config.appearance.ui_scale = 1.25;
        config.appearance.ui_font = "Noto Sans".into();
        config.appearance.rail_position = RailPosition::Right;
        config.appearance.rail_labels = true;
        config.appearance.side_panel_position = SidePanelPosition::Left;
        config.appearance.tabs_position = TabsPosition::BelowTitleBar;
        config.appearance.show_status_bar = false;
        config.appearance.window_decorations = Decorations::Native;
        config.appearance.reduce_motion = true;

        let text = config.to_toml_string();
        let (parsed, warnings, _) = parse(&text);
        assert_eq!(parsed, config);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn serialization_is_stable_and_commented() {
        let text = Config::default().to_toml_string();
        assert!(text.starts_with("# OpenSesh settings."));
        assert!(text.contains("schema_version = 1"));
        assert_eq!(text, Config::default().to_toml_string());
        assert!(text.contains("[general]") && text.contains("[appearance]"));
        let first_table = text.find('[').unwrap();
        assert!(text.find("schema_version").unwrap() < first_table);
    }

    #[test]
    fn invalid_values_fall_back_per_field_with_warnings() {
        let (config, warnings, _) = parse(
            r##"
            schema_version = 1
            [general]
            language = "Klingon!"
            restore_sessions = "yes"
            [appearance]
            theme = "sepia"
            density = "compact"
            accent = "#12345"
            ui_scale = 7.0
            rail_labels = true
            mystery = 3
            [plugins]
            enabled = true
            "##,
        );
        assert_eq!(config.general.language, "system");
        assert!(!config.general.restore_sessions);
        assert_eq!(config.appearance.theme, ThemeMode::System);
        assert_eq!(
            config.appearance.density,
            Density::Compact,
            "valid fields still apply"
        );
        assert_eq!(config.appearance.accent, Accent::Default);
        assert_eq!(config.appearance.ui_scale, 1.0);
        assert!(config.appearance.rail_labels);
        let joined = warnings.join("\n");
        for key in [
            "general.language",
            "general.restore_sessions",
            "appearance.theme",
            "appearance.accent",
            "appearance.ui_scale",
            "appearance.mystery",
            "plugins",
        ] {
            assert!(joined.contains(key), "missing warning for {key}:\n{joined}");
        }
    }

    #[test]
    fn unknown_sections_and_keys_survive_a_save() {
        // As written by a newer OpenSesh (same schema_version) sharing a synced folder.
        let (mut config, warnings, read_only) = parse(
            r##"
            # A comment: not kept.
            schema_version = 1
            zeta = "top-level value"
            [general]
            language = "es"
            future_flag = true
            [appearance]
            theme = "dark"
            density = "huge"
            [appearance.fonts]
            mono = "Iosevka"
            [terminal]
            font_size = 13
            [[plugins]]
            name = "a"
            [[plugins]]
            name = "b"
            "##,
        );
        assert!(!read_only);
        let joined = warnings.join("\n");
        for key in [
            "zeta",
            "general.future_flag",
            "appearance.fonts",
            "terminal",
            "plugins",
        ] {
            assert!(joined.contains(key), "missing warning for {key}:\n{joined}");
        }

        // A change made in the app, then a save.
        config.appearance.rail_labels = true;
        let saved = config.to_toml_string();
        let document: Table = saved.parse().unwrap();
        assert_eq!(document["zeta"].as_str(), Some("top-level value"));
        assert_eq!(document["general"]["future_flag"].as_bool(), Some(true));
        assert_eq!(document["general"]["language"].as_str(), Some("es"));
        assert_eq!(
            document["appearance"]["fonts"]["mono"].as_str(),
            Some("Iosevka")
        );
        assert_eq!(document["appearance"]["rail_labels"].as_bool(), Some(true));
        assert_eq!(document["terminal"]["font_size"].as_integer(), Some(13));
        let plugins = document["plugins"].as_array().unwrap();
        assert_eq!(plugins.len(), 2);
        assert_eq!(plugins[1]["name"].as_str(), Some("b"));
        // Invalid values of known keys still fall back to the default, and comments go.
        assert_eq!(
            document["appearance"]["density"].as_str(),
            Some("comfortable")
        );
        assert!(!saved.contains("A comment"));

        // Loading the saved file gives the same config, and saving again the same text.
        let (reloaded, _, _) = parse(&saved);
        assert_eq!(reloaded, config);
        assert_eq!(reloaded.to_toml_string(), saved);
    }

    #[test]
    fn known_keys_win_over_extra_entries() {
        let mut config = Config::default();
        config
            .extra
            .insert("schema_version".into(), Value::Integer(99));
        config
            .extra
            .insert("general".into(), Value::String("not a table".into()));
        let mut appearance = Table::new();
        appearance.insert("theme".into(), Value::String("light".into()));
        appearance.insert("sparkle".into(), Value::Boolean(true));
        config
            .extra
            .insert("appearance".into(), Value::Table(appearance));

        let (parsed, _, read_only) = parse(&config.to_toml_string());
        assert!(
            !read_only,
            "our schema_version is written, not the extra one"
        );
        assert_eq!(parsed.general, General::default());
        assert_eq!(parsed.appearance.theme, ThemeMode::System);
        let kept = parsed.extra["appearance"]["sparkle"].as_bool();
        assert_eq!(kept, Some(true));
        assert_eq!(parsed.extra.len(), 1, "{:?}", parsed.extra);
    }

    #[test]
    fn integer_scale_is_accepted() {
        let (config, warnings, _) = parse("[appearance]\nui_scale = 1\n");
        assert_eq!(config.appearance.ui_scale, 1.0);
        assert!(warnings.is_empty());
    }

    #[test]
    fn future_schema_is_read_only() {
        let (config, warnings, read_only) =
            parse("schema_version = 99\n[appearance]\ntheme = \"dark\"\n");
        assert!(read_only);
        assert_eq!(config.appearance.theme, ThemeMode::Dark);
        assert!(warnings[0].contains("newer"));
    }

    #[test]
    fn syntax_errors_are_reported_with_position() {
        let error = Config::from_toml_str("[appearance\ntheme = ").unwrap_err();
        assert!(error.contains("line"), "{error}");
    }

    #[test]
    fn language_codes() {
        for ok in ["system", "pseudo", "es", "en", "pt_BR", "zh_CN", "fil"] {
            assert!(valid_language(ok), "{ok}");
        }
        for bad in [
            "", "ES", "e", "pt-br", "pt_br", "english", "../es", "es_ESP",
        ] {
            assert!(!valid_language(bad), "{bad}");
        }
    }

    #[test]
    fn files_load_and_save_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE);

        let missing = load_file(&path).unwrap();
        assert!(!missing.existed);
        assert_eq!(missing.config, Config::default());

        let mut config = Config::default();
        config.appearance.density = Density::Compact;
        assert_eq!(save_file(&path, &config).unwrap(), WriteOutcome::Written);
        assert_eq!(save_file(&path, &config).unwrap(), WriteOutcome::Unchanged);

        let loaded = load_file(&path).unwrap();
        assert!(loaded.existed);
        assert_eq!(loaded.config, config);

        std::fs::write(&path, "not = [valid").unwrap();
        assert!(matches!(load_file(&path), Err(ConfigError::Syntax { .. })));
    }

    #[test]
    fn choice_identifiers_round_trip() {
        for value in Decorations::ALL {
            assert_eq!(value.as_str().parse::<Decorations>().unwrap(), *value);
        }
        for value in RailPosition::ALL {
            assert_eq!(value.as_str().parse::<RailPosition>().unwrap(), *value);
        }
        assert_eq!("default".parse::<Accent>().unwrap(), Accent::Default);
        assert!("amber".parse::<Accent>().is_err());
    }
}
