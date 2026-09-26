//! Terminal color themes (PLAN §4.3, §6.3): the model, the own TOML format, the built-in themes
//! and the user themes in `themes/*.toml`.
//!
//! A theme defines the default colors, the cursor, the selection, the search highlights and the
//! 16 ANSI colors. Importers ([`super::import`]) produce [`PartialColors`]: what the source file
//! had. [`PartialColors::complete`] derives what is missing (the cursor from the text color, the
//! selection and search colors from the palette), so every theme ends up with every color.
//!
//! Built-in themes: OpenSesh Dark and Light in the own format, and popular themes read from their
//! upstream Alacritty files (`assets/themes/upstream/`, licenses in `assets/themes/LICENSES/`)
//! through the Alacritty importer, so the importer runs on real files in every test.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml::{Table, Value};

use super::import::{self, Format};
use super::profile::FileProblem;
use super::settings::valid_id;
use crate::config::Warning;
use crate::theme::{AA_TEXT, Rgba, contrast_ratio};

/// Directory of the user themes inside the config directory.
pub const THEMES_DIR: &str = "themes";

/// Current layout of a theme file.
pub const SCHEMA_VERSION: i64 = 1;

/// ANSI color names, in index order (as the own format and Alacritty spell them).
pub const ANSI_NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

/// Every color of a theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThemeColors {
    /// Default text color.
    pub foreground: Rgba,
    /// Default background.
    pub background: Rgba,
    /// Cursor.
    pub cursor: Rgba,
    /// Character under a block cursor.
    pub cursor_text: Rgba,
    /// Background of selected text.
    pub selection_background: Rgba,
    /// Color of selected text; `None` keeps each character's own color.
    pub selection_foreground: Option<Rgba>,
    /// Background of search matches.
    pub match_background: Rgba,
    /// Text of search matches.
    pub match_foreground: Rgba,
    /// Background of the current search match.
    pub focused_match_background: Rgba,
    /// Text of the current search match.
    pub focused_match_foreground: Rgba,
    /// ANSI colors 0-7.
    pub normal: [Rgba; 8],
    /// ANSI colors 8-15.
    pub bright: [Rgba; 8],
}

impl ThemeColors {
    /// Whether the background is dark (text is light).
    #[must_use]
    pub fn is_dark(&self) -> bool {
        self.background.relative_luminance() < self.foreground.relative_luminance()
    }
}

/// The colors a theme file had; [`complete`](Self::complete) fills in the rest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartialColors {
    /// See [`ThemeColors::foreground`].
    pub foreground: Option<Rgba>,
    /// See [`ThemeColors::background`].
    pub background: Option<Rgba>,
    /// See [`ThemeColors::cursor`].
    pub cursor: Option<Rgba>,
    /// See [`ThemeColors::cursor_text`].
    pub cursor_text: Option<Rgba>,
    /// See [`ThemeColors::selection_background`].
    pub selection_background: Option<Rgba>,
    /// See [`ThemeColors::selection_foreground`].
    pub selection_foreground: Option<Rgba>,
    /// See [`ThemeColors::match_background`].
    pub match_background: Option<Rgba>,
    /// See [`ThemeColors::match_foreground`].
    pub match_foreground: Option<Rgba>,
    /// See [`ThemeColors::focused_match_background`].
    pub focused_match_background: Option<Rgba>,
    /// See [`ThemeColors::focused_match_foreground`].
    pub focused_match_foreground: Option<Rgba>,
    /// See [`ThemeColors::normal`].
    pub normal: [Option<Rgba>; 8],
    /// See [`ThemeColors::bright`].
    pub bright: [Option<Rgba>; 8],
}

/// Why a theme can't be made from a file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    /// The file isn't in the expected syntax.
    #[error("{0}")]
    Syntax(String),
    /// The file has no text or no background color.
    #[error("the theme has no {0} color")]
    Missing(&'static str),
    /// The format wasn't recognized.
    #[error(
        "not a theme file OpenSesh can read (iTerm2, Windows Terminal, Alacritty, Kitty, base16 or OpenSesh)"
    )]
    UnknownFormat,
    /// The file holds no theme.
    #[error("the file holds no color scheme")]
    Empty,
}

