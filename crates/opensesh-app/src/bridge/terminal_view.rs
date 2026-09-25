//! `TerminalItem`: the terminal for QML (Sprint 2, [ADR 0013]).
//!
//! The C++ base `TerminalItemBase` (`cpp/terminal_item.h`) draws through the Qt Quick scene graph
//! with its own glyph atlas and turns Qt input events into calls to its pure virtual functions.
//! This Rust QObject derives from it and implements them:
//!
//! - `fillFrame` hands the renderer a snapshot of the terminal: an
//!   [`opensesh_term::snapshot::Frame`] converted by [`write_frame`] to the flat FFI structs
//!   below. It runs on the scene graph render thread while the GUI thread is blocked, so it only
//!   touches Rust state (the session's snapshot takes the engine's fair lock briefly).
//! - `handleKey`, `handleShortcutOverride`, `handleMouse`, `handleWheel`, `handleHover`,
//!   `handleFocusChange`, `handleImeCommit` and `handleGridSize` receive input and layout changes
//!   on the GUI thread, and turn them into engine calls with the pure encoders of
//!   `opensesh_term::input`.
//!
//! **Sessions.** Setting `sessionId` attaches the item to the session of that tab in the
//! [registry](crate::terminal::registry), which owns it: the session outlives the item, so a view
//! can move (Sprint 4 splits) without restarting the shell. The first item to attach starts a
//! local shell once the grid size is known. The engine's notices reach the item through its
//! `CxxQtThread` (the registry's waker queues `drain` on the GUI thread, once per batch); Qt is
//! never called from engine threads. Only closing the tab (`TerminalSessions.close`) ends the
//! session; destroying the item only detaches it.
//!
//! **QML API** (besides the C++ base's `fontFamily`, `fontPointSize`, `padding`, `reduceMotion`,
//! the read-only `columns`, `lines`, `cellWidth`, `cellHeight`, `gridSizeChanged` and
//! `requestFrame()`):
//!
//! - properties: `sessionId`, `dark` (the OpenSesh dark or light terminal colors), `copyOnSelect`,
//!   and read-only `title`, `workingDirectory`, `running`, `exitCode`, `exitCodeKnown`,
//!   `hasSelection`, `displayOffset`, `historySize` (for a scroll bar) and `searchError`;
//! - signals: `bell()`, `exited(code)`, `activity()` (new content while the item is hidden) and
//!   `contextMenuRequested(x, y)` (right click, or the Menu key at the cursor);
//! - invokables: `copy()`, `paste()`, `pasteSelection()` (Linux primary selection), `selectAll()`,
//!   `clearSelection()`, `find(pattern, forward)`, `clearSearch()`, `scrollLines(n)`,
//!   `scrollTo(offset)`, `scrollToBottom()`, `clearScrollback()`, `restart()`, and for tests
//!   `screenText()` and `sendText(text)`.
//!
//! The gallery shows the renderer with `demo: true`, a built-in frame that exercises every
//! renderer feature, and no session.
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
        /// The terminal item.
        #[qobject]
        #[qml_element]
        #[base = TerminalItemBase]
        #[qproperty(bool, demo, READ, WRITE = set_demo, NOTIFY = demo_changed)]
        #[qproperty(bool, demo_dark, cxx_name = "demoDark", READ, WRITE = set_demo_dark, NOTIFY = demo_changed)]
        #[qproperty(i32, demo_cursor_shape, cxx_name = "demoCursorShape", READ, WRITE = set_demo_cursor_shape, NOTIFY = demo_changed)]
        #[qproperty(bool, demo_animated, cxx_name = "demoAnimated", READ, WRITE = set_demo_animated, NOTIFY = demo_changed)]
        #[qproperty(i32, session_id, cxx_name = "sessionId", READ, WRITE = set_session_id, NOTIFY = session_id_changed)]
        #[qproperty(bool, dark, READ, WRITE = set_dark, NOTIFY = dark_changed)]
        #[qproperty(bool, copy_on_select, cxx_name = "copyOnSelect", READ, WRITE, NOTIFY)]
        #[qproperty(QString, title, READ, NOTIFY = session_info_changed)]
        #[qproperty(QString, working_directory, cxx_name = "workingDirectory", READ, NOTIFY = session_info_changed)]
        #[qproperty(bool, running, READ, NOTIFY = session_info_changed)]
        #[qproperty(i32, exit_code, cxx_name = "exitCode", READ, NOTIFY = session_info_changed)]
        #[qproperty(bool, exit_code_known, cxx_name = "exitCodeKnown", READ, NOTIFY = session_info_changed)]
        #[qproperty(bool, has_selection, cxx_name = "hasSelection", READ, NOTIFY = has_selection_changed)]
        #[qproperty(i32, display_offset, cxx_name = "displayOffset", READ, NOTIFY = view_changed)]
        #[qproperty(i32, history_size, cxx_name = "historySize", READ, NOTIFY = view_changed)]
        #[qproperty(QString, search_error, cxx_name = "searchError", READ, NOTIFY = search_error_changed)]
        type TerminalItem = super::TerminalItemRust;

        /// Emitted when a demo property changes.
        #[qsignal]
        #[cxx_name = "demoChanged"]
        fn demo_changed(self: Pin<&mut TerminalItem>);

        /// Emitted when `sessionId` changes.
        #[qsignal]
        #[cxx_name = "sessionIdChanged"]
        fn session_id_changed(self: Pin<&mut TerminalItem>);

        /// Emitted when `dark` changes.
        #[qsignal]
        #[cxx_name = "darkChanged"]
        fn dark_changed(self: Pin<&mut TerminalItem>);

        /// Emitted when the title, the working directory or the running and exit state change.
        #[qsignal]
        #[cxx_name = "sessionInfoChanged"]
        fn session_info_changed(self: Pin<&mut TerminalItem>);

        /// Emitted when `hasSelection` changes.
        #[qsignal]
        #[cxx_name = "hasSelectionChanged"]
        fn has_selection_changed(self: Pin<&mut TerminalItem>);

        /// Emitted when the scroll position or the history size changes.
        #[qsignal]
        #[cxx_name = "viewChanged"]
        fn view_changed(self: Pin<&mut TerminalItem>);

        /// Emitted when `searchError` changes.
        #[qsignal]
        #[cxx_name = "searchErrorChanged"]
        fn search_error_changed(self: Pin<&mut TerminalItem>);

        /// The program rang the bell.
        #[qsignal]
        fn bell(self: Pin<&mut TerminalItem>);

        /// The program ended; `code` is meaningful only when `exitCodeKnown` is true.
        #[qsignal]
        fn exited(self: Pin<&mut TerminalItem>, code: i32);

        /// The terminal changed while the item is hidden (a background tab has new output).
        #[qsignal]
        fn activity(self: Pin<&mut TerminalItem>);

        /// Show the context menu at `x`, `y` (item coordinates).
        #[qsignal]
        #[cxx_name = "contextMenuRequested"]
        fn context_menu_requested(self: Pin<&mut TerminalItem>, x: f64, y: f64);

        /// Shows the built-in demo frame instead of a terminal.
        fn set_demo(self: Pin<&mut TerminalItem>, value: bool);
        /// Demo colors: the dark or the light palette.
        fn set_demo_dark(self: Pin<&mut TerminalItem>, value: bool);
        /// Demo cursor: 0 block, 1 hollow block, 2 beam, 3 underline, 4 hidden.
        fn set_demo_cursor_shape(self: Pin<&mut TerminalItem>, value: i32);
        /// Demo benchmark: every row changes on every frame.
        fn set_demo_animated(self: Pin<&mut TerminalItem>, value: bool);
        /// Attaches to the session of tab `id` (0 for none), starting a local shell if needed.
        fn set_session_id(self: Pin<&mut TerminalItem>, id: i32);
        /// The OpenSesh dark (true) or light terminal colors.
        fn set_dark(self: Pin<&mut TerminalItem>, value: bool);

        /// Copies the selection to the clipboard. Returns whether there was one.
        #[qinvokable]
        fn copy(self: Pin<&mut TerminalItem>) -> bool;

        /// Pastes the clipboard (sanitised, bracketed when the program asked for it).
        #[qinvokable]
        fn paste(self: Pin<&mut TerminalItem>);

        /// Pastes the primary selection (X11 and Wayland; nothing elsewhere).
        #[qinvokable]
        #[cxx_name = "pasteSelection"]
        fn paste_selection(self: Pin<&mut TerminalItem>);

        /// Selects the whole buffer, scrollback included.
        #[qinvokable]
        #[cxx_name = "selectAll"]
        fn select_all(self: Pin<&mut TerminalItem>);

        /// Removes the selection.
        #[qinvokable]
        #[cxx_name = "clearSelection"]
        fn clear_selection(self: Pin<&mut TerminalItem>);

        /// Finds the next match of a regex (`forward`: toward newer output) and scrolls to it.
        /// Returns whether there is a match; an invalid pattern sets `searchError`.
        #[qinvokable]
        fn find(self: Pin<&mut TerminalItem>, pattern: &QString, forward: bool) -> bool;

        /// Ends the search and removes its highlights.
        #[qinvokable]
        #[cxx_name = "clearSearch"]
        fn clear_search(self: Pin<&mut TerminalItem>);

        /// Scrolls through the history: positive `lines` go up (older output).
        #[qinvokable]
        #[cxx_name = "scrollLines"]
        fn scroll_lines(self: Pin<&mut TerminalItem>, lines: i32);

        /// Scrolls so the view is `offset` lines above the live screen.
        #[qinvokable]
        #[cxx_name = "scrollTo"]
        fn scroll_to(self: Pin<&mut TerminalItem>, offset: i32);

        /// Back to the live screen.
        #[qinvokable]
        #[cxx_name = "scrollToBottom"]
        fn scroll_to_bottom(self: Pin<&mut TerminalItem>);

        /// Clears the scrollback history (the screen stays).
        #[qinvokable]
        #[cxx_name = "clearScrollback"]
        fn clear_scrollback(self: Pin<&mut TerminalItem>);

        /// Ends the session of this tab and starts a new local shell. Returns whether it started.
        #[qinvokable]
        fn restart(self: Pin<&mut TerminalItem>) -> bool;

        /// The visible screen as text, one line per row (tests and the smoke test).
        #[qinvokable]
        #[cxx_name = "screenText"]
        fn screen_text(self: &TerminalItem) -> QString;

        /// Types `text` into the program, as the keyboard would (tests and the smoke test).
        #[qinvokable]
        #[cxx_name = "sendText"]
        fn send_text(self: Pin<&mut TerminalItem>, text: &QString);

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

        /// Pointer moved without a button pressed, or left the item (column and line -1).
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

        /// The grid or cell size changed (cells in device pixels).
        #[cxx_override]
        #[cxx_name = "handleGridSize"]
        fn handle_grid_size(
            self: Pin<&mut TerminalItem>,
            columns: i32,
            lines: i32,
            cell_width: i32,
            cell_height: i32,
        );
    }

    unsafe extern "RustQt" {
        /// `QQuickItem::update()`: schedules a new frame (GUI thread only).
        #[inherit]
        fn update(self: Pin<&mut TerminalItem>);

        /// `QQuickItem::isVisible()`.
        #[inherit]
        #[cxx_name = "isVisible"]
        fn is_visible(self: &TerminalItem) -> bool;

        /// `QQuickItem::hasActiveFocus()`.
        #[inherit]
        #[cxx_name = "hasActiveFocus"]
        fn has_active_focus(self: &TerminalItem) -> bool;

        /// Cell width in logical pixels.
        #[inherit]
        #[cxx_name = "cellWidth"]
        fn cell_width(self: &TerminalItem) -> f64;

        /// Cell height in logical pixels.
        #[inherit]
        #[cxx_name = "cellHeight"]
        fn cell_height(self: &TerminalItem) -> f64;

        /// Space around the grid in logical pixels.
        #[inherit]
        fn padding(self: &TerminalItem) -> f64;

        /// Sets the clipboard, or the primary selection where there is one.
        #[inherit]
        #[cxx_name = "setClipboardText"]
        fn set_clipboard_text(
            self: Pin<&mut TerminalItem>,
            text: &QString,
            primary_selection: bool,
        );

        /// Reads the clipboard, or the primary selection (empty where there is none).
        #[inherit]
        #[cxx_name = "clipboardText"]
        fn clipboard_text(self: &TerminalItem, primary_selection: bool) -> QString;

        /// Whether the platform has a primary selection (X11, most Wayland compositors).
        #[inherit]
        #[cxx_name = "supportsPrimarySelection"]
        fn supports_primary_selection(self: &TerminalItem) -> bool;

        /// Pointing hand over a link, text cursor elsewhere.
        #[inherit]
        #[cxx_name = "setLinkCursor"]
        fn set_link_cursor(self: Pin<&mut TerminalItem>, over_link: bool);

        /// Opens a URL with the desktop's handler.
        #[inherit]
        #[cxx_name = "openUrl"]
        fn open_url(self: &TerminalItem, url: &QString) -> bool;
    }

    impl cxx_qt::Threading for TerminalItem {}
}

