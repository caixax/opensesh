//! Design tokens (PLAN §5.1, §5.2) resolved from the user's appearance settings.
//!
//! The QML `Theme` singleton is a thin view over [`ResolvedTheme`]: every color used by the UI
//! comes from here, so the contrast guarantees can be unit-tested without Qt.
//!
//! Contrast rules (WCAG 2.1, see ADR 0006):
//! - text tokens (`text`, `text_muted`, `accent_fg`) reach 4.5:1 on `bg`, `surface` and `surface2`;
//! - text on filled accent / status colors is computed per color ([`best_text_on`]) instead of
//!   fixed, so any user accent stays readable;
//! - the translucent `selection` keeps `text` and `text_muted` at 4.5:1 and `accent_fg` (the
//!   icon of a selected item) at 3:1 when it is composited on `bg`, `surface` or `surface2`;
//! - non-text UI indicators reach 3:1: `focus_ring` and the status colors on `bg`, `surface` and
//!   `surface2`, `border_strong` on `surface` and `surface2`.

use std::fmt;
use std::str::FromStr;

/// WCAG AA minimum contrast for normal text (SC 1.4.3).
pub const AA_TEXT: f64 = 4.5;

/// WCAG AA minimum contrast for UI components and large text (SC 1.4.11 / 1.4.3).
pub const AA_UI: f64 = 3.0;

/// Default accent ("sesame amber") for dark surfaces (§5.2).
pub const DEFAULT_ACCENT_DARK: Rgba = Rgba::rgb(0xE6, 0xB4, 0x50);

/// Default accent for light surfaces (§5.2).
pub const DEFAULT_ACCENT_LIGHT: Rgba = Rgba::rgb(0xB7, 0x80, 0x0F);

/// Accent presets offered next to the default ("Sesame") accent, in display order: amber,
/// terracotta, rose, lavender, blue, teal and green. The single source for every accent picker.
pub const ACCENT_PRESETS: [Rgba; 7] = [
    Rgba::rgb(0xF2, 0x9E, 0x4C),
    Rgba::rgb(0xE0, 0x7A, 0x5F),
    Rgba::rgb(0xD9, 0x66, 0x7B),
    Rgba::rgb(0xA9, 0x83, 0xD8),
    Rgba::rgb(0x5B, 0x9B, 0xD5),
    Rgba::rgb(0x4D, 0xB6, 0xAC),
    Rgba::rgb(0x7C, 0xB3, 0x42),
];

/// Dark ink used for text on light fills (§5.2 dark `accentText`).
const INK_DARK: Rgba = Rgba::rgb(0x1A, 0x14, 0x06);

/// Light ink used for text on dark fills.
const INK_LIGHT: Rgba = Rgba::rgb(0xFF, 0xFF, 0xFF);

/// A color with 8-bit channels and straight (non-premultiplied) alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel (255 = opaque).
    pub a: u8,
}

/// Error returned when a color string is not `#RRGGBB`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid color `{0}`: expected #RRGGBB")]
pub struct ColorParseError(pub String);

impl Rgba {
    /// Opaque color from its channels.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Same color with another alpha.
    #[must_use]
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// `#RRGGBB` (alpha is ignored).
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// Relative luminance as defined by WCAG 2.x, in `0.0..=1.0` (alpha is ignored).
    #[must_use]
    pub fn relative_luminance(self) -> f64 {
        fn linear(channel: u8) -> f64 {
            let c = f64::from(channel) / 255.0;
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }

    /// Linear interpolation between two opaque colors: `t = 0` gives `self`, `t = 1` gives
    /// `other`. The result is opaque.
    #[must_use]
    pub fn mix(self, other: Self, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| {
            let value = f64::from(a) + (f64::from(b) - f64::from(a)) * t;
            // Rounded and clamped to 0..=255, so the cast can't truncate.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let channel = value.round().clamp(0.0, 255.0) as u8;
            channel
        };
        Self::rgb(
            lerp(self.r, other.r),
            lerp(self.g, other.g),
            lerp(self.b, other.b),
        )
    }

    /// The opaque color seen when this (possibly translucent) color is drawn over `background`:
    /// source-over compositing in 8-bit sRGB, as Qt Quick blends.
    #[must_use]
    pub fn over(self, background: Self) -> Self {
        background.mix(self, f64::from(self.a) / 255.0)
    }
}

impl FromStr for Rgba {
    type Err = ColorParseError;

