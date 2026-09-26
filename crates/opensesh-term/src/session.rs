//! A terminal session: the engine thread that parses a backend's output into an
//! `alacritty_terminal` `Term`, and the [`Session`] handle the GUI uses (PLAN §3.2, §3.3,
//! ADR 0012).
//!
//! Threads: the backend pushes [`BackendEvent`]s; one engine thread per session selects over
//! them, over commands from the handles and over a timer (synchronized-update deadline and
//! throttled notices). It parses already-received bytes under the fair lock in bounded chunks,
//! answers terminal queries (cursor position, colors, sizes) straight to the backend in order,
//! runs the [`SideParser`] and tells the GUI what changed through the `notify` callback, never
//! while holding the lock.
//!
//! Handle methods either queue a command (never block) or take the fair `Term` lock briefly on
//! the calling thread (bounded by one parse chunk); each method's documentation says which.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll as GridScroll};
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::{Cell as TermCell, Flags};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::search::Match;
use alacritty_terminal::term::{
    Config, Osc52, SEMANTIC_ESCAPE_CHARS, Term, TermDamage, TermMode, point_to_viewport,
    viewport_to_point,
};
use alacritty_terminal::vte::ansi::{
    CursorShape as EngineCursorShape, CursorStyle, NamedColor, Processor, Timeout,
};
use crossbeam_channel::{Receiver, Sender, TryRecvError, select};
use opensesh_core::terminal::highlight::HighlightStyle;

use crate::backend::{BackendEvent, TermSize, TerminalBackend};
use crate::encoding::Codec;
use crate::highlight::{Highlighter, RowText, RuleColor};
use crate::input::InputModes;
use crate::input::paste::encode_focus;
use crate::osc::{OscLimiter, SideParser};
use crate::palette::{ColorTable, DIM_BLEND, Palette, Rgb};
use crate::search::{Search, SearchError};
use crate::snapshot::{Cell, Cursor, CursorShape, Damage, Frame, Row, flags};

/// Most bytes parsed per `Term` lock hold. Bounds how long `snapshot` can wait for the engine.
const CHUNK_BYTES: usize = 16 * 1024;

/// Least contrast between selected text and the selection color; below it the text is drawn in
/// the palette's foreground.
const MIN_SELECTED_CONTRAST: f64 = 3.0;

/// Most combining marks per cell copied into a snapshot (the engine may store more).
const MAX_COMBINING_MARKS: usize = 8;

/// How long a synchronized update (DEC mode 2026) may hold the redraw back, as in vte.
const SYNC_HOLD: Duration = Duration::from_millis(150);

/// Most answerback replies per parsed chunk (a flood of ENQ bytes can't flood the program).
const MAX_ANSWERBACKS_PER_CHUNK: usize = 4;

/// A vte synchronized-update timeout that never asks vte to buffer.
///
/// With vte's own handler, the content of a synchronized update is buffered (up to 2 MiB) and
/// then parsed in one call, which would hold the `Term` lock for that whole block. Here vte
/// parses it like any other output, in [`CHUNK_BYTES`] steps, and the engine only delays the
/// redraw ([`SYNC_HOLD`]).
#[derive(Debug, Default)]
struct NoSyncBuffer;

impl Timeout for NoSyncBuffer {
    fn set_timeout(&mut self, _duration: Duration) {}

    fn clear_timeout(&mut self) {}

    fn pending_timeout(&self) -> bool {
        false
    }
}

/// The engine stops pulling backend events while this many bytes wait to be parsed, so a fast
/// producer is slowed down by the backend's bounded channel instead of filling memory.
const PENDING_LIMIT: usize = 64 * 1024;

/// Minimum time between two notices of the same kind (title, directory, bell, cursor blinking):
/// a program that spams them can't flood the GUI thread. The latest value always arrives.
const NOTICE_INTERVAL: Duration = Duration::from_millis(50);

/// Longest window title passed to the GUI, in characters.
const MAX_TITLE_CHARS: usize = 512;

/// What the engine tells the GUI. Delivered through the `notify` callback on the engine thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// The content changed: take a [`Session::snapshot`]. Coalesced: sent only when the dirty
    /// flag goes from clear to set, and [`Session::snapshot`] clears it, so there is at most one
    /// pending `Dirty` per frame.
    Dirty,
    /// The program set the window title (OSC 0/2), without control characters and at most 512
    /// characters long.
    Title(String),
    /// The program reset the title to the default.
    ResetTitle,
    /// The shell reported its working directory (OSC 7 for this machine), as a local path.
    WorkingDirectory(String),
    /// The program rang the bell (at most one per 50 ms).
    Bell,
    /// The program ended: its exit code, or `None` when it was killed or failed to start. Sent
    /// once, after all its output has been parsed.
    Exited(Option<i32>),
    /// The program turned cursor blinking on or off.
    CursorBlinking(bool),
    /// The program set the clipboard through OSC 52 (only when [`SessionOptions::osc52_copy`]
    /// allows it). At most one per 50 ms: the latest text wins.
    Clipboard(String),
}

/// The GUI's callback for [`Notice`]s. It runs on the engine thread, never while the `Term` is
/// locked; it must not block (typically it queues work onto the GUI thread).
pub type Notify = Arc<dyn Fn(Notice) + Send + Sync>;

/// Settings for a new [`Session`].
#[derive(Debug, Clone, PartialEq)]
pub struct SessionConfig {
    /// Initial size; use the size the backend was started with.
    pub size: TermSize,
    /// Colors.
    pub palette: Palette,
    /// The options that can also change later ([`Session::set_options`]).
    pub options: SessionOptions,
    /// Most lines one [`Session::search`] step scans (it runs on the caller's thread). Default
    /// 10,000, the whole default history: a step that finds nothing in 10,000 lines of 200
    /// columns takes about 16 ms (release build).
    pub search_max_lines: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            size: TermSize::default(),
            palette: Palette::default(),
            options: SessionOptions::default(),
            search_max_lines: 10_000,
        }
    }
}

/// Session options that can change while it runs (PLAN §6.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOptions {
    /// Lines of scrollback history.
    pub scrollback_lines: usize,
    /// Cursor shape when the program doesn't choose one (`Block`, `Beam` or `Underline`).
    pub cursor_shape: CursorShape,
    /// Whether that default cursor blinks.
    pub cursor_blinking: bool,
    /// A cursor turns into a hollow block while the terminal doesn't have the focus.
    pub cursor_hollow_unfocused: bool,
    /// Characters that end a word for double-click selection.
    pub word_separators: String,
    /// Programs may set the clipboard with OSC 52 ([`Notice::Clipboard`]); reading it is never
    /// allowed.
    pub osc52_copy: bool,
    /// Encoding of the program's input and output (a WHATWG name; UTF-8 needs no conversion).
    pub encoding: String,
    /// Reply to ENQ (printable ASCII; empty sends nothing).
    pub answerback: String,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            scrollback_lines: 10_000,
            cursor_shape: CursorShape::Block,
            cursor_blinking: false,
            cursor_hollow_unfocused: true,
            word_separators: SEMANTIC_ESCAPE_CHARS.to_owned(),
            osc52_copy: false,
            encoding: "UTF-8".to_owned(),
            answerback: String::new(),
        }
    }
}

impl SessionOptions {
    /// `alacritty_terminal`'s configuration for these options.
    fn engine_config(&self) -> Config {
        Config {
            scrolling_history: self.scrollback_lines,
            default_cursor_style: CursorStyle {
                shape: engine_cursor_shape(self.cursor_shape),
                blinking: self.cursor_blinking,
            },
            vi_mode_cursor_style: None,
            semantic_escape_chars: self.word_separators.clone(),
            // The kitty keyboard stack has a remote-triggerable panic in 0.26.0 (ADR 0012).
            kitty_keyboard: false,
            // OSC 52 is opt-in (PLAN §6.2), and never lets a program read the clipboard.
            osc52: if self.osc52_copy {
                Osc52::OnlyCopy
            } else {
                Osc52::Disabled
            },
        }
    }

    /// The answerback bytes: printable ASCII only, at most 64 bytes.
    fn answerback_bytes(&self) -> Vec<u8> {
        self.answerback
            .bytes()
            .filter(|byte| (0x20..0x7f).contains(byte))
            .take(64)
            .collect()
    }
}

/// A position in the viewport: `row` 0 is the top visible line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ViewportPoint {
    /// Row from the top of the viewport.
    pub row: u16,
    /// Column from the left.
    pub column: u16,
}

impl ViewportPoint {
    /// A point from its row and column.
    #[must_use]
    pub const fn new(row: u16, column: u16) -> Self {
        Self { row, column }
    }
}

/// Which half of a cell the pointer is on (selections start and end between cells).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// Left half.
    Left,
    /// Right half.
    Right,
}

/// How a selection grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SelectionKind {
    /// Character by character (drag).
    Simple,
    /// A rectangle (Alt+drag).
    Block,
    /// Whole words (double click).
    Semantic,
    /// Whole lines (triple click).
    Lines,
}

/// Scrolling the view through the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scroll {
    /// By lines: positive scrolls up into the history (older lines), negative back down.
    Lines(i32),
    /// One screen up.
    PageUp,
    /// One screen down.
    PageDown,
    /// To the oldest line.
    Top,
    /// Back to the live screen.
    Bottom,
}

/// Errors of [`Session::start`].
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The engine thread could not be started.
    #[error("could not start the terminal engine thread")]
    Thread(#[source] std::io::Error),
}

/// A handle to a running terminal session. Cheap to clone; all clones control the same session.
/// Dropping every handle stops the engine and shuts the backend down.
#[derive(Clone)]
pub struct Session {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").finish_non_exhaustive()
    }
}

struct Inner {
    shared: Arc<Shared>,
    commands: Sender<Command>,
}

/// State shared by the handles and the engine thread.
struct Shared {
    state: FairMutex<State>,
    dirty: AtomicBool,
    modes: AtomicU32,
    x10_mouse: AtomicBool,
    /// The engine's mouse modes are stale (see [`SideParser::engine_mouse_hidden`]).
    engine_mouse_hidden: AtomicBool,
    /// A synchronized update (DEC mode 2026) is open: snapshots keep showing the previous screen.
    synchronized: AtomicBool,
    /// A paced paste ([`Session::write_paced`]) still has lines to send.
    pasting: AtomicBool,
    closed: AtomicBool,
    notify: Notify,
    search_max_lines: usize,
}

impl Shared {
    fn emit(&self, notice: Notice) {
        if !self.closed.load(Ordering::Acquire) {
            (self.notify)(notice);
        }
    }

    /// Sets the dirty flag and sends [`Notice::Dirty`] on the clear-to-set transition only.
    fn mark_dirty(&self) {
        if !self.dirty.swap(true, Ordering::AcqRel) {
            self.emit(Notice::Dirty);
        }
    }
}

/// Everything behind the fair lock.
struct State {
    term: Term<Listener>,
    colors: ColorTable,
    palette: Palette,
    focused: bool,
    search: Option<Search>,
    /// Hovered link, grid coordinates, inclusive.
    link: Option<(Point, Point)>,
    /// A change the damage tracker doesn't see (palette, link, focus): redraw everything.
    full_redraw: bool,
    /// What the previous frame showed.
    shown: Shown,
    /// Draw the cursor as a hollow block while unfocused.
    hollow_unfocused: bool,
    /// Keyword highlighting rules, if any are on.
    highlighter: Option<Arc<Highlighter>>,
    /// Scratch buffers reused by every snapshot.
    row_damage: Vec<bool>,
    matches: Vec<Match>,
    row_text: RowText,
    row_styles: Vec<u16>,
}

