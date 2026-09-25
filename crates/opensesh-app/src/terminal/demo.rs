//! The built-in demo frame of the gallery's terminal section (`TerminalItem { demo: true }`):
//! every renderer feature on one screen. Its palettes are placeholders for the demo only; the
//! terminal's real palettes live in `opensesh_term::palette`.

use opensesh_term::snapshot::{Cell, Cursor, CursorShape, Damage, Frame, Row, flags};

/// A 16-color palette plus the special colors, `0xRRGGBB`.
pub struct Palette {
    pub foreground: u32,
    pub background: u32,
    pub cursor: u32,
    pub cursor_text: u32,
    pub selection: u32,
    pub search: u32,
    pub search_text: u32,
    pub ansi: [u32; 16],
}

/// OpenSesh Dark from PLAN §4.3.
pub const DARK: Palette = Palette {
    foreground: 0xD9DEE7,
    background: 0x121419,
    cursor: 0xE6B450,
    cursor_text: 0x121419,
    selection: 0x2B3242,
    search: 0xE6B450,
    search_text: 0x121419,
    ansi: [
        0x1C1F27, 0xF07178, 0x9BD68A, 0xE6C07B, 0x73B7F2, 0xC9A0F0, 0x6ED6D0, 0xC8CDD6, 0x4A5263,
        0xFF8F95, 0xB4EBA3, 0xF2D39A, 0x9ACCFA, 0xDDBDFB, 0x94E8E3, 0xF2F4F8,
    ],
};

/// A light placeholder for the demo, from the app's light theme colors.
pub const LIGHT: Palette = Palette {
    foreground: 0x1B1D22,
    background: 0xFFFFFF,
    cursor: 0x1B1D22,
    cursor_text: 0xFFFFFF,
    selection: 0xCFE0F5,
    search: 0xF2C14E,
    search_text: 0x1B1D22,
    ansi: [
        0x1B1D22, 0xC8374D, 0x1E8F52, 0x9A6B00, 0x2B6CB0, 0x8E44AD, 0x11808A, 0xB8BCC4, 0x5D6470,
        0xE0445C, 0x26A862, 0xB7800F, 0x3D86D1, 0xA45BC6, 0x1A9AA6, 0xE6E8EE,
    ],
};

const OPAQUE: u32 = 0xFF00_0000;

fn argb(rgb: u32) -> u32 {
    OPAQUE | (rgb & 0x00FF_FFFF)
}

/// Color `index` of the xterm 256-color palette.
pub fn indexed(palette: &Palette, index: u8) -> u32 {
    const LEVELS: [u32; 6] = [0, 95, 135, 175, 215, 255];
    let index = usize::from(index);
    let rgb = match index {
        0..=15 => palette.ansi[index],
        16..=231 => {
            let cube = index - 16;
            (LEVELS[cube / 36] << 16) | (LEVELS[(cube / 6) % 6] << 8) | LEVELS[cube % 6]
        }
        _ => {
            let level = 8 + 10 * (index as u32 - 232);
            (level << 16) | (level << 8) | level
        }
    };
    argb(rgb)
}

/// The color at `t` (0..1) on a hue wheel.
fn hue(t: f64) -> u32 {
    let h = (t.fract() * 6.0).max(0.0);
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    let channel = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u32;
    argb((channel(r) << 16) | (channel(g) << 8) | channel(b))
}

/// Readable text on `background`: black or white.
fn text_on(background: u32) -> u32 {
    let r = (background >> 16) & 0xFF;
    let g = (background >> 8) & 0xFF;
    let b = background & 0xFF;
    if r * 299 + g * 587 + b * 114 > 128_000 {
        argb(0x000000)
    } else {
        argb(0xFFFFFF)
    }
}

/// Dim (SGR 2): two thirds of the color's intensity.
fn dim(color: u32) -> u32 {
    let scale = |shift: u32| (((color >> shift) & 0xFF) * 2 / 3) << shift;
    OPAQUE | scale(16) | scale(8) | scale(0)
}

/// Characters the demo draws two cells wide (enough for its own text).
fn is_wide(ch: char) -> bool {
    matches!(u32::from(ch),
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFF01..=0xFF60
        | 0x1F300..=0x1F64F | 0x1F680..=0x1F6FF | 0x1F900..=0x1F9FF)
}

