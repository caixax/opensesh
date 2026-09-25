//! `TerminalItem`: the terminal grid for QML (Sprint 2, [ADR 0013]).
//!
//! The C++ base `TerminalItemBase` (`cpp/terminal_item.h`) draws through the Qt Quick scene graph
//! with its own glyph atlas and turns Qt input events into calls to its pure virtual functions.
//! This Rust QObject derives from it and implements them:
//!
//! - `fillFrame` hands the renderer a snapshot of the terminal: an
//!   [`opensesh_term::snapshot::Frame`] converted by [`write_frame`] to the flat FFI structs
//!   below. It runs on the scene graph render thread while the GUI thread is blocked, so it only
//!   touches Rust state.
//! - `handleKey`, `handleShortcutOverride`, `handleMouse`, `handleWheel`, `handleHover`,
//!   `handleFocusChange` and `handleImeCommit` receive input on the GUI thread.
//!
//! The QML type is `TerminalItem` (the name PLAN §3.2 uses): `TerminalView` is already the
//! terminal workspace view (`qml/views/TerminalView.qml`). Until the engine is wired in, the item
//! only draws `demo: true`, a built-in frame that exercises every renderer feature (the component
//! gallery's Terminal section). The C++ base adds the properties `fontFamily`, `fontPointSize`,
//! `padding`, `reduceMotion`, the read-only `columns`, `lines`, `cellWidth` and `cellHeight`, the
//! `gridSizeChanged(columns, lines)` signal and the `requestFrame()` invokable.
//!
//! The base's other public C++ API is reachable from Rust with more `#[inherit]` declarations in
//! the `unsafe extern "RustQt"` block, added when something uses them (cxx-qt accepts no
//! `#[allow(dead_code)]` there):
//!
//! ```ignore
//! #[inherit] fn columns(self: &TerminalItem) -> i32;
//! #[inherit] fn lines(self: &TerminalItem) -> i32;
//! #[inherit] #[cxx_name = "setClipboardText"]
//! fn set_clipboard_text(self: Pin<&mut TerminalItem>, text: &QString, primary_selection: bool);
//! #[inherit] #[cxx_name = "clipboardText"]
//! fn clipboard_text(self: &TerminalItem, primary_selection: bool) -> QString;
//! #[inherit] #[cxx_name = "supportsPrimarySelection"]
//! fn supports_primary_selection(self: &TerminalItem) -> bool;
//! #[inherit] #[qsignal] #[cxx_name = "gridSizeChanged"]
//! fn grid_size_changed(self: Pin<&mut TerminalItem>, columns: i32, lines: i32);
//! ```
//!
//! With `impl cxx_qt::Threading`, a session thread can queue `update()` on the item when the
//! terminal changed (one queued call per frame at most).
//!
//! [ADR 0013]: ../../../../docs/adr/0013-terminal-rendering.md

#[cxx_qt::bridge(namespace = "opensesh")]
pub mod qobject {
    /// Cell attribute bits in [`TerminalCell::flags`]: the values of
    /// `opensesh_term::snapshot::flags` (checked by a unit test).
    #[repr(u16)]
    #[derive(Debug)]
    enum TerminalCellFlag {
        /// Bold weight.
        Bold = 1,
        /// Italic style.
        Italic = 2,
        /// Single underline.
        Underline = 4,
        /// Double underline.
        DoubleUnderline = 8,
        /// Curly underline.
        CurlyUnderline = 16,
        /// Dotted underline.
        DottedUnderline = 32,
        /// Dashed underline.
        DashedUnderline = 64,
        /// Strikethrough.
        Strikeout = 128,
        /// First cell of a double-width character.
        Wide = 256,
        /// Second cell of a double-width character.
        WideSpacer = 512,
        /// Hyperlink (drawn underlined).
        Link = 1024,
        /// Selected.
        Selected = 2048,
        /// Search match.
        Match = 4096,
        /// Hidden text.
        Hidden = 8192,
    }

    /// Cursor shapes, as `opensesh_term::snapshot::CursorShape`.
    #[repr(u8)]
    #[derive(Debug)]
    enum TerminalCursorShape {
        /// Filled block.
        Block = 0,
        /// Outlined block.
        HollowBlock = 1,
        /// Vertical bar.
        Beam = 2,
        /// Line at the bottom of the cell.
        Underline = 3,
        /// Not drawn.
        Hidden = 4,
    }