/// View state that isn't part of `alacritty_terminal`'s damage: when it changes, the next frame
/// is a full one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Shown {
    columns: usize,
    lines: usize,
    display_offset: usize,
    selection: Option<SelectionRange>,
}

enum Command {
    Write(Vec<u8>),
    Paced(Vec<Vec<u8>>, Duration),
    CancelPaste,
    Options(SessionOptions),
    Resize(TermSize),
    Focus(bool),
    Redraw,
    Shutdown,
}

/// Receives `alacritty_terminal`'s events while the parser runs (under the lock): it only
/// queues them for the engine.
struct Listener(Sender<Event>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

/// `alacritty_terminal` wants its sizes through the `Dimensions` trait.
struct GridSize {
    columns: usize,
    lines: usize,
}

impl From<TermSize> for GridSize {
    fn from(size: TermSize) -> Self {
        let size = size.clamped();
        Self {
            columns: usize::from(size.columns),
            lines: usize::from(size.lines),
        }
    }
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

impl Session {
    /// Starts the engine thread for a backend that was spawned with `config.size`.
    ///
    /// # Errors
    /// [`SessionError::Thread`] if the thread can't be created (the backend is then shut down).
    pub fn start(
        backend: Box<dyn TerminalBackend>,
        events: Receiver<BackendEvent>,
        config: SessionConfig,
        notify: Notify,
    ) -> Result<Self, SessionError> {
        let size = config.size.clamped();
        let (term_events_tx, term_events) = crossbeam_channel::unbounded();
        let options = config.options;
        let term = Term::new(
            options.engine_config(),
            &GridSize::from(size),
            Listener(term_events_tx),
        );
        let state = State {
            term,
            colors: ColorTable::new(&config.palette),
            palette: config.palette,
            focused: false,
            search: None,
            link: None,
            full_redraw: true,
            shown: Shown::default(),
            hollow_unfocused: options.cursor_hollow_unfocused,
            highlighter: None,
            row_damage: Vec::new(),
            matches: Vec::new(),
            row_text: RowText::default(),
            row_styles: Vec::new(),
        };
        let shared = Arc::new(Shared {
            state: FairMutex::new(state),
            dirty: AtomicBool::new(false),
            modes: AtomicU32::new(TermMode::default().bits()),
            x10_mouse: AtomicBool::new(false),
            engine_mouse_hidden: AtomicBool::new(false),
            synchronized: AtomicBool::new(false),
            pasting: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            notify,
            search_max_lines: config.search_max_lines.max(1),
        });
        let (commands, command_rx) = crossbeam_channel::unbounded();
        let engine = Engine {
            shared: Arc::clone(&shared),
            backend: Some(backend),
            events,
            commands: command_rx,
            term_events,
            processor: Processor::new(),
            sync_hold: None,
            side: SideParser::new(),
            limiter: {
                let mut limiter = OscLimiter::default();
                limiter.set_clipboard(options.osc52_copy);
                limiter
            },
            codec: Codec::new(&options.encoding),
            decoded: Vec::new(),
            answerback: options.answerback_bytes(),
            paste: VecDeque::new(),
            paste_delay: Duration::ZERO,
            paste_next: None,
            clipboard: Throttled::default(),
            pending: Pending::default(),
            size,
            pending_resize: None,
            exit: None,
            exit_reported: false,
            title: Throttled::default(),
            directory: Throttled::default(),
            bell: Throttled::default(),
            blinking: Throttled::default(),
            cursor_blinking: options.cursor_blinking,
        };
        std::thread::Builder::new()
            .name("opensesh-term-engine".to_owned())
            .spawn(move || engine.run())
            .map_err(SessionError::Thread)?;
        Ok(Self {
            inner: Arc::new(Inner { shared, commands }),
        })
    }

    /// Queues bytes for the program (keystrokes, paste). Never blocks. It does not scroll the
    /// view; call `scroll(Scroll::Bottom)` first for typed input.
    pub fn write(&self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.send(Command::Write(bytes.to_vec()));
        }
    }

    /// Queues a resize: the engine resizes the `Term` first, then the backend (the other order
    /// corrupts full-screen redraws). Sizes are clamped to at least 2 x 1. Never blocks; the
    /// next [`Notice::Dirty`] frame has the new size.
    pub fn resize(&self, size: TermSize) {
        self.send(Command::Resize(size));
    }

    /// Scrolls the view through the history. Locks briefly.
    pub fn scroll(&self, scroll: Scroll) {
        let mut state = self.lock();
        let scroll = match scroll {
            // `alacritty_terminal` adds the delta to the offset as i32: keep it far from overflow.
            Scroll::Lines(lines) => GridScroll::Delta(lines.clamp(-(1 << 24), 1 << 24)),
            Scroll::PageUp => GridScroll::PageUp,
            Scroll::PageDown => GridScroll::PageDown,
            Scroll::Top => GridScroll::Top,
            Scroll::Bottom => GridScroll::Bottom,
        };
        state.term.scroll_display(scroll);
        drop(state);
        self.redraw();
    }

    /// Tells the session the terminal gained or lost the keyboard focus: the cursor turns hollow
    /// while unfocused, and the program gets `CSI I` / `CSI O` if it enabled focus reporting.
    /// Never blocks.
    pub fn focus_changed(&self, focused: bool) {
        self.send(Command::Focus(focused));
    }

    /// The modes that input encoding depends on, as of the last parsed chunk. Lock-free.
    ///
    /// When `x10_mouse` is set, X10 is the active mouse protocol and the mouse bits of `term`
    /// are stale (X10 was enabled after them).
    #[must_use]
    pub fn modes(&self) -> InputModes {
        let shared = &self.inner.shared;
        let mut term = TermMode::from_bits_truncate(shared.modes.load(Ordering::Acquire));
        if shared.engine_mouse_hidden.load(Ordering::Acquire) {
            term.remove(TermMode::MOUSE_MODE);
        }
        InputModes {
            term,
            x10_mouse: shared.x10_mouse.load(Ordering::Acquire),
        }
    }

    /// Starts a selection at `point` (replacing any previous one). Locks briefly.
    pub fn selection_start(&self, point: ViewportPoint, side: Side, kind: SelectionKind) {
        let mut state = self.lock();
        let point = grid_point(&state.term, point);
        let kind = match kind {
            SelectionKind::Simple => SelectionType::Simple,
            SelectionKind::Block => SelectionType::Block,
            SelectionKind::Semantic => SelectionType::Semantic,
            SelectionKind::Lines => SelectionType::Lines,
        };
        state.term.selection = Some(Selection::new(kind, point, engine_side(side)));
        drop(state);
        self.redraw();
    }

    /// Moves the end of the current selection to `point`. Locks briefly.
    pub fn selection_update(&self, point: ViewportPoint, side: Side) {
        let mut state = self.lock();
        let point = grid_point(&state.term, point);
        if let Some(selection) = state.term.selection.as_mut() {
            selection.update(point, engine_side(side));
        }
        drop(state);
        self.redraw();
    }

    /// Removes the selection. Locks briefly.
    pub fn selection_clear(&self) {
        let mut state = self.lock();
        let had_selection = state.term.selection.take().is_some();
        drop(state);
        if had_selection {
            self.redraw();
        }
    }

    /// Whether a non-empty selection exists (the engine clears it itself on some screen
    /// changes, e.g. switching to the alternate screen). Locks briefly.
    #[must_use]
    pub fn has_selection(&self) -> bool {
        self.lock()
            .term
            .selection
            .as_ref()
            .is_some_and(|selection| !selection.is_empty())
    }

    /// The selected text (wide characters, combining marks and wrapped lines handled; `Lines`
    /// selections end with a newline), or `None` without a non-empty selection. Locks briefly.
    #[must_use]
    pub fn selection_text(&self) -> Option<String> {
        let state = self.lock();
        state
            .term
            .selection_to_string()
            .filter(|text| !text.is_empty())
    }

    /// Finds the next match of `pattern` (a regex, smart-case), scrolls it into view and
    /// highlights it with the other visible matches until [`Session::search_clear`].
    ///
    /// `forward` searches toward newer output (down), otherwise toward older output (up). A new
    /// pattern starts at the viewport (its top when forward, its bottom when backward); repeating
    /// the same pattern moves from the current match and wraps around. An empty pattern clears
    /// the search. Returns the match's first and last cell in viewport coordinates, or `None`
    /// when nothing matches within [`SessionConfig::search_max_lines`] lines.
    ///
    /// Runs on the calling thread under the lock, bounded by `search_max_lines`.
    ///
    /// # Errors
    /// [`SearchError::InvalidPattern`] if `pattern` doesn't compile.
    pub fn search(
        &self,
        pattern: &str,
        forward: bool,
    ) -> Result<Option<(ViewportPoint, ViewportPoint)>, SearchError> {
        if pattern.is_empty() {
            self.search_clear();
            return Ok(None);
        }
        let max_lines = self.inner.shared.search_max_lines;
        let mut guard = self.lock();
        let state = &mut *guard;
        if state
            .search
            .as_ref()
            .is_none_or(|search| search.pattern() != pattern)
        {
            state.search = Some(Search::new(pattern)?);
        }
        let Some(search) = state.search.as_mut() else {
            return Ok(None);
        };
        let found = search.find(&state.term, forward, max_lines);
        let result = found.map(|found| {
            state.term.scroll_to_point(*found.start());
            (
                viewport_point(&state.term, *found.start()),
                viewport_point(&state.term, *found.end()),
            )
        });
        state.full_redraw = true;
        drop(guard);
        self.redraw();
        Ok(result)
    }

    /// Ends the search and removes its highlights. Locks briefly.
    pub fn search_clear(&self) {
        let mut state = self.lock();
        let had_search = state.search.take().is_some();
        // Every row was painted with match colors: the renderer must redraw them all.
        state.full_redraw |= had_search;
        drop(state);
        if had_search {
            self.redraw();
        }
    }

    /// Clears the scrollback history (as `CSI 3 J` does); the screen stays. A selection in the
    /// history is removed. Locks briefly.
    pub fn clear_history(&self) {
        use alacritty_terminal::vte::ansi::{ClearMode, Handler as _};
        let mut state = self.lock();
        state.term.clear_screen(ClearMode::Saved);
        drop(state);
        self.redraw();
    }

    /// Copies what changed since the previous snapshot into `out` (reusing its allocations):
    /// only damaged rows, or every row when [`Frame::damage`] is [`Damage::Full`] (first frame,
    /// resize, scrolling, selection, search, palette or focus changes). Colors are final.
    ///
    /// Clears the dirty flag first, so output parsed meanwhile sends a new [`Notice::Dirty`].
    /// Takes the fair lock; the wait is bounded by one parse chunk (16 KiB).
    pub fn snapshot(&self, out: &mut Frame) {
        let shared = &self.inner.shared;
        shared.dirty.store(false, Ordering::Release);
        if shared.synchronized.load(Ordering::Acquire) {
            // Nothing new until the update ends: the renderer keeps what it has (the damage
            // stays in the Term for the next frame).
            out.clear();
            return;
        }
        let mut state = shared.state.lock();
        state.fill(out);
    }

    /// [`Session::snapshot`] that always copies every row ([`Damage::Full`]), for a renderer that
    /// has no copy of the grid: a view that attached to a running session, or a rebuilt scene
    /// graph. Takes the fair lock.
    pub fn snapshot_full(&self, out: &mut Frame) {
        let shared = &self.inner.shared;
        shared.dirty.store(false, Ordering::Release);
        let mut state = shared.state.lock();
        state.full_redraw = true;
        state.fill(out);
    }

    /// The visible screen as text: one line per row, trailing spaces trimmed, wide-character
    /// spacers skipped, combining marks kept. For tests and the smoke test. Locks briefly.
    #[must_use]
    pub fn text_dump(&self) -> String {
        let state = self.lock();
        let term = &state.term;
        let grid = term.grid();
        let offset = i32::try_from(grid.display_offset()).unwrap_or(i32::MAX);
        let mut text = String::new();
        for row in 0..grid.screen_lines() {
            let line = Line(i32::try_from(row).unwrap_or(i32::MAX) - offset);
            let mut line_text = String::new();
            for cell in &grid[line][..] {
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                line_text.push(if cell.c == '\t' { ' ' } else { cell.c });
                if let Some(marks) = cell.zerowidth() {
                    line_text.extend(marks);
                }
            }
            text.push_str(line_text.trim_end());
            text.push('\n');
        }
        text
    }

    /// Applies new options (scrollback, default cursor, word separators, OSC 52, encoding,
    /// answerback) to the running session. Locks briefly.
    pub fn set_options(&self, options: SessionOptions) {
        let mut state = self.lock();
        state.term.set_options(options.engine_config());
        state.hollow_unfocused = options.cursor_hollow_unfocused;
        state.full_redraw = true;
        drop(state);
        self.send(Command::Options(options));
        self.redraw();
    }

    /// Turns keyword highlighting on with `highlighter`, or off with `None`. Locks briefly.
    pub fn set_highlighter(&self, highlighter: Option<Arc<Highlighter>>) {
        let highlighter = highlighter.filter(|highlighter| !highlighter.is_empty());
        let mut state = self.lock();
        state.highlighter = highlighter;
        state.full_redraw = true;
        drop(state);
        self.redraw();
    }

    /// Sends `chunks` (usually the lines of a paste) one at a time, `delay` apart, for devices
    /// that drop input sent too fast. Queued after any paced paste still going. Never blocks.
    pub fn write_paced(&self, chunks: Vec<Vec<u8>>, delay: Duration) {
        let chunks: Vec<Vec<u8>> = chunks.into_iter().filter(|c| !c.is_empty()).collect();
        if !chunks.is_empty() {
            self.inner.shared.pasting.store(true, Ordering::Release);
            self.send(Command::Paced(chunks, delay));
        }
    }

    /// Drops what is left of a paced paste.
    pub fn cancel_paste(&self) {
        self.send(Command::CancelPaste);
    }

    /// Whether a paced paste is still sending.
    #[must_use]
    pub fn is_pasting(&self) -> bool {
        self.inner.shared.pasting.load(Ordering::Acquire)
    }

    /// Replaces the colors (for example when the app switches between light and dark). Locks
    /// briefly.
    pub fn set_palette(&self, palette: Palette) {
        let mut state = self.lock();
        state.colors = ColorTable::new(&palette);
        state.palette = palette;
        state.full_redraw = true;
        drop(state);
        self.redraw();
    }

    /// The URI of the OSC 8 hyperlink at `point`, if any (the caller validates the scheme before
    /// opening it, PLAN §8). Locks briefly.
    #[must_use]
    pub fn hyperlink_at(&self, point: ViewportPoint) -> Option<String> {
        let state = self.lock();
        let point = grid_point(&state.term, point);
        state.term.grid()[point]
            .hyperlink()
            .map(|link| link.uri().to_owned())
    }

    /// Underlines the cells from `start` to `end` (inclusive, reading order) as a link, for
    /// example a detected URL under the pointer; `None` removes it. Locks briefly.
    pub fn set_link_highlight(&self, range: Option<(ViewportPoint, ViewportPoint)>) {
        let mut state = self.lock();
        let link = range.map(|(start, end)| {
            let (start, end) = (grid_point(&state.term, start), grid_point(&state.term, end));
            (start.min(end), start.max(end))
        });
        if state.link == link {
            return;
        }
        state.link = link;
        state.full_redraw = true;
        drop(state);
        self.redraw();
    }

    /// Ends the session: the backend shuts down (the program is hung up or terminated) and the
    /// engine thread exits, all in the background. Idempotent, never blocks. Notices already in
    /// flight may still arrive; none are sent afterwards. The last screen stays available to
    /// [`Session::snapshot`] and [`Session::text_dump`].
    pub fn shutdown(&self) {
        self.inner.shared.closed.store(true, Ordering::Release);
        self.send(Command::Shutdown);
    }

    fn send(&self, command: Command) {
        // Fails only after the engine stopped, when there is nothing left to do.
        let _ = self.inner.commands.send(command);
    }

    fn redraw(&self) {
        self.send(Command::Redraw);
    }

    fn lock(&self) -> impl std::ops::DerefMut<Target = State> + '_ {
        self.inner.shared.state.lock()
    }
}

