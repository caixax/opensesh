//! Pure helpers of the terminal item (`bridge/terminal_view.rs`): tab titles, link detection on
//! the rows the renderer showed, wheel steps and which half of a cell the pointer is on. Qt-free,
//! so they are unit-tested here.

use std::ops::Range;

use opensesh_term::links;
use opensesh_term::session::Side;
use opensesh_term::snapshot::{Frame, flags};

/// Qt's angle delta for one wheel notch (eighths of a degree).
pub const WHEEL_NOTCH: i32 = 120;

/// Lines the scrollback (or alternate scroll) moves per wheel notch.
pub const LINES_PER_NOTCH: i32 = 3;

/// Lines to scroll for `steps` wheel notches at the profile's `speed` ([`LINES_PER_NOTCH`] per
/// notch at 1.0), at least one line per event.
#[must_use]
pub fn scaled_lines(steps: i32, speed: f64) -> i32 {
    let speed = if speed.is_finite() {
        speed.clamp(0.1, 20.0)
    } else {
        1.0
    };
    let lines = (f64::from(steps) * f64::from(LINES_PER_NOTCH) * speed).round();
    // Bounded by the clamps: 50 steps x 3 lines x 20 fits easily.
    #[allow(clippy::cast_possible_truncation)]
    let lines = lines.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
    if lines == 0 { steps.signum() } else { lines }
}

/// Most wheel steps one event may produce (a runaway touchpad or a huge synthetic delta).
const MAX_WHEEL_STEPS: i32 = 50;

/// One cell of the renderer's copy of the screen: enough to find links.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextCell {
    /// The base character.
    pub ch: char,
    /// `opensesh_term::snapshot::flags` bits.
    pub flags: u16,
}

/// The characters of the rows shown in the last frame, by viewport row. Frames with partial
/// damage update only their rows.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScreenText {
    columns: u16,
    rows: Vec<Vec<TextCell>>,
}

impl ScreenText {
    /// Copies the rows of `frame` (all of them, or the damaged ones).
    pub fn apply(&mut self, frame: &Frame) {
        let lines = usize::from(frame.lines);
        if self.columns != frame.columns || self.rows.len() != lines {
            self.columns = frame.columns;
            self.rows = vec![Vec::new(); lines];
        }
        for row in &frame.rows {
            if let Some(target) = self.rows.get_mut(usize::from(row.index)) {
                target.clear();
                target.extend(row.cells.iter().map(|cell| TextCell {
                    ch: cell.ch,
                    flags: cell.flags,
                }));
            }
        }
    }

    /// The cells of viewport row `row`, if the last frame had it.
    #[must_use]
    pub fn row(&self, row: u16) -> Option<&[TextCell]> {
        self.rows.get(usize::from(row)).map(Vec::as_slice)
    }
}

/// A URL found in a row: the text and its first and last cell (inclusive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundUrl {
    /// The URL as written.
    pub url: String,
    /// First column.
    pub first: u16,
    /// Last column (the second cell of a wide character at the end).
    pub last: u16,
}

/// The URL of `row` that covers `column`, if any ([`links::find_urls`] on the row's text).
#[must_use]
pub fn url_at(row: &[TextCell], column: u16) -> Option<FoundUrl> {
    // The row's text without wide-character spacers, and each character's cells.
    let mut text = String::with_capacity(row.len());
    let mut cells: Vec<Range<u16>> = Vec::with_capacity(row.len());
    for (index, cell) in row.iter().enumerate() {
        let Ok(index) = u16::try_from(index) else {
            break;
        };
        if cell.flags & flags::WIDE_SPACER != 0 {
            if let Some(last) = cells.last_mut() {
                last.end = index + 1;
            }
            continue;
        }
        text.push(if cell.ch == '\0' { ' ' } else { cell.ch });
        cells.push(index..index + 1);
    }
    links::find_urls(&text).into_iter().find_map(|range| {
        let first = cells.get(range.start)?.start;
        let last = cells.get(range.end.checked_sub(1)?)?.end.checked_sub(1)?;
        (first..=last).contains(&column).then(|| FoundUrl {
            url: text.chars().skip(range.start).take(range.len()).collect(),
            first,
            last,
        })
    })
}

/// The tab title for a title the program set. On Windows the console host announces the shell's
/// executable path as the first title (`C:\Program Files\PowerShell\7\pwsh.exe`): only the file
/// name is shown.
#[must_use]
pub fn display_title(raw: &str, windows: bool) -> String {
    let trimmed = raw.trim();
    if windows && is_windows_path(trimmed) {
        if let Some(name) = trimmed
            .rsplit(['\\', '/'])
            .next()
            .filter(|name| !name.is_empty())
        {
            return name.to_owned();
        }
    }
    trimmed.to_owned()
}