impl PartialColors {
    /// Every color, deriving the missing ones. Missing ANSI colors come from OpenSesh Dark or
    /// Light (whichever matches the background) and are reported; a missing bright color reuses
    /// the normal one.
    ///
    /// # Errors
    ///
    /// [`ThemeError::Missing`] without a foreground or a background.
    pub fn complete(&self) -> Result<(ThemeColors, Vec<String>), ThemeError> {
        let foreground = self.foreground.ok_or(ThemeError::Missing("text"))?;
        let background = self.background.ok_or(ThemeError::Missing("background"))?;
        let dark = background.relative_luminance() < foreground.relative_luminance();
        let base = if dark {
            builtin_colors::OPENSESH_DARK
        } else {
            builtin_colors::OPENSESH_LIGHT
        };
        let mut notes = Vec::new();
        let mut normal = [Rgba::rgb(0, 0, 0); 8];
        for (index, slot) in normal.iter_mut().enumerate() {
            *slot = self.normal[index].unwrap_or_else(|| {
                notes.push(format!("no {} color: using OpenSesh's", ANSI_NAMES[index]));
                base.normal[index]
            });
        }
        let mut bright = normal;
        for (index, slot) in bright.iter_mut().enumerate() {
            if let Some(color) = self.bright[index] {
                *slot = color;
            }
        }

        let readable = |preferred: Rgba, on: Rgba| {
            if contrast_ratio(preferred, on) >= AA_TEXT {
                preferred
            } else if contrast_ratio(foreground, on) >= contrast_ratio(background, on) {
                foreground
            } else {
                background
            }
        };
        let selection_background = self
            .selection_background
            .unwrap_or_else(|| background.mix(foreground, 0.22));
        let yellow = normal[3];
        let match_background = self
            .match_background
            .unwrap_or_else(|| background.mix(yellow, 0.35));
        let focused_match_background = self.focused_match_background.unwrap_or(yellow);
        let colors = ThemeColors {
            foreground,
            background,
            cursor: self.cursor.unwrap_or(foreground),
            cursor_text: self.cursor_text.unwrap_or(background),
            selection_background,
            selection_foreground: self.selection_foreground,
            match_background,
            match_foreground: self
                .match_foreground
                .unwrap_or_else(|| readable(foreground, match_background)),
            focused_match_background,
            focused_match_foreground: self
                .focused_match_foreground
                .unwrap_or_else(|| readable(background, focused_match_background)),
            normal,
            bright,
        };
        Ok((colors, notes))
    }
}

impl From<&ThemeColors> for PartialColors {
    fn from(colors: &ThemeColors) -> Self {
        Self {
            foreground: Some(colors.foreground),
            background: Some(colors.background),
            cursor: Some(colors.cursor),
            cursor_text: Some(colors.cursor_text),
            selection_background: Some(colors.selection_background),
            selection_foreground: colors.selection_foreground,
            match_background: Some(colors.match_background),
            match_foreground: Some(colors.match_foreground),
            focused_match_background: Some(colors.focused_match_background),
            focused_match_foreground: Some(colors.focused_match_foreground),
            normal: colors.normal.map(Some),
            bright: colors.bright.map(Some),
        }
    }
}

/// A theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalTheme {
    /// File name without `.toml` for user themes; a fixed id for built-in ones ([`valid_id`]).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Author, if known.
    pub author: String,
    /// License of the color scheme, if known (an SPDX id).
    pub license: String,
    /// Where it comes from (a URL), if known.
    pub source: String,
    /// Shipped with OpenSesh (can't be edited or deleted, only duplicated).
    pub builtin: bool,
    /// The colors.
    pub colors: ThemeColors,
}