/// Converts a viewport point to a grid point, clamped to the visible grid.
fn grid_point<T>(term: &Term<T>, point: ViewportPoint) -> Point {
    let row = usize::from(point.row).min(term.screen_lines().saturating_sub(1));
    let column = usize::from(point.column).min(term.columns().saturating_sub(1));
    viewport_to_point(
        term.grid().display_offset(),
        Point::new(row, Column(column)),
    )
}

/// Converts a grid point to the viewport, clamped to its edges.
fn viewport_point<T>(term: &Term<T>, point: Point) -> ViewportPoint {
    let lines = term.screen_lines();
    let (row, column) = match point_to_viewport(term.grid().display_offset(), point) {
        Some(point) if point.line < lines => (point.line, point.column.0),
        Some(_) => (lines.saturating_sub(1), term.columns().saturating_sub(1)),
        None => (0, 0),
    };
    ViewportPoint::new(
        u16::try_from(row).unwrap_or(u16::MAX),
        u16::try_from(column).unwrap_or(u16::MAX),
    )
}

fn engine_side(side: Side) -> Direction {
    match side {
        Side::Left => Direction::Left,
        Side::Right => Direction::Right,
    }
}

fn engine_cursor_shape(shape: CursorShape) -> EngineCursorShape {
    match shape {
        CursorShape::Block => EngineCursorShape::Block,
        CursorShape::HollowBlock => EngineCursorShape::HollowBlock,
        CursorShape::Beam => EngineCursorShape::Beam,
        CursorShape::Underline => EngineCursorShape::Underline,
        CursorShape::Hidden => EngineCursorShape::Hidden,
    }
}

impl State {
    /// Fills a frame (see [`Session::snapshot`]).
    fn fill(&mut self, out: &mut Frame) {
        let columns = self.term.columns();
        let lines = self.term.screen_lines();
        let display_offset = self.term.grid().display_offset();
        let selection = self
            .term
            .selection
            .as_ref()
            .and_then(|selection| selection.to_range(&self.term));
        self.matches.clear();
        if let Some(search) = &self.search {
            search.visible_matches(&self.term, &mut self.matches);
        }
        let shown = Shown {
            columns,
            lines,
            display_offset,
            selection,
        };
        let cursor = self.cursor(lines, display_offset);

        // Search highlights can move with any output, so a search forces full frames.
        let mut full = self.full_redraw || self.search.is_some() || shown != self.shown;
        self.row_damage.clear();
        self.row_damage.resize(lines, false);
        match self.term.damage() {
            TermDamage::Full => full = true,
            TermDamage::Partial(damaged) => {
                for bounds in damaged {
                    if let Some(row) = self.row_damage.get_mut(bounds.line) {
                        *row = true;
                    }
                }
            }
        }
        self.term.reset_damage();
        self.full_redraw = false;
        self.shown = shown;

        let overrides = *self.term.colors();
        let background = self.colors.get(&overrides, NamedColor::Background as usize);
        out.columns = u16::try_from(columns).unwrap_or(u16::MAX);
        out.lines = u16::try_from(lines).unwrap_or(u16::MAX);
        out.damage = if full { Damage::Full } else { Damage::Partial };
        out.clusters.clear();
        out.cursor = cursor;
        out.background = background.to_argb();
        out.display_offset = display_offset;
        out.history_size = self.term.history_size();

        let painter = Painter {
            colors: &self.colors,
            overrides: &overrides,
            palette: &self.palette,
            selection,
            matches: &self.matches,
            focused_match: self.search.as_ref().and_then(Search::focused),
            link: self.link,
            highlighter: self.highlighter.as_deref(),
            contrast: self.palette.minimum_contrast > 1.0,
        };
        let grid = self.term.grid();
        let offset = i32::try_from(display_offset).unwrap_or(i32::MAX);
        let mut next_match = 0;
        let mut count = 0;
        for row in 0..lines {
            if !full && !self.row_damage.get(row).copied().unwrap_or(false) {
                continue;
            }
            if count == out.rows.len() {
                out.rows.push(Row::default());
            }
            let target = &mut out.rows[count];
            count += 1;
            target.index = u16::try_from(row).unwrap_or(u16::MAX);
            target.cells.clear();
            let line = Line(i32::try_from(row).unwrap_or(i32::MAX) - offset);
            let cells = &grid[line][..];
            self.row_styles.clear();
            if let Some(highlighter) = painter.highlighter {
                self.row_text.clear();
                for (column, cell) in cells.iter().enumerate() {
                    if cell
                        .flags
                        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                    {
                        continue;
                    }
                    let width = if cell.flags.contains(Flags::WIDE_CHAR) {
                        2
                    } else {
                        1
                    };
                    let ch = if cell.c == '\t' { ' ' } else { cell.c };
                    self.row_text
                        .push(ch, cell.zerowidth().unwrap_or(&[]), column, width);
                }
                highlighter.row(&self.row_text, columns, &mut self.row_styles);
            }
            // Two loops, each with its own copy of `cell`: the one without highlighting carries
            // no highlighting code at all (it costs about 12 % of a full snapshot otherwise).
            match painter.highlighter.filter(|_| !self.row_styles.is_empty()) {
                Some(highlighter) => {
                    for (column, cell) in cells.iter().enumerate() {
                        let point = Point::new(line, Column(column));
                        let style = match self.row_styles.get(column) {
                            Some(&mark) if mark > 0 => highlighter.style(usize::from(mark - 1)),
                            _ => None,
                        };
                        let painted = painter.cell::<true>(
                            cell,
                            point,
                            style,
                            &mut next_match,
                            &mut out.clusters,
                        );
                        target.cells.push(painted);
                    }
                }
                None => {
                    for (column, cell) in cells.iter().enumerate() {
                        let point = Point::new(line, Column(column));
                        let painted = painter.cell::<false>(
                            cell,
                            point,
                            None,
                            &mut next_match,
                            &mut out.clusters,
                        );
                        target.cells.push(painted);
                    }
                }
            }
        }
        out.rows.truncate(count);
    }

