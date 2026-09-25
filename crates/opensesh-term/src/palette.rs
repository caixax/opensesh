//! Terminal color schemes and how cell colors are resolved (PLAN §4.3, §6.2).
//!
//! A [`Palette`] holds what a theme defines: default foreground and background, cursor,
//! selection and search colors and the 16 ANSI colors. From it the engine builds the 269-entry
//! xterm table (16 ANSI colors, the 6x6x6 cube, the 24-step gray ramp and the special entries
//! that `alacritty_terminal` indexes). Colors a program sets at run time (OSC 4, 10, 11 and 12)
//! take precedence over the table; the snapshot resolves every cell to plain RGB, so the renderer
//! never sees the palette.

use alacritty_terminal::term::color::{COUNT, Colors};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb as VteRgb};

/// How far dim text (SGR 2) moves from its color toward the cell background (`0.0..=1.0`).
pub const DIM_BLEND: f32 = 0.35;

/// An opaque sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Rgb {
    /// A color from its channels.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// A color from `0xRRGGBB` (higher bits are ignored).
    #[must_use]
    pub const fn from_hex(rgb: u32) -> Self {
        let [_, r, g, b] = rgb.to_be_bytes();
        Self { r, g, b }
    }

    /// The color as `0xAARRGGBB` with full alpha, the format of [`crate::snapshot`].
    #[must_use]
    pub const fn to_argb(self) -> u32 {
        u32::from_be_bytes([0xFF, self.r, self.g, self.b])
    }

    /// Linear interpolation in sRGB: `t = 0` gives `self`, `t = 1` gives `other`.
    #[must_use]
    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| {
            let value = f32::from(a) + (f32::from(b) - f32::from(a)) * t;
            // Rounded and clamped to 0..=255 first, so the cast can't truncate or wrap.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let channel = value.round().clamp(0.0, 255.0) as u8;
            channel
        };
        Self::new(
            lerp(self.r, other.r),
            lerp(self.g, other.g),
            lerp(self.b, other.b),
        )
    }
}

impl From<VteRgb> for Rgb {
    fn from(color: VteRgb) -> Self {
        Self::new(color.r, color.g, color.b)
    }
}

impl From<Rgb> for VteRgb {
    fn from(color: Rgb) -> Self {
        Self {
            r: color.r,
            g: color.g,
            b: color.b,
        }
    }
}

/// A terminal color scheme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// Default text color.
    pub foreground: Rgb,
    /// Default background, also painted behind the padding around the grid.
    pub background: Rgb,
    /// Cursor color.
    pub cursor: Rgb,
    /// Color of the character under a block cursor.
    pub cursor_text: Rgb,
    /// Background of selected cells.
    pub selection_background: Rgb,
    /// Text color of selected cells; `None` keeps each cell's own text color.
    pub selection_foreground: Option<Rgb>,
    /// Background of search matches.
    pub match_background: Rgb,
    /// Text color of search matches.
    pub match_foreground: Rgb,
    /// Background of the current search match.
    pub focused_match_background: Rgb,
    /// Text color of the current search match.
    pub focused_match_foreground: Rgb,
    /// ANSI colors 0-7: black, red, green, yellow, blue, magenta, cyan, white.
    pub normal: [Rgb; 8],
    /// ANSI colors 8-15, the bright variants in the same order.
    pub bright: [Rgb; 8],
    /// Draw bold text that uses one of the 8 normal colors with its bright variant.
    pub bold_is_bright: bool,
}

impl Palette {
    /// OpenSesh Dark, exactly as PLAN §4.3 defines it. Search colors use the amber accent.
    pub const OPENSESH_DARK: Self = Self {
        foreground: Rgb::from_hex(0xD9DEE7),
        background: Rgb::from_hex(0x121419),
        cursor: Rgb::from_hex(0xE6B450),
        cursor_text: Rgb::from_hex(0x121419),
        selection_background: Rgb::from_hex(0x2B3242),
        selection_foreground: None,
        match_background: Rgb::from_hex(0x4A3C1C),
        match_foreground: Rgb::from_hex(0xF2F4F8),
        focused_match_background: Rgb::from_hex(0xE6B450),
        focused_match_foreground: Rgb::from_hex(0x121419),
        normal: [
            Rgb::from_hex(0x1C1F27),
            Rgb::from_hex(0xF07178),
            Rgb::from_hex(0x9BD68A),
            Rgb::from_hex(0xE6C07B),
            Rgb::from_hex(0x73B7F2),
            Rgb::from_hex(0xC9A0F0),
            Rgb::from_hex(0x6ED6D0),
            Rgb::from_hex(0xC8CDD6),
        ],
        bright: [
            Rgb::from_hex(0x4A5263),
            Rgb::from_hex(0xFF8F95),
            Rgb::from_hex(0xB4EBA3),
            Rgb::from_hex(0xF2D39A),
            Rgb::from_hex(0x9ACCFA),
            Rgb::from_hex(0xDDBDFB),
            Rgb::from_hex(0x94E8E3),
            Rgb::from_hex(0xF2F4F8),
        ],
        bold_is_bright: true,
    };