    /// One cell for the renderer (`opensesh_term::snapshot::Cell`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct TerminalCell {
        /// Unicode scalar value of the base character.
        ch: u32,
        /// `0`, or `1 +` the offset of this cell's entry in the `clusters` vector:
        /// `[count, code point...]` of its combining characters.
        cluster: u32,
        /// Foreground, `0xAARRGGBB`.
        fg: u32,
        /// Background, `0xAARRGGBB`.
        bg: u32,
        /// Underline color, `0xAARRGGBB`.
        underline: u32,
        /// [`TerminalCellFlag`] bits.
        flags: u16,
    }

    /// Everything about a frame except its rows.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TerminalFrameInfo {
        /// Grid width in cells.
        columns: u16,
        /// Grid height in cells.
        lines: u16,
        /// Whether the rows are every row of the grid (rows left out are blank).
        full: bool,
        /// Default background, `0xAARRGGBB`.
        background: u32,
        /// Cursor row in the viewport.
        cursor_row: u16,
        /// Cursor column.
        cursor_column: u16,
        /// Cursor shape.
        cursor_shape: TerminalCursorShape,
        /// Whether the cursor blinks (when focused, unless reduce motion is on).
        cursor_blinking: bool,
        /// Whether the cursor covers a double-width character.
        cursor_wide: bool,
        /// Cursor color, `0xAARRGGBB`.
        cursor_color: u32,
        /// Color of the character under a block cursor, `0xAARRGGBB`.
        cursor_text_color: u32,
    }

    /// What the renderer asks `fillFrame` for.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TerminalFrameRequest {
        /// Grid width that fits the item.
        columns: u16,
        /// Grid height that fits the item.
        lines: u16,
        /// Every row is needed (first frame, new grid size, scene graph rebuilt).
        full: bool,
    }

    /// A mouse press, release or move over the grid.
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct TerminalMouseEvent {
        /// `0` press, `1` release, `2` move (`TerminalItemBase::MouseKind`).
        kind: i32,
        /// The button that changed (`Qt::MouseButton`), `0` for moves.
        button: i32,
        /// Buttons held after the event (`Qt::MouseButtons`).
        buttons: i32,
        /// `Qt::KeyboardModifiers`.
        modifiers: i32,
        /// Position in the item, logical pixels.
        x: f64,
        /// Position in the item, logical pixels.
        y: f64,
        /// Cell under the pointer, clamped to the grid.
        column: i32,
        /// Cell under the pointer, clamped to the grid.
        line: i32,
        /// For presses: 1, 2 (double click) or 3 (triple click).
        click_count: i32,
    }

    /// A wheel or touchpad scroll over the grid.
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct TerminalWheelEvent {
        /// Position in the item, logical pixels.
        x: f64,
        /// Position in the item, logical pixels.
        y: f64,
        /// Cell under the pointer, clamped to the grid.
        column: i32,
        /// Cell under the pointer, clamped to the grid.
        line: i32,
        /// Qt's angle delta: eighths of a degree, 120 per wheel notch.
        angle_x: f64,
        /// Qt's angle delta: eighths of a degree, 120 per wheel notch.
        angle_y: f64,
        /// Touchpad scroll in pixels, when the platform reports it.
        pixel_x: f64,
        /// Touchpad scroll in pixels, when the platform reports it.
        pixel_y: f64,
        /// `Qt::KeyboardModifiers`.
        modifiers: i32,
    }

    // Qt types live in the global C++ namespace, not in the bridge's.
    #[namespace = ""]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "C++" {
        include!("opensesh-app/terminal_item.h");
        /// Hand-written C++ base: scene graph rendering and Qt input plumbing.
        type TerminalItemBase;
    }

    extern "RustQt" {
        /// The terminal grid item.
        #[qobject]
        #[qml_element]
        #[base = TerminalItemBase]
        #[qproperty(bool, demo, READ, WRITE = set_demo, NOTIFY = demo_changed)]
        #[qproperty(bool, demo_dark, cxx_name = "demoDark", READ, WRITE = set_demo_dark, NOTIFY = demo_changed)]
        #[qproperty(i32, demo_cursor_shape, cxx_name = "demoCursorShape", READ, WRITE = set_demo_cursor_shape, NOTIFY = demo_changed)]
        #[qproperty(bool, demo_animated, cxx_name = "demoAnimated", READ, WRITE = set_demo_animated, NOTIFY = demo_changed)]
        type TerminalItem = super::TerminalItemRust;

        /// Emitted when a demo property changes.
        #[qsignal]
        #[cxx_name = "demoChanged"]
        fn demo_changed(self: Pin<&mut TerminalItem>);

        /// Shows the built-in demo frame instead of a terminal.
        fn set_demo(self: Pin<&mut TerminalItem>, value: bool);
        /// Demo colors: the dark or the light palette.
        fn set_demo_dark(self: Pin<&mut TerminalItem>, value: bool);
        /// Demo cursor: 0 block, 1 hollow block, 2 beam, 3 underline, 4 hidden.
        fn set_demo_cursor_shape(self: Pin<&mut TerminalItem>, value: i32);
        /// Demo benchmark: every row changes on every frame.
        fn set_demo_animated(self: Pin<&mut TerminalItem>, value: bool);

        /// Called by the renderer on the render thread, GUI thread blocked (see the C++ base).
        #[cxx_override]
        #[cxx_name = "fillFrame"]
        fn fill_frame(
            self: Pin<&mut TerminalItem>,
            request: &TerminalFrameRequest,
            info: &mut TerminalFrameInfo,
            rows: &mut Vec<u16>,
            cells: &mut Vec<TerminalCell>,
            clusters: &mut Vec<u32>,
        ) -> bool;

        /// A key press; returns whether the terminal consumed it.
        #[cxx_override]
        #[cxx_name = "handleKey"]
        fn handle_key(
            self: Pin<&mut TerminalItem>,
            key: i32,
            modifiers: i32,
            text: &QString,
            keypad: bool,
            auto_repeat: bool,
        ) -> bool;

        /// Whether the terminal takes a key that is also a window shortcut (ADR 0011).
        #[cxx_override]
        #[cxx_name = "handleShortcutOverride"]
        fn handle_shortcut_override(self: Pin<&mut TerminalItem>, key: i32, modifiers: i32)
        -> bool;

        /// Mouse press (0), release (1) or move (2).
        #[cxx_override]
        #[cxx_name = "handleMouse"]
        fn handle_mouse(self: Pin<&mut TerminalItem>, event: &TerminalMouseEvent);

        /// Wheel or touchpad scroll.
        #[cxx_override]
        #[cxx_name = "handleWheel"]
        fn handle_wheel(self: Pin<&mut TerminalItem>, event: &TerminalWheelEvent);

        /// Pointer moved without a button pressed.
        #[cxx_override]
        #[cxx_name = "handleHover"]
        fn handle_hover(
            self: Pin<&mut TerminalItem>,
            x: f64,
            y: f64,
            column: i32,
            line: i32,
            modifiers: i32,
        );

        /// The item gained or lost the keyboard focus.
        #[cxx_override]
        #[cxx_name = "handleFocusChange"]
        fn handle_focus_change(self: Pin<&mut TerminalItem>, focused: bool);

        /// Text committed by an input method.
        #[cxx_override]
        #[cxx_name = "handleImeCommit"]
        fn handle_ime_commit(self: Pin<&mut TerminalItem>, text: &QString);
    }

    unsafe extern "RustQt" {
        /// `QQuickItem::update()`: schedules a new frame (GUI thread only).
        #[inherit]
        fn update(self: Pin<&mut TerminalItem>);
    }

    impl cxx_qt::Threading for TerminalItem {}
}

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use opensesh_term::snapshot::{self, CursorShape, Damage, Frame};