/// `X:\...`, `X:/...` or a UNC path `\\server\...`.
fn is_windows_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    let drive = bytes.len() > 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    drive || text.starts_with("\\\\")
}

/// Turns Qt wheel angle deltas into whole notches, keeping the remainder of smooth (touchpad)
/// scrolling for the next event.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WheelSteps {
    remainder_x: i32,
    remainder_y: i32,
}

impl WheelSteps {
    /// Adds an event's angle delta; returns the whole notches `(x, y)`, positive for up (y) and
    /// left (x), as Qt reports them. A change of direction drops the old remainder.
    pub fn add(&mut self, angle_x: f64, angle_y: f64) -> (i32, i32) {
        let x = Self::step(&mut self.remainder_x, angle_x);
        let y = Self::step(&mut self.remainder_y, angle_y);
        (x, y)
    }

    fn step(remainder: &mut i32, delta: f64) -> i32 {
        if !delta.is_finite() {
            return 0;
        }
        // Qt's deltas fit in an i32; clamp anything else.
        let delta = delta.round().clamp(
            -f64::from(WHEEL_NOTCH * MAX_WHEEL_STEPS),
            f64::from(WHEEL_NOTCH * MAX_WHEEL_STEPS),
        ) as i32;
        if delta == 0 {
            return 0;
        }
        if (*remainder > 0 && delta < 0) || (*remainder < 0 && delta > 0) {
            *remainder = 0;
        }
        *remainder += delta;
        let steps = *remainder / WHEEL_NOTCH;
        *remainder -= steps * WHEEL_NOTCH;
        steps.clamp(-MAX_WHEEL_STEPS, MAX_WHEEL_STEPS)
    }
}

/// Qt turns Alt + vertical wheel into a horizontal delta on Windows, Wayland and X11 (not on
/// macOS); undo it (`transposed`) so Alt+wheel still scrolls vertically.
#[must_use]
pub fn unswap_alt_wheel(transposed: bool, alt: bool, angle_x: f64, angle_y: f64) -> (f64, f64) {
    if transposed && alt && angle_y == 0.0 && angle_x != 0.0 {
        (0.0, angle_x)
    } else {
        (angle_x, angle_y)
    }
}

/// Which half of its cell the pointer at `x` (item coordinates) is on. Past the last column it
/// is the right half of that column, before the first the left half.
#[must_use]
pub fn side_of(x: f64, padding: f64, cell_width: f64, columns: u16) -> Side {
    if cell_width <= 0.0 || !x.is_finite() {
        return Side::Left;
    }
    let position = (x - padding) / cell_width;
    let right = position >= f64::from(columns) || (position >= 0.0 && position.fract() >= 0.5);
    if right { Side::Right } else { Side::Left }
}

#[cfg(test)]
mod tests {
    use opensesh_term::snapshot::{Cell, Damage, Row};

    use super::*;

    fn row(text: &str) -> Vec<TextCell> {
        text.chars().map(|ch| TextCell { ch, flags: 0 }).collect()
    }

    #[test]
    fn urls_are_found_under_the_pointer_only() {
        let cells = row("see https://example.com/a) now");
        let found = url_at(&cells, 10).unwrap();
        assert_eq!(found.url, "https://example.com/a");
        assert_eq!((found.first, found.last), (4, 24));
        assert_eq!(url_at(&cells, 4).unwrap().first, 4);
        assert!(url_at(&cells, 3).is_none());
        // The closing parenthesis is not part of the URL.
        assert!(url_at(&cells, 25).is_none());
        assert!(url_at(&row("no links here"), 3).is_none());
    }

    #[test]
    fn wide_characters_before_a_url_shift_its_cells() {
        // "中" takes two cells: the URL starts at column 3.
        let mut cells = vec![
            TextCell {
                ch: '中',
                flags: flags::WIDE,
            },
            TextCell {
                ch: ' ',
                flags: flags::WIDE_SPACER,
            },
            TextCell { ch: ' ', flags: 0 },
        ];
        cells.extend(row("http://a.io 中"));
        let found = url_at(&cells, 3).unwrap();
        assert_eq!(found.url, "http://a.io");
        assert_eq!((found.first, found.last), (3, 13));
        assert!(url_at(&cells, 1).is_none());
    }

