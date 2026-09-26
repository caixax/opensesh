//! Terminal options (PLAN §6.2) and how their layers combine.
//!
//! [`TerminalSettings`] holds a value for every option: a fully resolved profile.
//! [`TerminalOverrides`] holds some of them: one layer of the inheritance chain (the default
//! profile, a named profile, a group's or a host's `[terminal]` table, a tab). A layer that
//! leaves an option out inherits it from the layers before it; [`resolve`] applies them in order
//! over the built-in defaults.
//!
//! Every option is declared once in the `terminal_settings!` table below, with its TOML key,
//! type, default and check. Reading is lenient per key: an invalid value is skipped with a
//! [`Warning`] that names the key, and keys this version doesn't know are kept so a save never
//! drops a newer OpenSesh's options.

use std::fmt;

use toml::{Table, Value};

use crate::config::Warning;
use crate::theme::{Rgba, UnknownValue};

choice! {
    /// Font hinting (`QFont::HintingPreference`).
    Hinting {
        /// The platform's default.
        Default => "default",
        /// No hinting.
        None => "none",
        /// Vertical hinting only.
        Vertical => "vertical",
        /// Full hinting.
        Full => "full",
    }
    default Default
}

choice! {
    /// Cursor shape when the program doesn't choose one.
    CursorStyle {
        /// Filled block.
        Block => "block",
        /// Vertical bar.
        Beam => "beam",
        /// Line under the character.
        Underline => "underline",
    }
    default Block
}

choice! {
    /// How the background image fills the terminal.
    ImageFit {
        /// Scaled to cover the whole terminal, cropped.
        Cover => "cover",
        /// Scaled to fit inside the terminal, whole.
        Contain => "contain",
        /// Stretched to the terminal's size.
        Stretch => "stretch",
        /// Repeated at its own size.
        Tile => "tile",
        /// Centered at its own size.
        Center => "center",
    }
    default Cover
}

choice! {
    /// What a right click does (when the program doesn't take the mouse).
    RightClick {
        /// Opens the context menu.
        Menu => "menu",
        /// Pastes the clipboard.
        Paste => "paste",
    }
    default Menu
}

choice! {
    /// What programs may do with the clipboard through OSC 52.
    Osc52Access {
        /// Nothing.
        Off => "off",
        /// Set the clipboard (never read it).
        Copy => "copy",
    }
    default Off
}

choice! {
    /// What the bell does.
    BellStyle {
        /// A short flash of the terminal.
        Visual => "visual",
        /// The system's alert sound.
        Sound => "sound",
        /// A desktop notification when the window isn't active, a flash otherwise.
        Notification => "notification",
        /// Nothing.
        None => "none",
    }
    default Visual
}

choice! {
    /// What Backspace sends.
    BackspaceKey {
        /// DEL, `^?` (0x7f).
        Del => "del",
        /// BS, `^H` (0x08).
        CtrlH => "ctrl_h",
    }
    default Del
}

choice! {
    /// What Delete sends.
    DeleteKey {
        /// The VT220 sequence `ESC [ 3 ~`.
        Vt220 => "vt220",
        /// DEL, `^?` (0x7f).
        Del => "del",
    }
    default Vt220
}

/// A color that the theme provides unless the profile sets its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ThemeColor {
    /// The theme's color.
    #[default]
    Theme,
    /// A fixed color.
    Custom(Rgba),
}

impl ThemeColor {
    /// The fixed color, if any.
    #[must_use]
    pub const fn color(self) -> Option<Rgba> {
        match self {
            Self::Theme => None,
            Self::Custom(color) => Some(color),
        }
    }
}

impl fmt::Display for ThemeColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Theme => f.write_str("theme"),
            Self::Custom(color) => write!(f, "{color}"),
        }
    }
}

impl std::str::FromStr for ThemeColor {
    type Err = UnknownValue;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text == "theme" {
            return Ok(Self::Theme);
        }
        text.parse::<Rgba>()
            .map(Self::Custom)
            .map_err(|_| UnknownValue(text.to_owned()))
    }
}