use core::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use opensesh_term::backend::TermSize;
use opensesh_term::input::keys::{
    Key, KeyInput, KeyOptions, encode_key, modifiers_from_qt, qt, terminal_wants_shortcut,
};
use opensesh_term::input::mouse::{
    MouseAction, MouseButton, MouseInput, MouseProtocol, alternate_scroll, encode_mouse,
    mouse_protocol, mouse_reporting_active,
};
use opensesh_term::input::paste::encode_paste;
use opensesh_term::input::{InputModes, Modifiers};
use opensesh_term::links;
use opensesh_term::palette::Palette;
use opensesh_term::session::{Scroll, SelectionKind, Side, ViewportPoint};
use opensesh_term::snapshot::{self, CursorShape, Damage, Frame};

use crate::terminal::demo;
use crate::terminal::interaction::{
    LINES_PER_NOTCH, ScreenText, WheelSteps, display_title, side_of, unswap_alt_wheel, url_at,
};
use crate::terminal::registry::{self, LocalOptions, SessionEntry, SessionInfo, Waker};
use qobject::{
    TerminalCell, TerminalCursorShape, TerminalFrameInfo, TerminalFrameRequest, TerminalMouseEvent,
    TerminalWheelEvent,
};

/// What the next `fillFrame` has to send (demo only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Pending {
    /// Nothing changed.
    Nothing,
    /// Only the cursor changed.
    Cursor,
    /// Every row.
    Everything,
}