/// Zero-width characters the demo attaches to the previous cell.
fn is_combining(ch: char) -> bool {
    matches!(u32::from(ch), 0x0300..=0x036F | 0x20D0..=0x20FF | 0xFE00..=0xFE0F)
}

#[derive(Clone, Copy)]
struct Style {
    fg: u32,
    bg: u32,
    underline: u32,
    flags: u16,
}

/// Writes text into one row, cell by cell.
struct RowWriter<'a> {
    cells: Vec<Cell>,
    clusters: &'a mut Vec<Vec<char>>,
    columns: usize,
    base: Style,
}

impl<'a> RowWriter<'a> {
    fn new(columns: usize, base: Style, clusters: &'a mut Vec<Vec<char>>) -> Self {
        Self {
            cells: Vec::with_capacity(columns),
            clusters,
            columns,
            base,
        }
    }

    fn cell(style: Style, ch: char) -> Cell {
        Cell {
            ch,
            cluster: 0,
            fg: style.fg,
            bg: style.bg,
            underline: style.underline,
            flags: style.flags,
        }
    }

    fn text(&mut self, text: &str, style: Style) -> &mut Self {
        for ch in text.chars() {
            if is_combining(ch) {
                self.attach(ch);
            } else if is_wide(ch) {
                if self.cells.len() + 2 > self.columns {
                    // No room for both halves: a blank, as terminals do.
                    self.push(Self::cell(style, ' '));
                } else {
                    let mut first = Self::cell(style, ch);
                    first.flags |= flags::WIDE;
                    let mut spacer = Self::cell(style, ' ');
                    spacer.flags |= flags::WIDE_SPACER;
                    self.push(first);
                    self.push(spacer);
                }
            } else {
                self.push(Self::cell(style, ch));
            }
        }
        self
    }

    fn plain(&mut self, text: &str) -> &mut Self {
        let base = self.base;
        self.text(text, base)
    }

    fn push(&mut self, cell: Cell) {
        if self.cells.len() < self.columns {
            self.cells.push(cell);
        }
    }

    /// Adds a combining character to the last base cell.
    fn attach(&mut self, mark: char) {
        let Some(position) = self
            .cells
            .iter()
            .rposition(|cell| cell.flags & flags::WIDE_SPACER == 0)
        else {
            return;
        };
        let cell = &mut self.cells[position];
        if cell.cluster == 0 {
            self.clusters.push(vec![mark]);
            cell.cluster = u32::try_from(self.clusters.len()).unwrap_or(0);
        } else if let Some(marks) = self.clusters.get_mut(cell.cluster as usize - 1) {
            marks.push(mark);
        }
    }

    fn column(&self) -> usize {
        self.cells.len()
    }

    fn finish(mut self, index: u16) -> Row {
        let blank = Self::cell(self.base, ' ');
        self.cells.resize(self.columns, blank);
        Row {
            index,
            cells: self.cells,
        }
    }
}

/// Fills `frame` with the demo at `columns` x `lines`. With `animation`, every row is
/// pseudo-random text that changes with the tick (the benchmark).
pub fn build(
    frame: &mut Frame,
    columns: u16,
    lines: u16,
    dark: bool,
    cursor_shape: CursorShape,
    animation: Option<u64>,
) {
    let palette = if dark { &DARK } else { &LIGHT };
    frame.clear();
    frame.columns = columns;
    frame.lines = lines;
    frame.damage = Damage::Full;
    frame.background = argb(palette.background);
    frame.display_offset = 0;
    frame.history_size = 0;
    let fg = argb(palette.foreground);
    let base = Style {
        fg,
        bg: frame.background,
        underline: fg,
        flags: 0,
    };
    let width = usize::from(columns);
    let mut cursor = (0_usize, 0_usize);
    for line in 0..lines {
        let mut row = RowWriter::new(width, base, &mut frame.clusters);
        match animation {
            Some(tick) => flood(&mut row, palette, tick.wrapping_add(u64::from(line))),
            None => {
                if let Some(position) =
                    write_line(&mut row, usize::from(line), palette, base, columns, lines)
                {
                    cursor = (usize::from(line), position);
                }
            }
        }
        frame.rows.push(row.finish(line));
    }
    frame.cursor = Cursor {
        row: u16::try_from(cursor.0).unwrap_or(0),
        column: u16::try_from(cursor.1.min(width.saturating_sub(1))).unwrap_or(0),
        shape: if animation.is_some() {
            CursorShape::Hidden
        } else {
            cursor_shape
        },
        blinking: true,
        wide: false,
        color: argb(palette.cursor),
        text_color: argb(palette.cursor_text),
    };
}

