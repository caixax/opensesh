//! Importing and exporting terminal themes (PLAN §6.3).
//!
//! Importers read a theme file of another terminal into [`PartialColors`] (only what the file
//! defines; [`PartialColors::complete`] derives the rest):
//!
//! | Format | Files | Notes |
//! |---|---|---|
//! | iTerm2 | `.itermcolors` (XML property list) | components are read as sRGB |
//! | Windows Terminal | `.json`: one scheme, a list, or `settings.json` with `schemes` | comments and trailing commas allowed |
//! | Alacritty | `.toml` with `[colors.primary]` | `CellForeground` / `CellBackground` mean "derive" |
//! | Kitty | `.conf` | `## name:` / `## author:` / `## license:` metadata |
//! | base16 | `.yaml` (flat, or tinted-theming's `palette:`) | the base16-shell mapping |
//! | OpenSesh | `.toml` with `[colors]` (PLAN §4.3) | |
//!
//! Exporters write the own format ([`TerminalTheme::to_toml_string`]) and Alacritty's
//! ([`to_alacritty`]).

use std::path::Path;

use toml::{Table, Value};

use super::theme::{
    ANSI_NAMES, PartialColors, TerminalTheme, ThemeColors, ThemeError, parse_color, title_from_id,
};
use crate::theme::Rgba;

/// Largest theme file read, in bytes.
pub const MAX_THEME_FILE: u64 = 1024 * 1024;

/// A theme file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// OpenSesh's own TOML (PLAN §4.3).
    OpenSesh,
    /// Alacritty TOML.
    Alacritty,
    /// iTerm2 `.itermcolors`.
    ITerm2,
    /// Windows Terminal JSON.
    WindowsTerminal,
    /// Kitty `.conf`.
    Kitty,
    /// base16 YAML.
    Base16,
}

impl Format {
    /// Every format, in the order the import dialog lists them.
    pub const ALL: [Self; 6] = [
        Self::ITerm2,
        Self::WindowsTerminal,
        Self::Alacritty,
        Self::Kitty,
        Self::Base16,
        Self::OpenSesh,
    ];

    /// Display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OpenSesh => "OpenSesh",
            Self::Alacritty => "Alacritty",
            Self::ITerm2 => "iTerm2",
            Self::WindowsTerminal => "Windows Terminal",
            Self::Kitty => "Kitty",
            Self::Base16 => "base16",
        }
    }

    /// File name patterns, for a file dialog filter.
    #[must_use]
    pub const fn patterns(self) -> &'static [&'static str] {
        match self {
            Self::OpenSesh | Self::Alacritty => &["*.toml"],
            Self::ITerm2 => &["*.itermcolors"],
            Self::WindowsTerminal => &["*.json"],
            Self::Kitty => &["*.conf"],
            Self::Base16 => &["*.yaml", "*.yml"],
        }
    }

    /// The format of a file, from its name and then its content.
    #[must_use]
    pub fn detect(file_name: &str, text: &str) -> Option<Self> {
        let extension = Path::new(file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase);
        match extension.as_deref() {
            Some("itermcolors") => return Some(Self::ITerm2),
            Some("json") => return Some(Self::WindowsTerminal),
            Some("conf") => return Some(Self::Kitty),
            Some("yaml" | "yml") => return Some(Self::Base16),
            Some("toml") => return Some(toml_flavor(text)),
            _ => {}
        }
        let trimmed = text.trim_start_matches('\u{feff}').trim_start();
        if trimmed.starts_with('<') {
            Some(Self::ITerm2)
        } else if trimmed.starts_with('{')
            || (trimmed.starts_with('[')
                && serde_json::from_str::<serde_json::Value>(&strip_json_extras(text)).is_ok())
        {
            Some(Self::WindowsTerminal)
        } else if text.contains("base00") {
            Some(Self::Base16)
        } else if text.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("color0 ") || line.starts_with("color0\t")
        }) {
            Some(Self::Kitty)
        } else if text.parse::<Table>().is_ok() {
            Some(toml_flavor(text))
        } else {
            None
        }
    }
}