use qobject::{
    TerminalCell, TerminalCursorShape, TerminalFrameInfo, TerminalFrameRequest, TerminalMouseEvent,
    TerminalWheelEvent,
};

/// What the next `fillFrame` has to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Pending {
    /// Nothing changed.
    Nothing,
    /// Only the cursor changed.
    Cursor,
    /// Every row.
    Everything,
}

/// Rust state behind `TerminalItem`.
#[derive(Debug)]
pub struct TerminalItemRust {
    demo: bool,
    demo_dark: bool,
    demo_cursor_shape: i32,
    demo_animated: bool,
    pending: Pending,
    /// Grid size of the last frame sent.
    sent_size: (u16, u16),
    /// Whether the last frame sent had content (so turning the demo off must clear it).
    showing: bool,
    frame: Frame,
    tick: u64,
}

impl Default for TerminalItemRust {
    fn default() -> Self {
        Self {
            demo: false,
            demo_dark: true,
            demo_cursor_shape: 0,
            demo_animated: false,
            pending: Pending::Nothing,
            sent_size: (0, 0),
            showing: false,
            frame: Frame::default(),
            tick: 0,
        }
    }
}

impl TerminalItemRust {
    /// Renderer callback (see `fillFrame` in the C++ base). Returns whether a frame was written.
    fn fill(
        &mut self,
        request: &TerminalFrameRequest,
        info: &mut TerminalFrameInfo,
        rows: &mut Vec<u16>,
        cells: &mut Vec<TerminalCell>,
        clusters: &mut Vec<u32>,
    ) -> bool {
        let TerminalFrameRequest {
            columns,
            lines,
            full,
        } = *request;
        let resized = self.sent_size != (columns, lines);
        if !self.demo {
            // No engine yet: clear what the demo left on screen, once.
            if !(self.showing || full) {
                return false;
            }
            blank_frame(&mut self.frame, columns, lines);
            self.showing = false;
        } else {
            let everything = full || resized || self.pending == Pending::Everything;
            if !everything && !self.demo_animated && self.pending == Pending::Nothing {
                return false;
            }
            let shape = cursor_shape_from_index(self.demo_cursor_shape);
            if everything || self.demo_animated {
                let animation = self.demo_animated.then_some(self.tick);
                demo::build(
                    &mut self.frame,
                    columns,
                    lines,
                    self.demo_dark,
                    shape,
                    animation,
                );
                self.frame.damage = if everything {
                    Damage::Full
                } else {
                    Damage::Partial
                };
                self.tick = self.tick.wrapping_add(1);
            } else {
                // Only the cursor changed: no rows.
                self.frame.clear();
                self.frame.cursor.shape = shape;
            }
            self.showing = true;
        }
        write_frame(&self.frame, info, rows, cells, clusters);
        self.pending = Pending::Nothing;
        self.sent_size = (columns, lines);
        true
    }

