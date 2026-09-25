//! Render snapshots: what the GUI needs to draw the visible part of a terminal (PLAN §3.3 step 5).
//!
//! A [`Frame`] is filled while the `Term` is locked, then the lock is released and the renderer
//! works only on the copy. Colors are resolved here, in Rust, so the renderer never needs the
//! palette: inverse, dim, bold-as-bright, hidden, selection, search matches and the cursor's cell
//! colors are already applied. Every color is `0xAARRGGBB`.
//!
//! These types are the contract between the engine ([`crate::session`]) and the renderer (the
//! C++ `TerminalItemBase` through the app's cxx-qt bridge). Keep them plain data.

/// Cell attribute bits in [`Cell::flags`].
pub mod flags {
    /// Bold weight.
    pub const BOLD: u16 = 1 << 0;
    /// Italic style.
    pub const ITALIC: u16 = 1 << 1;
    /// Single underline.
    pub const UNDERLINE: u16 = 1 << 2;
    /// Double underline.
    pub const DOUBLE_UNDERLINE: u16 = 1 << 3;
    /// Curly (wavy) underline.
    pub const CURLY_UNDERLINE: u16 = 1 << 4;
    /// Dotted underline.
    pub const DOTTED_UNDERLINE: u16 = 1 << 5;
    /// Dashed underline.
    pub const DASHED_UNDERLINE: u16 = 1 << 6;
    /// Strikethrough.
    pub const STRIKEOUT: u16 = 1 << 7;
    /// The first cell of a double-width character; the next cell is a [`WIDE_SPACER`].
    pub const WIDE: u16 = 1 << 8;
    /// The second cell of a double-width character: draw nothing but its background.
    pub const WIDE_SPACER: u16 = 1 << 9;
    /// Part of an OSC 8 hyperlink or a detected URL under the pointer (draw an underline).
    pub const LINK: u16 = 1 << 10;
    /// Selected (colors already swapped to the selection colors).
    pub const SELECTED: u16 = 1 << 11;
    /// Part of a search match (colors already applied).
    pub const MATCH: u16 = 1 << 12;
    /// Hidden text (SGR 8): colors already make the glyph invisible; the renderer may skip it.
    pub const HIDDEN: u16 = 1 << 13;

    /// Any underline style.
    pub const ANY_UNDERLINE: u16 =
        UNDERLINE | DOUBLE_UNDERLINE | CURLY_UNDERLINE | DOTTED_UNDERLINE | DASHED_UNDERLINE;
}

/// One cell of the visible grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// The base character (`' '` for an empty cell).
    pub ch: char,
    /// Combining characters (zero-width) that follow `ch`: `0` for none, else `index + 1` into
    /// [`Frame::clusters`], whose entry lists them.
    pub cluster: u32,
    /// Foreground (text) color, `0xAARRGGBB`.
    pub fg: u32,
    /// Background color, `0xAARRGGBB`. Equal to [`Frame::background`] for the default background.
    pub bg: u32,
    /// Underline color, `0xAARRGGBB` (the foreground unless SGR 58 set one).
    pub underline: u32,
    /// Attribute bits from [`flags`].
    pub flags: u16,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            cluster: 0,
            fg: 0,
            bg: 0,
            underline: 0,
            flags: 0,
        }
    }
}

/// One visible row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    /// Row index in the viewport, `0` at the top.
    pub index: u16,
    /// Exactly [`Frame::columns`] cells.
    pub cells: Vec<Cell>,
}

/// How the cursor is drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CursorShape {
    /// A filled block.
    #[default]
    Block,
    /// An outlined block (the terminal doesn't have the focus).
    HollowBlock,
    /// A vertical bar at the left edge of the cell.
    Beam,
    /// A line at the bottom of the cell.
    Underline,
    /// Not drawn (DECTCEM off, or scrolled away from the cursor).
    Hidden,
}

/// The cursor in viewport coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    /// Viewport row.
    pub row: u16,
    /// Column.
    pub column: u16,
    /// Shape to draw ([`CursorShape::Hidden`] when it must not be drawn).
    pub shape: CursorShape,
    /// Whether the program asked for a blinking cursor (the GUI decides whether to blink, e.g.
    /// never with reduce motion).
    pub blinking: bool,
    /// Whether the cursor cell holds a double-width character (draw it two cells wide).
    pub wide: bool,
    /// Cursor color, `0xAARRGGBB`.
    pub color: u32,
    /// Color of the character under a block cursor, `0xAARRGGBB`.
    pub text_color: u32,
}

/// What changed since the previous frame of the same session.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Damage {
    /// Only the rows in [`Frame::rows`] changed.
    #[default]
    Partial,
    /// Everything must be redrawn ([`Frame::rows`] holds every row): first frame, resize,
    /// scroll, palette or selection change.
    Full,
}

/// A copy of the visible terminal, ready to draw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frame {
    /// Grid width in cells.
    pub columns: u16,
    /// Grid height in cells.
    pub lines: u16,
    /// Whether [`Frame::rows`] holds every row or only the damaged ones.
    pub damage: Damage,
    /// The rows to (re)draw.
    pub rows: Vec<Row>,
    /// Combining characters of cells whose [`Cell::cluster`] is not zero.
    pub clusters: Vec<Vec<char>>,
    /// The cursor.
    pub cursor: Cursor,
    /// Default background, `0xAARRGGBB` (paint it behind the whole item, padding included).
    pub background: u32,
    /// How many lines the view is scrolled back into the history (`0` = at the bottom).
    pub display_offset: usize,
    /// Lines in the scrollback history (for the scrollbar).
    pub history_size: usize,
}

impl Frame {
    /// Clears the frame for reuse, keeping its allocations.
    pub fn clear(&mut self) {
        self.rows.clear();
        self.clusters.clear();
        self.damage = Damage::Partial;
    }
}