/// How long after a change the app itself made (a resize, new colors, attaching) a redraw of a
/// hidden terminal is not reported as activity.
const QUIET_AFTER_CHANGE: Duration = Duration::from_millis(600);

/// Most wheel reports written for one wheel event in mouse reporting mode.
const MAX_WHEEL_REPORTS: i32 = 10;

// `Qt::MouseButton` values.
const QT_LEFT_BUTTON: i32 = 0x1;
const QT_RIGHT_BUTTON: i32 = 0x2;
const QT_MIDDLE_BUTTON: i32 = 0x4;

/// `TerminalItemBase::MousePress` / `MouseRelease` / `MouseMove`.
const MOUSE_PRESS: i32 = 0;
const MOUSE_RELEASE: i32 = 1;
const MOUSE_MOVE: i32 = 2;

/// The session an item shows.
struct Attached {
    entry: Arc<SessionEntry>,
    /// Tells this attachment apart from later ones (see [`registry::next_token`]).
    token: u64,
}

/// Mouse state between events (GUI thread).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct MouseState {
    /// Qt buttons whose press was reported to the program (their release is reported too).
    reported: i32,
    /// Cell of the last reported event: motion is reported only when the cell changes.
    last_cell: Option<ViewportPoint>,
    /// The left button is dragging a local selection.
    selecting: bool,
    /// The press opened a link: its move and release events are ignored.
    link_click: bool,
}

/// A link under the pointer while Ctrl is held.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Link {
    url: String,
    /// The cells of a detected URL (an OSC 8 link is always underlined, so it has none).
    range: Option<(ViewportPoint, ViewportPoint)>,
}

/// Rust state behind `TerminalItem`.
pub struct TerminalItemRust {
    // Demo (gallery).
    demo: bool,
    demo_dark: bool,
    demo_cursor_shape: i32,
    demo_animated: bool,
    pending: Pending,
    tick: u64,

    // QML properties.
    session_id: i32,
    dark: bool,
    copy_on_select: bool,
    title: QString,
    working_directory: QString,
    running: bool,
    exit_code: i32,
    exit_code_known: bool,
    has_selection: bool,
    display_offset: i32,
    history_size: i32,
    search_error: QString,

    // Written by `fillFrame` (render thread, GUI thread blocked) and read on the GUI thread.
    /// Grid size of the last frame sent.
    sent_size: (u16, u16),
    /// Whether the last frame sent had content (so the screen is cleared when it goes away).
    showing: bool,
    /// The last snapshot (reused between frames).
    frame: Frame,
    /// The text of the rows shown, for link detection.
    screen: ScreenText,
    /// Display offset and history size of the last frame.
    view_latest: (usize, usize),
    /// The values the properties were last set to.
    view_published: (usize, usize),
    /// A `sync_view` call is queued on the GUI thread.
    view_sync_queued: bool,
    /// The next frame must hold every row (the renderer has no copy of this session's grid).
    needs_full: bool,
    /// The view may be scrolled into the history: typed input scrolls back down first.
    scrolled: bool,

    // GUI thread.
    attached: Option<Attached>,
    /// Boxed: `CxxQtThread` is not `Unpin`, and `rust_mut()` needs this struct to be.
    thread: Option<Box<CxxQtThread<qobject::TerminalItem>>>,
    /// Grid size and cell size, once the item has a window and a size.
    grid: Option<TermSize>,
    /// Redraws of a hidden terminal before this instant are not activity.
    quiet_until: Option<Instant>,
    /// The focus state last told to the session.
    focus_sent: Option<bool>,
    mouse: MouseState,
    wheel: WheelSteps,
    /// The pointer is over an openable link (pointing-hand cursor).
    link_hovered: bool,
    /// The detected URL currently underlined.
    link_range: Option<(ViewportPoint, ViewportPoint)>,
}

impl std::fmt::Debug for TerminalItemRust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalItemRust")
            .field("demo", &self.demo)
            .field("session_id", &self.session_id)
            .field("attached", &self.attached.is_some())
            .field("grid", &self.grid)
            .finish_non_exhaustive()
    }
}

impl Default for TerminalItemRust {
    fn default() -> Self {
        Self {
            demo: false,
            demo_dark: true,
            demo_cursor_shape: 0,
            demo_animated: false,
            pending: Pending::Nothing,
            tick: 0,
            session_id: 0,
            dark: true,
            copy_on_select: false,
            title: QString::default(),
            working_directory: QString::default(),
            running: false,
            exit_code: 0,
            exit_code_known: false,
            has_selection: false,
            display_offset: 0,
            history_size: 0,
            search_error: QString::default(),
            sent_size: (0, 0),
            showing: false,
            frame: Frame::default(),
            screen: ScreenText::default(),
            view_latest: (0, 0),
            view_published: (0, 0),
            view_sync_queued: false,
            needs_full: true,
            scrolled: false,
            attached: None,
            thread: None,
            grid: None,
            quiet_until: None,
            focus_sent: None,
            mouse: MouseState::default(),
            wheel: WheelSteps::default(),
            link_hovered: false,
            link_range: None,
        }
    }
}