    /// OpenSesh Light: a warm paper background close to the app's light surfaces, the light
    /// amber accent for the cursor and search, and ANSI colors dark enough for 4.5:1 text
    /// contrast (the normal ones, "white" included) or 3:1 (the bright ones, used for bold).
    pub const OPENSESH_LIGHT: Self = Self {
        foreground: Rgb::from_hex(0x1F232B),
        background: Rgb::from_hex(0xFAF9F5),
        cursor: Rgb::from_hex(0xB7800F),
        cursor_text: Rgb::from_hex(0xFFFFFF),
        selection_background: Rgb::from_hex(0xE6DAB8),
        selection_foreground: None,
        match_background: Rgb::from_hex(0xF3E2AE),
        match_foreground: Rgb::from_hex(0x1F232B),
        focused_match_background: Rgb::from_hex(0xD99A1F),
        focused_match_foreground: Rgb::from_hex(0x1F232B),
        normal: [
            Rgb::from_hex(0x1F232B),
            Rgb::from_hex(0xB3261E),
            Rgb::from_hex(0x2A6E2F),
            Rgb::from_hex(0x855C00),
            Rgb::from_hex(0x1F5FBF),
            Rgb::from_hex(0x8E3FB5),
            Rgb::from_hex(0x0B6E78),
            Rgb::from_hex(0x5F6672),
        ],
        bright: [
            Rgb::from_hex(0x6B7280),
            Rgb::from_hex(0xD2453B),
            Rgb::from_hex(0x3D8C42),
            Rgb::from_hex(0xA77A0C),
            Rgb::from_hex(0x3A7BD5),
            Rgb::from_hex(0xA95CCF),
            Rgb::from_hex(0x178A94),
            Rgb::from_hex(0x7D8491),
        ],
        bold_is_bright: true,
    };
}

impl Default for Palette {
    fn default() -> Self {
        Self::OPENSESH_DARK
    }
}

/// The 269 theme colors `alacritty_terminal` indexes (see `alacritty_terminal::term::color`),
/// built from a [`Palette`].
#[derive(Debug, Clone)]
pub(crate) struct ColorTable {
    colors: [Rgb; COUNT],
    bold_is_bright: bool,
}

impl ColorTable {
    /// Fills the xterm table: ANSI colors, 6x6x6 cube (`0, 95, 135, 175, 215, 255`), gray ramp
    /// (`8, 18, ..., 238`), then foreground, background, cursor, the dim ANSI colors, bright
    /// foreground and dim foreground.
    pub(crate) fn new(palette: &Palette) -> Self {
        let mut colors = [palette.foreground; COUNT];
        for (index, color) in palette.normal.iter().chain(&palette.bright).enumerate() {
            colors[index] = *color;
        }
        let level = |step: usize| -> u8 {
            // step < 6, so the value is at most 255.
            u8::try_from(if step == 0 { 0 } else { step * 40 + 55 }).unwrap_or(u8::MAX)
        };
        for (cube, color) in colors[16..232].iter_mut().enumerate() {
            *color = Rgb::new(level(cube / 36), level(cube / 6 % 6), level(cube % 6));
        }
        for (step, color) in colors[232..256].iter_mut().enumerate() {
            let gray = u8::try_from(step * 10 + 8).unwrap_or(u8::MAX);
            *color = Rgb::new(gray, gray, gray);
        }
        colors[NamedColor::Foreground as usize] = palette.foreground;
        colors[NamedColor::Background as usize] = palette.background;
        colors[NamedColor::Cursor as usize] = palette.cursor;
        for (offset, color) in palette.normal.iter().enumerate() {
            colors[NamedColor::DimBlack as usize + offset] =
                color.mix(palette.background, DIM_BLEND);
        }
        colors[NamedColor::BrightForeground as usize] = palette.foreground;
        colors[NamedColor::DimForeground as usize] =
            palette.foreground.mix(palette.background, DIM_BLEND);
        Self {
            colors,
            bold_is_bright: palette.bold_is_bright,
        }
    }