/// How an option's type is read from and written to TOML.
pub trait SettingValue: Sized + Clone + PartialEq + fmt::Debug {
    /// Parses a TOML value, or explains why it doesn't fit.
    ///
    /// # Errors
    ///
    /// A short reason, e.g. "expected true or false, found string".
    fn from_toml(value: &Value) -> Result<Self, String>;
    /// The TOML value written to the file.
    fn to_toml(&self) -> Value;
}

fn expected(what: &str, value: &Value) -> String {
    format!("expected {what}, found {}", value.type_str())
}

impl SettingValue for bool {
    fn from_toml(value: &Value) -> Result<Self, String> {
        value
            .as_bool()
            .ok_or_else(|| expected("true or false", value))
    }

    fn to_toml(&self) -> Value {
        Value::Boolean(*self)
    }
}

impl SettingValue for String {
    fn from_toml(value: &Value) -> Result<Self, String> {
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| expected("a string", value))
    }

    fn to_toml(&self) -> Value {
        Value::String(self.clone())
    }
}

impl SettingValue for f64 {
    fn from_toml(value: &Value) -> Result<Self, String> {
        #[allow(clippy::cast_precision_loss)] // Small integers like 12 are exact in f64.
        match value {
            Value::Float(number) if number.is_finite() => Ok(*number),
            Value::Integer(number) => Ok(*number as f64),
            _ => Err(expected("a number", value)),
        }
    }

    fn to_toml(&self) -> Value {
        Value::Float(*self)
    }
}

impl SettingValue for u32 {
    fn from_toml(value: &Value) -> Result<Self, String> {
        match value {
            Value::Integer(number) => {
                u32::try_from(*number).map_err(|_| format!("{number} is out of range"))
            }
            _ => Err(expected("a whole number", value)),
        }
    }

    fn to_toml(&self) -> Value {
        Value::Integer(i64::from(*self))
    }
}

impl SettingValue for Vec<String> {
    fn from_toml(value: &Value) -> Result<Self, String> {
        let items = value
            .as_array()
            .ok_or_else(|| expected("a list of strings", value))?;
        items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| expected("a list of strings", item))
            })
            .collect()
    }

    fn to_toml(&self) -> Value {
        Value::Array(self.iter().cloned().map(Value::String).collect())
    }
}

/// Implements [`SettingValue`] for types stored as their `Display` / `FromStr` text.
macro_rules! text_setting {
    ($($ty:ty),+) => {$(
        impl SettingValue for $ty {
            fn from_toml(value: &Value) -> Result<Self, String> {
                let text = value.as_str().ok_or_else(|| expected("a string", value))?;
                text.parse().map_err(|_| format!("unknown value `{text}`"))
            }

            fn to_toml(&self) -> Value {
                Value::String(self.to_string())
            }
        }
    )+};
}

/// Implements [`SettingValue`] for the `choice!` enums.
macro_rules! choice_setting {
    ($($ty:ty),+) => {$(
        impl SettingValue for $ty {
            fn from_toml(value: &Value) -> Result<Self, String> {
                let text = value.as_str().ok_or_else(|| expected("a string", value))?;
                text.parse().map_err(|_| {
                    let allowed: Vec<&str> = Self::ALL.iter().map(|v| v.as_str()).collect();
                    format!("unknown value `{text}` (one of: {})", allowed.join(", "))
                })
            }

            fn to_toml(&self) -> Value {
                Value::String(self.as_str().to_owned())
            }
        }
    )+};
}

text_setting!(ThemeColor);
choice_setting!(
    Hinting,
    CursorStyle,
    ImageFit,
    RightClick,
    Osc52Access,
    BellStyle,
    BackspaceKey,
    DeleteKey
);