impl Drop for TerminalItemRust {
    fn drop(&mut self) {
        // The session lives on in the registry until its tab closes.
        if let Some(attached) = self.attached.take() {
            attached.entry.detach(attached.token);
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
        if self.demo {
            return self.fill_demo(request, info, rows, cells, clusters);
        }
        if let Some(attached) = &self.attached {
            let session = attached.entry.session();
            if full || self.needs_full {
                session.snapshot_full(&mut self.frame);
                self.needs_full = false;
            } else {
                session.snapshot(&mut self.frame);
            }
            self.screen.apply(&self.frame);
            self.publish_view();
            self.showing = true;
        } else {
            // No session: clear what was on screen, once.
            if !(self.showing || full) {
                return false;
            }
            blank_frame(&mut self.frame, columns, lines);
            self.showing = false;
            self.needs_full = true;
        }
        write_frame(&self.frame, info, rows, cells, clusters);
        self.sent_size = (columns, lines);
        true
    }

    fn fill_demo(
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
        write_frame(&self.frame, info, rows, cells, clusters);
        self.pending = Pending::Nothing;
        self.sent_size = (columns, lines);
        true
    }

    /// After a snapshot (render thread): hands the scroll position and history size to the GUI
    /// thread when they changed, with one queued call at a time.
    fn publish_view(&mut self) {
        let latest = (self.frame.display_offset, self.frame.history_size);
        self.view_latest = latest;
        self.scrolled = self.frame.display_offset > 0;
        if latest == self.view_published || self.view_sync_queued {
            return;
        }
        if let Some(thread) = &self.thread {
            self.view_sync_queued = thread.queue(|item| item.sync_view()).is_ok();
        }
    }

    fn request(&mut self, pending: Pending) {
        self.pending = self.pending.max(pending);
    }

    /// The attached session, if any.
    fn entry(&self) -> Option<Arc<SessionEntry>> {
        self.attached
            .as_ref()
            .map(|attached| Arc::clone(&attached.entry))
    }

    /// Whether a redraw now is the app's own doing rather than the program's.
    fn quiet(&self) -> bool {
        self.quiet_until.is_some_and(|until| Instant::now() < until)
    }

    /// The link at `point`: an OSC 8 hyperlink, else a URL detected in the row's text.
    fn link_at(&self, entry: &SessionEntry, point: ViewportPoint) -> Option<Link> {
        if let Some(url) = entry.session().hyperlink_at(point) {
            return Some(Link { url, range: None });
        }
        let found = url_at(self.screen.row(point.row)?, point.column)?;
        Some(Link {
            url: found.url,
            range: Some((
                ViewportPoint::new(point.row, found.first),
                ViewportPoint::new(point.row, found.last),
            )),
        })
    }
}

/// The terminal colors for the app's dark or light theme.
fn palette_for(dark: bool) -> Palette {
    if dark {
        Palette::OPENSESH_DARK
    } else {
        Palette::OPENSESH_LIGHT
    }
}

/// A grid size from the C++ side, `None` while any part is 0 (no window yet).
fn term_size(columns: i32, lines: i32, cell_width: i32, cell_height: i32) -> Option<TermSize> {
    let positive = |value: i32| u16::try_from(value).ok().filter(|&value| value > 0);
    Some(TermSize {
        columns: positive(columns)?,
        lines: positive(lines)?,
        cell_width: positive(cell_width)?,
        cell_height: positive(cell_height)?,
    })
}

/// A cell from the C++ side (already clamped to the grid).
fn viewport_point(column: i32, line: i32) -> ViewportPoint {
    let clamp = |value: i32| u16::try_from(value.max(0)).unwrap_or(u16::MAX);
    ViewportPoint::new(clamp(line), clamp(column))
}

/// `Qt::KeyboardModifiers` as the encoders take them.
fn qt_bits(modifiers: i32) -> u32 {
    u32::from_ne_bytes(modifiers.to_ne_bytes())
}

/// The mouse button a Qt button value stands for.
fn mouse_button(qt_button: i32) -> Option<MouseButton> {
    match qt_button {
        QT_LEFT_BUTTON => Some(MouseButton::Left),
        QT_MIDDLE_BUTTON => Some(MouseButton::Middle),
        QT_RIGHT_BUTTON => Some(MouseButton::Right),
        _ => None,
    }
}

/// The button reported with motion: the lowest one held (left, middle, right).
fn held_button(qt_buttons: i32) -> MouseButton {
    if qt_buttons & QT_LEFT_BUTTON != 0 {
        MouseButton::Left
    } else if qt_buttons & QT_MIDDLE_BUTTON != 0 {
        MouseButton::Middle
    } else if qt_buttons & QT_RIGHT_BUTTON != 0 {
        MouseButton::Right
    } else {
        MouseButton::None
    }
}

/// `(code, known)` for the `exitCode` and `exitCodeKnown` properties.
fn exit_status(exit: Option<Option<i32>>) -> (i32, bool) {
    match exit {
        Some(Some(code)) => (code, true),
        _ => (-1, false),
    }
}

/// Whether the terminal takes a key away from the window shortcuts ([ADR 0011]): function keys
/// without Ctrl or Alt, except F11 (full screen). Ctrl+F6 / Ctrl+Shift+F6 therefore always leave
/// the terminal, and every Ctrl+Shift / Alt app shortcut keeps working.
///
/// [ADR 0011]: ../../../../docs/adr/0011-focus-regions-and-function-keys.md
fn wants_shortcut_override(key: i32, modifiers: i32) -> bool {
    let bits = qt_bits(modifiers);
    let key = Key::from_qt(key, bits & qt::KEYPAD_MODIFIER != 0);
    terminal_wants_shortcut(&key, modifiers_from_qt(bits))
}

impl qobject::TerminalItem {
    /// See the bridge declaration.
    pub fn set_demo(mut self: Pin<&mut Self>, value: bool) {
        if self.demo == value {
            return;
        }
        let mut state = self.as_mut().rust_mut();
        state.demo = value;
        state.needs_full = true;
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

    /// See the bridge declaration.
    pub fn set_session_id(mut self: Pin<&mut Self>, id: i32) {
        let id = id.max(0);
        if self.session_id == id {
            return;
        }
        self.as_mut().detach();
        self.as_mut().rust_mut().session_id = id;
        self.as_mut().session_id_changed();
        self.as_mut().attach_or_start();
    }

    /// See the bridge declaration.
    pub fn set_dark(mut self: Pin<&mut Self>, value: bool) {
        if self.dark == value {
            return;
        }
        {
            let mut state = self.as_mut().rust_mut();
            state.dark = value;
            state.quiet_until = Some(Instant::now() + QUIET_AFTER_CHANGE);
        }
        if let Some(entry) = self.entry() {
            entry.session().set_palette(palette_for(value));
        }
        self.as_mut().dark_changed();
    }

    /// Attaches to the session of `sessionId`, or starts it once the grid size is known.
    fn attach_or_start(mut self: Pin<&mut Self>) {
        let id = self.session_id;
        if id <= 0 || self.attached.is_some() {
            return;
        }
        let entry = match registry::get(id) {
            Some(entry) => entry,
            None => {
                // The shell starts with the size of the grid it is shown in.
                let Some(size) = self.grid else {
                    return;
                };
                let options = LocalOptions {
                    size,
                    palette: palette_for(self.dark),
                };
                match registry::open_local(id, options) {
                    Ok(entry) => entry,
                    Err(error) => {
                        tracing::error!(id, %error, "could not start a local terminal");
                        self.as_mut().publish_info(
                            &SessionInfo {
                                exit: Some(None),
                                ..SessionInfo::default()
                            },
                            false,
                        );
                        self.as_mut().exited(-1);
                        return;
                    }
                }
            }
        };
        self.as_mut().attach(entry);
    }

    fn attach(mut self: Pin<&mut Self>, entry: Arc<SessionEntry>) {
        let token = registry::next_token();
        let thread = self.qt_thread();
        let waker: Waker = {
            let thread = thread.clone();
            Arc::new(move || thread.queue(move |item| item.drain(token)).is_ok())
        };
        let focused = self.has_active_focus();
        let (dark, grid) = (self.dark, self.grid);
        {
            let mut state = self.as_mut().rust_mut();
            state.attached = Some(Attached {
                entry: Arc::clone(&entry),
                token,
            });
            state.thread = Some(Box::new(thread));
            state.needs_full = true;
            state.quiet_until = Some(Instant::now() + QUIET_AFTER_CHANGE);
            state.focus_sent = Some(focused);
            state.mouse = MouseState::default();
        }
        let session = entry.session();
        session.set_palette(palette_for(dark));
        if let Some(size) = grid {
            session.resize(size);
        }
        session.focus_changed(focused);
        entry.attach(token, waker);
        self.as_mut().publish_info(&entry.info(), true);
        self.update();
    }

    /// Lets go of the session (it keeps running in the registry).
    fn detach(mut self: Pin<&mut Self>) {
        let Some(attached) = self.as_mut().rust_mut().attached.take() else {
            return;
        };
        attached.entry.detach(attached.token);
        if self.link_range.is_some() {
            attached.entry.session().set_link_highlight(None);
        }
        {
            let mut state = self.as_mut().rust_mut();
            state.needs_full = true;
            state.focus_sent = None;
            state.mouse = MouseState::default();
            state.link_range = None;
        }
        if self.link_hovered {
            self.as_mut().rust_mut().link_hovered = false;
            self.as_mut().set_link_cursor(false);
        }
        self.as_mut().publish_info(&SessionInfo::default(), false);
        self.as_mut().set_has_selection_value(false);
        self.update();
    }

    /// Takes the session's events (GUI thread; queued by the registry's waker).
    fn drain(mut self: Pin<&mut Self>, token: u64) {
        let Some(entry) = self
            .attached
            .as_ref()
            .filter(|attached| attached.token == token)
            .map(|attached| Arc::clone(&attached.entry))
        else {
            return;
        };
        let Some((events, info)) = entry.take_events(token) else {
            return;
        };
        if events.dirty {
            self.as_mut().on_dirty(&entry);
        }
        if events.info || events.exited {
            self.as_mut().publish_info(&info, true);
        }
        if events.bell {
            self.as_mut().bell();
        }
        if events.exited {
            let (code, _) = exit_status(info.exit);
            tracing::info!(id = entry.id(), code = ?info.exit, "terminal program exited");
            self.as_mut().exited(code);
        }
    }

    /// The screen changed: draw it, or report activity while hidden.
    fn on_dirty(mut self: Pin<&mut Self>, entry: &SessionEntry) {
        if self.is_visible() {
            self.update();
            return;
        }
        if self.quiet() {
            // The app's own change (a resize, new colors). Take the snapshot nobody draws, so the
            // engine sends a new notice when the program writes something.
            let mut state = self.as_mut().rust_mut();
            entry.session().snapshot(&mut state.frame);
            state.needs_full = true;
        } else {
            self.as_mut().activity();
        }
    }

    /// Mirrors the session's title, directory and exit state into the properties.
    fn publish_info(mut self: Pin<&mut Self>, info: &SessionInfo, attached: bool) {
        let title = info
            .title
            .as_deref()
            .map(|title| display_title(title, cfg!(windows)))
            .unwrap_or_default();
        let title = QString::from(&title);
        let directory = QString::from(info.working_directory.as_deref().unwrap_or_default());
        let running = attached && info.exit.is_none();
        let (code, known) = exit_status(info.exit);
        let changed = self.title != title
            || self.working_directory != directory
            || self.running != running
            || self.exit_code != code
            || self.exit_code_known != known;
        if !changed {
            return;
        }
        {
            let mut state = self.as_mut().rust_mut();
            state.title = title;
            state.working_directory = directory;
            state.running = running;
            state.exit_code = code;
            state.exit_code_known = known;
        }
        self.as_mut().session_info_changed();
    }

    /// Sets the scroll properties from the last frame (queued by `fillFrame`).
    fn sync_view(mut self: Pin<&mut Self>) {
        let (offset, history) = self.view_latest;
        {
            let mut state = self.as_mut().rust_mut();
            state.view_sync_queued = false;
            state.view_published = (offset, history);
        }
        let offset = i32::try_from(offset).unwrap_or(i32::MAX);
        let history = i32::try_from(history).unwrap_or(i32::MAX);
        if self.display_offset != offset || self.history_size != history {
            {
                let mut state = self.as_mut().rust_mut();
                state.display_offset = offset;
                state.history_size = history;
            }
            self.as_mut().view_changed();
        }
    }

    fn set_has_selection_value(mut self: Pin<&mut Self>, value: bool) {
        if self.has_selection != value {
            self.as_mut().rust_mut().has_selection = value;
            self.as_mut().has_selection_changed();
        }
    }

    fn set_search_error_value(mut self: Pin<&mut Self>, value: QString) {
        if self.search_error != value {
            self.as_mut().rust_mut().search_error = value;
            self.as_mut().search_error_changed();
        }
    }

    /// Writes typed or pasted input, back on the live screen first.
    fn send_input(mut self: Pin<&mut Self>, entry: &SessionEntry, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if self.scrolled {
            entry.session().scroll(Scroll::Bottom);
            self.as_mut().rust_mut().scrolled = false;
        }
        entry.session().write(bytes);
    }

    /// Pastes `text` (sanitised and bracketed by `encode_paste`).
    fn paste_text(self: Pin<&mut Self>, text: &str) {
        let Some(entry) = self.entry() else {
            return;
        };
        let bytes = encode_paste(text, &entry.session().modes());
        self.send_input(&entry, &bytes);
    }

    /// A selection gesture ended: update `hasSelection`, copy on select, and set the primary
    /// selection where there is one.
    fn finish_selection(mut self: Pin<&mut Self>, entry: &SessionEntry) {
        let text = entry.session().selection_text();
        self.as_mut().set_has_selection_value(text.is_some());
        let Some(text) = text else {
            return;
        };
        let text = QString::from(&text);
        if self.copy_on_select {
            self.as_mut().set_clipboard_text(&text, false);
        }
        if self.supports_primary_selection() {
            self.as_mut().set_clipboard_text(&text, true);
        }
    }

    /// Opens `url` if it is a kind of link that may be opened without asking (PLAN §8).
    fn open_link(&self, url: &str) -> bool {
        // Only the scheme is logged: a URL can carry tokens.
        let scheme = url.split(':').next().unwrap_or_default();
        if !links::is_openable(url) {
            tracing::info!(scheme, "not opening a link of this kind");
            return false;
        }
        let opened = self.open_url(&QString::from(url));
        if opened {
            tracing::info!(scheme, "opened a link");
        } else {
            tracing::warn!(scheme, "the desktop could not open a link");
        }
        opened
    }

    /// Underlines the openable link under the pointer and shows a pointing hand, or removes both.
    fn set_link_hover(mut self: Pin<&mut Self>, entry: &SessionEntry, link: Option<&Link>) {
        let hovered = link.is_some();
        let range = link.and_then(|link| link.range);
        if self.link_range != range {
            entry.session().set_link_highlight(range);
            self.as_mut().rust_mut().link_range = range;
        }
        if self.link_hovered != hovered {
            self.as_mut().rust_mut().link_hovered = hovered;
            self.as_mut().set_link_cursor(hovered);
        }
    }

    /// The pixel position of the bottom-right corner of the cursor cell (for the context menu).
    fn cursor_position(&self) -> (f64, f64) {
        let cursor = self.frame.cursor;
        let padding = self.padding();
        (
            padding + (f64::from(cursor.column) + 1.0) * self.cell_width(),
            padding + (f64::from(cursor.row) + 1.0) * self.cell_height(),
        )
    }

    // ---- Invokables ------------------------------------------------------------------------

    /// See the bridge declaration.
    pub fn copy(mut self: Pin<&mut Self>) -> bool {
        let Some(entry) = self.entry() else {
            return false;
        };
        match entry.session().selection_text() {
            Some(text) => {
                self.as_mut()
                    .set_clipboard_text(&QString::from(&text), false);
                true
            }
            None => {
                self.as_mut().set_has_selection_value(false);
                false
            }
        }
    }

    /// See the bridge declaration.
    pub fn paste(self: Pin<&mut Self>) {
        let text = self.clipboard_text(false).to_string();
        self.paste_text(&text);
    }

    /// See the bridge declaration.
    pub fn paste_selection(self: Pin<&mut Self>) {
        if !self.supports_primary_selection() {
            return;
        }
        let text = self.clipboard_text(true).to_string();
        self.paste_text(&text);
    }

    /// See the bridge declaration.
    pub fn select_all(self: Pin<&mut Self>) {
        let (Some(entry), Some(size)) = (self.entry(), self.grid) else {
            return;
        };
        let session = entry.session();
        let offset = self.display_offset;
        // Viewport points: from the first cell of the oldest line to the last cell on screen.
        session.scroll(Scroll::Top);
        session.selection_start(ViewportPoint::new(0, 0), Side::Left, SelectionKind::Simple);
        session.scroll(Scroll::Bottom);
        session.selection_update(
            ViewportPoint::new(size.lines.saturating_sub(1), size.columns.saturating_sub(1)),
            Side::Right,
        );
        if offset > 0 {
            session.scroll(Scroll::Lines(offset));
        }
        self.finish_selection(&entry);
    }

    /// See the bridge declaration.
    pub fn clear_selection(mut self: Pin<&mut Self>) {
        if let Some(entry) = self.entry() {
            entry.session().selection_clear();
        }
        self.as_mut().set_has_selection_value(false);
    }

    /// See the bridge declaration.
    pub fn find(mut self: Pin<&mut Self>, pattern: &QString, forward: bool) -> bool {
        let Some(entry) = self.entry() else {
            return false;
        };
        let (found, error) = match entry.session().search(&pattern.to_string(), forward) {
            Ok(found) => (found.is_some(), String::new()),
            Err(error) => (false, error.to_string()),
        };
        if found {
            self.as_mut().rust_mut().scrolled = true;
        }
        self.as_mut().set_search_error_value(QString::from(&error));
        found
    }

    /// See the bridge declaration.
    pub fn clear_search(mut self: Pin<&mut Self>) {
        if let Some(entry) = self.entry() {
            entry.session().search_clear();
        }
        self.as_mut().set_search_error_value(QString::default());
    }

    /// See the bridge declaration.
    pub fn scroll_lines(mut self: Pin<&mut Self>, lines: i32) {
        if let Some(entry) = self.entry() {
            entry.session().scroll(Scroll::Lines(lines));
            self.as_mut().rust_mut().scrolled = true;
        }
    }

    /// See the bridge declaration.
    pub fn scroll_to(mut self: Pin<&mut Self>, offset: i32) {
        let Some(entry) = self.entry() else {
            return;
        };
        // Absolute: the display offset of the last frame may be out of date while dragging.
        let session = entry.session();
        session.scroll(Scroll::Bottom);
        if offset > 0 {
            session.scroll(Scroll::Lines(offset));
        }
        self.as_mut().rust_mut().scrolled = offset > 0;
    }

    /// See the bridge declaration.
    pub fn scroll_to_bottom(mut self: Pin<&mut Self>) {
        if let Some(entry) = self.entry() {
            entry.session().scroll(Scroll::Bottom);
            self.as_mut().rust_mut().scrolled = false;
        }
    }

    /// See the bridge declaration.
    pub fn clear_scrollback(self: Pin<&mut Self>) {
        if let Some(entry) = self.entry() {
            entry.session().clear_history();
        }
    }

    /// See the bridge declaration.
    pub fn restart(mut self: Pin<&mut Self>) -> bool {
        let id = self.session_id;
        let Some(size) = self.grid else {
            return false;
        };
        if id <= 0 {
            return false;
        }
        self.as_mut().detach();
        tracing::info!(id, "restarting a local terminal");
        let options = LocalOptions {
            size,
            palette: palette_for(self.dark),
        };
        match registry::restart_local(id, options) {
            Ok(entry) => {
                self.as_mut().attach(entry);
                true
            }
            Err(error) => {
                tracing::error!(id, %error, "could not restart a local terminal");
                false
            }
        }
    }

    /// See the bridge declaration.
    pub fn screen_text(&self) -> QString {
        self.entry()
            .map(|entry| QString::from(&entry.session().text_dump()))
            .unwrap_or_default()
    }

    /// See the bridge declaration.
    pub fn send_text(self: Pin<&mut Self>, text: &QString) {
        let Some(entry) = self.entry() else {
            return;
        };
        let text = text.to_string();
        self.send_input(&entry, text.as_bytes());
    }

    // ---- C++ overrides ---------------------------------------------------------------------

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
        mut self: Pin<&mut Self>,
        key: i32,
        modifiers: i32,
        text: &QString,
        _keypad: bool,
        _auto_repeat: bool,
    ) -> bool {
        // Without a session nothing consumes keys, so Tab still moves the focus (the gallery).
        let Some(entry) = self.entry() else {
            return false;
        };
        let input = KeyInput::from_qt(key, qt_bits(modifiers), &text.to_string());
        let session = entry.session();
        let modes = session.modes();
        // The Menu key opens the context menu at the cursor.
        if input.key == Key::Menu && input.mods.is_empty() {
            let (x, y) = self.cursor_position();
            self.as_mut().context_menu_requested(x, y);
            return true;
        }
        // Shift+PageUp/PageDown/Home/End scroll the history, except on the alternate screen
        // (full-screen programs get them).
        let shift_only = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        if input.mods == shift_only && !modes.alternate_screen() {
            let scroll = match input.key {
                Key::PageUp => Some(Scroll::PageUp),
                Key::PageDown => Some(Scroll::PageDown),
                Key::Home => Some(Scroll::Top),
                Key::End => Some(Scroll::Bottom),
                _ => None,
            };
            if let Some(scroll) = scroll {
                session.scroll(scroll);
                self.as_mut().rust_mut().scrolled = scroll != Scroll::Bottom;
                return true;
            }
        }
        let Some(bytes) = encode_key(&input, &modes, &KeyOptions::default()) else {
            return false;
        };
        self.send_input(&entry, &bytes);
        true
    }