    fn request(&mut self, pending: Pending) {
        self.pending = self.pending.max(pending);
    }
}

impl qobject::TerminalItem {
    /// See the bridge declaration.
    pub fn set_demo(mut self: Pin<&mut Self>, value: bool) {
        if self.demo == value {
            return;
        }
        let mut state = self.as_mut().rust_mut();
        state.demo = value;
        state.request(Pending::Everything);
        self.as_mut().demo_changed();
        self.update();
    }

    /// See the bridge declaration.
    pub fn set_demo_dark(mut self: Pin<&mut Self>, value: bool) {
        if self.demo_dark == value {
            return;
        }
        let mut state = self.as_mut().rust_mut();
        state.demo_dark = value;
        state.request(Pending::Everything);
        self.as_mut().demo_changed();
        self.update();
    }

    /// See the bridge declaration.
    pub fn set_demo_cursor_shape(mut self: Pin<&mut Self>, value: i32) {
        let value = value.clamp(0, 4);
        if self.demo_cursor_shape == value {
            return;
        }
        let mut state = self.as_mut().rust_mut();
        state.demo_cursor_shape = value;
        state.request(Pending::Cursor);
        self.as_mut().demo_changed();
        self.update();
    }

    /// See the bridge declaration.
    pub fn set_demo_animated(mut self: Pin<&mut Self>, value: bool) {
        if self.demo_animated == value {
            return;
        }
        let mut state = self.as_mut().rust_mut();
        state.demo_animated = value;
        // Back to the static demo when the benchmark stops.
        state.request(Pending::Everything);
        self.as_mut().demo_changed();
        self.update();
    }

    /// See the bridge declaration and `TerminalItemBase::fillFrame`.
    fn fill_frame(
        self: Pin<&mut Self>,
        request: &TerminalFrameRequest,
        info: &mut TerminalFrameInfo,
        rows: &mut Vec<u16>,
        cells: &mut Vec<TerminalCell>,
        clusters: &mut Vec<u32>,
    ) -> bool {
        let mut state = self.rust_mut();
        state.fill(request, info, rows, cells, clusters)
    }

    fn handle_key(
        self: Pin<&mut Self>,
        key: i32,
        modifiers: i32,
        text: &QString,
        keypad: bool,
        auto_repeat: bool,
    ) -> bool {
        // Key text is typed input: it may be a password, so only its length is logged.
        tracing::debug!(
            key = format_args!("{key:#x}"),
            modifiers = format_args!("{modifiers:#x}"),
            text_len = text.to_string().chars().count(),
            keypad,
            auto_repeat,
            demo = self.demo,
            "terminal key"
        );
        // Nothing consumes keys until the engine is connected, so Tab still moves the focus.
        false
    }