impl TerminalTheme {
    /// Parses the own TOML format (PLAN §4.3). `id` is the file name without `.toml`.
    ///
    /// # Errors
    ///
    /// [`ThemeError::Syntax`] for invalid TOML, [`ThemeError::Missing`] without text or
    /// background color.
    pub fn from_toml_str(id: &str, text: &str) -> Result<(Self, Vec<Warning>), ThemeError> {
        let root: Table = text
            .parse()
            .map_err(|error: toml::de::Error| ThemeError::Syntax(error.to_string()))?;
        let mut warnings = Vec::new();
        let string = |key: &str| {
            root.get(key)
                .and_then(Value::as_str)
                .map(|value| value.trim().to_owned())
                .unwrap_or_default()
        };
        if let Some(Value::Integer(version)) = root.get("schema_version") {
            if *version > SCHEMA_VERSION {
                warnings.push(Warning {
                    key: "schema_version".to_owned(),
                    message: format!(
                        "version {version} is newer than this OpenSesh supports ({SCHEMA_VERSION})"
                    ),
                });
            }
        }
        let name = string("name");
        let colors_table = match root.get("colors") {
            Some(Value::Table(table)) => table.clone(),
            _ => Table::new(),
        };
        let mut partial = PartialColors::default();
        let mut color = |table: &Table, key: &str, prefix: &str| -> Option<Rgba> {
            let value = table.get(key)?;
            match value.as_str().map(parse_color) {
                Some(Some(color)) => Some(color),
                _ => {
                    warnings.push(Warning {
                        key: format!("{prefix}{key}"),
                        message: "expected a color like #RRGGBB".to_owned(),
                    });
                    None
                }
            }
        };
        partial.foreground = color(&colors_table, "foreground", "colors.");
        partial.background = color(&colors_table, "background", "colors.");
        partial.cursor = color(&colors_table, "cursor", "colors.");
        partial.cursor_text = color(&colors_table, "cursor_text", "colors.");
        partial.selection_background = color(&colors_table, "selection_bg", "colors.");
        partial.selection_foreground = color(&colors_table, "selection_fg", "colors.");
        partial.match_background = color(&colors_table, "search_bg", "colors.");
        partial.match_foreground = color(&colors_table, "search_fg", "colors.");
        partial.focused_match_background = color(&colors_table, "search_focused_bg", "colors.");
        partial.focused_match_foreground = color(&colors_table, "search_focused_fg", "colors.");
        for (group, target) in [
            ("normal", &mut partial.normal),
            ("bright", &mut partial.bright),
        ] {
            if let Some(Value::Table(table)) = colors_table.get(group) {
                for (index, ansi) in ANSI_NAMES.iter().enumerate() {
                    target[index] = color(table, ansi, &format!("colors.{group}."));
                }
            }
        }
        let (colors, notes) = partial.complete()?;
        warnings.extend(notes.into_iter().map(|message| Warning {
            key: "colors".to_owned(),
            message,
        }));
        Ok((
            Self {
                id: id.to_owned(),
                name: if name.is_empty() {
                    title_from_id(id)
                } else {
                    name
                },
                author: string("author"),
                license: string("license"),
                source: string("source"),
                builtin: false,
                colors,
            },
            warnings,
        ))
    }

    /// The own TOML format. Every color is written, so the file shows the whole theme.
    #[must_use]
    pub fn to_toml_string(&self) -> String {
        let c = &self.colors;
        let hex = |color: Rgba| Value::String(color.to_hex());
        let mut colors = Table::new();
        colors.insert("foreground".into(), hex(c.foreground));
        colors.insert("background".into(), hex(c.background));
        colors.insert("cursor".into(), hex(c.cursor));
        colors.insert("cursor_text".into(), hex(c.cursor_text));
        colors.insert("selection_bg".into(), hex(c.selection_background));
        if let Some(color) = c.selection_foreground {
            colors.insert("selection_fg".into(), hex(color));
        }
        colors.insert("search_bg".into(), hex(c.match_background));
        colors.insert("search_fg".into(), hex(c.match_foreground));
        colors.insert("search_focused_bg".into(), hex(c.focused_match_background));
        colors.insert("search_focused_fg".into(), hex(c.focused_match_foreground));
        for (group, values) in [("normal", c.normal), ("bright", c.bright)] {
            let mut table = Table::new();
            for (name, color) in ANSI_NAMES.iter().zip(values) {
                table.insert((*name).to_owned(), hex(color));
            }
            colors.insert(group.to_owned(), Value::Table(table));
        }
        let mut root = Table::new();
        root.insert("schema_version".into(), Value::Integer(SCHEMA_VERSION));
        root.insert("name".into(), Value::String(self.name.clone()));
        for (key, value) in [
            ("author", &self.author),
            ("license", &self.license),
            ("source", &self.source),
        ] {
            if !value.is_empty() {
                root.insert(key.to_owned(), Value::String(value.clone()));
            }
        }
        root.insert("colors".into(), Value::Table(colors));
        format!("# OpenSesh terminal theme.\n\n{root}")
    }
}