    fn handle_shortcut_override(self: Pin<&mut Self>, key: i32, modifiers: i32) -> bool {
        self.attached.is_some() && wants_shortcut_override(key, modifiers)
    }

    fn handle_mouse(mut self: Pin<&mut Self>, event: &TerminalMouseEvent) {
        let Some(entry) = self.entry() else {
            return;
        };
        let mods = modifiers_from_qt(qt_bits(event.modifiers));
        let point = viewport_point(event.column, event.line);
        match event.kind {
            MOUSE_PRESS => self.as_mut().mouse_press(&entry, event, mods, point),
            MOUSE_MOVE => self.as_mut().mouse_move(&entry, event, mods, point),
            MOUSE_RELEASE => self.as_mut().mouse_release(&entry, event, mods, point),
            _ => {}
        }
    }

    fn mouse_press(
        mut self: Pin<&mut Self>,
        entry: &SessionEntry,
        event: &TerminalMouseEvent,
        mods: Modifiers,
        point: ViewportPoint,
    ) {
        let Some(button) = mouse_button(event.button) else {
            return;
        };
        let session = entry.session();
        // Ctrl+click opens a link, even while a program reports the mouse (the link is visibly
        // underlined while Ctrl is held).
        if button == MouseButton::Left && mods.ctrl {
            if let Some(link) = self.link_at(entry, point) {
                if self.open_link(&link.url) {
                    self.as_mut().rust_mut().mouse.link_click = true;
                    return;
                }
            }
        }
        let modes = session.modes();
        if mouse_reporting_active(&modes, mods) {
            // Lines in the scrollback are not reported.
            if self.display_offset == 0 {
                let input = MouseInput {
                    action: MouseAction::Press,
                    button,
                    column: point.column,
                    row: point.row,
                    mods,
                };
                if let Some(bytes) = encode_mouse(&input, &modes) {
                    session.write(&bytes);
                }
                let mut state = self.as_mut().rust_mut();
                state.mouse.reported |= event.button;
                state.mouse.last_cell = Some(point);
            }
            return;
        }
        match button {
            MouseButton::Left => {
                let side = side_of(
                    event.x,
                    self.padding(),
                    self.cell_width(),
                    self.grid.map_or(0, |grid| grid.columns),
                );
                if mods.shift && self.has_selection {
                    session.selection_update(point, side);
                } else {
                    let kind = match event.click_count {
                        2 => SelectionKind::Semantic,
                        3 => SelectionKind::Lines,
                        _ if mods.alt => SelectionKind::Block,
                        _ => SelectionKind::Simple,
                    };
                    session.selection_start(point, side, kind);
                }
                self.as_mut().rust_mut().mouse.selecting = true;
            }
            MouseButton::Middle => self.paste_selection(),
            MouseButton::Right => self.as_mut().context_menu_requested(event.x, event.y),
            _ => {}
        }
    }