    fn handle_shortcut_override(self: Pin<&mut Self>, key: i32, modifiers: i32) -> bool {
        wants_shortcut_override(key, modifiers)
    }

    fn handle_mouse(self: Pin<&mut Self>, event: &TerminalMouseEvent) {
        if event.kind != MOUSE_MOVE {
            tracing::debug!(?event, "terminal mouse");
        }
    }

    fn handle_wheel(self: Pin<&mut Self>, event: &TerminalWheelEvent) {
        tracing::debug!(?event, "terminal wheel");
    }

    fn handle_hover(
        self: Pin<&mut Self>,
        _x: f64,
        _y: f64,
        _column: i32,
        _line: i32,
        _modifiers: i32,
    ) {
        // Link hover arrives with the engine (URL detection).
    }

    fn handle_focus_change(self: Pin<&mut Self>, focused: bool) {
        tracing::debug!(focused, "terminal focus");
    }

    fn handle_ime_commit(self: Pin<&mut Self>, text: &QString) {
        // Committed text is typed input: only its length is logged.
        tracing::debug!(
            text_len = text.to_string().chars().count(),
            "terminal input method commit"
        );
    }
}

/// `TerminalItemBase::MousePress` / `MouseRelease` / `MouseMove`.
const MOUSE_MOVE: i32 = 2;

// Qt key and modifier values (qnamespace.h).
const QT_KEY_F1: i32 = 0x0100_0030;
const QT_KEY_F11: i32 = 0x0100_003a;
const QT_KEY_F35: i32 = 0x0100_0052;
const QT_CONTROL_MODIFIER: i32 = 0x0400_0000;
const QT_ALT_MODIFIER: i32 = 0x0800_0000;

/// Whether the terminal takes a key away from the window shortcuts ([ADR 0011]): function keys
/// without Ctrl or Alt, except F11 (full screen). Ctrl+F6 / Ctrl+Shift+F6 therefore always leave
/// the terminal, and every Ctrl+Shift / Alt app shortcut keeps working.
///
/// [ADR 0011]: ../../../../docs/adr/0011-focus-regions-and-function-keys.md
fn wants_shortcut_override(key: i32, modifiers: i32) -> bool {
    let function_key = (QT_KEY_F1..=QT_KEY_F35).contains(&key);
    function_key && key != QT_KEY_F11 && modifiers & (QT_CONTROL_MODIFIER | QT_ALT_MODIFIER) == 0
}

fn cursor_shape_from_index(index: i32) -> CursorShape {
    match index {
        1 => CursorShape::HollowBlock,
        2 => CursorShape::Beam,
        3 => CursorShape::Underline,
        4 => CursorShape::Hidden,
        _ => CursorShape::Block,
    }
}

fn ffi_cursor_shape(shape: CursorShape) -> TerminalCursorShape {
    match shape {
        CursorShape::Block => TerminalCursorShape::Block,
        CursorShape::HollowBlock => TerminalCursorShape::HollowBlock,
        CursorShape::Beam => TerminalCursorShape::Beam,
        CursorShape::Underline => TerminalCursorShape::Underline,
        CursorShape::Hidden => TerminalCursorShape::Hidden,
    }
}

/// A frame of `columns` x `lines` empty cells on a transparent background, cursor hidden.
fn blank_frame(frame: &mut Frame, columns: u16, lines: u16) {
    frame.clear();
    frame.columns = columns;
    frame.lines = lines;
    frame.damage = Damage::Full;
    frame.background = 0;
    frame.cursor = snapshot::Cursor {
        shape: CursorShape::Hidden,
        ..snapshot::Cursor::default()
    };
    frame.rows.extend((0..lines).map(|index| snapshot::Row {
        index,
        cells: vec![snapshot::Cell::default(); usize::from(columns)],
    }));
}