/// Row `line` of the static demo. Returns the cursor column if the cursor goes on it.
fn write_line(
    row: &mut RowWriter<'_>,
    line: usize,
    palette: &Palette,
    base: Style,
    columns: u16,
    lines: u16,
) -> Option<usize> {
    let with = |flags: u16| Style { flags, ..base };
    let fg = |color: u32| Style {
        fg: color,
        underline: color,
        ..base
    };
    let label = Style {
        fg: dim(base.fg),
        ..base
    };
    match line {
        0 => {
            row.text("OpenSesh terminal renderer", with(flags::BOLD))
                .text(&format!("  {columns}x{lines} cells"), label);
        }
        1 => {
            row.text("Styles    ", label)
                .plain("normal ")
                .text("bold", with(flags::BOLD))
                .plain(" ")
                .text("italic", with(flags::ITALIC))
                .plain(" ")
                .text("bold italic", with(flags::BOLD | flags::ITALIC))
                .plain(" ")
                .text("dim", fg(dim(base.fg)))
                .plain(" ")
                .text(
                    "inverse",
                    Style {
                        fg: base.bg,
                        bg: base.fg,
                        ..base
                    },
                )
                .plain(" hidden[")
                .text(
                    "secret",
                    Style {
                        fg: base.bg,
                        flags: flags::HIDDEN,
                        ..base
                    },
                )
                .plain("] ")
                .text("strike", with(flags::STRIKEOUT));
        }
        2 => {
            let red = indexed(palette, 9);
            row.text("Underline ", label)
                .text("single", with(flags::UNDERLINE))
                .plain(" ")
                .text("double", with(flags::DOUBLE_UNDERLINE))
                .plain(" ")
                .text("curly", with(flags::CURLY_UNDERLINE))
                .plain(" ")
                .text("dotted", with(flags::DOTTED_UNDERLINE))
                .plain(" ")
                .text("dashed", with(flags::DASHED_UNDERLINE))
                .plain(" ")
                .text(
                    "colored",
                    Style {
                        underline: red,
                        flags: flags::CURLY_UNDERLINE,
                        ..base
                    },
                )
                .plain(" ")
                .text(
                    "spelling",
                    Style {
                        underline: red,
                        flags: flags::UNDERLINE,
                        ..base
                    },
                );
        }
        3 => {
            row.text("16 colors ", label);
            for index in 0..16_u8 {
                let bg = indexed(palette, index);
                row.text(
                    &format!("{index:>2} "),
                    Style {
                        fg: text_on(bg),
                        bg,
                        ..base
                    },
                );
            }
        }
        4 => {
            row.text("256       ", label);
            for index in 232..=255_u8 {
                let bg = indexed(palette, index);
                row.text("  ", Style { bg, ..base });
            }
        }
        5..=7 => {
            // The 6x6x6 cube, two red levels per line.
            row.text("          ", label);
            let first = (line - 5) * 2;
            for red in first..first + 2 {
                for green in 0..6 {
                    for blue in 0..6 {
                        let index = u8::try_from(16 + red * 36 + green * 6 + blue).unwrap_or(0);
                        row.text(
                            " ",
                            Style {
                                bg: indexed(palette, index),
                                ..base
                            },
                        );
                    }
                }
            }
        }
        8 => {
            row.text("Truecolor ", label);
            let span = usize::from(columns).saturating_sub(row.column()).max(1);
            for i in 0..span {
                let bg = hue(i as f64 / span as f64);
                row.text(" ", Style { bg, ..base });
            }
        }
        9 => {
            row.text("          ", label);
            let text = "The quick brown fox jumps over the lazy dog: gradient foreground";
            let count = text.chars().count().max(1);
            for (i, ch) in text.chars().enumerate() {
                let color = hue(i as f64 / count as f64);
                row.text(ch.encode_utf8(&mut [0; 4]), fg(color));
            }
        }
        10 => {
            row.text("Wide      ", label)
                .plain("中文字符 日本語テキスト 한국어 ｗｉｄｅ|");
        }
        11 => {
            row.text("Combining ", label)
                .plain("e\u{301} a\u{308} n\u{303} o\u{302}\u{323} q\u{307}\u{323} Z\u{335} ")
                .plain("cafe\u{301} |");
        }
        12 => {
            row.text("Emoji     ", label)
                .plain("😀 🚀 👍 🎉 🐧 ❤\u{fe0f} ✔ ⚠ ★ |");
        }
        13 => {
            row.text("Box       ", label)
                .plain("┌──┬──┐ ╔══╦══╗ ╭──╮ ░▒▓█ ▁▂▃▄▅▆▇█ ⠿⣿ ●○■□◆ ←↑→↓");
        }
        14 => {
            row.text("          ", label)
                .plain("│  │  │ ║  ║  ║ │  │ ▗▄▖ ▐█▌");
        }
        15 => {
            row.text("          ", label)
                .plain("└──┴──┘ ╚══╩══╝ ╰──╯ ▝▀▘ ▛▀▜");
        }
        16 => {
            row.text("Powerline ", label);
            let segments = [
                (" user@host ", 4_u8),
                (" ~/src/opensesh ", 2),
                ("  main ", 3),
            ];
            for (i, (text, color)) in segments.iter().enumerate() {
                let bg = indexed(palette, *color);
                row.text(
                    text,
                    Style {
                        fg: text_on(bg),
                        bg,
                        flags: flags::BOLD,
                        ..base
                    },
                );
                let next = segments
                    .get(i + 1)
                    .map_or(base.bg, |(_, next)| indexed(palette, *next));
                row.text(
                    "\u{e0b0}",
                    Style {
                        fg: bg,
                        bg: next,
                        ..base
                    },
                );
            }
            row.plain(" \u{e0b1} \u{e0b2}\u{e0b3} \u{e0a0}");
        }
        17 => {
            let selection = argb(palette.selection);
            row.text("Selection ", label).plain("this ").text(
                "text is selected",
                Style {
                    bg: selection,
                    flags: flags::SELECTED,
                    ..base
                },
            );
        }
        18 => {
            row.text("Search    ", label).plain("find the ").text(
                "match",
                Style {
                    fg: argb(palette.search_text),
                    bg: argb(palette.search),
                    flags: flags::MATCH,
                    ..base
                },
            );
            row.plain(" in this line");
        }
        19 => {
            let blue = indexed(palette, 12);
            row.text("Link      ", label).text(
                "https://example.org/opensesh",
                Style {
                    fg: blue,
                    underline: blue,
                    flags: flags::LINK,
                    ..base
                },
            );
        }
        20 => {
            row.text("Cursor    ", label)
                .plain("block, hollow, beam or underline; blinks while focused");
        }
        21 => {
            let green = indexed(palette, 2);
            row.text("$ ", fg(green)).plain("echo hello ");
            return Some(row.column());
        }
        _ => {}
    }
    None
}