    fn mouse_move(
        mut self: Pin<&mut Self>,
        entry: &SessionEntry,
        event: &TerminalMouseEvent,
        mods: Modifiers,
        point: ViewportPoint,
    ) {
        if self.mouse.link_click {
            return;
        }
        let session = entry.session();
        if self.mouse.reported != 0 {
            // A reported press: motion goes to the program when the cell changes (button-event
            // and any-event tracking; `encode_mouse` drops it in the other protocols).
            if self.mouse.last_cell != Some(point) {
                let modes = session.modes();
                let input = MouseInput {
                    action: MouseAction::Move,
                    button: held_button(event.buttons),
                    column: point.column,
                    row: point.row,
                    mods,
                };
                if let Some(bytes) = encode_mouse(&input, &modes) {
                    session.write(&bytes);
                }
                self.as_mut().rust_mut().mouse.last_cell = Some(point);
            }
            return;
        }
        if !self.mouse.selecting || event.buttons & QT_LEFT_BUTTON == 0 {
            return;
        }
        // Dragging above or below the grid scrolls the history.
        let padding = self.padding();
        let bottom =
            padding + f64::from(self.grid.map_or(0, |grid| grid.lines)) * self.cell_height();
        if event.y < padding {
            session.scroll(Scroll::Lines(1));
            self.as_mut().rust_mut().scrolled = true;
        } else if event.y > bottom {
            session.scroll(Scroll::Lines(-1));
        }
        let side = side_of(
            event.x,
            padding,
            self.cell_width(),
            self.grid.map_or(0, |grid| grid.columns),
        );
        session.selection_update(point, side);
    }