/// Own format (`[colors]` with `foreground`) or Alacritty (`[colors.primary]`).
fn toml_flavor(text: &str) -> Format {
    let own = text
        .parse::<Table>()
        .ok()
        .and_then(|root| root.get("colors").and_then(Value::as_table).cloned())
        .is_some_and(|colors| colors.contains_key("foreground") && !colors.contains_key("primary"));
    if own {
        Format::OpenSesh
    } else {
        Format::Alacritty
    }
}

/// One theme read from a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedTheme {
    /// Name from the file, or from the file name.
    pub name: String,
    /// Author, if the file says.
    pub author: String,
    /// License, if the file says.
    pub license: String,
    /// Every color (missing ones derived).
    pub colors: ThemeColors,
    /// What was skipped or derived.
    pub warnings: Vec<String>,
}

/// Why a file couldn't be imported.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The file couldn't be read.
    #[error("could not read the file")]
    Read(#[source] std::io::Error),
    /// The file is larger than [`MAX_THEME_FILE`].
    #[error("the file is too large for a theme")]
    TooLarge,
    /// The file is not a theme, or a broken one.
    #[error(transparent)]
    Theme(#[from] ThemeError),
}

/// Reads and imports a theme file, detecting its format.
///
/// # Errors
///
/// [`ImportError`] when the file can't be read or holds no theme.
pub fn import_file(path: &Path) -> Result<Vec<ImportedTheme>, ImportError> {
    let size = std::fs::metadata(path).map_err(ImportError::Read)?.len();
    if size > MAX_THEME_FILE {
        return Err(ImportError::TooLarge);
    }
    let bytes = std::fs::read(path).map_err(ImportError::Read)?;
    let text = String::from_utf8_lossy(&bytes);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let format = Format::detect(file_name, &text).ok_or(ThemeError::UnknownFormat)?;
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("theme");
    Ok(import_text(&text, format, stem)?)
}

/// Imports `text` in `format`. `fallback_name` names a theme whose file has no name (usually
/// the file name without extension).
///
/// # Errors
///
/// [`ThemeError`] when the text is not a theme of that format.
pub fn import_text(
    text: &str,
    format: Format,
    fallback_name: &str,
) -> Result<Vec<ImportedTheme>, ThemeError> {
    let text = text.trim_start_matches('\u{feff}');
    let fallback = title_from_id(fallback_name);
    let raw = match format {
        Format::OpenSesh => {
            let (theme, warnings) = TerminalTheme::from_toml_str(fallback_name, text)?;
            return Ok(vec![ImportedTheme {
                name: theme.name,
                author: theme.author,
                license: theme.license,
                colors: theme.colors,
                warnings: warnings.iter().map(ToString::to_string).collect(),
            }]);
        }
        Format::Alacritty => vec![alacritty(text, &fallback)?],
        Format::ITerm2 => vec![iterm2(text, &fallback)?],
        Format::WindowsTerminal => windows_terminal(text, &fallback)?,
        Format::Kitty => vec![kitty(text, &fallback)],
        Format::Base16 => vec![base16(text, &fallback)?],
    };
    if raw.is_empty() {
        return Err(ThemeError::Empty);
    }
    raw.into_iter()
        .map(|theme| {
            let (colors, notes) = theme.colors.complete()?;
            let mut warnings = theme.warnings;
            warnings.extend(notes);
            Ok(ImportedTheme {
                name: theme.name,
                author: theme.author,
                license: theme.license,
                colors,
                warnings,
            })
        })
        .collect()
}

/// A theme as read, before the missing colors are derived.
struct RawTheme {
    name: String,
    author: String,
    license: String,
    colors: PartialColors,
    warnings: Vec<String>,
}

impl RawTheme {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            author: String::new(),
            license: String::new(),
            colors: PartialColors::default(),
            warnings: Vec::new(),
        }
    }

    /// Parses a color written as text; `derive` words (e.g. Alacritty's `CellForeground`) mean
    /// "let OpenSesh choose" and are not an error.
    fn color(&mut self, what: &str, text: &str, derive: &[&str]) -> Option<Rgba> {
        let text = text.trim();
        if derive.iter().any(|word| word.eq_ignore_ascii_case(text)) {
            return None;
        }
        let color = parse_color(text);
        if color.is_none() {
            self.warnings.push(format!(
                "{what}: `{text}` is not a color OpenSesh reads; skipped"
            ));
        }
        color
    }
}