    /// Parses `#RRGGBB` (case-insensitive).
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let error = || ColorParseError(text.to_owned());
        let hex = text.strip_prefix('#').ok_or_else(error)?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(error());
        }
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| error());
        Ok(Self::rgb(channel(0)?, channel(2)?, channel(4)?))
    }
}

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// WCAG contrast ratio between two opaque colors, in `1.0..=21.0`.
#[must_use]
pub fn contrast_ratio(a: Rgba, b: Rgba) -> f64 {
    let (la, lb) = (a.relative_luminance(), b.relative_luminance());
    let (lighter, darker) = if la >= lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Text color for content drawn on a filled `background`: the house inks when they are readable,
/// pure black or white otherwise. The result always reaches [`AA_TEXT`] (the better of black and
/// white is at least 4.58:1 on any color).
#[must_use]
pub fn best_text_on(background: Rgba) -> Rgba {
    let better = |a: Rgba, b: Rgba| {
        if contrast_ratio(a, background) >= contrast_ratio(b, background) {
            a
        } else {
            b
        }
    };
    let ink = better(INK_DARK, INK_LIGHT);
    if contrast_ratio(ink, background) >= AA_TEXT {
        ink
    } else {
        better(Rgba::rgb(0, 0, 0), Rgba::rgb(0xFF, 0xFF, 0xFF))
    }
}

/// Moves `color` towards `target` in small steps until it reaches `min_ratio` against every
/// color in `against`. Returns `target` if no intermediate step is good enough.
fn ensure_contrast(color: Rgba, target: Rgba, against: &[Rgba], min_ratio: f64) -> Rgba {
    const STEPS: u32 = 40;
    (0..=STEPS)
        .map(|step| color.mix(target, f64::from(step) / f64::from(STEPS)))
        .find(|candidate| {
            against
                .iter()
                .all(|&bg| contrast_ratio(*candidate, bg) >= min_ratio)
        })
        .unwrap_or(target)
}

/// Whether `ink` reaches `min_ratio` on the opaque `fill` produced by compositing. Qt's 8-bit
/// blending may round each channel one unit away from [`Rgba::over`], so the check uses `fill`
/// moved one unit towards `ink` (the direction that lowers the contrast).
fn readable_on_composite(ink: Rgba, fill: Rgba, min_ratio: f64) -> bool {
    let towards_ink = |channel: u8| {
        if ink.relative_luminance() > fill.relative_luminance() {
            channel.saturating_add(1)
        } else {
            channel.saturating_sub(1)
        }
    };
    let worst = Rgba::rgb(
        towards_ink(fill.r),
        towards_ink(fill.g),
        towards_ink(fill.b),
    );
    contrast_ratio(ink, worst) >= min_ratio
}

/// Alpha of the `selection` fill: the scheme's `base_alpha`, lowered until every ink drawn on a
/// selected item stays readable over the fill composited on each of `surfaces`: `text` and
/// `text_muted` at [`AA_TEXT`], and `accent_fg` (the icon of a selected row) at [`AA_UI`].
fn selection_alpha(palette: &Palette, base_alpha: u8, surfaces: &[Rgba]) -> u8 {
    let inks = [
        (palette.text, AA_TEXT),
        (palette.text_muted, AA_TEXT),
        (palette.accent_fg, AA_UI),
    ];
    (0..=base_alpha)
        .rev()
        .find(|&alpha| {
            surfaces.iter().all(|&surface| {
                let fill = palette.accent.with_alpha(alpha).over(surface);
                inks.iter()
                    .all(|&(ink, min_ratio)| readable_on_composite(ink, fill, min_ratio))
            })
        })
        // Alpha 0 leaves the surfaces themselves, which every ink is already readable on.
        .unwrap_or(0)
}

/// Light or dark appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorScheme {
    /// Dark surfaces, light text.
    Dark,
    /// Light surfaces, dark text.
    Light,
}