/// Converts a [`Frame`] to the renderer's FFI form (see `TerminalItemBase::fillFrame`): the
/// damaged rows' cells back to back, exactly `frame.columns` per row (shorter rows are padded
/// with blank cells on the frame background, longer ones cut), and the combining characters
/// flattened into `clusters`.
pub fn write_frame(
    frame: &Frame,
    info: &mut TerminalFrameInfo,
    rows: &mut Vec<u16>,
    cells: &mut Vec<TerminalCell>,
    clusters: &mut Vec<u32>,
) {
    *info = TerminalFrameInfo {
        columns: frame.columns,
        lines: frame.lines,
        full: frame.damage == Damage::Full,
        background: frame.background,
        cursor_row: frame.cursor.row,
        cursor_column: frame.cursor.column,
        cursor_shape: ffi_cursor_shape(frame.cursor.shape),
        cursor_blinking: frame.cursor.blinking,
        cursor_wide: frame.cursor.wide,
        cursor_color: frame.cursor.color,
        cursor_text_color: frame.cursor.text_color,
    };
    rows.clear();
    cells.clear();
    clusters.clear();
    let columns = usize::from(frame.columns);
    let blank = TerminalCell {
        ch: u32::from(' '),
        cluster: 0,
        fg: frame.background,
        bg: frame.background,
        underline: frame.background,
        flags: 0,
    };
    for row in frame.rows.iter().filter(|row| row.index < frame.lines) {
        rows.push(row.index);
        for cell in row.cells.iter().take(columns) {
            let cluster = match cell.cluster.checked_sub(1) {
                Some(index) => match frame.clusters.get(index as usize) {
                    Some(marks) if !marks.is_empty() => {
                        let offset = clusters.len();
                        clusters.push(u32::try_from(marks.len()).unwrap_or(u32::MAX));
                        clusters.extend(marks.iter().map(|&mark| u32::from(mark)));
                        u32::try_from(offset + 1).unwrap_or(0)
                    }
                    _ => 0,
                },
                None => 0,
            };
            cells.push(TerminalCell {
                ch: u32::from(cell.ch),
                cluster,
                fg: cell.fg,
                bg: cell.bg,
                underline: cell.underline,
                flags: cell.flags,
            });
        }
        cells.extend(std::iter::repeat_n(
            blank,
            columns.saturating_sub(row.cells.len()),
        ));
    }
}

/// The built-in demo frame (gallery): every renderer feature on one screen. Its palettes are
/// placeholders for the demo only; the terminal's real palettes live in `opensesh_term`.
mod demo {
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
            0x1C1F27, 0xF07178, 0x9BD68A, 0xE6C07B, 0x73B7F2, 0xC9A0F0, 0x6ED6D0, 0xC8CDD6,
            0x4A5263, 0xFF8F95, 0xB4EBA3, 0xF2D39A, 0x9ACCFA, 0xDDBDFB, 0x94E8E3, 0xF2F4F8,
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
            0x1B1D22, 0xC8374D, 0x1E8F52, 0x9A6B00, 0x2B6CB0, 0x8E44AD, 0x11808A, 0xB8BCC4,
            0x5D6470, 0xE0445C, 0x26A862, 0xB7800F, 0x3D86D1, 0xA45BC6, 0x1A9AA6, 0xE6E8EE,
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
}

#[cfg(test)]
mod tests {
    use opensesh_term::snapshot::{Cell, Cursor, Row, flags};

    use super::qobject::TerminalCellFlag;
    use super::*;

    fn info() -> TerminalFrameInfo {
        TerminalFrameInfo {
            columns: 0,
            lines: 0,
            full: false,
            background: 0,
            cursor_row: 0,
            cursor_column: 0,
            cursor_shape: TerminalCursorShape::Hidden,
            cursor_blinking: false,
            cursor_wide: false,
            cursor_color: 0,
            cursor_text_color: 0,
        }
    }

    #[test]
    fn ffi_flags_match_the_snapshot_contract() {
        let pairs = [
            (TerminalCellFlag::Bold, flags::BOLD),
            (TerminalCellFlag::Italic, flags::ITALIC),
            (TerminalCellFlag::Underline, flags::UNDERLINE),
            (TerminalCellFlag::DoubleUnderline, flags::DOUBLE_UNDERLINE),
            (TerminalCellFlag::CurlyUnderline, flags::CURLY_UNDERLINE),
            (TerminalCellFlag::DottedUnderline, flags::DOTTED_UNDERLINE),
            (TerminalCellFlag::DashedUnderline, flags::DASHED_UNDERLINE),
            (TerminalCellFlag::Strikeout, flags::STRIKEOUT),
            (TerminalCellFlag::Wide, flags::WIDE),
            (TerminalCellFlag::WideSpacer, flags::WIDE_SPACER),
            (TerminalCellFlag::Link, flags::LINK),
            (TerminalCellFlag::Selected, flags::SELECTED),
            (TerminalCellFlag::Match, flags::MATCH),
            (TerminalCellFlag::Hidden, flags::HIDDEN),
        ];
        let mut all = 0;
        for (ffi, contract) in pairs {
            assert_eq!(ffi.repr, contract, "{ffi:?}");
            all |= contract;
        }
        // Every contract flag has an FFI twin.
        assert_eq!(all, (1 << 14) - 1);
    }