// ------------------------------------------------------------------------------------ Alacritty

fn alacritty(text: &str, fallback: &str) -> Result<RawTheme, ThemeError> {
    let root: Table = text
        .parse()
        .map_err(|error: toml::de::Error| ThemeError::Syntax(error.to_string()))?;
    let colors = root
        .get("colors")
        .and_then(Value::as_table)
        .ok_or(ThemeError::Empty)?;
    let mut theme = RawTheme::new(fallback);
    let lookup = |path: &[&str]| -> Option<String> {
        let mut value = colors.get(path[0])?;
        for key in &path[1..] {
            value = value.as_table()?.get(*key)?;
        }
        value.as_str().map(str::to_owned)
    };
    const CELL: &[&str] = &["CellForeground", "CellBackground"];
    let read = |theme: &mut RawTheme, path: &[&str]| -> Option<Rgba> {
        let text = lookup(path)?;
        theme.color(&format!("colors.{}", path.join(".")), &text, CELL)
    };
    let c = &mut PartialColors::default();
    c.foreground = read(&mut theme, &["primary", "foreground"]);
    c.background = read(&mut theme, &["primary", "background"]);
    c.cursor = read(&mut theme, &["cursor", "cursor"]);
    c.cursor_text = read(&mut theme, &["cursor", "text"]);
    c.selection_background = read(&mut theme, &["selection", "background"]);
    c.selection_foreground = read(&mut theme, &["selection", "text"]);
    c.match_background = read(&mut theme, &["search", "matches", "background"]);
    c.match_foreground = read(&mut theme, &["search", "matches", "foreground"]);
    c.focused_match_background = read(&mut theme, &["search", "focused_match", "background"]);
    c.focused_match_foreground = read(&mut theme, &["search", "focused_match", "foreground"]);
    for (index, name) in ANSI_NAMES.iter().enumerate() {
        c.normal[index] = read(&mut theme, &["normal", name]);
        c.bright[index] = read(&mut theme, &["bright", name]);
    }
    theme.colors = *c;
    Ok(theme)
}

/// Alacritty's TOML for a theme (the `[colors]` part of `alacritty.toml`).
#[must_use]
pub fn to_alacritty(theme: &TerminalTheme) -> String {
    let c = &theme.colors;
    let hex = |color: Rgba| Value::String(color.to_hex().to_ascii_lowercase());
    let pair = |fg: Value, bg: Value, fg_key: &str| {
        let mut table = Table::new();
        table.insert(fg_key.to_owned(), fg);
        table.insert("background".to_owned(), bg);
        Value::Table(table)
    };
    let mut colors = Table::new();
    colors.insert(
        "primary".into(),
        pair(hex(c.foreground), hex(c.background), "foreground"),
    );
    let mut cursor = Table::new();
    cursor.insert("cursor".into(), hex(c.cursor));
    cursor.insert("text".into(), hex(c.cursor_text));
    colors.insert("cursor".into(), Value::Table(cursor));
    let selection_text = c
        .selection_foreground
        .map_or_else(|| Value::String("CellForeground".into()), hex);
    colors.insert(
        "selection".into(),
        pair(selection_text, hex(c.selection_background), "text"),
    );
    let mut search = Table::new();
    search.insert(
        "matches".into(),
        pair(
            hex(c.match_foreground),
            hex(c.match_background),
            "foreground",
        ),
    );
    search.insert(
        "focused_match".into(),
        pair(
            hex(c.focused_match_foreground),
            hex(c.focused_match_background),
            "foreground",
        ),
    );
    colors.insert("search".into(), Value::Table(search));
    for (group, values) in [("normal", c.normal), ("bright", c.bright)] {
        let mut table = Table::new();
        for (name, color) in ANSI_NAMES.iter().zip(values) {
            table.insert((*name).to_owned(), hex(color));
        }
        colors.insert(group.to_owned(), Value::Table(table));
    }
    let mut root = Table::new();
    root.insert("colors".into(), Value::Table(colors));
    format!("# {} (exported from OpenSesh)\n\n{root}", theme.name)
}