/// Appearance chosen by the user (§6.1).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    /// Follow the operating system.
    #[default]
    System,
    /// Always dark.
    Dark,
    /// Always light.
    Light,
}

impl ThemeMode {
    /// All values, in UI order.
    pub const ALL: [Self; 3] = [Self::System, Self::Dark, Self::Light];

    /// Stable identifier used in `config.toml` and QML.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    /// Scheme to use given what the operating system reports.
    #[must_use]
    pub const fn resolve(self, system: ColorScheme) -> ColorScheme {
        match self {
            Self::System => system,
            Self::Dark => ColorScheme::Dark,
            Self::Light => ColorScheme::Light,
        }
    }
}

impl FromStr for ThemeMode {
    type Err = UnknownValue;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.as_str() == text)
            .ok_or_else(|| UnknownValue(text.to_owned()))
    }
}

/// UI density (§5.1).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    /// Roomy controls (default).
    #[default]
    Comfortable,
    /// Smaller controls and rows.
    Compact,
}

impl Density {
    /// All values, in UI order.
    pub const ALL: [Self; 2] = [Self::Comfortable, Self::Compact];

    /// Stable identifier used in `config.toml` and QML.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Compact => "compact",
        }
    }
}

impl FromStr for Density {
    type Err = UnknownValue;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|density| density.as_str() == text)
            .ok_or_else(|| UnknownValue(text.to_owned()))
    }
}

/// Error for a string that is not one of the allowed identifiers.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown value `{0}`")]
pub struct UnknownValue(pub String);

/// Everything the theme depends on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeInputs {
    /// User choice.
    pub mode: ThemeMode,
    /// What the operating system currently reports.
    pub system_scheme: ColorScheme,
    /// Custom accent; `None` uses the §5.2 default of the active scheme.
    pub accent: Option<Rgba>,
    /// UI density.
    pub density: Density,
    /// User zoom for fonts and metrics, clamped to [`UI_SCALE_RANGE`].
    pub ui_scale: f64,
    /// Disable animations.
    pub reduce_motion: bool,
}

impl Default for ThemeInputs {
    fn default() -> Self {
        Self {
            mode: ThemeMode::System,
            system_scheme: ColorScheme::Dark,
            accent: None,
            density: Density::Comfortable,
            ui_scale: 1.0,
            reduce_motion: false,
        }
    }
}

/// Allowed UI scale factors.
pub const UI_SCALE_RANGE: std::ops::RangeInclusive<f64> = 0.8..=1.5;

/// Every color token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Window background.
    pub bg: Rgba,
    /// Cards, panels, popups.
    pub surface: Rgba,
    /// Raised or alternate surfaces (inputs, hover rows, rail).
    pub surface2: Rgba,
    /// Hairline separators (decorative, low contrast by design).
    pub border: Rgba,
    /// Outlines that identify controls (inputs, checkboxes): 3:1 on `surface`.
    pub border_strong: Rgba,
    /// Primary text.
    pub text: Rgba,
    /// Secondary text.
    pub text_muted: Rgba,
    /// Disabled text and icons (exempt from contrast rules).
    pub text_disabled: Rgba,
    /// Accent fill (sesame amber by default).
    pub accent: Rgba,
    /// Text and icons on an `accent` fill.
    pub accent_text: Rgba,
    /// Accent usable as text/icon color on `bg`, `surface` and `surface2`.
    pub accent_fg: Rgba,
    /// Success fill / indicator.
    pub success: Rgba,
    /// Warning fill / indicator.
    pub warning: Rgba,
    /// Danger fill / indicator.
    pub danger: Rgba,
    /// Informational fill / indicator.
    pub info: Rgba,
    /// Keyboard focus indicator.
    pub focus_ring: Rgba,
    /// Overlay drawn on hovered items.
    pub hover: Rgba,
    /// Overlay drawn on pressed items.
    pub pressed: Rgba,
    /// Text selection and selected rows: the accent, translucent. Its alpha keeps `text`,
    /// `text_muted` and `accent_fg` (as an icon) readable on it over every surface.
    pub selection: Rgba,
    /// Dimming layer behind modal dialogs.
    pub scrim: Rgba,
}