/// Font sizes, in points.
pub const FONT_SIZE_RANGE: std::ops::RangeInclusive<f64> = 4.0..=96.0;
/// Longest scrollback, in lines (about 2.4 GB at 200 columns for the largest value).
pub const MAX_SCROLLBACK_LINES: u32 = 100_000;
/// The default word separators for double-click selection (alacritty's list).
pub const DEFAULT_WORD_SEPARATORS: &str = ",│`|:\"' ()[]{}<>\t";
/// The default terminal type.
pub const DEFAULT_TERM: &str = "xterm-256color";
/// Id of the built-in dark theme.
pub const DEFAULT_DARK_THEME: &str = "opensesh-dark";
/// Id of the built-in light theme.
pub const DEFAULT_LIGHT_THEME: &str = "opensesh-light";

/// Character encodings a terminal can use (canonical WHATWG names, as `encoding_rs` spells
/// them). Only encodings where ASCII bytes mean ASCII are listed: escape sequences must pass
/// through unchanged (so no UTF-16 and no ISO-2022-JP).
pub const ENCODINGS: &[&str] = &[
    "UTF-8",
    "IBM866",
    "ISO-8859-2",
    "ISO-8859-3",
    "ISO-8859-4",
    "ISO-8859-5",
    "ISO-8859-6",
    "ISO-8859-7",
    "ISO-8859-8",
    "ISO-8859-10",
    "ISO-8859-13",
    "ISO-8859-14",
    "ISO-8859-15",
    "ISO-8859-16",
    "KOI8-R",
    "KOI8-U",
    "macintosh",
    "windows-874",
    "windows-1250",
    "windows-1251",
    "windows-1252",
    "windows-1253",
    "windows-1254",
    "windows-1255",
    "windows-1256",
    "windows-1257",
    "windows-1258",
    "x-mac-cyrillic",
    "GBK",
    "gb18030",
    "Big5",
    "EUC-JP",
    "Shift_JIS",
    "EUC-KR",
];

type Check<T> = fn(&T) -> Result<(), String>;

fn any<T>(_: &T) -> Result<(), String> {
    Ok(())
}

fn in_range<T: PartialOrd + fmt::Display>(value: &T, low: T, high: T) -> Result<(), String> {
    if *value < low || *value > high {
        Err(format!("{value} is outside {low}..={high}"))
    } else {
        Ok(())
    }
}

fn plain_text(text: &str, max_chars: usize) -> Result<(), String> {
    if text.chars().count() > max_chars {
        return Err(format!("longer than {max_chars} characters"));
    }
    if text.chars().any(char::is_control) {
        return Err("contains control characters".to_owned());
    }
    Ok(())
}

// Checks are `fn(&T)` of the option's own type.
#[allow(clippy::ptr_arg)]
fn font_name(name: &String) -> Result<(), String> {
    plain_text(name, 128)
}

// Checks are `fn(&T)` of the option's own type.
#[allow(clippy::ptr_arg)]
fn font_names(names: &Vec<String>) -> Result<(), String> {
    if names.len() > 16 {
        return Err("more than 16 fonts".to_owned());
    }
    names.iter().try_for_each(font_name)
}

fn font_size(size: &f64) -> Result<(), String> {
    in_range(size, *FONT_SIZE_RANGE.start(), *FONT_SIZE_RANGE.end())
}

fn font_weight(weight: &u32) -> Result<(), String> {
    in_range(weight, 100, 900)
}

fn line_height(value: &f64) -> Result<(), String> {
    in_range(value, 0.8, 2.0)
}

fn letter_spacing(value: &f64) -> Result<(), String> {
    in_range(value, -2.0, 10.0)
}

fn unit(value: &f64) -> Result<(), String> {
    in_range(value, 0.0, 1.0)
}

fn contrast(value: &f64) -> Result<(), String> {
    in_range(value, 1.0, 21.0)
}

fn padding(value: &u32) -> Result<(), String> {
    in_range(value, 0, 64)
}

fn scrollback(value: &u32) -> Result<(), String> {
    in_range(value, 0, MAX_SCROLLBACK_LINES)
}

fn scroll_speed(value: &f64) -> Result<(), String> {
    in_range(value, 0.25, 10.0)
}

fn paste_delay(value: &u32) -> Result<(), String> {
    in_range(value, 0, 5000)
}