// --------------------------------------------------------------------------------------- iTerm2

/// Which element's text is being read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlistText {
    None,
    Key,
    Number,
}

fn iterm2(text: &str, fallback: &str) -> Result<RawTheme, ThemeError> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut depth = 0_usize;
    let mut text_kind = PlistText::None;
    let mut entry = String::new();
    let mut component = String::new();
    let mut rgb = [None::<f64>; 3];
    let mut found: Vec<(String, [f64; 3])> = Vec::new();
    loop {
        let event = reader
            .read_event()
            .map_err(|error| ThemeError::Syntax(format!("not a valid property list: {error}")))?;
        match event {
            Event::Start(element) => {
                text_kind = match element.name().as_ref() {
                    "dict" => {
                        depth += 1;
                        if depth == 2 {
                            rgb = [None; 3];
                        }
                        PlistText::None
                    }
                    "key" => PlistText::Key,
                    "real" | "integer" => PlistText::Number,
                    _ => PlistText::None,
                };
            }
            Event::Text(content) => {
                let content = content.xml10_content();
                let content = content.trim();
                match (text_kind, depth) {
                    (PlistText::Key, 1) => entry = content.to_owned(),
                    (PlistText::Key, 2) => component = content.to_owned(),
                    (PlistText::Number, 2) => {
                        let slot = match component.as_str() {
                            "Red Component" => Some(0),
                            "Green Component" => Some(1),
                            "Blue Component" => Some(2),
                            _ => None,
                        };
                        if let (Some(slot), Ok(value)) = (slot, content.parse::<f64>()) {
                            rgb[slot] = Some(value);
                        }
                    }
                    _ => {}
                }
            }
            Event::End(element) => {
                if element.name().as_ref() == "dict" {
                    if depth == 2 {
                        if let [Some(r), Some(g), Some(b)] = rgb {
                            found.push((entry.clone(), [r, g, b]));
                        }
                    }
                    depth = depth.saturating_sub(1);
                }
                text_kind = PlistText::None;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if found.is_empty() {
        return Err(ThemeError::Empty);
    }
    let to_rgba = |[r, g, b]: [f64; 3]| {
        let channel = |value: f64| {
            // Clamped to 0..=255 first, so the cast can't truncate.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let byte = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            byte
        };
        Rgba::rgb(channel(r), channel(g), channel(b))
    };
    // Newer iTerm2 exports may only have "(Dark)" / "(Light)" variants: take the plain key, then
    // the dark one.
    let get = |key: &str| {
        found
            .iter()
            .find(|(name, _)| name == key)
            .or_else(|| {
                found
                    .iter()
                    .find(|(name, _)| *name == format!("{key} (Dark)"))
            })
            .map(|(_, rgb)| to_rgba(*rgb))
    };
    let mut theme = RawTheme::new(fallback);
    let c = &mut theme.colors;
    c.foreground = get("Foreground Color");
    c.background = get("Background Color");
    c.cursor = get("Cursor Color");
    c.cursor_text = get("Cursor Text Color");
    c.selection_background = get("Selection Color");
    c.selection_foreground = get("Selected Text Color");
    for index in 0..8 {
        c.normal[index] = get(&format!("Ansi {index} Color"));
        c.bright[index] = get(&format!("Ansi {} Color", index + 8));
    }
    Ok(theme)
}

// ------------------------------------------------------------------------------ Windows Terminal

fn windows_terminal(text: &str, fallback: &str) -> Result<Vec<RawTheme>, ThemeError> {
    let clean = strip_json_extras(text);
    let root: serde_json::Value = serde_json::from_str(&clean)
        .map_err(|error| ThemeError::Syntax(format!("not valid JSON: {error}")))?;
    let schemes: Vec<&serde_json::Value> = match &root {
        serde_json::Value::Array(items) => items.iter().collect(),
        serde_json::Value::Object(object) => match object.get("schemes") {
            Some(serde_json::Value::Array(items)) => items.iter().collect(),
            _ => vec![&root],
        },
        _ => Vec::new(),
    };
    const ANSI: [&str; 8] = [
        "black", "red", "green", "yellow", "blue", "purple", "cyan", "white",
    ];
    let mut themes = Vec::new();
    for scheme in schemes {
        let Some(object) = scheme.as_object() else {
            continue;
        };
        if !object.contains_key("background") && !object.contains_key("black") {
            continue;
        }
        let name = object
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map_or_else(|| fallback.to_owned(), |name| name.trim().to_owned());
        let mut theme = RawTheme::new(&name);
        let read = |theme: &mut RawTheme, key: &str| -> Option<Rgba> {
            let text = object.get(key)?.as_str()?;
            theme.color(key, text, &[])
        };
        let mut c = PartialColors {
            foreground: read(&mut theme, "foreground"),
            background: read(&mut theme, "background"),
            cursor: read(&mut theme, "cursorColor"),
            selection_background: read(&mut theme, "selectionBackground"),
            ..PartialColors::default()
        };
        for (index, name) in ANSI.iter().enumerate() {
            c.normal[index] = read(&mut theme, name);
            let bright = format!("bright{}{}", name[..1].to_ascii_uppercase(), &name[1..]);
            c.bright[index] = read(&mut theme, &bright);
        }
        theme.colors = c;
        themes.push(theme);
    }
    Ok(themes)
}

/// Removes `//` and `/* */` comments and trailing commas, which Windows Terminal's
/// `settings.json` allows and JSON doesn't.
#[must_use]
pub fn strip_json_extras(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut without_comments = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            without_comments.push(c);
            if c == '\\' {
                if let Some(next) = chars.get(i + 1) {
                    without_comments.push(*next);
                    i += 1;
                }
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match (c, chars.get(i + 1)) {
            ('"', _) => {
                in_string = true;
                without_comments.push(c);
                i += 1;
            }
            ('/', Some('/')) => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            ('/', Some('*')) => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 2;
            }
            _ => {
                without_comments.push(c);
                i += 1;
            }
        }
    }
    // Trailing commas: a comma followed (after spaces) by `}` or `]`, outside strings.
    let chars: Vec<char> = without_comments.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut in_string = false;
    let mut escaped = false;
    for (i, &c) in chars.iter().enumerate() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
        }
        if c == ',' {
            let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            if matches!(next, Some('}' | ']')) {
                continue;
            }
        }
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------------------------------- Kitty

fn kitty(text: &str, fallback: &str) -> RawTheme {
    let mut theme = RawTheme::new(fallback);
    let mut c = PartialColors::default();
    // Keywords that mean "the cell's own color": OpenSesh derives these.
    const CELL: &[&str] = &["none", "background", "foreground"];
    for line in text.lines() {
        let line = line.trim();
        if let Some(meta) = line.strip_prefix("##") {
            if let Some((key, value)) = meta.split_once(':') {
                let value = value.trim().to_owned();
                match key.trim().to_ascii_lowercase().as_str() {
                    "name" if !value.is_empty() => theme.name = value,
                    "author" => theme.author = value,
                    "license" => theme.license = value,
                    _ => {}
                }
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(key), Some(value)) = (parts.next(), parts.next()) else {
            continue;
        };
        let slot = match key {
            "foreground" => &mut c.foreground,
            "background" => &mut c.background,
            "cursor" => &mut c.cursor,
            "cursor_text_color" => &mut c.cursor_text,
            "selection_foreground" => &mut c.selection_foreground,
            "selection_background" => &mut c.selection_background,
            _ => match key
                .strip_prefix("color")
                .and_then(|n| n.parse::<usize>().ok())
            {
                Some(n @ 0..8) => &mut c.normal[n],
                Some(n @ 8..16) => &mut c.bright[n - 8],
                _ => continue,
            },
        };
        *slot = theme.color(key, value, CELL);
    }
    // kitty's own defaults, for a file that leaves them out.
    if c.foreground.is_none() {
        c.foreground = Some(Rgba::rgb(0xdd, 0xdd, 0xdd));
        theme
            .warnings
            .push("no foreground: using kitty's default".to_owned());
    }
    if c.background.is_none() {
        c.background = Some(Rgba::rgb(0, 0, 0));
        theme
            .warnings
            .push("no background: using kitty's default".to_owned());
    }
    theme.colors = c;
    theme
}

// --------------------------------------------------------------------------------------- base16

fn base16(text: &str, fallback: &str) -> Result<RawTheme, ThemeError> {
    let mut theme = RawTheme::new(fallback);
    let mut base = [None::<Rgba>; 16];
    let mut any_key = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---" {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        any_key = true;
        let key = key.trim().trim_matches(['"', '\'']);
        let value = yaml_scalar(value);
        let top_level = !line.starts_with([' ', '\t']);
        match key {
            "scheme" | "name" if top_level && !value.is_empty() => theme.name = value,
            "author" if top_level => theme.author = value,
            _ => {
                let index = key
                    .strip_prefix("base")
                    .filter(|hex| hex.len() == 2)
                    .and_then(|hex| usize::from_str_radix(hex, 16).ok())
                    .filter(|index| *index < 16);
                if let Some(index) = index {
                    base[index] = theme.color(key, &value, &[]);
                }
            }
        }
    }
    if !any_key {
        return Err(ThemeError::Syntax("not a base16 YAML file".to_owned()));
    }
    // The base16-shell mapping: colors 9-14 repeat 1-6.
    let ansi = [
        base[0x00], base[0x08], base[0x0B], base[0x0A], base[0x0D], base[0x0E], base[0x0C],
        base[0x05],
    ];
    let bright = [
        base[0x03], base[0x08], base[0x0B], base[0x0A], base[0x0D], base[0x0E], base[0x0C],
        base[0x07],
    ];
    theme.colors = PartialColors {
        foreground: base[0x05],
        background: base[0x00],
        cursor: base[0x05],
        cursor_text: base[0x00],
        selection_background: base[0x02],
        selection_foreground: base[0x05],
        normal: ansi,
        bright,
        ..PartialColors::default()
    };
    Ok(theme)
}

/// The value of a one-line YAML scalar: quoted or plain, with a trailing ` # comment` removed.
fn yaml_scalar(raw: &str) -> String {
    let raw = raw.trim();
    for quote in ['"', '\''] {
        if let Some(rest) = raw.strip_prefix(quote) {
            return rest
                .split_once(quote)
                .map_or(rest, |(value, _)| value)
                .to_owned();
        }
    }
    raw.split_once(" #")
        .map_or(raw, |(value, _)| value)
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! fixture {
        ($name:literal) => {
            include_str!(concat!("../../tests/fixtures/themes/", $name))
        };
    }

    const UPSTREAM_DRACULA: &str = include_str!("../../../../assets/themes/upstream/dracula.toml");

    fn one(text: &str, format: Format, name: &str) -> ImportedTheme {
        let mut themes = import_text(text, format, name).unwrap();
        assert_eq!(themes.len(), 1);
        themes.remove(0)
    }

    fn hex(value: u32) -> Rgba {
        let [_, r, g, b] = value.to_be_bytes();
        Rgba::rgb(r, g, b)
    }

    #[test]
    fn dracula_reads_the_same_from_alacritty_windows_terminal_and_kitty() {
        let alacritty = one(UPSTREAM_DRACULA, Format::Alacritty, "dracula");
        let wt = one(
            fixture!("dracula-windows-terminal.json"),
            Format::WindowsTerminal,
            "x",
        );
        let kitty = one(fixture!("dracula-kitty.conf"), Format::Kitty, "dracula");
        for theme in [&alacritty, &wt, &kitty] {
            assert_eq!(theme.colors.normal, alacritty.colors.normal);
            assert_eq!(theme.colors.bright, alacritty.colors.bright);
            assert_eq!(theme.colors.foreground, hex(0xf8f8f2));
            assert_eq!(theme.colors.background, hex(0x282a36));
            assert_eq!(theme.colors.selection_background, hex(0x44475a));
            assert!(theme.warnings.is_empty(), "{:?}", theme.warnings);
        }
        assert_eq!(wt.name, "Dracula");
        assert_eq!(alacritty.name, "Dracula");
        assert_eq!(kitty.colors.selection_foreground, Some(hex(0xffffff)));
        assert_eq!(wt.colors.normal[5], hex(0xff79c6), "purple is magenta");
    }

    #[test]
    fn iterm2_plist() {
        // Dracula's iTerm2 port (v1.2.5) has its own background and text colors.
        let theme = one(fixture!("dracula.itermcolors"), Format::ITerm2, "Dracula");
        assert_eq!(theme.name, "Dracula");
        assert!(theme.warnings.is_empty(), "{:?}", theme.warnings);
        assert_eq!(theme.colors.background, hex(0x1e1f29));
        assert_eq!(theme.colors.foreground, hex(0xe6e6e6));
        assert_eq!(theme.colors.normal[1], hex(0xff5555));
        assert_eq!(theme.colors.normal[2], hex(0x50fa7b));
        assert_eq!(theme.colors.bright[0], hex(0x555555));
    }

    #[test]
    fn base16_flat_and_palette_layouts() {
        let dracula = one(fixture!("dracula-base16.yaml"), Format::Base16, "x");
        assert_eq!(dracula.name, "Dracula");
        assert!(dracula.author.starts_with("clach04"));
        assert!(dracula.warnings.is_empty(), "{:?}", dracula.warnings);
        assert_eq!(dracula.colors.background, hex(0x282a36));
        assert_eq!(dracula.colors.normal[1], hex(0xff5555));
        assert_eq!(
            dracula.colors.bright[1],
            hex(0xff5555),
            "base16-shell repeats 1-6"
        );
        assert_eq!(dracula.colors.bright[0], hex(0x6272a4));
        assert_eq!(dracula.colors.selection_background, hex(0x44475a));

        let mocha = one(
            fixture!("catppuccin-mocha-base16.yaml"),
            Format::Base16,
            "x",
        );
        assert_eq!(mocha.colors.background, hex(0x1e1e2e));

        let flat = one(
            "scheme: \"Flat\"\nauthor: \"me\"\nbase00: \"101010\"\nbase05: 'e0e0e0' # text\nbase08: ff0000\n",
            Format::Base16,
            "x",
        );
        assert_eq!(flat.name, "Flat");
        assert_eq!(flat.colors.foreground, hex(0xe0e0e0));
        assert_eq!(flat.colors.normal[1], hex(0xff0000));
    }

    #[test]
    fn windows_terminal_settings_with_comments_and_several_schemes() {
        let text = r##"
        // Windows Terminal settings
        {
            "profiles": { "list": [] },
            /* two schemes */
            "schemes": [
                { "name": "One", "background": "#000000", "foreground": "#FFFFFF", "red": "#F00", },
                { "name": "Two // not a comment", "background": "#FFFFFF", "foreground": "#000000" },
            ],
        }
        "##;
        let themes = import_text(text, Format::WindowsTerminal, "x").unwrap();
        assert_eq!(themes.len(), 2);
        assert_eq!(themes[0].name, "One");
        assert_eq!(themes[0].colors.normal[1], hex(0xff0000));
        assert_eq!(themes[1].name, "Two // not a comment");
        assert!(matches!(
            import_text("{\"profiles\": {}}", Format::WindowsTerminal, "x"),
            Err(ThemeError::Empty)
        ));
    }

    #[test]
    fn kitty_metadata_and_unknown_values() {
        let theme = one(
            "## name: Night Owl\n## author: Sarah\n## license: MIT\nforeground #d6deeb\nbackground #011627\ncolor1 red\ncolor9 #ef5350\nurl_color #fff\n",
            Format::Kitty,
            "x",
        );
        assert_eq!(theme.name, "Night Owl");
        assert_eq!(theme.author, "Sarah");
        assert_eq!(theme.license, "MIT");
        assert_eq!(theme.colors.bright[1], hex(0xef5350));
        assert!(
            theme.warnings.iter().any(|w| w.contains("color1")),
            "{:?}",
            theme.warnings
        );
    }

    #[test]
    fn export_to_alacritty_and_back() {
        let set = super::super::theme::ThemeSet::default();
        for theme in set.list() {
            let text = to_alacritty(theme);
            let back = one(&text, Format::Alacritty, &theme.id);
            assert_eq!(back.colors, theme.colors, "{}", theme.id);
            assert!(
                back.warnings.is_empty(),
                "{}: {:?}",
                theme.id,
                back.warnings
            );
        }
    }

    #[test]
    fn formats_are_detected_by_name_then_content() {
        let cases = [
            ("a.itermcolors", "", Format::ITerm2),
            ("a.json", "", Format::WindowsTerminal),
            ("a.conf", "", Format::Kitty),
            ("a.YML", "", Format::Base16),
            (
                "a.toml",
                "[colors.primary]\nforeground = \"#fff\"\n",
                Format::Alacritty,
            ),
            (
                "a.toml",
                "[colors]\nforeground = \"#fff\"\n",
                Format::OpenSesh,
            ),
            ("a", "<?xml version=\"1.0\"?><plist/>", Format::ITerm2),
            ("a", "  {\"name\": 1}", Format::WindowsTerminal),
            ("a", "palette:\n  base00: \"#000000\"\n", Format::Base16),
            ("a", "foreground #fff\ncolor0 #000\n", Format::Kitty),
            ("a", "[colors.normal]\nred = \"#f00\"\n", Format::Alacritty),
        ];
        for (name, text, format) in cases {
            assert_eq!(Format::detect(name, text), Some(format), "{name}: {text}");
        }
        assert_eq!(Format::detect("notes.txt", "hello there"), None);
    }

    #[test]
    fn broken_files_are_errors() {
        assert!(matches!(
            import_text("[colors", Format::Alacritty, "x"),
            Err(ThemeError::Syntax(_))
        ));
        assert!(matches!(
            import_text("<plist><dict>", Format::ITerm2, "x"),
            Err(ThemeError::Syntax(_) | ThemeError::Empty)
        ));
        assert!(matches!(
            import_text("[colors.normal]\nred = \"#f00\"\n", Format::Alacritty, "x"),
            Err(ThemeError::Missing(_))
        ));
    }

    #[test]
    fn files_are_read_with_a_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Dracula.json");
        std::fs::write(&path, fixture!("dracula-windows-terminal.json")).unwrap();
        let themes = import_file(&path).unwrap();
        assert_eq!(themes[0].name, "Dracula");
        let big = dir.path().join("big.conf");
        std::fs::write(&big, vec![b'#'; 2 * 1024 * 1024]).unwrap();
        assert!(matches!(import_file(&big), Err(ImportError::TooLarge)));
        assert!(matches!(
            import_file(&dir.path().join("missing.conf")),
            Err(ImportError::Read(_))
        ));
    }
}