/// Sizes that depend on density and UI scale (all in logical pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// Base spacing unit (4 px scale, §5.1).
    pub spacing: f64,
    /// Height of buttons, text fields, combo boxes.
    pub control_height: f64,
    /// Height of small controls (tags, compact buttons).
    pub control_height_small: f64,
    /// Height of list rows.
    pub row_height: f64,
    /// Horizontal padding inside controls.
    pub control_padding: f64,
    /// Default icon size.
    pub icon_size: f64,
    /// Small icon size.
    pub icon_size_small: f64,
    /// Height of the title bar / tab row.
    pub title_bar_height: f64,
    /// Width of the rail without labels.
    pub rail_width: f64,
    /// Width of the rail with labels.
    pub rail_width_labels: f64,
    /// Height of the status bar.
    pub status_bar_height: f64,
    /// Corner radius of cards and dialogs (§5.1: 8 px).
    pub radius_card: f64,
    /// Corner radius of controls (§5.1: 6 px).
    pub radius_control: f64,
    /// Corner radius of small elements (tags, badges).
    pub radius_small: f64,
    /// Font size of captions and badges.
    pub font_size_small: f64,
    /// Body text size.
    pub font_size: f64,
    /// Section titles.
    pub font_size_large: f64,
    /// Page titles.
    pub font_size_title: f64,
    /// Short animations (hover, press), 0 with reduced motion.
    pub duration_fast: f64,
    /// Regular animations (open/close), 0 with reduced motion.
    pub duration_normal: f64,
}

/// Result of resolving [`ThemeInputs`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedTheme {
    /// Active scheme.
    pub scheme: ColorScheme,
    /// Colors.
    pub palette: Palette,
    /// Sizes.
    pub metrics: Metrics,
    /// Whether the chosen accent is hard to see as a fill on the background (< 3:1).
    pub accent_low_contrast: bool,
}

/// Base colors of the dark scheme (§5.2).
fn dark_base() -> Palette {
    let bg = Rgba::rgb(0x0F, 0x11, 0x15);
    let text = Rgba::rgb(0xE6, 0xE8, 0xEE);
    Palette {
        bg,
        surface: Rgba::rgb(0x16, 0x19, 0x20),
        surface2: Rgba::rgb(0x1D, 0x21, 0x2A),
        border: Rgba::rgb(0x2A, 0x2F, 0x3A),
        border_strong: Rgba::rgb(0x2A, 0x2F, 0x3A),
        text,
        text_muted: Rgba::rgb(0x9A, 0xA3, 0xB2),
        text_disabled: text,
        accent: DEFAULT_ACCENT_DARK,
        accent_text: INK_DARK,
        accent_fg: DEFAULT_ACCENT_DARK,
        success: Rgba::rgb(0x5F, 0xD3, 0x8D),
        warning: Rgba::rgb(0xF2, 0xC1, 0x4E),
        danger: Rgba::rgb(0xF2, 0x66, 0x7A),
        info: Rgba::rgb(0x6C, 0xB6, 0xFF),
        focus_ring: DEFAULT_ACCENT_DARK,
        hover: text.with_alpha(0x10),
        pressed: text.with_alpha(0x1C),
        selection: DEFAULT_ACCENT_DARK.with_alpha(0x55),
        scrim: Rgba::rgb(0, 0, 0).with_alpha(0x8C),
    }
}