/// Parses `#RRGGBB`, `#RGB`, `0xRRGGBB` or `RRGGBB` (any case).
#[must_use]
pub fn parse_color(text: &str) -> Option<Rgba> {
    let text = text.trim();
    let hex = text
        .strip_prefix('#')
        .or_else(|| text.strip_prefix("0x"))
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize, len: usize| u8::from_str_radix(&hex[i..i + len], 16).ok();
    match hex.len() {
        6 => Some(Rgba::rgb(channel(0, 2)?, channel(2, 2)?, channel(4, 2)?)),
        3 => {
            let short = |i: usize| channel(i, 1).map(|v| v * 17);
            Some(Rgba::rgb(short(0)?, short(1)?, short(2)?))
        }
        _ => None,
    }
}

/// "catppuccin-mocha" or "gruvbox_dark" as "Catppuccin Mocha" and "Gruvbox Dark".
#[must_use]
pub fn title_from_id(id: &str) -> String {
    id.split(['-', '_', '.', ' '])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The OpenSesh palettes (PLAN §4.3 for the dark one), shared with the engine's defaults.
pub mod builtin_colors {
    use super::{Rgba, ThemeColors};

    const fn hex(value: u32) -> Rgba {
        let [_, r, g, b] = value.to_be_bytes();
        Rgba::rgb(r, g, b)
    }

    /// OpenSesh Dark, exactly as PLAN §4.3 defines it; search uses the amber accent.
    pub const OPENSESH_DARK: ThemeColors = ThemeColors {
        foreground: hex(0xD9DEE7),
        background: hex(0x121419),
        cursor: hex(0xE6B450),
        cursor_text: hex(0x121419),
        selection_background: hex(0x2B3242),
        selection_foreground: None,
        match_background: hex(0x4A3C1C),
        match_foreground: hex(0xF2F4F8),
        focused_match_background: hex(0xE6B450),
        focused_match_foreground: hex(0x121419),
        normal: [
            hex(0x1C1F27),
            hex(0xF07178),
            hex(0x9BD68A),
            hex(0xE6C07B),
            hex(0x73B7F2),
            hex(0xC9A0F0),
            hex(0x6ED6D0),
            hex(0xC8CDD6),
        ],
        bright: [
            hex(0x4A5263),
            hex(0xFF8F95),
            hex(0xB4EBA3),
            hex(0xF2D39A),
            hex(0x9ACCFA),
            hex(0xDDBDFB),
            hex(0x94E8E3),
            hex(0xF2F4F8),
        ],
    };

    /// OpenSesh Light: a warm paper background, the light amber accent, and ANSI colors dark
    /// enough for 4.5:1 text (the normal ones) or 3:1 (the bright ones, used for bold).
    pub const OPENSESH_LIGHT: ThemeColors = ThemeColors {
        foreground: hex(0x1F232B),
        background: hex(0xFAF9F5),
        cursor: hex(0xB7800F),
        cursor_text: hex(0xFFFFFF),
        selection_background: hex(0xE6DAB8),
        selection_foreground: None,
        match_background: hex(0xF3E2AE),
        match_foreground: hex(0x1F232B),
        focused_match_background: hex(0xD99A1F),
        focused_match_foreground: hex(0x1F232B),
        normal: [
            hex(0x1F232B),
            hex(0xB3261E),
            hex(0x2A6E2F),
            hex(0x855C00),
            hex(0x1F5FBF),
            hex(0x8E3FB5),
            hex(0x0B6E78),
            hex(0x5F6672),
        ],
        bright: [
            hex(0x6B7280),
            hex(0xD2453B),
            hex(0x3D8C42),
            hex(0xA77A0C),
            hex(0x3A7BD5),
            hex(0xA95CCF),
            hex(0x178A94),
            hex(0x7D8491),
        ],
    };
}

/// A built-in theme: its file and where it comes from.
struct Builtin {
    id: &'static str,
    name: &'static str,
    format: Format,
    text: &'static str,
    author: &'static str,
    license: &'static str,
    source: &'static str,
}

macro_rules! upstream {
    ($file:literal) => {
        include_str!(concat!("../../../../assets/themes/upstream/", $file))
    };
}

const BUILTINS: &[Builtin] = &[
    Builtin {
        id: "opensesh-dark",
        name: "OpenSesh Dark",
        format: Format::OpenSesh,
        text: include_str!("../../../../assets/themes/opensesh-dark.toml"),
        author: "OpenSesh",
        license: "GPL-3.0-or-later",
        source: "https://github.com/caixax/opensesh",
    },
    Builtin {
        id: "opensesh-light",
        name: "OpenSesh Light",
        format: Format::OpenSesh,
        text: include_str!("../../../../assets/themes/opensesh-light.toml"),
        author: "OpenSesh",
        license: "GPL-3.0-or-later",
        source: "https://github.com/caixax/opensesh",
    },
    Builtin {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        format: Format::Alacritty,
        text: upstream!("catppuccin-mocha.toml"),
        author: "Catppuccin",
        license: "MIT",
        source: "https://github.com/catppuccin/alacritty",
    },
    Builtin {
        id: "catppuccin-macchiato",
        name: "Catppuccin Macchiato",
        format: Format::Alacritty,
        text: upstream!("catppuccin-macchiato.toml"),
        author: "Catppuccin",
        license: "MIT",
        source: "https://github.com/catppuccin/alacritty",
    },
    Builtin {
        id: "catppuccin-frappe",
        name: "Catppuccin Frappé",
        format: Format::Alacritty,
        text: upstream!("catppuccin-frappe.toml"),
        author: "Catppuccin",
        license: "MIT",
        source: "https://github.com/catppuccin/alacritty",
    },
    Builtin {
        id: "catppuccin-latte",
        name: "Catppuccin Latte",
        format: Format::Alacritty,
        text: upstream!("catppuccin-latte.toml"),
        author: "Catppuccin",
        license: "MIT",
        source: "https://github.com/catppuccin/alacritty",
    },
    Builtin {
        id: "dracula",
        name: "Dracula",
        format: Format::Alacritty,
        text: upstream!("dracula.toml"),
        author: "Dracula Theme",
        license: "MIT",
        source: "https://github.com/dracula/alacritty",
    },
    Builtin {
        id: "nord",
        name: "Nord",
        format: Format::Alacritty,
        text: upstream!("nord.toml"),
        author: "Sven Greb",
        license: "MIT",
        source: "https://github.com/alacritty/alacritty-theme",
    },
    Builtin {
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        format: Format::Alacritty,
        text: upstream!("gruvbox_dark.toml"),
        author: "Pavel Pertsev (morhetz)",
        license: "MIT",
        source: "https://github.com/alacritty/alacritty-theme",
    },
    Builtin {
        id: "gruvbox-light",
        name: "Gruvbox Light",
        format: Format::Alacritty,
        text: upstream!("gruvbox_light.toml"),
        author: "Pavel Pertsev (morhetz)",
        license: "MIT",
        source: "https://github.com/alacritty/alacritty-theme",
    },
    Builtin {
        id: "tokyo-night",
        name: "Tokyo Night",
        format: Format::Alacritty,
        text: upstream!("tokyonight_night.toml"),
        author: "Folke Lemaitre",
        license: "Apache-2.0",
        source: "https://github.com/folke/tokyonight.nvim",
    },
    Builtin {
        id: "tokyo-night-storm",
        name: "Tokyo Night Storm",
        format: Format::Alacritty,
        text: upstream!("tokyonight_storm.toml"),
        author: "Folke Lemaitre",
        license: "Apache-2.0",
        source: "https://github.com/folke/tokyonight.nvim",
    },
    Builtin {
        id: "tokyo-night-day",
        name: "Tokyo Night Day",
        format: Format::Alacritty,
        text: upstream!("tokyonight_day.toml"),
        author: "Folke Lemaitre",
        license: "Apache-2.0",
        source: "https://github.com/folke/tokyonight.nvim",
    },
    Builtin {
        id: "solarized-dark",
        name: "Solarized Dark",
        format: Format::Alacritty,
        text: upstream!("solarized_dark.toml"),
        author: "Ethan Schoonover",
        license: "MIT",
        source: "https://github.com/alacritty/alacritty-theme",
    },
    Builtin {
        id: "solarized-light",
        name: "Solarized Light",
        format: Format::Alacritty,
        text: upstream!("solarized_light.toml"),
        author: "Ethan Schoonover",
        license: "MIT",
        source: "https://github.com/alacritty/alacritty-theme",
    },
];

/// The built-in themes, in the order the theme list shows them.
#[must_use]
pub fn builtin_themes() -> Vec<TerminalTheme> {
    BUILTINS
        .iter()
        .filter_map(|builtin| {
            let imported = import::import_text(builtin.text, builtin.format, builtin.id).ok()?;
            let theme = imported.into_iter().next()?;
            Some(TerminalTheme {
                id: builtin.id.to_owned(),
                name: builtin.name.to_owned(),
                author: builtin.author.to_owned(),
                license: builtin.license.to_owned(),
                source: builtin.source.to_owned(),
                builtin: true,
                colors: theme.colors,
            })
        })
        .collect()
}

/// Every theme: the built-in ones and the user's, by id. A user theme with a built-in's id
/// replaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeSet {
    themes: BTreeMap<String, TerminalTheme>,
    order: Vec<String>,
}