    fn mouse_release(
        mut self: Pin<&mut Self>,
        entry: &SessionEntry,
        event: &TerminalMouseEvent,
        mods: Modifiers,
        point: ViewportPoint,
    ) {
        if self.mouse.link_click {
            if event.buttons == 0 {
                self.as_mut().rust_mut().mouse.link_click = false;
            }
            return;
        }
        if self.mouse.reported & event.button != 0 {
            self.as_mut().rust_mut().mouse.reported &= !event.button;
            if let Some(button) = mouse_button(event.button) {
                let session = entry.session();
                let modes = session.modes();
                let input = MouseInput {
                    action: MouseAction::Release,
                    button,
                    column: point.column,
                    row: point.row,
                    mods,
                };
                if let Some(bytes) = encode_mouse(&input, &modes) {
                    session.write(&bytes);
                }
            }
            return;
        }
        if event.button == QT_LEFT_BUTTON && self.mouse.selecting {
            self.as_mut().rust_mut().mouse.selecting = false;
            self.finish_selection(entry);
        }
    }

    fn handle_wheel(mut self: Pin<&mut Self>, event: &TerminalWheelEvent) {
        let Some(entry) = self.entry() else {
            return;
        };
        let mods = modifiers_from_qt(qt_bits(event.modifiers));
        let (angle_x, angle_y) =
            unswap_alt_wheel(cfg!(windows), mods.alt, event.angle_x, event.angle_y);
        let (steps_x, steps_y) = self.as_mut().rust_mut().wheel.add(angle_x, angle_y);
        if steps_x == 0 && steps_y == 0 {
            return;
        }
        let session = entry.session();
        let modes = session.modes();
        if mouse_reporting_active(&modes, mods) {
            if self.display_offset == 0 {
                let point = viewport_point(event.column, event.line);
                report_wheel(
                    &entry,
                    &modes,
                    point,
                    mods,
                    steps_y,
                    MouseButton::WheelUp,
                    MouseButton::WheelDown,
                );
                report_wheel(
                    &entry,
                    &modes,
                    point,
                    mods,
                    steps_x,
                    MouseButton::WheelLeft,
                    MouseButton::WheelRight,
                );
            }
            return;
        }
        if steps_y == 0 {
            return;
        }
        let lines = steps_y.saturating_mul(LINES_PER_NOTCH);
        if !mods.shift {
            if let Some(bytes) = alternate_scroll(lines, &modes) {
                session.write(&bytes);
                return;
            }
        }
        session.scroll(Scroll::Lines(lines));
        self.as_mut().rust_mut().scrolled = true;
    }