/// One row of the benchmark: runs of pseudo-random printable ASCII in varied colors.
fn flood(row: &mut RowWriter<'_>, palette: &Palette, seed: u64) {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = || {
        // xorshift64
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    while row.column() < row.columns {
        let run = 3 + (next() % 9) as usize;
        let bits = next();
        let fg = indexed(palette, (bits % 16) as u8);
        let bg = if (bits >> 8) & 7 == 0 {
            indexed(palette, ((bits >> 16) & 7) as u8)
        } else {
            row.base.bg
        };
        let flags = if (bits >> 24) & 7 == 0 {
            flags::BOLD
        } else {
            0
        };
        let style = Style {
            fg,
            bg,
            underline: fg,
            flags,
        };
        for _ in 0..run {
            let value = next();
            let ch = if value % 6 == 0 {
                ' '
            } else {
                char::from(33 + (value % 94) as u8)
            };
            row.text(ch.encode_utf8(&mut [0; 4]), style);
        }
    }
}

#[cfg(test)]
mod tests {
    use opensesh_term::snapshot::{CursorShape, Damage, Frame, flags};

    use super::*;

    fn check_rows(frame: &Frame) {
        for row in &frame.rows {
            assert_eq!(
                row.cells.len(),
                usize::from(frame.columns),
                "row {}",
                row.index
            );
            for (i, cell) in row.cells.iter().enumerate() {
                if cell.flags & flags::WIDE != 0 {
                    assert!(
                        row.cells
                            .get(i + 1)
                            .is_some_and(|next| next.flags & flags::WIDE_SPACER != 0),
                        "row {} column {i}: wide cell without spacer",
                        row.index
                    );
                }
                if cell.cluster != 0 {
                    assert!(frame.clusters.len() >= cell.cluster as usize);
                }
            }
        }
    }