    /// The cursor in viewport coordinates, with its final shape and colors.
    fn cursor(&self, lines: usize, display_offset: usize) -> Cursor {
        let term = &self.term;
        let content = term.renderable_content();
        let point = content.cursor.point;
        let viewport = point_to_viewport(display_offset, point).filter(|point| point.line < lines);
        let shape = match (viewport, content.cursor.shape) {
            (None, _) | (_, EngineCursorShape::Hidden) => CursorShape::Hidden,
            _ if !self.focused && self.hollow_unfocused => CursorShape::HollowBlock,
            (_, EngineCursorShape::Block) => CursorShape::Block,
            (_, EngineCursorShape::HollowBlock) => CursorShape::HollowBlock,
            (_, EngineCursorShape::Beam) => CursorShape::Beam,
            (_, EngineCursorShape::Underline) => CursorShape::Underline,
        };
        let overrides = term.colors();
        let color = self.colors.get(overrides, NamedColor::Cursor as usize);
        // A program-chosen cursor color (OSC 12) gets the background as its text color.
        let text_color = if overrides[NamedColor::Cursor].is_some() {
            self.colors.get(overrides, NamedColor::Background as usize)
        } else {
            self.palette.cursor_text
        };
        let (row, column) = viewport.map_or((0, 0), |point| (point.line, point.column.0));
        let wide = term.grid()[point].flags.contains(Flags::WIDE_CHAR);
        Cursor {
            row: u16::try_from(row).unwrap_or(u16::MAX),
            column: u16::try_from(column).unwrap_or(u16::MAX),
            shape,
            blinking: term.cursor_style().blinking,
            wide,
            color: color.to_argb(),
            text_color: text_color.to_argb(),
        }
    }
}

/// Resolves cells to their final colors and flags.
struct Painter<'a> {
    colors: &'a ColorTable,
    overrides: &'a Colors,
    palette: &'a Palette,
    selection: Option<SelectionRange>,
    matches: &'a [Match],
    focused_match: Option<&'a Match>,
    link: Option<(Point, Point)>,
    highlighter: Option<&'a Highlighter>,
    /// The palette asks for a minimum contrast.
    contrast: bool,
}

impl Painter<'_> {
    /// `next_match` walks `matches` in step with the cells (both are in reading order).
    /// Applies a keyword highlighting rule's style. Kept out of `cell` (and out of line), so the
    /// common path without highlighting stays small enough to inline.
    #[inline(never)]
    fn apply_highlight(
        &self,
        style: &HighlightStyle,
        fg: &mut Rgb,
        bg: &mut Rgb,
        underline: &mut Option<Rgb>,
        out_flags: &mut u16,
    ) {
        if let Some(color) = style.foreground {
            *fg = self.rule_color(color);
            *underline = None;
        }
        if let Some(color) = style.background {
            *bg = self.rule_color(color);
        }
        if style.bold {
            *out_flags |= flags::BOLD;
        }
        if style.underline && *out_flags & UNDERLINE_FLAGS == 0 {
            *out_flags |= flags::UNDERLINE;
        }
    }

    /// Lightens or darkens text below the palette's minimum contrast (out of line, as above).
    #[inline(never)]
    fn enforce_contrast(&self, fg: &mut Rgb, underline: &mut Rgb, bg: Rgb) {
        let adjusted = fg.with_contrast(bg, self.palette.minimum_contrast);
        if *underline == *fg {
            *underline = adjusted;
        }
        *fg = adjusted;
    }

    /// A rule color from the current table (OSC 4 changes apply to it too).
    fn rule_color(&self, color: opensesh_core::terminal::highlight::HighlightColor) -> Rgb {
        match RuleColor::from(color) {
            RuleColor::Indexed(index) => self.colors.get(self.overrides, index),
            RuleColor::Rgb(r, g, b) => Rgb::new(r, g, b),
        }
    }

    /// One cell with its final colors. `HIGHLIGHT` compiles the keyword highlighting in or out.
    fn cell<const HIGHLIGHT: bool>(
        &self,
        cell: &TermCell,
        point: Point,
        highlight: Option<&HighlightStyle>,
        next_match: &mut usize,
        clusters: &mut Vec<Vec<char>>,
    ) -> Cell {
        let engine_flags = cell.flags;
        let bold = engine_flags.contains(Flags::BOLD);
        let mut fg = self.colors.foreground(self.overrides, cell.fg, bold);
        let mut bg = self.colors.background(self.overrides, cell.bg);
        let mut underline = cell
            .underline_color()
            .map(|color| self.colors.foreground(self.overrides, color, bold));
        if engine_flags.contains(Flags::DIM) {
            fg = fg.mix(bg, DIM_BLEND);
            underline = underline.map(|color| color.mix(bg, DIM_BLEND));
        }
        if engine_flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        let mut out_flags = map_flags(engine_flags);
        if HIGHLIGHT {
            if let Some(style) = highlight {
                self.apply_highlight(style, &mut fg, &mut bg, &mut underline, &mut out_flags);
            }
        }
        let mut underline = underline.unwrap_or(fg);

        // Both halves of a wide character share the selection state.
        let selected = self.selection.is_some_and(|selection| {
            selection.contains(point)
                || (engine_flags.contains(Flags::WIDE_CHAR)
                    && selection.contains(Point::new(point.line, point.column + 1)))
                || (engine_flags.contains(Flags::WIDE_CHAR_SPACER)
                    && point.column.0 > 0
                    && selection.contains(Point::new(point.line, point.column - 1)))
        });
        while self
            .matches
            .get(*next_match)
            .is_some_and(|found| *found.end() < point)
        {
            *next_match += 1;
        }
        let in_match = self
            .matches
            .get(*next_match)
            .filter(|found| *found.start() <= point);
        if selected {
            out_flags |= flags::SELECTED;
            bg = self.palette.selection_background;
            if let Some(color) = self.palette.selection_foreground {
                fg = color;
                underline = color;
            } else if fg.contrast(bg) < MIN_SELECTED_CONTRAST {
                // Reverse video and dark text would vanish on the selection color.
                fg = self.palette.foreground;
                underline = fg;
            }
        } else if let Some(found) = in_match {
            out_flags |= flags::MATCH;
            let (match_fg, match_bg) = if self.focused_match == Some(found) {
                (
                    self.palette.focused_match_foreground,
                    self.palette.focused_match_background,
                )
            } else {
                (self.palette.match_foreground, self.palette.match_background)
            };
            fg = match_fg;
            bg = match_bg;
            underline = match_fg;
        }
        if in_match.is_some() {
            out_flags |= flags::MATCH;
        }
        if cell.hyperlink().is_some()
            || self
                .link
                .is_some_and(|(start, end)| start <= point && point <= end)
        {
            out_flags |= flags::LINK;
        }
        if engine_flags.contains(Flags::HIDDEN) {
            fg = bg;
            underline = bg;
        } else if self.contrast {
            self.enforce_contrast(&mut fg, &mut underline, bg);
        }

        let spacer =
            engine_flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
        let ch = if spacer || cell.c == '\t' {
            ' '
        } else {
            cell.c
        };
        let cluster = match cell.zerowidth() {
            Some(marks) if !marks.is_empty() && !spacer => {
                clusters.push(marks.iter().take(MAX_COMBINING_MARKS).copied().collect());
                u32::try_from(clusters.len()).unwrap_or(0)
            }
            _ => 0,
        };
        Cell {
            ch,
            cluster,
            fg: fg.to_argb(),
            bg: bg.to_argb(),
            underline: underline.to_argb(),
            flags: out_flags,
        }
    }
}

/// Every underline style bit.
const UNDERLINE_FLAGS: u16 = flags::UNDERLINE
    | flags::DOUBLE_UNDERLINE
    | flags::CURLY_UNDERLINE
    | flags::DOTTED_UNDERLINE
    | flags::DASHED_UNDERLINE;

/// Engine cell flags to the snapshot's flag bits.
fn map_flags(engine: Flags) -> u16 {
    const MAP: [(Flags, u16); 11] = [
        (Flags::BOLD, flags::BOLD),
        (Flags::ITALIC, flags::ITALIC),
        (Flags::UNDERLINE, flags::UNDERLINE),
        (Flags::DOUBLE_UNDERLINE, flags::DOUBLE_UNDERLINE),
        (Flags::UNDERCURL, flags::CURLY_UNDERLINE),
        (Flags::DOTTED_UNDERLINE, flags::DOTTED_UNDERLINE),
        (Flags::DASHED_UNDERLINE, flags::DASHED_UNDERLINE),
        (Flags::STRIKEOUT, flags::STRIKEOUT),
        (Flags::WIDE_CHAR, flags::WIDE),
        (Flags::WIDE_CHAR_SPACER, flags::WIDE_SPACER),
        (Flags::HIDDEN, flags::HIDDEN),
    ];
    MAP.iter()
        .filter(|(engine_flag, _)| engine.contains(*engine_flag))
        .fold(0, |bits, (_, bit)| bits | bit)
}