// Checks are `fn(&T)` of the option's own type.
#[allow(clippy::ptr_arg)]
fn path(value: &String) -> Result<(), String> {
    plain_text(value, 4096)
}

// Checks are `fn(&T)` of the option's own type.
#[allow(clippy::ptr_arg)]
fn separators(value: &String) -> Result<(), String> {
    if value.chars().any(|c| c.is_control() && c != '\t') {
        return Err("contains control characters".to_owned());
    }
    if value.chars().count() > 64 {
        return Err("longer than 64 characters".to_owned());
    }
    Ok(())
}

/// Whether `id` can name a theme, a profile or a highlight set: lowercase letters, digits, `-`,
/// `_` and `.`, starting with a letter or a digit, at most 64 characters.
#[must_use]
pub fn valid_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    let first_ok = bytes
        .next()
        .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    first_ok
        && id.len() <= 64
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b))
}

fn theme_id(id: &String) -> Result<(), String> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(format!("`{id}` is not a theme id"))
    }
}

// Checks are `fn(&T)` of the option's own type.
#[allow(clippy::ptr_arg)]
fn set_ids(ids: &Vec<String>) -> Result<(), String> {
    if ids.len() > 32 {
        return Err("more than 32 rule sets".to_owned());
    }
    match ids.iter().find(|id| !valid_id(id)) {
        Some(id) => Err(format!("`{id}` is not a rule set id")),
        None => Ok(()),
    }
}

/// Whether `term` is an acceptable `TERM` value: 1 to 64 ASCII letters, digits and `-_.+`.
#[must_use]
pub fn valid_term(term: &str) -> bool {
    (1..=64).contains(&term.len())
        && term
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.+".contains(&b))
}

fn term(value: &String) -> Result<(), String> {
    if valid_term(value) {
        Ok(())
    } else {
        Err(format!("`{value}` is not a terminal type"))
    }
}

/// The canonical name of `label` in [`ENCODINGS`] (case-insensitive), if it is one.
#[must_use]
pub fn canonical_encoding(label: &str) -> Option<&'static str> {
    ENCODINGS
        .iter()
        .copied()
        .find(|name| name.eq_ignore_ascii_case(label))
}

fn encoding(value: &String) -> Result<(), String> {
    match canonical_encoding(value) {
        Some(name) if name == value => Ok(()),
        Some(name) => Err(format!("write `{name}`")),
        None => Err(format!("unknown encoding `{value}`")),
    }
}

/// The answerback reply goes to the program as typed text: printable ASCII only, so it can never
/// carry a control sequence or a newline that runs a command.
// Checks are `fn(&T)` of the option's own type.
#[allow(clippy::ptr_arg)]
fn answerback(value: &String) -> Result<(), String> {
    if value.len() > 64 {
        return Err("longer than 64 characters".to_owned());
    }
    if value.bytes().all(|b| (0x20..0x7f).contains(&b)) {
        Ok(())
    } else {
        Err("only printable ASCII characters are allowed".to_owned())
    }
}

/// Why a value was not accepted by [`TerminalOverrides::set`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SetError {
    /// No option has this key.
    #[error("unknown terminal option `{0}`")]
    UnknownKey(String),
    /// The value doesn't fit the option.
    #[error("{key}: {reason}")]
    Invalid {
        /// The option.
        key: String,
        /// Why.
        reason: String,
    },
}