    #[test]
    fn the_demo_exercises_every_feature() {
        let mut frame = Frame::default();
        build(&mut frame, 100, 24, true, CursorShape::Beam, None);
        assert_eq!(frame.rows.len(), 24);
        assert_eq!(frame.damage, Damage::Full);
        check_rows(&frame);
        let mut seen = 0_u16;
        let mut colors = std::collections::BTreeSet::new();
        for cell in frame.rows.iter().flat_map(|row| &row.cells) {
            seen |= cell.flags;
            colors.insert(cell.bg);
        }
        for flag in [
            flags::BOLD,
            flags::ITALIC,
            flags::UNDERLINE,
            flags::DOUBLE_UNDERLINE,
            flags::CURLY_UNDERLINE,
            flags::DOTTED_UNDERLINE,
            flags::DASHED_UNDERLINE,
            flags::STRIKEOUT,
            flags::WIDE,
            flags::WIDE_SPACER,
            flags::LINK,
            flags::SELECTED,
            flags::MATCH,
            flags::HIDDEN,
        ] {
            assert_ne!(seen & flag, 0, "flag {flag:#x} missing");
        }
        // 16 + 24 + 216 palette colors plus a truecolor gradient.
        assert!(colors.len() > 256 + 20, "{} colors", colors.len());
        // Combining marks, emoji and Powerline are there.
        assert!(
            frame
                .clusters
                .iter()
                .any(|marks| marks.contains(&'\u{301}'))
        );
        let text: String = frame
            .rows
            .iter()
            .flat_map(|row| &row.cells)
            .map(|cell| cell.ch)
            .collect();
        for needle in ['😀', '中', '┌', '\u{e0b0}', '⣿'] {
            assert!(text.contains(needle), "{needle} missing");
        }
        assert_eq!(frame.cursor.shape, CursorShape::Beam);
        assert_eq!(frame.cursor.row, 21);
    }

    #[test]
    fn the_demo_fits_any_size() {
        for (columns, lines) in [(1, 1), (2, 1), (7, 3), (80, 24), (300, 90)] {
            let mut frame = Frame::default();
            build(&mut frame, columns, lines, false, CursorShape::Block, None);
            assert_eq!(frame.rows.len(), usize::from(lines));
            check_rows(&frame);
            assert!(frame.cursor.row < lines && frame.cursor.column < columns.max(1));
            build(
                &mut frame,
                columns,
                lines,
                true,
                CursorShape::Block,
                Some(7),
            );
            check_rows(&frame);
            assert_eq!(frame.cursor.shape, CursorShape::Hidden);
        }
    }

    #[test]
    fn the_256_color_palette_follows_xterm() {
        assert_eq!(indexed(&DARK, 1), 0xFFF0_7178);
        assert_eq!(indexed(&DARK, 16), 0xFF00_0000);
        assert_eq!(indexed(&DARK, 196), 0xFFFF_0000);
        assert_eq!(indexed(&DARK, 231), 0xFFFF_FFFF);
        assert_eq!(indexed(&DARK, 232), 0xFF08_0808);
        assert_eq!(indexed(&DARK, 255), 0xFFEE_EEEE);
    }
}