    fn handle_hover(
        mut self: Pin<&mut Self>,
        _x: f64,
        _y: f64,
        column: i32,
        line: i32,
        modifiers: i32,
    ) {
        let Some(entry) = self.entry() else {
            return;
        };
        if column < 0 || line < 0 {
            // The pointer left the item.
            self.as_mut().set_link_hover(&entry, None);
            return;
        }
        let mods = modifiers_from_qt(qt_bits(modifiers));
        let point = viewport_point(column, line);
        let session = entry.session();
        let modes = session.modes();
        // Any-event tracking also reports motion without a button.
        let any_event = mouse_protocol(&modes) == Some(MouseProtocol::AnyEvent);
        if any_event
            && !mods.shift
            && self.display_offset == 0
            && self.mouse.last_cell != Some(point)
        {
            let input = MouseInput {
                action: MouseAction::Move,
                button: MouseButton::None,
                column: point.column,
                row: point.row,
                mods,
            };
            if let Some(bytes) = encode_mouse(&input, &modes) {
                session.write(&bytes);
            }
            self.as_mut().rust_mut().mouse.last_cell = Some(point);
        }
        // Links under the pointer while Ctrl is held.
        let link = if mods.ctrl {
            self.link_at(&entry, point)
                .filter(|link| links::is_openable(&link.url))
        } else {
            None
        };
        self.as_mut().set_link_hover(&entry, link.as_ref());
    }

    fn handle_focus_change(mut self: Pin<&mut Self>, focused: bool) {
        let Some(entry) = self.entry() else {
            return;
        };
        if self.focus_sent != Some(focused) {
            entry.session().focus_changed(focused);
            self.as_mut().rust_mut().focus_sent = Some(focused);
        }
        if !focused {
            self.as_mut().set_link_hover(&entry, None);
        }
    }

    fn handle_ime_commit(self: Pin<&mut Self>, text: &QString) {
        let Some(entry) = self.entry() else {
            return;
        };
        // A commit is typing, not pasting (VTE does the same).
        let text = text.to_string();
        self.send_input(&entry, text.as_bytes());
    }

    fn handle_grid_size(
        mut self: Pin<&mut Self>,
        columns: i32,
        lines: i32,
        cell_width: i32,
        cell_height: i32,
    ) {
        let size = term_size(columns, lines, cell_width, cell_height);
        {
            let mut state = self.as_mut().rust_mut();
            if state.grid == size {
                return;
            }
            state.grid = size;
            state.quiet_until = Some(Instant::now() + QUIET_AFTER_CHANGE);
        }
        let Some(size) = size else {
            return;
        };
        match self.entry() {
            Some(entry) => entry.session().resize(size),
            None => self.as_mut().attach_or_start(),
        }
    }
}

/// Reports `steps` wheel notches (positive: `positive` button, negative: `negative`), at most
/// [`MAX_WHEEL_REPORTS`].
fn report_wheel(
    entry: &SessionEntry,
    modes: &InputModes,
    point: ViewportPoint,
    mods: Modifiers,
    steps: i32,
    positive: MouseButton,
    negative: MouseButton,
) {
    if steps == 0 {
        return;
    }
    let input = MouseInput {
        action: MouseAction::Press,
        button: if steps > 0 { positive } else { negative },
        column: point.column,
        row: point.row,
        mods,
    };
    let Some(bytes) = encode_mouse(&input, modes) else {
        return;
    };
    for _ in 0..steps.unsigned_abs().min(MAX_WHEEL_REPORTS.unsigned_abs()) {
        entry.session().write(&bytes);
    }
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
        const CONTROL: i32 = 0x0400_0000;
        const ALT: i32 = 0x0800_0000;
        const META: i32 = 0x1000_0000;
        const KEYPAD: i32 = 0x2000_0000;
        let f6 = qt::KEY_F1 + 5;
        let f11 = qt::KEY_F1 + 10;
        assert!(wants_shortcut_override(f6, 0));
        assert!(wants_shortcut_override(f6, SHIFT));
        assert!(wants_shortcut_override(qt::KEY_F1, META | KEYPAD));
        assert!(wants_shortcut_override(qt::KEY_F24, 0));
        // F25-F35 have no sequence, so the app keeps them.
        assert!(!wants_shortcut_override(qt::KEY_F35, 0));
        assert!(!wants_shortcut_override(f6, CONTROL));
        assert!(!wants_shortcut_override(f6, CONTROL | SHIFT));
        assert!(!wants_shortcut_override(f6, ALT));
        assert!(!wants_shortcut_override(f11, 0));
        // Not function keys: Ctrl+Shift+T, plain letters, Tab.
        assert!(!wants_shortcut_override(0x54, CONTROL | SHIFT));
        assert!(!wants_shortcut_override(0x41, 0));
        assert!(!wants_shortcut_override(qt::KEY_TAB, 0));
    }

    #[test]
    fn qt_values_map_to_engine_values() {
        assert_eq!(mouse_button(QT_LEFT_BUTTON), Some(MouseButton::Left));
        assert_eq!(mouse_button(QT_MIDDLE_BUTTON), Some(MouseButton::Middle));
        assert_eq!(mouse_button(QT_RIGHT_BUTTON), Some(MouseButton::Right));
        assert_eq!(mouse_button(0x8), None);
        assert_eq!(
            held_button(QT_RIGHT_BUTTON | QT_MIDDLE_BUTTON),
            MouseButton::Middle
        );
        assert_eq!(
            held_button(QT_RIGHT_BUTTON | QT_LEFT_BUTTON),
            MouseButton::Left
        );
        assert_eq!(held_button(0), MouseButton::None);
        assert_eq!(viewport_point(3, 7), ViewportPoint::new(7, 3));
        assert_eq!(viewport_point(-1, 70_000), ViewportPoint::new(u16::MAX, 0));
        assert_eq!(qt_bits(i32::MIN), 0x8000_0000);
        assert_eq!(
            term_size(80, 24, 9, 20),
            Some(TermSize {
                columns: 80,
                lines: 24,
                cell_width: 9,
                cell_height: 20,
            })
        );
        assert_eq!(term_size(80, 0, 9, 20), None);
        assert_eq!(term_size(80, 24, 0, 20), None);
        assert_eq!(term_size(-1, 24, 9, 20), None);
        assert_eq!(exit_status(Some(Some(0))), (0, true));
        assert_eq!(
            exit_status(Some(Some(-1_073_741_510))),
            (-1_073_741_510, true)
        );
        assert_eq!(exit_status(Some(None)), (-1, false));
        assert_eq!(exit_status(None), (-1, false));
        assert_eq!(palette_for(true), Palette::OPENSESH_DARK);
        assert_eq!(palette_for(false), Palette::OPENSESH_LIGHT);
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