    #[test]
    fn screen_text_follows_full_and_partial_frames() {
        let cell = |ch| Cell {
            ch,
            ..Cell::default()
        };
        let mut screen = ScreenText::default();
        let mut frame = Frame {
            columns: 2,
            lines: 2,
            damage: Damage::Full,
            rows: vec![
                Row {
                    index: 0,
                    cells: vec![cell('a'), cell('b')],
                },
                Row {
                    index: 1,
                    cells: vec![cell('c'), cell('d')],
                },
            ],
            ..Frame::default()
        };
        screen.apply(&frame);
        assert_eq!(screen.row(1).unwrap()[1].ch, 'd');
        frame.damage = Damage::Partial;
        frame.rows = vec![Row {
            index: 1,
            cells: vec![cell('x'), cell('y')],
        }];
        screen.apply(&frame);
        assert_eq!(screen.row(0).unwrap()[0].ch, 'a');
        assert_eq!(screen.row(1).unwrap()[0].ch, 'x');
        assert!(screen.row(2).is_none());
        // A new size starts over.
        frame.columns = 3;
        screen.apply(&frame);
        assert!(screen.row(0).unwrap().is_empty());
    }

    #[test]
    fn windows_shell_paths_show_their_file_name() {
        let pwsh = r"C:\Program Files\PowerShell\7\pwsh.exe";
        assert_eq!(display_title(pwsh, true), "pwsh.exe");
        assert_eq!(
            display_title(r"C:/Windows/system32/cmd.exe", true),
            "cmd.exe"
        );
        assert_eq!(display_title(r"\\server\share\tool.exe", true), "tool.exe");
        // Not a path, or not Windows: unchanged (trimmed).
        assert_eq!(
            display_title("Windows PowerShell", true),
            "Windows PowerShell"
        );
        assert_eq!(display_title("user@host: ~/src", true), "user@host: ~/src");
        assert_eq!(display_title(pwsh, false), pwsh);
        assert_eq!(display_title("  vim  ", false), "vim");
        assert_eq!(display_title(r"C:\", true), r"C:\");
    }

    #[test]
    fn wheel_deltas_become_notches_with_a_remainder() {
        let mut wheel = WheelSteps::default();
        assert_eq!(wheel.add(0.0, 120.0), (0, 1));
        assert_eq!(wheel.add(0.0, -240.0), (0, -2));
        // Touchpad: small deltas add up.
        assert_eq!(wheel.add(0.0, 50.0), (0, 0));
        assert_eq!(wheel.add(0.0, 50.0), (0, 0));
        assert_eq!(wheel.add(0.0, 50.0), (0, 1));
        // Turning around drops the remainder (30 left over from above).
        assert_eq!(wheel.add(0.0, -100.0), (0, 0));
        assert_eq!(wheel.add(0.0, -20.0), (0, -1));
        assert_eq!(wheel.add(120.0, 0.0), (1, 0));
        assert_eq!(wheel.add(f64::NAN, f64::INFINITY), (0, 0));
        assert_eq!(wheel.add(0.0, 1e12), (0, MAX_WHEEL_STEPS));
    }

    #[test]
    fn alt_wheel_is_vertical_again() {
        assert_eq!(unswap_alt_wheel(true, true, 120.0, 0.0), (0.0, 120.0));
        assert_eq!(unswap_alt_wheel(true, false, 120.0, 0.0), (120.0, 0.0));
        assert_eq!(unswap_alt_wheel(false, true, 120.0, 0.0), (120.0, 0.0));
        assert_eq!(unswap_alt_wheel(true, true, 0.0, -120.0), (0.0, -120.0));
    }

    #[test]
    fn the_pointer_is_on_the_left_or_right_half_of_a_cell() {
        // Cells 10 px wide after 4 px of padding, 5 columns.
        assert_eq!(side_of(4.0, 4.0, 10.0, 5), Side::Left);
        assert_eq!(side_of(8.9, 4.0, 10.0, 5), Side::Left);
        assert_eq!(side_of(9.0, 4.0, 10.0, 5), Side::Right);
        assert_eq!(side_of(20.0, 4.0, 10.0, 5), Side::Right);
        assert_eq!(side_of(1.0, 4.0, 10.0, 5), Side::Left);
        assert_eq!(side_of(500.0, 4.0, 10.0, 5), Side::Right);
        assert_eq!(side_of(5.0, 4.0, 0.0, 5), Side::Left);
    }

    #[test]
    fn scroll_speed_scales_the_wheel() {
        assert_eq!(scaled_lines(1, 1.0), 3);
        assert_eq!(scaled_lines(-2, 1.0), -6);
        assert_eq!(scaled_lines(1, 2.5), 8);
        assert_eq!(scaled_lines(1, 0.25), 1);
        assert_eq!(scaled_lines(-1, 0.1), -1, "at least one line");
        assert_eq!(scaled_lines(1, f64::NAN), 3);
    }
}