/// Base colors of the light scheme (§5.2).
fn light_base() -> Palette {
    let bg = Rgba::rgb(0xF6, 0xF5, 0xF2);
    let text = Rgba::rgb(0x1B, 0x1D, 0x22);
    Palette {
        bg,
        surface: Rgba::rgb(0xFF, 0xFF, 0xFF),
        surface2: Rgba::rgb(0xEF, 0xED, 0xE8),
        border: Rgba::rgb(0xE0, 0xDC, 0xD3),
        border_strong: Rgba::rgb(0xE0, 0xDC, 0xD3),
        text,
        text_muted: Rgba::rgb(0x5D, 0x64, 0x70),
        text_disabled: text,
        accent: DEFAULT_ACCENT_LIGHT,
        accent_text: Rgba::rgb(0xFF, 0xFF, 0xFF),
        accent_fg: DEFAULT_ACCENT_LIGHT,
        success: Rgba::rgb(0x1E, 0x8F, 0x52),
        warning: Rgba::rgb(0xB7, 0x80, 0x0F),
        danger: Rgba::rgb(0xC8, 0x37, 0x4D),
        info: Rgba::rgb(0x2B, 0x6C, 0xB0),
        focus_ring: DEFAULT_ACCENT_LIGHT,
        hover: text.with_alpha(0x0D),
        pressed: text.with_alpha(0x17),
        selection: DEFAULT_ACCENT_LIGHT.with_alpha(0x40),
        scrim: Rgba::rgb(0, 0, 0).with_alpha(0x52),
    }
}

/// Resolves the full theme.
#[must_use]
pub fn resolve(inputs: &ThemeInputs) -> ResolvedTheme {
    let scheme = inputs.mode.resolve(inputs.system_scheme);
    let mut palette = match scheme {
        ColorScheme::Dark => dark_base(),
        ColorScheme::Light => light_base(),
    };
    let surfaces = [palette.bg, palette.surface, palette.surface2];
    // Colors used as text move towards the text color until they are readable.
    let readable_target = palette.text;

    if let Some(accent) = inputs.accent {
        palette.accent = accent.with_alpha(255);
    }
    palette.accent_text = best_text_on(palette.accent);
    palette.accent_fg = ensure_contrast(palette.accent, readable_target, &surfaces, AA_TEXT);
    palette.focus_ring = palette.accent_fg;
    // The base palette's selection alpha is the strongest tint allowed.
    let alpha = selection_alpha(&palette, palette.selection.a, &surfaces);
    palette.selection = palette.accent.with_alpha(alpha);
    palette.border_strong = ensure_contrast(
        palette.border,
        readable_target,
        &[palette.surface, palette.surface2],
        AA_UI,
    );
    palette.text_disabled = palette.text_muted.mix(palette.surface, 0.45);
    for status in [
        &mut palette.success,
        &mut palette.warning,
        &mut palette.danger,
        &mut palette.info,
    ] {
        // Status icons and outlines are drawn on any surface (e.g. notices on `surface2`).
        *status = ensure_contrast(*status, readable_target, &surfaces, AA_UI);
    }

    let accent_low_contrast = contrast_ratio(palette.accent, palette.bg) < AA_UI;
    ResolvedTheme {
        scheme,
        palette,
        metrics: metrics(inputs.density, inputs.ui_scale, inputs.reduce_motion),
        accent_low_contrast,
    }
}