impl Default for ThemeSet {
    fn default() -> Self {
        let mut set = Self {
            themes: BTreeMap::new(),
            order: Vec::new(),
        };
        for theme in builtin_themes() {
            set.insert(theme);
        }
        set
    }
}

impl ThemeSet {
    /// The built-in themes plus every `*.toml` in `dir` (a missing directory is fine). Files
    /// that can't be read, or whose name isn't a valid id, are skipped and reported.
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
                    message: format!("could not list the themes: {error}"),
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
                    message: "skipped: a theme file name uses lowercase letters, digits, - _ and ."
                        .to_owned(),
                });
                continue;
            }
            let result = std::fs::read_to_string(&path)
                .map_err(|error| format!("could not read: {error}"))
                .and_then(|text| {
                    TerminalTheme::from_toml_str(id, &text).map_err(|error| error.to_string())
                });
            match result {
                Ok((theme, warnings)) => {
                    problems.extend(warnings.into_iter().map(|warning| FileProblem {
                        path: path.clone(),
                        message: warning.to_string(),
                    }));
                    set.insert(theme);
                }
                Err(message) => problems.push(FileProblem {
                    path: path.clone(),
                    message: format!("skipped: {message}"),
                }),
            }
        }
        (set, problems)
    }

    /// Adds or replaces a theme.
    pub fn insert(&mut self, theme: TerminalTheme) {
        if !self.themes.contains_key(&theme.id) {
            self.order.push(theme.id.clone());
        }
        self.themes.insert(theme.id.clone(), theme);
    }

    /// Removes a user theme (built-in ones stay). Returns it.
    pub fn remove(&mut self, id: &str) -> Option<TerminalTheme> {
        if self.themes.get(id).is_none_or(|theme| theme.builtin) {
            return None;
        }
        self.order.retain(|other| other != id);
        let removed = self.themes.remove(id);
        // A user theme that replaced a built-in one gives it back.
        if let Some(builtin) = builtin_themes().into_iter().find(|theme| theme.id == id) {
            self.insert(builtin);
        }
        removed
    }

    /// The theme with `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&TerminalTheme> {
        self.themes.get(id)
    }

    /// The theme with `id`, or OpenSesh Dark or Light (by `dark`) when there is none.
    #[must_use]
    pub fn get_or_default(&self, id: &str, dark: bool) -> TerminalTheme {
        self.themes.get(id).cloned().unwrap_or_else(|| {
            let fallback = if dark {
                super::settings::DEFAULT_DARK_THEME
            } else {
                super::settings::DEFAULT_LIGHT_THEME
            };
            self.themes
                .get(fallback)
                .cloned()
                .unwrap_or_else(|| TerminalTheme {
                    id: fallback.to_owned(),
                    name: title_from_id(fallback),
                    author: String::new(),
                    license: String::new(),
                    source: String::new(),
                    builtin: true,
                    colors: if dark {
                        builtin_colors::OPENSESH_DARK
                    } else {
                        builtin_colors::OPENSESH_LIGHT
                    },
                })
        })
    }

    /// Every theme: built-in ones first in their order, then the user's by name.
    #[must_use]
    pub fn list(&self) -> Vec<&TerminalTheme> {
        let mut builtin: Vec<&TerminalTheme> = self
            .order
            .iter()
            .filter_map(|id| self.themes.get(id))
            .filter(|theme| theme.builtin)
            .collect();
        let mut user: Vec<&TerminalTheme> = self
            .themes
            .values()
            .filter(|theme| !theme.builtin)
            .collect();
        user.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        builtin.append(&mut user);
        builtin
    }

    /// A new id for a theme called `name` that no theme uses yet.
    #[must_use]
    pub fn unique_id(&self, name: &str) -> String {
        let base = super::profile::slug(name);
        let base = if base.is_empty() {
            "theme".to_owned()
        } else {
            base
        };
        let mut id = base.clone();
        let mut n = 2;
        while self.themes.contains_key(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }
}