/// Declares every option once: the two structs, the key list and the table conversions.
macro_rules! terminal_settings {
    ($(
        $(#[$meta:meta])*
        $field:ident: $ty:ty = $default:expr, check $check:expr;
    )+) => {
        /// Every terminal option with a value: a resolved profile.
        #[derive(Debug, Clone, PartialEq)]
        pub struct TerminalSettings {
            $($(#[$meta])* pub $field: $ty,)+
        }

        impl Default for TerminalSettings {
            fn default() -> Self {
                Self { $($field: $default,)+ }
            }
        }

        /// Some terminal options: one layer of the inheritance chain. `None` inherits.
        #[derive(Debug, Clone, Default, PartialEq)]
        pub struct TerminalOverrides {
            $($(#[$meta])* pub $field: Option<$ty>,)+
        }

        /// Every option key, in the order of the settings page.
        pub const KEYS: &[&str] = &[$(stringify!($field)),+];

        impl TerminalSettings {
            /// The value of `key` as TOML, or `None` for an unknown key.
            #[must_use]
            pub fn get(&self, key: &str) -> Option<Value> {
                match key {
                    $(stringify!($field) => Some(SettingValue::to_toml(&self.$field)),)+
                    _ => None,
                }
            }
        }

        impl TerminalOverrides {
            /// Copies the options this layer sets into `settings`.
            pub fn apply_to(&self, settings: &mut TerminalSettings) {
                $(if let Some(value) = &self.$field {
                    settings.$field = value.clone();
                })+
            }

            /// Copies the options `other` sets over this layer's.
            pub fn merge(&mut self, other: &Self) {
                $(if let Some(value) = &other.$field {
                    self.$field = Some(value.clone());
                })+
            }

            /// The value this layer sets for `key`, as TOML.
            #[must_use]
            pub fn get(&self, key: &str) -> Option<Value> {
                match key {
                    $(stringify!($field) => self.$field.as_ref().map(SettingValue::to_toml),)+
                    _ => None,
                }
            }

            /// Sets `key` from a TOML value, after checking it.
            ///
            /// # Errors
            ///
            /// [`SetError::UnknownKey`] or [`SetError::Invalid`]; the layer is unchanged.
            pub fn set(&mut self, key: &str, value: &Value) -> Result<(), SetError> {
                let invalid = |reason: String| SetError::Invalid { key: key.to_owned(), reason };
                match key {
                    $(stringify!($field) => {
                        let parsed = <$ty as SettingValue>::from_toml(value).map_err(invalid)?;
                        let check: Check<$ty> = $check;
                        check(&parsed).map_err(invalid)?;
                        self.$field = Some(parsed);
                        Ok(())
                    })+
                    _ => Err(SetError::UnknownKey(key.to_owned())),
                }
            }

            /// Stops setting `key` (it inherits again). Returns whether it was set.
            pub fn clear(&mut self, key: &str) -> bool {
                match key {
                    $(stringify!($field) => self.$field.take().is_some(),)+
                    _ => false,
                }
            }

            /// Whether this layer sets `key`.
            #[must_use]
            pub fn is_set(&self, key: &str) -> bool {
                match key {
                    $(stringify!($field) => self.$field.is_some(),)+
                    _ => false,
                }
            }

            /// The keys this layer sets, in [`KEYS`] order.
            #[must_use]
            pub fn set_keys(&self) -> Vec<&'static str> {
                let mut keys = Vec::new();
                $(if self.$field.is_some() {
                    keys.push(stringify!($field));
                })+
                keys
            }
        }
    };
}

terminal_settings! {
    // ---- Font
    /// Font family; empty uses the bundled JetBrains Mono.
    font_family: String = String::new(), check font_name;
    /// Families tried, in order, for characters the main font lacks (before the system's own
    /// fallback).
    font_fallbacks: Vec<String> = Vec::new(), check font_names;
    /// Size in points.
    font_size: f64 = 11.0, check font_size;
    /// Weight of normal text (100 to 900; 400 is regular).
    font_weight: u32 = 400, check font_weight;
    /// Weight of bold text (700 is bold).
    font_weight_bold: u32 = 700, check font_weight;
    /// Draw italic text in italics (otherwise upright).
    font_italic: bool = true, check any;
    /// Line height as a multiple of the font's own.
    line_height: f64 = 1.0, check line_height;
    /// Extra space between characters, in logical pixels (negative tightens).
    letter_spacing: f64 = 0.0, check letter_spacing;
    /// Smooth glyph edges.
    antialiasing: bool = true, check any;
    /// Hinting.
    hinting: Hinting = Hinting::Default, check any;
    /// Programming ligatures (experimental, ADR 0015).
    ligatures: bool = false, check any;

    // ---- Colors
    /// Theme while the app uses its dark scheme.
    theme_dark: String = DEFAULT_DARK_THEME.to_owned(), check theme_id;
    /// Theme while the app uses its light scheme.
    theme_light: String = DEFAULT_LIGHT_THEME.to_owned(), check theme_id;
    /// Bold text in one of the 8 normal colors uses the bright variant.
    bold_is_bright: bool = true, check any;
    /// Least contrast between text and its background (1 turns the adjustment off, 4.5 is
    /// WCAG AA); text below it is lightened or darkened.
    minimum_contrast: f64 = 1.0, check contrast;
    /// Cursor color.
    cursor_color: ThemeColor = ThemeColor::Theme, check any;
    /// Color of the character under a block cursor.
    cursor_text_color: ThemeColor = ThemeColor::Theme, check any;
    /// Background of selected text.
    selection_background: ThemeColor = ThemeColor::Theme, check any;
    /// Color of selected text ("theme" keeps each character's own color when the theme has
    /// none).
    selection_foreground: ThemeColor = ThemeColor::Theme, check any;

    // ---- Cursor
    /// Cursor shape.
    cursor_shape: CursorStyle = CursorStyle::Block, check any;
    /// The cursor blinks.
    cursor_blinking: bool = false, check any;
    /// A block cursor turns hollow when the terminal loses the focus.
    cursor_hollow_unfocused: bool = true, check any;

    // ---- Window
    /// Space around the text, in logical pixels.
    padding: u32 = 8, check padding;
    /// Opacity of the default background (1 is opaque).
    background_opacity: f64 = 1.0, check unit;
    /// Background image; empty for none.
    background_image: String = String::new(), check path;
    /// How much the background color covers the image (0 shows the image as it is).
    background_image_dim: f64 = 0.8, check unit;
    /// How the image fills the terminal.
    background_image_fit: ImageFit = ImageFit::Cover, check any;

    // ---- Scrolling
    /// Lines of history.
    scrollback_lines: u32 = 10_000, check scrollback;
    /// Multiplies the scroll distance of the wheel and the touchpad.
    scroll_speed: f64 = 1.0, check scroll_speed;
    /// Animate wheel scrolling line by line.
    smooth_scroll: bool = false, check any;

    // ---- Selection and clipboard
    /// Characters that end a word for double-click selection.
    word_separators: String = DEFAULT_WORD_SEPARATORS.to_owned(), check separators;
    /// Copy the selection to the clipboard as soon as it is made.
    copy_on_select: bool = false, check any;
    /// What a right click does.
    right_click: RightClick = RightClick::Menu, check any;
    /// Linux: selecting text sets the primary selection, and the middle button pastes it.
    primary_selection: bool = true, check any;
    /// Clipboard access for programs through OSC 52 (opt-in).
    osc52: Osc52Access = Osc52Access::Off, check any;

    // ---- Behavior
    /// What the bell does.
    bell: BellStyle = BellStyle::Visual, check any;
    /// `TERM` for new terminals.
    term: String = DEFAULT_TERM.to_owned(), check term;
    /// What Backspace sends.
    backspace: BackspaceKey = BackspaceKey::Del, check any;
    /// What Delete sends.
    delete: DeleteKey = DeleteKey::Vt220, check any;
    /// Alt acts as Meta (it sends ESC before the key).
    alt_as_meta: bool = true, check any;
    /// Character encoding of the program's input and output.
    encoding: String = "UTF-8".to_owned(), check encoding;
    /// Reply to ENQ (Ctrl+E); empty sends nothing.
    answerback: String = String::new(), check answerback;
    /// Pause between the lines of a paste, in milliseconds (for slow devices).
    paste_line_delay_ms: u32 = 0, check paste_delay;
    /// Keyword highlighting rule sets, by id (PLAN §6.5).
    highlight_sets: Vec<String> = Vec::new(), check set_ids;
}

impl TerminalOverrides {
    /// Whether this layer sets nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.set_keys().is_empty()
    }

    /// Reads a `[terminal]` table. Invalid values are skipped with a warning named
    /// `{prefix}.{key}`; unknown keys are returned untouched (to be written back).
    #[must_use]
    pub fn from_table(table: &Table, prefix: &str) -> (Self, Vec<Warning>, Table) {
        let mut overrides = Self::default();
        let mut warnings = Vec::new();
        let mut unknown = Table::new();
        for (key, value) in table {
            match overrides.set(key, value) {
                Ok(()) => {}
                Err(SetError::UnknownKey(_)) => {
                    warnings.push(Warning {
                        key: format!("{prefix}.{key}"),
                        message: "unknown setting, not used by this version (kept in the file)"
                            .to_owned(),
                    });
                    unknown.insert(key.clone(), value.clone());
                }
                Err(SetError::Invalid { reason, .. }) => warnings.push(Warning {
                    key: format!("{prefix}.{key}"),
                    message: format!("{reason}; inherited instead"),
                }),
            }
        }
        (overrides, warnings, unknown)
    }

    /// The options this layer sets, as a TOML table (keys in alphabetical order when written).
    #[must_use]
    pub fn to_table(&self) -> Table {
        let mut table = Table::new();
        for key in self.set_keys() {
            if let Some(value) = self.get(key) {
                table.insert(key.to_owned(), value);
            }
        }
        table
    }
}

/// Applies `layers` in order over the built-in defaults.
#[must_use]
pub fn resolve<'a>(layers: impl IntoIterator<Item = &'a TerminalOverrides>) -> TerminalSettings {
    let mut settings = TerminalSettings::default();
    for layer in layers {
        layer.apply_to(&mut settings);
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> Table {
        text.parse().unwrap()
    }

    #[test]
    fn every_key_round_trips_through_toml() {
        let defaults = TerminalSettings::default();
        let mut layer = TerminalOverrides::default();
        for key in KEYS {
            let value = defaults.get(key).unwrap();
            layer.set(key, &value).unwrap();
            assert_eq!(layer.get(key), Some(value), "{key}");
        }
        assert_eq!(layer.set_keys(), KEYS);
        assert_eq!(resolve([&layer]), defaults);

        let (read, warnings, unknown) = TerminalOverrides::from_table(&layer.to_table(), "t");
        assert_eq!(read, layer);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(unknown.is_empty());
    }

    #[test]
    fn layers_apply_in_order_and_unset_keys_inherit() {
        let (global, _, _) = TerminalOverrides::from_table(
            &table("font_size = 13\ncursor_shape = \"beam\"\ntheme_dark = \"dracula\""),
            "global",
        );
        let (group, _, _) =
            TerminalOverrides::from_table(&table("font_size = 15\nbell = \"none\""), "group");
        let (host, _, _) = TerminalOverrides::from_table(&table("font_size = 9.5"), "host");
        let (tab, _, _) =
            TerminalOverrides::from_table(&table("cursor_shape = \"underline\""), "tab");

        let settings = resolve([&global, &group, &host, &tab]);
        assert_eq!(settings.font_size, 9.5, "the host wins over the group");
        assert_eq!(settings.bell, BellStyle::None, "from the group");
        assert_eq!(
            settings.cursor_shape,
            CursorStyle::Underline,
            "from the tab"
        );
        assert_eq!(settings.theme_dark, "dracula", "from the global profile");
        assert_eq!(
            settings.term, DEFAULT_TERM,
            "nobody sets it: built-in default"
        );

        let only_global = resolve([&global]);
        assert_eq!(only_global.font_size, 13.0);
        assert_eq!(only_global.bell, BellStyle::Visual);
    }

    #[test]
    fn invalid_values_are_skipped_with_warnings_and_unknown_keys_kept() {
        let (layer, warnings, unknown) = TerminalOverrides::from_table(
            &table(
                r##"
                font_size = 200
                font_weight = 450
                cursor_shape = "triangle"
                term = "xterm 256"
                answerback = "hi\u0007"
                encoding = "utf-8"
                selection_background = "#12345"
                cursor_color = "#E6B450"
                future_option = 3
                "##,
            ),
            "profiles.default",
        );
        assert_eq!(layer.font_weight, Some(450));
        assert_eq!(
            layer.cursor_color,
            Some(ThemeColor::Custom(Rgba::rgb(0xE6, 0xB4, 0x50)))
        );
        assert_eq!(layer.set_keys(), ["font_weight", "cursor_color"]);
        let joined: Vec<String> = warnings.iter().map(ToString::to_string).collect();
        let joined = joined.join("\n");
        for key in [
            "font_size",
            "cursor_shape",
            "term",
            "answerback",
            "encoding",
            "selection_background",
            "future_option",
        ] {
            assert!(
                joined.contains(&format!("profiles.default.{key}")),
                "{key}:\n{joined}"
            );
        }
        assert!(joined.contains("write `UTF-8`"), "{joined}");
        assert_eq!(unknown["future_option"].as_integer(), Some(3));
    }

    #[test]
    fn clear_makes_a_key_inherit_again() {
        let mut layer = TerminalOverrides::default();
        layer.set("bell", &Value::String("sound".into())).unwrap();
        assert!(layer.is_set("bell"));
        assert!(layer.clear("bell"));
        assert!(!layer.clear("bell"));
        assert!(layer.is_empty());
        assert_eq!(
            layer.set("nope", &Value::Boolean(true)),
            Err(SetError::UnknownKey("nope".into()))
        );
    }

    #[test]
    fn merge_keeps_the_later_layer() {
        let mut base = TerminalOverrides {
            font_size: Some(12.0),
            bell: Some(BellStyle::Sound),
            ..TerminalOverrides::default()
        };
        let later = TerminalOverrides {
            font_size: Some(14.0),
            ..TerminalOverrides::default()
        };
        base.merge(&later);
        assert_eq!(base.font_size, Some(14.0));
        assert_eq!(base.bell, Some(BellStyle::Sound));
    }

    #[test]
    fn ids_terms_and_encodings() {
        for ok in [
            "dracula",
            "catppuccin-mocha",
            "a",
            "tokyo_night.storm",
            "0x",
        ] {
            assert!(valid_id(ok), "{ok}");
        }
        for bad in ["", "Dracula", "-x", "a b", "../x", &"x".repeat(65)] {
            assert!(!valid_id(bad), "{bad}");
        }
        for ok in ["xterm-256color", "vt100", "screen.xterm-256color", "rxvt+x"] {
            assert!(valid_term(ok), "{ok}");
        }
        for bad in ["", "xterm 256", "x\ny", "é"] {
            assert!(!valid_term(bad), "{bad}");
        }
        assert_eq!(canonical_encoding("shift_jis"), Some("Shift_JIS"));
        assert_eq!(canonical_encoding("latin1"), None);
        assert_eq!(ENCODINGS[0], "UTF-8");
    }

    #[test]
    fn answerback_is_printable_ascii_only() {
        let mut layer = TerminalOverrides::default();
        assert!(
            layer
                .set("answerback", &Value::String("OpenSesh".into()))
                .is_ok()
        );
        for bad in ["rm -rf ~\r", "\u{1b}[0c", "café"] {
            assert!(
                layer.set("answerback", &Value::String(bad.into())).is_err(),
                "{bad:?}"
            );
        }
        assert_eq!(layer.answerback.as_deref(), Some("OpenSesh"));
    }

    #[test]
    fn choice_values_round_trip() {
        for value in BellStyle::ALL {
            assert_eq!(value.as_str().parse::<BellStyle>().unwrap(), *value);
        }
        assert_eq!("theme".parse::<ThemeColor>().unwrap(), ThemeColor::Theme);
        assert_eq!(ThemeColor::Theme.to_string(), "theme");
        let error = BellStyle::from_toml(&Value::String("loud".into())).unwrap_err();
        assert!(
            error.contains("visual, sound, notification, none"),
            "{error}"
        );
    }
}