    #[test]
    fn cursor_shapes_map_one_to_one() {
        let shapes = [
            (CursorShape::Block, 0),
            (CursorShape::HollowBlock, 1),
            (CursorShape::Beam, 2),
            (CursorShape::Underline, 3),
            (CursorShape::Hidden, 4),
        ];
        for (shape, repr) in shapes {
            assert_eq!(ffi_cursor_shape(shape).repr, repr);
            assert_eq!(cursor_shape_from_index(i32::from(repr)), shape);
        }
        assert_eq!(cursor_shape_from_index(-1), CursorShape::Block);
        assert_eq!(cursor_shape_from_index(99), CursorShape::Block);
    }

    #[test]
    fn frames_are_flattened_with_clusters_and_fixed_widths() {
        let cell = |ch: char| Cell {
            ch,
            fg: 0xFF11_1111,
            bg: 0xFF22_2222,
            underline: 0xFF33_3333,
            ..Cell::default()
        };
        let mut accented = cell('e');
        accented.cluster = 2;
        let frame = Frame {
            columns: 3,
            lines: 4,
            damage: Damage::Partial,
            rows: vec![
                Row {
                    index: 2,
                    cells: vec![cell('a'), accented, cell('b'), cell('c')],
                },
                Row {
                    index: 0,
                    cells: vec![cell('x')],
                },
                // Outside the grid: dropped.
                Row {
                    index: 4,
                    cells: vec![cell('z'); 3],
                },
            ],
            clusters: vec![vec!['\u{300}'], vec!['\u{301}', '\u{323}']],
            cursor: Cursor {
                row: 2,
                column: 1,
                shape: CursorShape::Beam,
                blinking: true,
                wide: false,
                color: 0xFFAA_BBCC,
                text_color: 0xFF00_0000,
            },
            background: 0xFF12_1419,
            display_offset: 0,
            history_size: 0,
        };
        let mut out = info();
        let (mut rows, mut cells, mut clusters) = (Vec::new(), Vec::new(), Vec::new());
        write_frame(&frame, &mut out, &mut rows, &mut cells, &mut clusters);

        assert_eq!((out.columns, out.lines, out.full), (3, 4, false));
        assert_eq!(out.cursor_shape, TerminalCursorShape::Beam);
        assert!(out.cursor_blinking);
        assert_eq!((out.cursor_row, out.cursor_column), (2, 1));
        assert_eq!(out.background, 0xFF12_1419);
        assert_eq!(rows, [2, 0]);
        assert_eq!(cells.len(), 6);
        let chars: Vec<u32> = cells.iter().map(|cell| cell.ch).collect();
        assert_eq!(chars, ['a', 'e', 'b', 'x', ' ', ' '].map(u32::from));
        // The accented cell points at `[2, U+0301, U+0323]`.
        assert_eq!(cells[1].cluster, 1);
        assert_eq!(clusters, [2, 0x301, 0x323]);
        // Padding uses the frame background.
        assert_eq!(cells[4].bg, 0xFF12_1419);
        assert_eq!(cells[0].underline, 0xFF33_3333);
    }

    #[test]
    fn bad_cluster_references_are_dropped() {
        let frame = Frame {
            columns: 2,
            lines: 1,
            rows: vec![Row {
                index: 0,
                cells: vec![
                    Cell {
                        ch: 'a',
                        cluster: 7,
                        ..Cell::default()
                    },
                    Cell {
                        ch: 'b',
                        cluster: 1,
                        ..Cell::default()
                    },
                ],
            }],
            clusters: vec![Vec::new()],
            ..Frame::default()
        };
        let mut out = info();
        let (mut rows, mut cells, mut clusters) = (Vec::new(), Vec::new(), Vec::new());
        write_frame(&frame, &mut out, &mut rows, &mut cells, &mut clusters);
        assert_eq!(cells[0].cluster, 0);
        assert_eq!(cells[1].cluster, 0);
        assert!(clusters.is_empty());
    }