/// Path of user theme `id` inside `config_dir`.
#[must_use]
pub fn theme_path(config_dir: &Path, id: &str) -> PathBuf {
    config_dir.join(THEMES_DIR).join(format!("{id}.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::AA_UI;

    #[test]
    fn opensesh_dark_file_is_plan_4_3() {
        let set = ThemeSet::default();
        let dark = set.get("opensesh-dark").unwrap();
        assert_eq!(dark.colors, builtin_colors::OPENSESH_DARK);
        let light = set.get("opensesh-light").unwrap();
        assert_eq!(light.colors, builtin_colors::OPENSESH_LIGHT);
        assert!(dark.colors.is_dark() && !light.colors.is_dark());
    }

    #[test]
    fn every_builtin_theme_loads_and_is_readable() {
        let themes = builtin_themes();
        assert_eq!(
            themes.len(),
            BUILTINS.len(),
            "a built-in theme failed to load"
        );
        for theme in &themes {
            assert!(valid_id(&theme.id), "{}", theme.id);
            let imported = import::import_text(
                BUILTINS
                    .iter()
                    .find(|builtin| builtin.id == theme.id)
                    .unwrap()
                    .text,
                BUILTINS
                    .iter()
                    .find(|builtin| builtin.id == theme.id)
                    .unwrap()
                    .format,
                &theme.id,
            )
            .unwrap();
            assert!(
                imported[0].warnings.is_empty(),
                "{}: {:?}",
                theme.id,
                imported[0].warnings
            );
            let c = &theme.colors;
            let text = contrast_ratio(c.foreground, c.background);
            // Solarized's low-contrast text is its design (4.1:1 on light); keep a floor.
            assert!(text >= AA_UI, "{}: text {text:.2}", theme.id);
            let cursor = contrast_ratio(c.cursor, c.background);
            assert!(cursor >= 1.5, "{}: cursor {cursor:.2}", theme.id);
        }
    }

    #[test]
    fn upstream_values_survive_the_import() {
        let set = ThemeSet::default();
        let mocha = set.get("catppuccin-mocha").unwrap();
        assert_eq!(mocha.colors.background, Rgba::rgb(0x1e, 0x1e, 0x2e));
        assert_eq!(mocha.colors.cursor, Rgba::rgb(0xf5, 0xe0, 0xdc));
        assert_eq!(mocha.colors.normal[1], Rgba::rgb(0xf3, 0x8b, 0xa8));
        assert_eq!(
            mocha.colors.focused_match_background,
            Rgba::rgb(0xa6, 0xe3, 0xa1)
        );
        let dracula = set.get("dracula").unwrap();
        assert_eq!(dracula.colors.bright[4], Rgba::rgb(0xd6, 0xac, 0xff));
        assert_eq!(
            dracula.colors.selection_foreground, None,
            "CellForeground keeps the cell's color"
        );
        let nord = set.get("nord").unwrap();
        assert_eq!(nord.colors.cursor, nord.colors.foreground, "derived");
        assert_eq!(nord.license, "MIT");
    }

    #[test]
    fn own_format_round_trips() {
        let set = ThemeSet::default();
        for theme in set.list() {
            let text = theme.to_toml_string();
            let (again, warnings) = TerminalTheme::from_toml_str(&theme.id, &text).unwrap();
            assert!(warnings.is_empty(), "{}: {warnings:?}", theme.id);
            assert_eq!(again.colors, theme.colors, "{}", theme.id);
            assert_eq!(again.name, theme.name);
            assert_eq!(again.license, theme.license);
        }
    }

    #[test]
    fn partial_own_files_are_completed_with_notes() {
        let (theme, warnings) = TerminalTheme::from_toml_str(
            "mine",
            "[colors]\nforeground = \"#EEEEEE\"\nbackground = \"#101010\"\ncursor = \"nope\"\n[colors.normal]\nred = \"#FF0000\"\n",
        )
        .unwrap();
        assert_eq!(theme.name, "Mine");
        assert_eq!(theme.colors.normal[1], Rgba::rgb(255, 0, 0));
        assert_eq!(
            theme.colors.bright[1],
            Rgba::rgb(255, 0, 0),
            "bright reuses normal"
        );
        assert_eq!(
            theme.colors.normal[2],
            builtin_colors::OPENSESH_DARK.normal[2]
        );
        assert_eq!(theme.colors.cursor, theme.colors.foreground);
        // One warning for the bad cursor, seven notes for the missing normal colors.
        assert_eq!(warnings.len(), 8, "{warnings:?}");
        assert!(matches!(
            TerminalTheme::from_toml_str("x", "[colors]\nforeground = \"#FFFFFF\"\n"),
            Err(ThemeError::Missing("background"))
        ));
    }

    #[test]
    fn derived_search_colors_are_readable() {
        let partial = PartialColors {
            foreground: Some(Rgba::rgb(0x83, 0x94, 0x96)),
            background: Some(Rgba::rgb(0x00, 0x2b, 0x36)),
            normal: [Some(Rgba::rgb(0xb5, 0x89, 0x00)); 8],
            ..PartialColors::default()
        };
        let (colors, notes) = partial.complete().unwrap();
        assert!(notes.is_empty());
        assert!(
            contrast_ratio(
                colors.focused_match_foreground,
                colors.focused_match_background
            ) >= contrast_ratio(colors.foreground, colors.focused_match_background).min(AA_TEXT)
        );
    }

    #[test]
    fn colors_parse_in_every_spelling() {
        assert_eq!(parse_color("#1e1e2e"), Some(Rgba::rgb(0x1e, 0x1e, 0x2e)));
        assert_eq!(parse_color("0x1E1E2E"), Some(Rgba::rgb(0x1e, 0x1e, 0x2e)));
        assert_eq!(parse_color("1e1e2e"), Some(Rgba::rgb(0x1e, 0x1e, 0x2e)));
        assert_eq!(parse_color("#fa0"), Some(Rgba::rgb(0xff, 0xaa, 0x00)));
        for bad in ["", "#12345", "red", "#GGGGGG", "CellForeground"] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
        assert_eq!(title_from_id("catppuccin-mocha"), "Catppuccin Mocha");
        assert_eq!(title_from_id("gruvbox_dark"), "Gruvbox Dark");
    }

    #[test]
    fn user_themes_load_replace_and_remove() {
        let dir = tempfile::tempdir().unwrap();
        let mine = TerminalTheme {
            id: "mine".into(),
            name: "Mine".into(),
            author: String::new(),
            license: String::new(),
            source: String::new(),
            builtin: false,
            colors: builtin_colors::OPENSESH_LIGHT,
        };
        std::fs::write(dir.path().join("mine.toml"), mine.to_toml_string()).unwrap();
        let mut dracula = mine.clone();
        dracula.id = "dracula".into();
        dracula.name = "My Dracula".into();
        std::fs::write(dir.path().join("dracula.toml"), dracula.to_toml_string()).unwrap();
        std::fs::write(dir.path().join("broken.toml"), "[colors").unwrap();

        let (mut set, problems) = ThemeSet::load_dir(dir.path());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(set.get("mine").unwrap().name, "Mine");
        assert_eq!(set.get("dracula").unwrap().name, "My Dracula");
        let names: Vec<&str> = set.list().iter().map(|t| t.id.as_str()).collect();
        assert_eq!(names.first(), Some(&"opensesh-dark"));
        // User themes come last, by name: "Mine", then "My Dracula".
        assert_eq!(names[names.len() - 2..], ["mine", "dracula"]);

        assert!(set.remove("opensesh-dark").is_none(), "built-in");
        assert!(set.remove("dracula").is_some());
        assert_eq!(
            set.get("dracula").unwrap().name,
            "Dracula",
            "the built-in is back"
        );
        assert_eq!(set.unique_id("Dracula"), "dracula-2");
        assert_eq!(
            set.get_or_default("missing", false).id,
            super::super::settings::DEFAULT_LIGHT_THEME
        );
    }
}