/// Sizes for a density, UI scale and motion preference.
#[must_use]
pub fn metrics(density: Density, ui_scale: f64, reduce_motion: bool) -> Metrics {
    let scale = if ui_scale.is_finite() {
        ui_scale.clamp(*UI_SCALE_RANGE.start(), *UI_SCALE_RANGE.end())
    } else {
        1.0
    };
    let s = |value: f64| (value * scale).round();
    let (control, control_small, row, padding, icon, icon_small, title, rail, status, font) =
        match density {
            Density::Comfortable => (36.0, 28.0, 40.0, 12.0, 18.0, 16.0, 42.0, 60.0, 26.0, 14.0),
            Density::Compact => (30.0, 24.0, 32.0, 8.0, 16.0, 14.0, 36.0, 48.0, 22.0, 13.0),
        };
    let (fast, normal) = if reduce_motion {
        (0.0, 0.0)
    } else {
        (120.0, 180.0)
    };
    Metrics {
        spacing: s(4.0),
        control_height: s(control),
        control_height_small: s(control_small),
        row_height: s(row),
        control_padding: s(padding),
        icon_size: s(icon),
        icon_size_small: s(icon_small),
        title_bar_height: s(title),
        rail_width: s(rail),
        rail_width_labels: s(rail + 132.0),
        status_bar_height: s(status),
        radius_card: s(8.0),
        radius_control: s(6.0),
        radius_small: s(4.0),
        font_size_small: s(font - 2.0),
        font_size: s(font),
        font_size_large: s(font + 3.0),
        font_size_title: s(font + 8.0),
        duration_fast: fast,
        duration_normal: normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themes() -> Vec<ResolvedTheme> {
        let mut out = Vec::new();
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            for density in Density::ALL {
                out.push(resolve(&ThemeInputs {
                    mode,
                    density,
                    ..ThemeInputs::default()
                }));
            }
        }
        out
    }

    fn assert_contrast(fg: Rgba, bg: Rgba, min: f64, what: &str) {
        let ratio = contrast_ratio(fg, bg);
        assert!(
            ratio >= min,
            "{what}: {fg} on {bg} is {ratio:.2}:1, needs {min}:1"
        );
    }

    /// Every ink drawn on a selected item stays readable on the selection composited over each
    /// surface, even if the compositor rounds any channel one unit away from `Rgba::over`.
    fn assert_selection_readable(p: &Palette, what: &str) {
        let inks = [
            ("text", p.text, AA_TEXT),
            ("text_muted", p.text_muted, AA_TEXT),
            ("accent_fg", p.accent_fg, AA_UI),
        ];
        for surface in [p.bg, p.surface, p.surface2] {
            let fill = p.selection.over(surface);
            let nudge = |channel: u8, delta: i16| {
                u8::try_from((i16::from(channel) + delta).clamp(0, 255)).unwrap()
            };
            for (dr, dg, db) in
                (-1..=1).flat_map(|r| (-1..=1).flat_map(move |g| (-1..=1).map(move |b| (r, g, b))))
            {
                let rounded = Rgba::rgb(nudge(fill.r, dr), nudge(fill.g, dg), nudge(fill.b, db));
                for (name, ink, min) in inks {
                    assert_contrast(
                        ink,
                        rounded,
                        min,
                        &format!("{what}: {name} on selection over {surface}"),
                    );
                }
            }
        }
    }

    #[test]
    fn contrast_matches_known_values() {
        let black = Rgba::rgb(0, 0, 0);
        let white = Rgba::rgb(255, 255, 255);
        assert!((contrast_ratio(black, white) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio(white, white) - 1.0).abs() < 1e-9);
        // The §5.2 light accent with white text fails AA: the reason accent_text is computed.
        let ratio = contrast_ratio(white, DEFAULT_ACCENT_LIGHT);
        assert!((3.3..3.6).contains(&ratio), "{ratio}");
    }

    #[test]
    fn hex_round_trips_and_rejects_garbage() {
        let color: Rgba = "#e6b450".parse().unwrap();
        assert_eq!(color, DEFAULT_ACCENT_DARK);
        assert_eq!(color.to_hex(), "#E6B450");
        for bad in [
            "", "E6B450", "#E6B45", "#E6B4500", "#GGGGGG", "#e6b45z", "#é6b45",
        ] {
            assert!(bad.parse::<Rgba>().is_err(), "{bad}");
        }
    }

    #[test]
    fn text_tokens_reach_aa_on_every_surface() {
        for theme in themes() {
            let p = theme.palette;
            for surface in [p.bg, p.surface, p.surface2] {
                assert_contrast(p.text, surface, AA_TEXT, "text");
                assert_contrast(p.text_muted, surface, AA_TEXT, "text_muted");
                assert_contrast(p.accent_fg, surface, AA_TEXT, "accent_fg");
            }
            assert_contrast(p.accent_text, p.accent, AA_TEXT, "accent_text");
            assert_selection_readable(&p, "default accent");
        }
    }

    #[test]
    fn default_selection_is_readable_and_still_visible() {
        // The alphas documented in ADR 0006.
        for (mode, base, alpha) in [
            (ThemeMode::Dark, dark_base(), 0x28),
            (ThemeMode::Light, light_base(), 0x1D),
        ] {
            let p = resolve(&ThemeInputs {
                mode,
                ..ThemeInputs::default()
            })
            .palette;
            assert_eq!(p.selection.with_alpha(255), p.accent);
            assert_eq!(p.selection.a, alpha, "{mode:?}");
            assert!(p.selection.a <= base.selection.a, "{mode:?}");
            // The review case: muted text on the selected row of a card was 3.24:1 in dark mode.
            let fill = p.selection.over(p.surface);
            assert_contrast(p.text_muted, fill, AA_TEXT, "text_muted on selection");
            // Still a visible tint.
            assert_ne!(fill, p.surface);
        }
    }

    #[test]
    fn ui_indicators_reach_three_to_one() {
        for theme in themes() {
            let p = theme.palette;
            for surface in [p.surface, p.surface2] {
                assert_contrast(p.border_strong, surface, AA_UI, "border_strong");
            }
            for surface in [p.bg, p.surface, p.surface2] {
                assert_contrast(p.focus_ring, surface, AA_UI, "focus_ring");
                for (name, status) in [
                    ("success", p.success),
                    ("warning", p.warning),
                    ("danger", p.danger),
                    ("info", p.info),
                ] {
                    assert_contrast(status, surface, AA_UI, name);
                    assert_contrast(best_text_on(status), status, AA_TEXT, name);
                }
            }
        }
    }

    #[test]
    fn status_colors_keep_the_plan_values_except_the_light_warning() {
        // As documented in ADR 0006: only the light warning is too light for `surface2`.
        for (mode, base) in [
            (ThemeMode::Dark, dark_base()),
            (ThemeMode::Light, light_base()),
        ] {
            let p = resolve(&ThemeInputs {
                mode,
                ..ThemeInputs::default()
            })
            .palette;
            assert_eq!(p.success, base.success, "{mode:?}");
            assert_eq!(p.danger, base.danger, "{mode:?}");
            assert_eq!(p.info, base.info, "{mode:?}");
            let warning = if mode == ThemeMode::Light {
                "#B37E0F"
            } else {
                "#F2C14E"
            };
            assert_eq!(p.warning.to_hex(), warning, "{mode:?}");
        }
    }

    #[test]
    fn dark_defaults_keep_the_plan_values() {
        let p = resolve(&ThemeInputs {
            mode: ThemeMode::Dark,
            ..ThemeInputs::default()
        })
        .palette;
        assert_eq!(p.bg.to_hex(), "#0F1115");
        assert_eq!(p.accent.to_hex(), "#E6B450");
        assert_eq!(p.accent_text.to_hex(), "#1A1406");
        assert_eq!(p.accent_fg, p.accent, "the dark accent is already readable");
    }

    #[test]
    fn light_accent_text_is_computed_for_contrast() {
        let p = resolve(&ThemeInputs {
            mode: ThemeMode::Light,
            ..ThemeInputs::default()
        })
        .palette;
        assert_eq!(p.accent.to_hex(), "#B7800F");
        assert_eq!(p.accent_text, INK_DARK);
        assert_ne!(
            p.accent_fg, p.accent,
            "the light accent needs darkening for text"
        );
    }

    #[test]
    fn any_custom_accent_stays_readable() {
        // 216 samples of the RGB cube, plus every preset of the accent pickers.
        let steps = || (0..=255).step_by(51);
        let grid = steps()
            .flat_map(|r| steps().flat_map(move |g| steps().map(move |b| Rgba::rgb(r, g, b))));
        let accents: Vec<Rgba> = grid.chain(ACCENT_PRESETS).collect();
        assert_eq!(accents.len(), 216 + ACCENT_PRESETS.len());
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            for &accent in &accents {
                let theme = resolve(&ThemeInputs {
                    mode,
                    accent: Some(accent),
                    ..ThemeInputs::default()
                });
                let p = theme.palette;
                assert_eq!(p.accent, accent);
                assert_contrast(p.accent_text, accent, AA_TEXT, "accent_text");
                for surface in [p.bg, p.surface, p.surface2] {
                    assert_contrast(p.accent_fg, surface, AA_TEXT, "accent_fg");
                }
                assert_selection_readable(&p, &format!("{mode:?} accent {accent}"));
                // Lowered as needed, but never to nothing.
                assert_eq!(p.selection.with_alpha(255), accent);
                assert!(
                    p.selection.a >= 0x0C,
                    "{mode:?} {accent}: {:#04X}",
                    p.selection.a
                );
                assert_eq!(
                    theme.accent_low_contrast,
                    contrast_ratio(accent, p.bg) < AA_UI
                );
            }
        }
    }

    #[test]
    fn system_mode_follows_the_os() {
        for (system, expected) in [
            (ColorScheme::Dark, ColorScheme::Dark),
            (ColorScheme::Light, ColorScheme::Light),
        ] {
            let theme = resolve(&ThemeInputs {
                system_scheme: system,
                ..ThemeInputs::default()
            });
            assert_eq!(theme.scheme, expected);
        }
        let forced = resolve(&ThemeInputs {
            mode: ThemeMode::Light,
            system_scheme: ColorScheme::Dark,
            ..ThemeInputs::default()
        });
        assert_eq!(forced.scheme, ColorScheme::Light);
    }

    #[test]
    fn density_scale_and_motion_change_metrics() {
        let comfortable = metrics(Density::Comfortable, 1.0, false);
        let compact = metrics(Density::Compact, 1.0, false);
        assert!(compact.control_height < comfortable.control_height);
        assert!(compact.row_height < comfortable.row_height);
        assert_eq!(comfortable.spacing, 4.0);
        assert_eq!(comfortable.radius_card, 8.0);
        assert_eq!(comfortable.radius_control, 6.0);
        assert_eq!(comfortable.duration_fast, 120.0);
        assert_eq!(comfortable.duration_normal, 180.0);

        let zoomed = metrics(Density::Comfortable, 1.25, false);
        assert_eq!(zoomed.control_height, 45.0);
        let clamped = metrics(Density::Comfortable, 9.0, false);
        assert_eq!(clamped, metrics(Density::Comfortable, 1.5, false));
        let nan = metrics(Density::Comfortable, f64::NAN, false);
        assert_eq!(nan, comfortable);

        let still = metrics(Density::Comfortable, 1.0, true);
        assert_eq!(still.duration_fast, 0.0);
        assert_eq!(still.duration_normal, 0.0);
    }

    #[test]
    fn identifiers_round_trip() {
        for mode in ThemeMode::ALL {
            assert_eq!(mode.as_str().parse::<ThemeMode>(), Ok(mode));
        }
        for density in Density::ALL {
            assert_eq!(density.as_str().parse::<Density>(), Ok(density));
        }
        assert!("sepia".parse::<ThemeMode>().is_err());
    }
}