    #[test]
    fn function_keys_reach_the_terminal_except_f11_and_ctrl_or_alt() {
        const SHIFT: i32 = 0x0200_0000;
        const META: i32 = 0x1000_0000;
        const KEYPAD: i32 = 0x2000_0000;
        let f6 = QT_KEY_F1 + 5;
        assert!(wants_shortcut_override(f6, 0));
        assert!(wants_shortcut_override(f6, SHIFT));
        assert!(wants_shortcut_override(QT_KEY_F1, META | KEYPAD));
        assert!(wants_shortcut_override(QT_KEY_F35, 0));
        assert!(!wants_shortcut_override(f6, QT_CONTROL_MODIFIER));
        assert!(!wants_shortcut_override(f6, QT_CONTROL_MODIFIER | SHIFT));
        assert!(!wants_shortcut_override(f6, QT_ALT_MODIFIER));
        assert!(!wants_shortcut_override(QT_KEY_F11, 0));
        // Not function keys: Ctrl+Shift+T, plain letters, Tab.
        assert!(!wants_shortcut_override(0x54, QT_CONTROL_MODIFIER | SHIFT));
        assert!(!wants_shortcut_override(0x41, 0));
        assert!(!wants_shortcut_override(0x0100_0001, 0));
    }

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
        demo::build(&mut frame, 100, 24, true, CursorShape::Beam, None);
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
            demo::build(&mut frame, columns, lines, false, CursorShape::Block, None);
            assert_eq!(frame.rows.len(), usize::from(lines));
            check_rows(&frame);
            assert!(frame.cursor.row < lines && frame.cursor.column < columns.max(1));
            demo::build(
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
        assert_eq!(demo::indexed(&demo::DARK, 1), 0xFFF0_7178);
        assert_eq!(demo::indexed(&demo::DARK, 16), 0xFF00_0000);
        assert_eq!(demo::indexed(&demo::DARK, 196), 0xFFFF_0000);
        assert_eq!(demo::indexed(&demo::DARK, 231), 0xFFFF_FFFF);
        assert_eq!(demo::indexed(&demo::DARK, 232), 0xFF08_0808);
        assert_eq!(demo::indexed(&demo::DARK, 255), 0xFFEE_EEEE);
    }

    fn fill(state: &mut TerminalItemRust, columns: u16, lines: u16, full: bool) -> Option<usize> {
        let mut out = info();
        let (mut rows, mut cells, mut clusters) = (Vec::new(), Vec::new(), Vec::new());
        let request = TerminalFrameRequest {
            columns,
            lines,
            full,
        };
        state
            .fill(&request, &mut out, &mut rows, &mut cells, &mut clusters)
            .then_some(rows.len())
    }

    #[test]
    fn frames_are_only_sent_when_something_changed() {
        let mut state = TerminalItemRust::default();
        // No demo, nothing shown: nothing to send, unless a full frame is asked for.
        assert_eq!(fill(&mut state, 80, 24, false), None);
        assert_eq!(fill(&mut state, 80, 24, true), Some(24));

        state.demo = true;
        state.request(Pending::Everything);
        assert_eq!(fill(&mut state, 80, 24, false), Some(24));
        assert_eq!(fill(&mut state, 80, 24, false), None);
        // A cursor change sends no rows; a resize sends every row.
        state.demo_cursor_shape = 2;
        state.request(Pending::Cursor);
        assert_eq!(fill(&mut state, 80, 24, false), Some(0));
        assert_eq!(state.frame.cursor.shape, CursorShape::Beam);
        assert_eq!(fill(&mut state, 100, 30, false), Some(30));
        // The benchmark sends every row on every frame.
        state.demo_animated = true;
        assert_eq!(fill(&mut state, 100, 30, false), Some(30));
        assert_eq!(fill(&mut state, 100, 30, false), Some(30));

        // Turning the demo off clears the screen once.
        state.demo = false;
        state.demo_animated = false;
        assert_eq!(fill(&mut state, 100, 30, false), Some(30));
        assert_eq!(state.frame.background, 0);
        assert_eq!(state.frame.cursor.shape, CursorShape::Hidden);
        assert_eq!(fill(&mut state, 100, 30, false), None);
    }
}