    /// Theme color at `index`, unless the program overrode it (OSC 4/10/11/12).
    pub(crate) fn get(&self, overrides: &Colors, index: usize) -> Rgb {
        if index >= COUNT {
            return self.colors[NamedColor::Foreground as usize];
        }
        overrides[index].map_or(self.colors[index], Rgb::from)
    }

    /// A cell's text color. Bold text in one of the 8 normal colors uses its bright variant
    /// when the palette asks for it; dim is applied by the caller (it needs the background).
    pub(crate) fn foreground(&self, overrides: &Colors, color: Color, bold: bool) -> Rgb {
        let brighten = bold && self.bold_is_bright;
        match color {
            Color::Spec(rgb) => rgb.into(),
            Color::Named(named) => {
                let index = named as usize;
                let index = if brighten && index < 8 {
                    index + 8
                } else {
                    index
                };
                self.get(overrides, index)
            }
            Color::Indexed(index) => {
                let index = usize::from(index);
                let index = if brighten && index < 8 {
                    index + 8
                } else {
                    index
                };
                self.get(overrides, index)
            }
        }
    }

    /// A cell's background color.
    pub(crate) fn background(&self, overrides: &Colors, color: Color) -> Rgb {
        match color {
            Color::Spec(rgb) => rgb.into(),
            Color::Named(named) => self.get(overrides, named as usize),
            Color::Indexed(index) => self.get(overrides, usize::from(index)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opensesh_core::theme::{AA_TEXT, AA_UI, Rgba, contrast_ratio};

    fn contrast(a: Rgb, b: Rgb) -> f64 {
        contrast_ratio(Rgba::rgb(a.r, a.g, a.b), Rgba::rgb(b.r, b.g, b.b))
    }

    #[test]
    fn dark_palette_is_plan_4_3() {
        let dark = Palette::OPENSESH_DARK;
        assert_eq!(dark.foreground, Rgb::from_hex(0xD9DEE7));
        assert_eq!(dark.background, Rgb::from_hex(0x121419));
        assert_eq!(dark.cursor, Rgb::from_hex(0xE6B450));
        assert_eq!(dark.cursor_text, Rgb::from_hex(0x121419));
        assert_eq!(dark.selection_background, Rgb::from_hex(0x2B3242));
        let normal = [
            0x1C1F27, 0xF07178, 0x9BD68A, 0xE6C07B, 0x73B7F2, 0xC9A0F0, 0x6ED6D0, 0xC8CDD6,
        ];
        let bright = [
            0x4A5263, 0xFF8F95, 0xB4EBA3, 0xF2D39A, 0x9ACCFA, 0xDDBDFB, 0x94E8E3, 0xF2F4F8,
        ];
        assert_eq!(dark.normal, normal.map(Rgb::from_hex));
        assert_eq!(dark.bright, bright.map(Rgb::from_hex));
        assert_eq!(Palette::default(), dark);
    }

    /// Text colors that must stay readable in both palettes (WCAG AA, 4.5:1).
    fn assert_readable(palette: &Palette, name: &str) {
        let bg = palette.background;
        let fg_ratio = contrast(palette.foreground, bg);
        assert!(fg_ratio >= AA_TEXT, "{name}: foreground {fg_ratio:.2}");
        for (index, color) in palette.normal.iter().enumerate().skip(1) {
            let ratio = contrast(*color, bg);
            assert!(ratio >= AA_TEXT, "{name}: normal color {index} {ratio:.2}");
        }
        let pairs = [
            (
                "text on selection",
                palette.foreground,
                palette.selection_background,
            ),
            ("match", palette.match_foreground, palette.match_background),
            (
                "focused match",
                palette.focused_match_foreground,
                palette.focused_match_background,
            ),
            ("cursor text", palette.cursor_text, palette.cursor),
        ];
        for (what, fg, bg) in pairs {
            let ratio = contrast(fg, bg);
            // The cursor's own text sits on a 1-cell block: large-text / UI rule.
            let minimum = if what == "cursor text" {
                AA_UI
            } else {
                AA_TEXT
            };
            assert!(ratio >= minimum, "{name}: {what} {ratio:.2}");
        }
        let cursor = contrast(palette.cursor, bg);
        assert!(cursor >= AA_UI, "{name}: cursor on background {cursor:.2}");
    }

    #[test]
    fn dark_palette_contrast() {
        assert_readable(&Palette::OPENSESH_DARK, "dark");
    }

    #[test]
    fn light_palette_contrast() {
        let light = Palette::OPENSESH_LIGHT;
        assert_readable(&light, "light");
        // Bright colors are what bold text uses (bold-as-bright): keep them at 3:1 at least.
        for (index, color) in light.bright.iter().enumerate() {
            let ratio = contrast(*color, light.background);
            assert!(ratio >= AA_UI, "light: bright color {index} {ratio:.2}");
        }
    }

    #[test]
    fn xterm_cube_and_gray_ramp() {
        let table = ColorTable::new(&Palette::OPENSESH_DARK);
        let none = Colors::default();
        assert_eq!(table.get(&none, 16), Rgb::new(0, 0, 0));
        assert_eq!(table.get(&none, 196), Rgb::new(255, 0, 0));
        assert_eq!(table.get(&none, 21), Rgb::new(0, 0, 255));
        assert_eq!(table.get(&none, 110), Rgb::new(135, 175, 215));
        assert_eq!(table.get(&none, 231), Rgb::new(255, 255, 255));
        assert_eq!(table.get(&none, 232), Rgb::new(8, 8, 8));
        assert_eq!(table.get(&none, 255), Rgb::new(238, 238, 238));
        assert_eq!(table.get(&none, 1), Rgb::from_hex(0xF07178));
        assert_eq!(table.get(&none, 9), Rgb::from_hex(0xFF8F95));
        assert_eq!(
            table.get(&none, NamedColor::Background as usize),
            Rgb::from_hex(0x121419)
        );
    }

    #[test]
    fn program_overrides_win() {
        let table = ColorTable::new(&Palette::OPENSESH_DARK);
        let mut overrides = Colors::default();
        overrides[1] = Some(VteRgb { r: 1, g: 2, b: 3 });
        overrides[NamedColor::Background] = Some(VteRgb { r: 9, g: 9, b: 9 });
        assert_eq!(table.get(&overrides, 1), Rgb::new(1, 2, 3));
        assert_eq!(
            table.background(&overrides, Color::Named(NamedColor::Background)),
            Rgb::new(9, 9, 9)
        );
        // Out-of-range indices fall back to the foreground instead of panicking.
        assert_eq!(table.get(&overrides, COUNT + 5), Rgb::from_hex(0xD9DEE7));
    }

    #[test]
    fn bold_is_bright_only_for_the_eight_normal_colors() {
        let none = Colors::default();
        let table = ColorTable::new(&Palette::OPENSESH_DARK);
        let red = Color::Named(NamedColor::Red);
        assert_eq!(table.foreground(&none, red, true), Rgb::from_hex(0xFF8F95));
        assert_eq!(table.foreground(&none, red, false), Rgb::from_hex(0xF07178));
        assert_eq!(
            table.foreground(&none, Color::Indexed(2), true),
            Rgb::from_hex(0xB4EBA3)
        );
        assert_eq!(
            table.foreground(&none, Color::Indexed(100), true),
            table.get(&none, 100)
        );
        assert_eq!(
            table.foreground(&none, Color::Named(NamedColor::Foreground), true),
            Rgb::from_hex(0xD9DEE7)
        );
        let plain = ColorTable::new(&Palette {
            bold_is_bright: false,
            ..Palette::OPENSESH_DARK
        });
        assert_eq!(plain.foreground(&none, red, true), Rgb::from_hex(0xF07178));
    }

    #[test]
    fn rgb_helpers() {
        assert_eq!(Rgb::from_hex(0x12_3456).to_argb(), 0xFF12_3456);
        let black = Rgb::new(0, 0, 0);
        let white = Rgb::new(255, 255, 255);
        assert_eq!(black.mix(white, 0.5), Rgb::new(128, 128, 128));
        assert_eq!(black.mix(white, 2.0), white);
        assert_eq!(white.mix(black, -1.0), white);
    }
}