/// Keeps a title or error text printable and short.
fn sanitize(text: &str, max_chars: usize) -> String {
    text.chars()
        .filter(|ch| !ch.is_control())
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Output waiting to be parsed: the backend's chunks, consumed front to back.
#[derive(Default)]
struct Pending {
    chunks: VecDeque<Vec<u8>>,
    /// Bytes of the front chunk already parsed.
    offset: usize,
    /// Unparsed bytes in total.
    len: usize,
}

impl Pending {
    fn push(&mut self, bytes: Vec<u8>) {
        if !bytes.is_empty() {
            self.len += bytes.len();
            self.chunks.push_back(bytes);
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn consume(&mut self, count: usize) {
        self.offset += count;
        self.len = self.len.saturating_sub(count);
        if self
            .chunks
            .front()
            .is_some_and(|front| self.offset >= front.len())
        {
            self.chunks.pop_front();
            self.offset = 0;
        }
    }
}

/// A notice that is sent at most once per [`NOTICE_INTERVAL`]; the latest value wins.
struct Throttled<T> {
    pending: Option<T>,
    last_sent: Option<Instant>,
    last_value: Option<T>,
}

impl<T> Default for Throttled<T> {
    fn default() -> Self {
        Self {
            pending: None,
            last_sent: None,
            last_value: None,
        }
    }
}

impl<T: Clone + PartialEq> Throttled<T> {
    fn set(&mut self, value: T) {
        self.pending = Some(value);
    }

    /// When the pending value may be sent.
    fn due(&self) -> Option<Instant> {
        self.pending.as_ref()?;
        Some(
            self.last_sent
                .map_or_else(Instant::now, |sent| sent + NOTICE_INTERVAL),
        )
    }

    /// The pending value if it's due (or `force`) and differs from the last one sent.
    fn take(&mut self, now: Instant, force: bool, dedupe: bool) -> Option<T> {
        let due = self
            .last_sent
            .is_none_or(|sent| now >= sent + NOTICE_INTERVAL);
        if !(due || force) {
            return None;
        }
        let value = self.pending.take()?;
        if dedupe && self.last_value.as_ref() == Some(&value) {
            return None;
        }
        self.last_sent = Some(now);
        self.last_value = Some(value.clone());
        Some(value)
    }
}

/// The per-session engine thread.
struct Engine {
    shared: Arc<Shared>,
    backend: Option<Box<dyn TerminalBackend>>,
    events: Receiver<BackendEvent>,
    commands: Receiver<Command>,
    term_events: Receiver<Event>,
    /// vte never buffers synchronized updates here ([`NoSyncBuffer`]): their content is parsed in
    /// the usual bounded chunks, and the engine only holds the redraw back.
    processor: Processor<NoSyncBuffer>,
    /// When the current synchronized update began (the redraw is held until it ends or
    /// [`SYNC_HOLD`] passes).
    sync_hold: Option<Instant>,
    side: SideParser,
    /// Caps OSC strings before both parsers (hostile titles, ADR 0012).
    limiter: OscLimiter,
    /// Converts a legacy encoding to and from UTF-8; `None` for UTF-8.
    codec: Option<Codec>,
    /// Scratch buffer for decoded output.
    decoded: Vec<u8>,
    /// Reply to ENQ.
    answerback: Vec<u8>,
    /// Chunks of a paced paste still to send, and when the next one goes.
    paste: VecDeque<Vec<u8>>,
    paste_delay: Duration,
    paste_next: Option<Instant>,
    /// Latest OSC 52 text not sent yet.
    clipboard: Throttled<String>,
    pending: Pending,
    size: TermSize,
    pending_resize: Option<TermSize>,
    /// The backend reported the program's end (the code, or `None` when unknown).
    exit: Option<Option<i32>>,
    exit_reported: bool,
    title: Throttled<Option<String>>,
    directory: Throttled<String>,
    bell: Throttled<()>,
    blinking: Throttled<bool>,
    cursor_blinking: bool,
}

/// What woke the engine up.
enum Wake {
    Command(Option<Command>),
    Backend(Option<BackendEvent>),
    Timer,
}

impl Engine {
    fn run(mut self) {
        loop {
            if self.pending.is_empty() {
                let timer = self
                    .next_deadline()
                    .map_or_else(crossbeam_channel::never, crossbeam_channel::at);
                let wake = {
                    let commands = &self.commands;
                    let events = &self.events;
                    select! {
                        recv(commands) -> command => Wake::Command(command.ok()),
                        recv(events) -> event => Wake::Backend(event.ok()),
                        recv(timer) -> _ => Wake::Timer,
                    }
                };
                match wake {
                    // Every handle is gone.
                    Wake::Command(None) => break,
                    Wake::Command(Some(command)) => {
                        if !self.command(command) {
                            break;
                        }
                    }
                    Wake::Backend(Some(event)) => self.backend_event(event),
                    Wake::Backend(None) => self.backend_finished(),
                    Wake::Timer => {}
                }
            }
            // Commands first: typing and resizing stay responsive during heavy output.
            if !self.drain_commands() {
                break;
            }
            self.pull_output();
            self.parse_chunk();
            self.timers(Instant::now());
            self.report_exit();
        }
        if let Some(backend) = self.backend.take() {
            backend.shutdown();
        }
    }

    /// Handles queued commands; `false` means stop.
    fn drain_commands(&mut self) -> bool {
        loop {
            match self.commands.try_recv() {
                Ok(command) => {
                    if !self.command(command) {
                        return false;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return false,
            }
        }
        if let Some(size) = self.pending_resize.take() {
            self.apply_resize(size);
        }
        true
    }

    /// Handles one command; `false` means stop.
    fn command(&mut self, command: Command) -> bool {
        match command {
            Command::Write(bytes) => self.write_input(&bytes),
            Command::Paced(chunks, delay) => {
                self.paste.extend(chunks);
                self.paste_delay = delay;
                if self.paste_next.is_none() {
                    self.paste_next = Some(Instant::now());
                    self.timers(Instant::now());
                }
            }
            Command::CancelPaste => self.end_paste(),
            Command::Options(options) => {
                // A new codec only for a new encoding: the old one may hold half a character.
                let same = match &self.codec {
                    Some(codec) => codec.name().eq_ignore_ascii_case(options.encoding.trim()),
                    None => Codec::new(&options.encoding).is_none(),
                };
                if !same {
                    self.codec = Codec::new(&options.encoding);
                }
                self.answerback = options.answerback_bytes();
                self.limiter.set_clipboard(options.osc52_copy);
            }
            // Coalesced: only the last size of a burst is applied.
            Command::Resize(size) => self.pending_resize = Some(size.clamped()),
            Command::Focus(focused) => {
                let shared = Arc::clone(&self.shared);
                let mut state = shared.state.lock();
                let changed = state.focused != focused;
                state.focused = focused;
                state.term.is_focused = focused;
                // Only the engine's modes matter for focus reports.
                let modes = InputModes {
                    term: *state.term.mode(),
                    x10_mouse: false,
                };
                drop(state);
                if let Some(report) = encode_focus(focused, &modes) {
                    self.write_backend(report);
                }
                if changed {
                    self.shared.mark_dirty();
                }
            }
            Command::Redraw => {
                // Scrolling queues `MouseCursorDirty` events; nothing else to do with them.
                while self.term_events.try_recv().is_ok() {}
                self.shared.mark_dirty();
            }
            Command::Shutdown => return false,
        }
        true
    }

    fn apply_resize(&mut self, size: TermSize) {
        if (size.columns, size.lines) != (self.size.columns, self.size.lines) {
            let shared = Arc::clone(&self.shared);
            let mut state = shared.state.lock();
            state.term.resize(GridSize::from(size));
            drop(state);
            self.shared.mark_dirty();
        }
        if size != self.size {
            self.size = size;
            if let Some(backend) = &self.backend {
                let _ = backend.resize(size);
            }
        }
    }

    /// Typed or pasted input: converted to the session's encoding first.
    fn write_input(&mut self, bytes: &[u8]) {
        match &mut self.codec {
            Some(codec) => {
                let encoded = codec.encode(bytes);
                self.write_backend(&encoded);
            }
            None => self.write_backend(bytes),
        }
    }

    fn end_paste(&mut self) {
        self.paste.clear();
        self.paste_next = None;
        self.shared.pasting.store(false, Ordering::Release);
    }

    fn write_backend(&self, bytes: &[u8]) {
        if let Some(backend) = &self.backend {
            // Fails only once the backend stopped: input to a finished program is dropped.
            let _ = backend.write(bytes);
        }
    }

    fn backend_event(&mut self, event: BackendEvent) {
        match event {
            BackendEvent::Output(bytes) => self.pending.push(bytes),
            BackendEvent::Exited(code) => self.exit = Some(code),
            BackendEvent::Error(message) => {
                tracing::warn!(%message, "terminal backend error");
                // Shown in the terminal itself, in bold red, like other terminals do.
                let text = format!("\r\n\x1b[1;31m{}\x1b[0m\r\n", sanitize(&message, 1024));
                self.pending.push(text.into_bytes());
            }
        }
    }

    /// The backend's channel closed: it is completely finished.
    fn backend_finished(&mut self) {
        self.events = crossbeam_channel::never();
        if self.exit.is_none() {
            self.exit = Some(None);
        }
        if let Some(backend) = self.backend.take() {
            backend.shutdown();
        }
    }

    /// Moves available backend output into `pending`, up to [`PENDING_LIMIT`].
    fn pull_output(&mut self) {
        while self.pending.len < PENDING_LIMIT {
            match self.events.try_recv() {
                Ok(event) => self.backend_event(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.backend_finished();
                    break;
                }
            }
        }
    }

    /// Parses up to [`CHUNK_BYTES`] of pending output under one lock hold.
    fn parse_chunk(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let shared = Arc::clone(&self.shared);
        let mut state = shared.state.lock();
        let mut processed = 0;
        while processed < CHUNK_BYTES {
            let Some(front) = self.pending.chunks.front() else {
                break;
            };
            let available = &front[self.pending.offset..];
            let bytes = &available[..available.len().min(CHUNK_BYTES - processed)];
            let count = bytes.len();
            let bytes = match &mut self.codec {
                Some(codec) => {
                    self.decoded.clear();
                    codec.decode(bytes, &mut self.decoded);
                    self.decoded.as_slice()
                }
                None => bytes,
            };
            let limited = self.limiter.filter(bytes);
            let bytes = limited.as_deref().unwrap_or(bytes);
            self.side.advance(bytes);
            self.processor.advance(&mut state.term, bytes);
            processed += count;
            self.pending.consume(count);
        }
        self.after_parse(&mut state);
        drop(state);
        self.publish();
        // Inside a synchronized update (DEC mode 2026) the screen is not shown until the update
        // ends or times out.
        match (self.side.synchronized(), self.sync_hold) {
            (true, None) => {
                self.sync_hold = Some(Instant::now());
                self.shared.synchronized.store(true, Ordering::Release);
            }
            (true, Some(_)) => {}
            (false, _) => {
                self.sync_hold = None;
                self.shared.synchronized.store(false, Ordering::Release);
                self.shared.mark_dirty();
            }
        }
    }

    /// Handles the parser's events, still under the lock: replies go to the backend at once and
    /// in order (ConPTY waits for the cursor position reply before starting the shell).
    fn after_parse(&mut self, state: &mut State) {
        while let Ok(event) = self.term_events.try_recv() {
            match event {
                Event::PtyWrite(text) => self.write_backend(text.as_bytes()),
                Event::ColorRequest(index, format) => {
                    let color = state.colors.get(state.term.colors(), index);
                    self.write_backend(format(color.into()).as_bytes());
                }
                Event::TextAreaSizeRequest(format) => {
                    // The reply multiplies these as u16: keep the products in range.
                    let size = self.size;
                    let window = WindowSize {
                        num_lines: size.lines,
                        num_cols: size.columns,
                        cell_width: size.cell_width.min(u16::MAX / size.columns.max(1)),
                        cell_height: size.cell_height.min(u16::MAX / size.lines.max(1)),
                    };
                    self.write_backend(format(window).as_bytes());
                }
                Event::Title(title) => self.title.set(Some(sanitize(&title, MAX_TITLE_CHARS))),
                Event::ResetTitle => self.title.set(None),
                Event::Bell => self.bell.set(()),
                // Only sent when OSC 52 copying is on (`Osc52::OnlyCopy`); loads never are.
                Event::ClipboardStore(_, text) => self.clipboard.set(text),
                Event::ClipboardLoad(..) => {
                    tracing::debug!("OSC 52 clipboard read refused");
                }
                Event::CursorBlinkingChange
                | Event::MouseCursorDirty
                | Event::Wakeup
                | Event::Exit
                | Event::ChildExit(_) => {}
            }
        }
        let blinking = state.term.cursor_style().blinking;
        if blinking != self.cursor_blinking {
            self.cursor_blinking = blinking;
            self.blinking.set(blinking);
        }
        self.shared
            .modes
            .store(state.term.mode().bits(), Ordering::Release);
    }

    /// Publishes side-parser results.
    fn publish(&mut self) {
        self.shared
            .x10_mouse
            .store(self.side.x10_mouse(), Ordering::Release);
        self.shared
            .engine_mouse_hidden
            .store(self.side.engine_mouse_hidden(), Ordering::Release);
        if let Some(directory) = self.side.take_working_directory() {
            self.directory.set(directory);
        }
        let enquiries = self.side.take_enquiries();
        if !self.answerback.is_empty() {
            for _ in 0..enquiries.min(MAX_ANSWERBACKS_PER_CHUNK) {
                self.write_backend(&self.answerback);
            }
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        [
            self.sync_hold.map(|start| start + SYNC_HOLD),
            self.title.due(),
            self.directory.due(),
            self.bell.due(),
            self.blinking.due(),
            self.clipboard.due(),
            self.paste_next,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn timers(&mut self, now: Instant) {
        if self.sync_hold.is_some_and(|start| now >= start + SYNC_HOLD) {
            // The program never ended its synchronized update: show what it drew.
            self.sync_hold = None;
            self.side.end_synchronized();
            self.shared.synchronized.store(false, Ordering::Release);
            self.shared.mark_dirty();
        }
        if self.paste_next.is_some_and(|next| now >= next) {
            match self.paste.pop_front() {
                Some(chunk) => {
                    self.write_input(&chunk);
                    self.paste_next = Some(now + self.paste_delay);
                }
                None => self.end_paste(),
            }
            if self.paste.is_empty() {
                self.end_paste();
            }
        }
        self.flush_notices(now, false);
    }

    /// Sends the throttled notices that are due (all of them with `force`).
    fn flush_notices(&mut self, now: Instant, force: bool) {
        if let Some(title) = self.title.take(now, force, true) {
            self.shared.emit(match title {
                Some(title) => Notice::Title(title),
                None => Notice::ResetTitle,
            });
        }
        if let Some(directory) = self.directory.take(now, force, true) {
            self.shared.emit(Notice::WorkingDirectory(directory));
        }
        if self.bell.take(now, force, false).is_some() {
            self.shared.emit(Notice::Bell);
        }
        if let Some(blinking) = self.blinking.take(now, force, true) {
            self.shared.emit(Notice::CursorBlinking(blinking));
        }
        if let Some(text) = self.clipboard.take(now, force, false) {
            self.shared.emit(Notice::Clipboard(text));
        }
    }

    /// Sends [`Notice::Exited`] once the program ended and its output is parsed.
    fn report_exit(&mut self) {
        if self.exit_reported || !self.pending.is_empty() {
            return;
        }
        let Some(code) = self.exit else {
            return;
        };
        self.exit_reported = true;
        self.flush_notices(Instant::now(), true);
        self.shared.emit(Notice::Exited(code));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BackendError;
    use crate::palette::Rgb;
    use std::sync::Mutex;

    const TIMEOUT: Duration = Duration::from_secs(5);

    /// What the test backend saw.
    #[derive(Debug, Default)]
    struct Seen {
        written: Vec<u8>,
        sizes: Vec<TermSize>,
        shutdown: bool,
    }

    struct TestBackend(Arc<Mutex<Seen>>);

    impl TerminalBackend for TestBackend {
        fn write(&self, bytes: &[u8]) -> Result<(), BackendError> {
            self.0.lock().unwrap().written.extend_from_slice(bytes);
            Ok(())
        }

        fn resize(&self, size: TermSize) -> Result<(), BackendError> {
            self.0.lock().unwrap().sizes.push(size);
            Ok(())
        }

        fn shutdown(&self) {
            self.0.lock().unwrap().shutdown = true;
        }
    }

    struct Harness {
        session: Session,
        output: Sender<BackendEvent>,
        notices: Receiver<Notice>,
        seen: Arc<Mutex<Seen>>,
    }

    fn start(columns: u16, lines: u16) -> Harness {
        start_with(SessionConfig {
            size: TermSize::new(columns, lines),
            ..SessionConfig::default()
        })
    }

    fn start_with(config: SessionConfig) -> Harness {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let (output, events) = crossbeam_channel::unbounded();
        let (notice_tx, notices) = crossbeam_channel::unbounded();
        let notify: Notify = Arc::new(move |notice| {
            let _ = notice_tx.send(notice);
        });
        let session = Session::start(
            Box::new(TestBackend(Arc::clone(&seen))),
            events,
            config,
            notify,
        )
        .unwrap();
        Harness {
            session,
            output,
            notices,
            seen,
        }
    }

    impl Harness {
        fn feed(&self, bytes: &[u8]) {
            self.output
                .send(BackendEvent::Output(bytes.to_vec()))
                .unwrap();
        }

        /// Feeds bytes and waits until the engine parsed them (the text contains `marker`).
        fn feed_until(&self, bytes: &[u8], marker: &str) {
            self.feed(bytes);
            self.wait_for(|session| session.text_dump().contains(marker));
        }

        fn wait_for(&self, condition: impl Fn(&Session) -> bool) {
            let deadline = Instant::now() + TIMEOUT;
            while !condition(&self.session) {
                assert!(
                    Instant::now() < deadline,
                    "timed out:\n{}",
                    self.session.text_dump()
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        fn wait_notice(&self, wanted: impl Fn(&Notice) -> bool) -> Notice {
            let deadline = Instant::now() + TIMEOUT;
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                let notice = self.notices.recv_timeout(left).expect("notice in time");
                if wanted(&notice) {
                    return notice;
                }
            }
        }

        fn written(&self) -> Vec<u8> {
            self.seen.lock().unwrap().written.clone()
        }

        fn wait_written(&self, expected: &[u8]) {
            let deadline = Instant::now() + TIMEOUT;
            while !self
                .written()
                .windows(expected.len())
                .any(|window| window == expected)
            {
                assert!(
                    Instant::now() < deadline,
                    "never wrote {:?}; wrote {:?}",
                    String::from_utf8_lossy(expected),
                    String::from_utf8_lossy(&self.written())
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        fn frame(&self) -> Frame {
            let mut frame = Frame::default();
            self.session.snapshot(&mut frame);
            frame
        }

        fn full_frame(&self) -> Frame {
            // A palette change forces a full frame without touching the content.
            self.session.set_palette(Palette::OPENSESH_DARK);
            self.frame()
        }
    }

    fn cell_at(frame: &Frame, row: u16, column: usize) -> Cell {
        frame
            .rows
            .iter()
            .find(|candidate| candidate.index == row)
            .expect("row in frame")
            .cells[column]
    }

    fn argb(hex: u32) -> u32 {
        Rgb::from_hex(hex).to_argb()
    }

    #[test]
    fn text_and_basic_frame() {
        let h = start(20, 4);
        h.feed_until(b"hello\r\nworld", "world");
        assert_eq!(h.session.text_dump(), "hello\nworld\n\n\n");
        let frame = h.frame();
        assert_eq!((frame.columns, frame.lines), (20, 4));
        assert_eq!(frame.damage, Damage::Full);
        assert_eq!(frame.rows.len(), 4);
        assert_eq!(frame.rows[0].cells.len(), 20);
        assert_eq!(frame.background, argb(0x121419));
        let h_cell = cell_at(&frame, 0, 0);
        assert_eq!(h_cell.ch, 'h');
        assert_eq!(h_cell.fg, argb(0xD9DEE7));
        assert_eq!(h_cell.bg, frame.background);
        assert_eq!(h_cell.underline, h_cell.fg);
        assert_eq!(frame.cursor.row, 1);
        assert_eq!(frame.cursor.column, 5);
        // Unfocused until told otherwise.
        assert_eq!(frame.cursor.shape, CursorShape::HollowBlock);
        assert_eq!(frame.cursor.color, argb(0xE6B450));
    }

    #[test]
    fn sgr_colors_and_flags() {
        let h = start(40, 3);
        h.feed_until(
            b"\x1b[31mR\x1b[1mB\x1b[0m\x1b[38;5;196mC\x1b[38;2;1;2;3mT\x1b[0m\x1b[7mI\x1b[0m\x1b[2mD\x1b[0m\x1b[8mH\x1b[0mend",
            "end",
        );
        let frame = h.frame();
        let red = cell_at(&frame, 0, 0);
        assert_eq!(red.fg, argb(0xF07178));
        let bold_red = cell_at(&frame, 0, 1);
        assert_eq!(bold_red.fg, argb(0xFF8F95), "bold is bright");
        assert_eq!(bold_red.flags & flags::BOLD, flags::BOLD);
        assert_eq!(cell_at(&frame, 0, 2).fg, argb(0xFF0000));
        assert_eq!(cell_at(&frame, 0, 3).fg, argb(0x010203));
        let inverse = cell_at(&frame, 0, 4);
        assert_eq!((inverse.fg, inverse.bg), (argb(0x121419), argb(0xD9DEE7)));
        let dim = cell_at(&frame, 0, 5);
        let expected_dim = Rgb::from_hex(0xD9DEE7).mix(Rgb::from_hex(0x121419), DIM_BLEND);
        assert_eq!(dim.fg, expected_dim.to_argb());
        let hidden = cell_at(&frame, 0, 6);
        assert_eq!(hidden.flags & flags::HIDDEN, flags::HIDDEN);
        assert_eq!(hidden.fg, hidden.bg);
    }

    #[test]
    fn underline_styles_and_colors() {
        let h = start(20, 2);
        h.feed_until(
            b"\x1b[4ma\x1b[4:2mb\x1b[4:3;58:2::255:0:0mc\x1b[4:4md\x1b[4:5me\x1b[0;9mf\x1b[0;3mg\x1b[0mend",
            "end",
        );
        let frame = h.frame();
        let expect = [
            (0, flags::UNDERLINE),
            (1, flags::DOUBLE_UNDERLINE),
            (2, flags::CURLY_UNDERLINE),
            (3, flags::DOTTED_UNDERLINE),
            (4, flags::DASHED_UNDERLINE),
            (5, flags::STRIKEOUT),
            (6, flags::ITALIC),
        ];
        for (column, flag) in expect {
            let cell = cell_at(&frame, 0, column);
            assert_eq!(cell.flags & flag, flag, "column {column}");
        }
        assert_eq!(cell_at(&frame, 0, 2).underline, argb(0xFF0000));
        assert_eq!(cell_at(&frame, 0, 0).underline, argb(0xD9DEE7));
    }

    #[test]
    fn wide_and_combining_characters() {
        let h = start(10, 2);
        h.feed_until("界e\u{301}x".as_bytes(), "x");
        let frame = h.frame();
        let wide = cell_at(&frame, 0, 0);
        assert_eq!(wide.ch, '界');
        assert_eq!(wide.flags & flags::WIDE, flags::WIDE);
        let spacer = cell_at(&frame, 0, 1);
        assert_eq!(spacer.flags & flags::WIDE_SPACER, flags::WIDE_SPACER);
        assert_eq!(spacer.ch, ' ');
        let combined = cell_at(&frame, 0, 2);
        assert_eq!(combined.ch, 'e');
        assert_ne!(combined.cluster, 0);
        assert_eq!(
            frame.clusters[combined.cluster as usize - 1],
            vec!['\u{301}']
        );
        assert_eq!(cell_at(&frame, 0, 3).cluster, 0);
        assert!(h.session.text_dump().starts_with("界e\u{301}x"));
    }

    #[test]
    fn partial_damage_after_a_full_frame() {
        let h = start(10, 5);
        h.feed_until(b"a\r\nb\r\nc", "c");
        assert_eq!(h.frame().damage, Damage::Full);
        h.feed_until(b"Z", "cZ");
        let frame = h.frame();
        assert_eq!(frame.damage, Damage::Partial);
        let rows: Vec<u16> = frame.rows.iter().map(|row| row.index).collect();
        assert_eq!(rows, [2]);
        // Nothing changed since: only the cursor row is reported.
        let again = h.frame();
        assert_eq!(again.damage, Damage::Partial);
        assert!(again.rows.iter().all(|row| row.index == 2));
    }

    #[test]
    fn dirty_is_coalesced_until_snapshot() {
        let h = start(30, 3);
        h.feed_until(b"one", "one");
        h.wait_notice(|notice| *notice == Notice::Dirty);
        h.feed_until(b"two", "onetwo");
        std::thread::sleep(Duration::from_millis(20));
        assert!(
            !h.notices.try_iter().any(|notice| notice == Notice::Dirty),
            "no second Dirty before a snapshot"
        );
        h.frame();
        h.feed_until(b"three", "onetwothree");
        h.wait_notice(|notice| *notice == Notice::Dirty);
    }

    #[test]
    fn selection_marks_cells_and_copies_text() {
        let h = start(20, 3);
        h.feed_until(b"hello world\r\nsecond line", "second line");
        h.full_frame();
        h.session
            .selection_start(ViewportPoint::new(0, 6), Side::Left, SelectionKind::Simple);
        h.session
            .selection_update(ViewportPoint::new(1, 5), Side::Right);
        assert_eq!(h.session.selection_text().as_deref(), Some("world\nsecond"));
        let frame = h.frame();
        assert_eq!(
            frame.damage,
            Damage::Full,
            "selection changes redraw everything"
        );
        let selected = cell_at(&frame, 0, 6);
        assert_eq!(selected.flags & flags::SELECTED, flags::SELECTED);
        assert_eq!(selected.bg, argb(0x2B3242));
        assert_eq!(cell_at(&frame, 0, 5).flags & flags::SELECTED, 0);

        h.session.selection_start(
            ViewportPoint::new(1, 2),
            Side::Left,
            SelectionKind::Semantic,
        );
        assert_eq!(h.session.selection_text().as_deref(), Some("second"));
        h.session
            .selection_start(ViewportPoint::new(0, 0), Side::Left, SelectionKind::Lines);
        assert_eq!(h.session.selection_text().as_deref(), Some("hello world\n"));
        h.session
            .selection_start(ViewportPoint::new(0, 1), Side::Left, SelectionKind::Block);
        h.session
            .selection_update(ViewportPoint::new(1, 3), Side::Right);
        assert_eq!(h.session.selection_text().as_deref(), Some("ell\neco"));
        h.session.selection_clear();
        assert_eq!(h.session.selection_text(), None);
        // A click without a drag selects nothing.
        h.session
            .selection_start(ViewportPoint::new(0, 1), Side::Left, SelectionKind::Simple);
        assert_eq!(h.session.selection_text(), None);
    }

    #[test]
    fn search_scrolls_and_highlights() {
        let h = start_with(SessionConfig {
            size: TermSize::new(20, 3),
            options: SessionOptions {
                scrollback_lines: 100,
                ..SessionOptions::default()
            },
            ..SessionConfig::default()
        });
        let mut text = Vec::new();
        for index in 0..30 {
            text.extend_from_slice(format!("line {index}\r\n").as_bytes());
        }
        text.extend_from_slice(b"last");
        h.feed_until(&text, "last");
        assert!(matches!(
            h.session.search("(", false),
            Err(SearchError::InvalidPattern(_))
        ));
        let (start, end) = h
            .session
            .search("line 7", false)
            .unwrap()
            .expect("a match in the history");
        assert_eq!(end.column - start.column, 5);
        let frame = h.frame();
        assert!(frame.display_offset > 0, "scrolled into the history");
        let found = cell_at(&frame, start.row, usize::from(start.column));
        assert_eq!(found.flags & flags::MATCH, flags::MATCH);
        assert_eq!(found.bg, argb(0xE6B450), "focused match color");
        assert_eq!(
            h.session.text_dump().lines().nth(usize::from(start.row)),
            Some("line 7")
        );
        // The same pattern again moves on (and wraps: it's the only match).
        assert_eq!(
            h.session.search("line 7", false).unwrap(),
            Some((start, end))
        );
        // Word boundaries work (rewritten to ASCII ones, which the lazy DFA supports).
        let (start, _) = h
            .session
            .search(r"\bline 2\b", true)
            .unwrap()
            .expect("line 2 in the history");
        assert_eq!(
            h.session.text_dump().lines().nth(usize::from(start.row)),
            Some("line 2")
        );
        assert_eq!(h.session.search("no such text", true).unwrap(), None);
        // Take the frame with the matches, so the next one would be partial without a reason.
        h.session.search("line", true).unwrap();
        let _ = h.frame();
        h.session.search_clear();
        let frame = h.frame();
        assert_eq!(
            frame.damage,
            Damage::Full,
            "every highlighted row is redrawn"
        );
        assert!(
            frame
                .rows
                .iter()
                .flat_map(|row| &row.cells)
                .all(|cell| cell.flags & flags::MATCH == 0)
        );
        h.session.scroll(Scroll::Bottom);
        h.wait_for(|session| session.text_dump().contains("last"));
    }

    #[test]
    fn scrolling_through_history() {
        let h = start_with(SessionConfig {
            size: TermSize::new(10, 2),
            options: SessionOptions {
                scrollback_lines: 50,
                ..SessionOptions::default()
            },
            ..SessionConfig::default()
        });
        h.feed_until(b"a\r\nb\r\nc\r\nd", "d");
        assert_eq!(h.frame().history_size, 2);
        h.session.scroll(Scroll::Lines(1));
        assert_eq!(h.session.text_dump(), "b\nc\n");
        h.session.scroll(Scroll::Top);
        assert_eq!(h.session.text_dump(), "a\nb\n");
        let frame = h.frame();
        assert_eq!(frame.display_offset, 2);
        assert_eq!(
            frame.cursor.shape,
            CursorShape::Hidden,
            "cursor scrolled away"
        );
        h.session.scroll(Scroll::PageDown);
        assert_eq!(h.session.text_dump(), "c\nd\n");
        // Extreme deltas are clamped instead of overflowing.
        h.session.scroll(Scroll::Lines(i32::MAX));
        assert_eq!(h.session.text_dump(), "a\nb\n");
        h.session.scroll(Scroll::Lines(i32::MIN));
        assert_eq!(h.session.text_dump(), "c\nd\n");
    }

    #[test]
    fn replies_go_to_the_backend_in_order() {
        let h = start(80, 24);
        // Cursor position, then a color query, then primary device attributes.
        h.feed(b"\x1b[6n\x1b]11;?\x07\x1b[c");
        h.wait_written(b"\x1b[1;1R\x1b]11;rgb:1212/1414/1919\x07\x1b[?6c");
        // Program-set colors are reported back.
        h.feed(b"\x1b]10;rgb:12/34/56\x07\x1b]10;?\x1b\\");
        h.wait_written(b"\x1b]10;rgb:1212/3434/5656\x1b\\");
    }

    #[test]
    fn text_area_size_uses_the_pixel_size() {
        let h = start_with(SessionConfig {
            size: TermSize {
                columns: 80,
                lines: 24,
                cell_width: 9,
                cell_height: 18,
            },
            ..SessionConfig::default()
        });
        h.feed(b"\x1b[14t");
        h.wait_written(b"\x1b[4;432;720t");
    }

    #[test]
    fn title_bell_directory_and_blinking_notices() {
        let h = start(20, 2);
        h.feed(b"\x1b]0;my\x01 title\x07");
        assert_eq!(
            h.wait_notice(|notice| matches!(notice, Notice::Title(_))),
            Notice::Title("my title".to_owned())
        );
        h.feed(b"\x07\x07\x07");
        h.wait_notice(|notice| *notice == Notice::Bell);
        h.feed(b"\x1b]7;file:///tmp/dir\x07");
        h.wait_notice(|notice| matches!(notice, Notice::WorkingDirectory(_)));
        h.feed(b"\x1b[5 q");
        assert_eq!(
            h.wait_notice(|notice| matches!(notice, Notice::CursorBlinking(_))),
            Notice::CursorBlinking(true)
        );
    }

    #[test]
    fn notices_are_throttled() {
        let h = start(20, 2);
        for index in 0..200 {
            h.feed(format!("\x1b]2;title {index}\x07").as_bytes());
        }
        h.feed(b"done");
        h.wait_for(|session| session.text_dump().contains("done"));
        std::thread::sleep(NOTICE_INTERVAL * 3);
        let titles: Vec<_> = h
            .notices
            .try_iter()
            .filter_map(|notice| match notice {
                Notice::Title(title) => Some(title),
                _ => None,
            })
            .collect();
        assert!(titles.len() < 20, "{} title notices", titles.len());
        assert_eq!(titles.last().map(String::as_str), Some("title 199"));
    }

    #[test]
    fn focus_reporting_and_hollow_cursor() {
        let h = start(20, 2);
        h.session.focus_changed(true);
        h.wait_for(|session| {
            let mut frame = Frame::default();
            session.snapshot(&mut frame);
            frame.cursor.shape == CursorShape::Block
        });
        assert!(h.written().is_empty(), "no report without mode 1004");
        h.feed_until(b"\x1b[?1004hok", "ok");
        h.session.focus_changed(false);
        h.wait_written(b"\x1b[O");
        h.session.focus_changed(true);
        h.wait_written(b"\x1b[O\x1b[I");
    }

    #[test]
    fn modes_follow_the_program() {
        let h = start(20, 2);
        assert!(!h.session.modes().term.contains(TermMode::APP_CURSOR));
        h.feed_until(b"\x1b[?1h\x1b[?2004h\x1b[?9hx", "x");
        let modes = h.session.modes();
        assert!(modes.term.contains(TermMode::APP_CURSOR));
        assert!(modes.term.contains(TermMode::BRACKETED_PASTE));
        assert!(modes.x10_mouse);
        h.feed_until(b"\x1b[?1000hy", "xy");
        let modes = h.session.modes();
        assert!(!modes.x10_mouse);
        assert!(modes.term.contains(TermMode::MOUSE_REPORT_CLICK));
    }

    #[test]
    fn synchronized_updates_hold_the_redraw() {
        let h = start(20, 2);
        h.feed_until(b"before", "before");
        h.feed(b"\x1b[?2026h");
        std::thread::sleep(Duration::from_millis(20));
        h.frame();
        while h.notices.try_recv().is_ok() {}
        // The update's content is parsed in the usual bounded chunks (no 2 MiB flush under the
        // lock later), but no redraw is requested and snapshots show nothing new.
        h.feed_until(b"hidden", "hidden");
        std::thread::sleep(Duration::from_millis(20));
        assert!(!h.notices.try_iter().any(|notice| notice == Notice::Dirty));
        assert!(
            h.frame().rows.is_empty(),
            "the renderer keeps the old screen"
        );
        h.feed(b"\x1b[?2026l");
        h.wait_notice(|notice| *notice == Notice::Dirty);
        let frame = h.frame();
        assert!(
            !frame.rows.is_empty(),
            "the held damage is delivered at the end"
        );
        // An update that never ends is shown after the 150 ms deadline.
        while h.notices.try_recv().is_ok() {}
        h.feed(b"\x1b[?2026hstuck");
        h.wait_notice(|notice| *notice == Notice::Dirty);
        assert!(!h.frame().rows.is_empty());
    }

    #[test]
    fn resetting_x10_turns_mouse_reporting_off() {
        let h = start(20, 2);
        h.feed_until(b"\x1b[?1000h\x1b[?9hx", "x");
        assert!(h.session.modes().x10_mouse);
        h.feed_until(b"\x1b[?9ly", "xy");
        let modes = h.session.modes();
        assert!(!modes.x10_mouse);
        assert!(
            !modes.term.intersects(TermMode::MOUSE_MODE),
            "the older 1000 mode doesn't come back"
        );
    }

    #[test]
    fn resize_goes_to_the_term_then_the_backend() {
        let h = start(20, 4);
        h.feed_until(b"text", "text");
        h.session.resize(TermSize::new(0, 0));
        h.wait_for(|session| {
            let mut frame = Frame::default();
            session.snapshot(&mut frame);
            (frame.columns, frame.lines) == (2, 1)
        });
        h.session.resize(TermSize::new(30, 6));
        h.wait_for(|session| session.text_dump().lines().count() == 6);
        let sizes = h.seen.lock().unwrap().sizes.clone();
        assert_eq!(sizes, [TermSize::new(2, 1), TermSize::new(30, 6)]);
        assert_eq!(h.frame().columns, 30);
    }

    #[test]
    fn exit_is_reported_after_the_output() {
        let h = start(20, 2);
        h.feed(b"bye");
        h.output.send(BackendEvent::Exited(Some(3))).unwrap();
        assert_eq!(
            h.wait_notice(|notice| matches!(notice, Notice::Exited(_))),
            Notice::Exited(Some(3))
        );
        assert!(h.session.text_dump().contains("bye"));
    }

    #[test]
    fn backend_errors_are_shown_and_disconnect_means_exit() {
        let Harness {
            session,
            output,
            notices,
            seen,
        } = start(40, 3);
        output
            .send(BackendEvent::Error("could not start[2J it".to_owned()))
            .unwrap();
        drop(output);
        let exited = notices
            .iter()
            .find(|notice| matches!(notice, Notice::Exited(_)));
        assert_eq!(exited, Some(Notice::Exited(None)));
        assert!(session.text_dump().contains("could not start[2J it"));
        let mut frame = Frame::default();
        session.snapshot(&mut frame);
        let first = cell_at(&frame, 1, 0);
        assert_eq!(first.fg, argb(0xFF8F95), "bold red");
        assert!(seen.lock().unwrap().shutdown);
    }

    #[test]
    fn shutdown_stops_the_backend_and_silences_notices() {
        let h = start(20, 2);
        h.session.shutdown();
        h.session.shutdown();
        let deadline = Instant::now() + TIMEOUT;
        while !h.seen.lock().unwrap().shutdown {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        // The engine is gone: the backend's channel has no receiver any more.
        assert!(
            h.output
                .send(BackendEvent::Output(b"late".to_vec()))
                .is_err()
        );
        std::thread::sleep(Duration::from_millis(20));
        assert!(h.notices.try_recv().is_err());
        // The last screen stays readable.
        assert_eq!(h.session.text_dump(), "\n\n");
    }

    #[test]
    fn dropping_every_handle_stops_the_engine() {
        let h = start(20, 2);
        let clone = h.session.clone();
        drop(clone);
        let seen = Arc::clone(&h.seen);
        drop(h.session);
        let deadline = Instant::now() + TIMEOUT;
        while !seen.lock().unwrap().shutdown {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn palette_overrides_and_links() {
        let h = start(30, 2);
        h.feed_until(
            b"\x1b]4;1;rgb:00/ff/00\x07\x1b[31mG\x1b[0m \x1b]8;;https://example.org\x07link\x1b]8;;\x07 plain",
            "plain",
        );
        let frame = h.frame();
        assert_eq!(cell_at(&frame, 0, 0).fg, argb(0x00FF00));
        assert_eq!(cell_at(&frame, 0, 2).flags & flags::LINK, flags::LINK);
        assert_eq!(cell_at(&frame, 0, 7).flags & flags::LINK, 0);
        assert_eq!(
            h.session.hyperlink_at(ViewportPoint::new(0, 3)).as_deref(),
            Some("https://example.org")
        );
        assert_eq!(h.session.hyperlink_at(ViewportPoint::new(0, 8)), None);
        h.session
            .set_link_highlight(Some((ViewportPoint::new(0, 8), ViewportPoint::new(0, 12))));
        let frame = h.frame();
        assert_eq!(cell_at(&frame, 0, 10).flags & flags::LINK, flags::LINK);
        h.session.set_palette(Palette::OPENSESH_LIGHT);
        let frame = h.frame();
        assert_eq!(frame.background, argb(0xFAF9F5));
        assert_eq!(frame.damage, Damage::Full);
    }
    #[test]
    fn a_huge_title_is_capped_and_later_output_still_arrives() {
        let harness = start(40, 5);
        let mut stream = b"\x1b]2;".to_vec();
        stream.extend(std::iter::repeat_n(b'A', 64 * 1024));
        stream.push(0x07);
        // Each push copies the title; with the cap this costs at most 4096 x 8 KiB.
        for _ in 0..4096 {
            stream.extend_from_slice(b"\x1b[22t");
        }
        stream.extend_from_slice(b"done");
        harness.feed_until(&stream, "done");
        let title = harness.wait_notice(|notice| matches!(notice, Notice::Title(_)));
        let Notice::Title(title) = title else {
            unreachable!()
        };
        assert!(title.chars().count() <= MAX_TITLE_CHARS);
        assert!(title.starts_with("AAAA"));
    }
    #[test]
    fn the_alternate_screen_clears_the_selection() {
        let h = start(20, 3);
        h.feed_until(b"select me", "select me");
        let at = |column| ViewportPoint { row: 0, column };
        h.session
            .selection_start(at(0), Side::Left, SelectionKind::Simple);
        h.session.selection_update(at(5), Side::Right);
        assert!(h.session.has_selection());
        // vim, less and tmux switch screens: the engine drops the selection by itself.
        h.feed_until(b"\x1b[?1049halt", "alt");
        assert!(!h.session.has_selection());
    }
    #[test]
    fn selected_text_stays_readable_and_wide_characters_select_whole() {
        let h = start(20, 2);
        // Reverse video, then a wide character (two cells).
        h.feed_until(b"\x1b[7mrev\x1b[0m \xe4\xb8\xad!", "rev");
        let at = |column| ViewportPoint { row: 0, column };
        h.session
            .selection_start(at(0), Side::Left, SelectionKind::Simple);
        // Ends inside the wide character: its left half (column 4) only.
        h.session.selection_update(at(4), Side::Right);
        let frame = h.frame();
        let palette = Palette::OPENSESH_DARK;
        let reverse = cell_at(&frame, 0, 0);
        assert_eq!(reverse.flags & flags::SELECTED, flags::SELECTED);
        let fg = Rgb::new(
            u8::try_from((reverse.fg >> 16) & 0xFF).unwrap(),
            u8::try_from((reverse.fg >> 8) & 0xFF).unwrap(),
            u8::try_from(reverse.fg & 0xFF).unwrap(),
        );
        assert!(fg.contrast(palette.selection_background) >= 3.0);
        let left = cell_at(&frame, 0, 4);
        let right = cell_at(&frame, 0, 5);
        assert_eq!(left.flags & flags::SELECTED, flags::SELECTED);
        assert_eq!(
            right.flags & flags::SELECTED,
            flags::SELECTED,
            "the spacer too"
        );
    }

    #[test]
    fn answerback_replies_to_enq_only_when_set() {
        let h = start(20, 3);
        h.feed_until(b"a\x05b", "ab");
        assert!(h.written().is_empty(), "no answerback by default");
        h.session.set_options(SessionOptions {
            answerback: "OpenSesh\r\x1b[x".into(),
            ..SessionOptions::default()
        });
        h.feed_until(b"\x05c", "abc");
        h.wait_written(b"OpenSesh");
        assert_eq!(
            h.written(),
            b"OpenSesh[x",
            "control characters are never sent"
        );
    }

    #[test]
    fn legacy_encodings_convert_both_ways() {
        let h = start_with(SessionConfig {
            size: TermSize::new(20, 3),
            options: SessionOptions {
                encoding: "windows-1252".into(),
                ..SessionOptions::default()
            },
            ..SessionConfig::default()
        });
        h.feed_until(b"caf\xe9 \x80", "caf\u{e9} \u{20ac}");
        h.session.write("\u{e9}\u{20ac}".as_bytes());
        h.wait_written(b"\xe9\x80");
        // Back to UTF-8 while running.
        h.session.set_options(SessionOptions::default());
        h.feed_until("na\u{ef}ve".as_bytes(), "na\u{ef}ve");
    }

    #[test]
    fn paced_writes_arrive_in_order_with_pauses() {
        let h = start(20, 3);
        let started = Instant::now();
        h.session.write_paced(
            vec![b"one\r".to_vec(), b"two\r".to_vec(), b"three\r".to_vec()],
            Duration::from_millis(30),
        );
        assert!(h.session.is_pasting());
        h.wait_written(b"one\rtwo\rthree\r");
        assert!(started.elapsed() >= Duration::from_millis(60));
        h.wait_for(|session| !session.is_pasting());
    }

    #[test]
    fn a_paced_write_can_be_cancelled() {
        let h = start(20, 3);
        h.session.write_paced(
            vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()],
            Duration::from_secs(10),
        );
        h.wait_written(b"a");
        h.session.cancel_paste();
        h.wait_for(|session| !session.is_pasting());
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(h.written(), b"a");
    }

    #[test]
    fn osc52_sets_the_clipboard_only_when_allowed() {
        let h = start(20, 3);
        // "hello" in base64.
        h.feed_until(b"\x1b]52;c;aGVsbG8=\x07x", "x");
        std::thread::sleep(Duration::from_millis(80));
        assert!(
            !h.notices
                .try_iter()
                .any(|notice| matches!(notice, Notice::Clipboard(_))),
            "off by default"
        );
        h.session.set_options(SessionOptions {
            osc52_copy: true,
            ..SessionOptions::default()
        });
        h.feed_until(b"\x1b]52;c;aGVsbG8=\x07y", "xy");
        let notice = h.wait_notice(|notice| matches!(notice, Notice::Clipboard(_)));
        assert_eq!(notice, Notice::Clipboard("hello".into()));
        // Programs can never read the clipboard.
        h.feed_until(b"\x1b]52;c;?\x07z", "xyz");
        assert!(h.written().is_empty());
    }

    #[test]
    fn highlight_rules_style_matching_cells() {
        use opensesh_core::terminal::highlight::{
            HighlightColor, HighlightRule, HighlightSet, HighlightStyle,
        };
        let set = HighlightSet {
            id: "t".into(),
            name: "t".into(),
            builtin: false,
            rules: vec![HighlightRule {
                pattern: "ERROR".into(),
                ignore_case: false,
                style: HighlightStyle {
                    foreground: Some(HighlightColor::Ansi(1)),
                    background: None,
                    bold: true,
                    underline: true,
                },
            }],
        };
        let h = start(20, 3);
        h.feed_until(b"ok ERROR x", "ERROR");
        h.session
            .set_highlighter(Some(Arc::new(Highlighter::new([&set]))));
        let frame = h.frame();
        let hit = cell_at(&frame, 0, 3);
        assert_eq!(hit.fg, argb(0xF07178), "the theme's red");
        assert_ne!(hit.flags & flags::BOLD, 0);
        assert_ne!(hit.flags & flags::UNDERLINE, 0);
        assert_eq!(cell_at(&frame, 0, 0).flags & flags::BOLD, 0);

        h.session.set_highlighter(None);
        let frame = h.frame();
        assert_eq!(cell_at(&frame, 0, 3).flags & flags::BOLD, 0);
    }

    #[test]
    fn minimum_contrast_lifts_unreadable_text() {
        let h = start_with(SessionConfig {
            size: TermSize::new(20, 3),
            palette: Palette {
                minimum_contrast: 4.5,
                ..Palette::OPENSESH_DARK
            },
            ..SessionConfig::default()
        });
        // ANSI black on the dark background: about 1.1:1 as the theme has it.
        h.feed_until(b"\x1b[30mX\x1b[0m", "X");
        let cell = cell_at(&h.frame(), 0, 0);
        let fg = Rgb::from_hex(cell.fg);
        let bg = Rgb::from_hex(cell.bg);
        assert!(fg.contrast(bg) >= 4.5, "{:.2}", fg.contrast(bg));
    }

    #[test]
    fn the_cursor_can_stay_solid_when_unfocused() {
        let h = start(20, 3);
        h.feed_until(b"x", "x");
        assert_eq!(h.frame().cursor.shape, CursorShape::HollowBlock);
        h.session.set_options(SessionOptions {
            cursor_hollow_unfocused: false,
            ..SessionOptions::default()
        });
        assert_eq!(h.frame().cursor.shape, CursorShape::Block);
    }

    #[test]
    fn word_separators_change_while_running() {
        let h = start(30, 3);
        h.feed_until(b"alpha.beta gamma", "gamma");
        h.session.selection_start(
            ViewportPoint::new(0, 1),
            Side::Left,
            SelectionKind::Semantic,
        );
        assert_eq!(h.session.selection_text().as_deref(), Some("alpha.beta"));
        h.session.set_options(SessionOptions {
            word_separators: " .".into(),
            ..SessionOptions::default()
        });
        h.session.selection_start(
            ViewportPoint::new(0, 1),
            Side::Left,
            SelectionKind::Semantic,
        );
        assert_eq!(h.session.selection_text().as_deref(), Some("alpha"));
    }
}
