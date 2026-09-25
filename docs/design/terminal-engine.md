# Terminal engine: API and threads

This document is for code that uses `opensesh-term`: the GUI bridge, the terminal item, and the SSH and serial backends later. [ADR 0012](../adr/0012-terminal-engine-and-session-threads.md) explains the choices. The rustdoc of each item is the detailed reference.

## 1. Threads

```
          GUI thread (Qt)                                     per session
 ┌──────────────────────────────┐   Command (unbounded)   ┌───────────────────────────────┐
 │ Session handle               │ ──────────────────────▶ │ engine thread                 │
 │  write / resize / focus      │   Write, Resize,        │  select! {                    │
 │  shutdown                    │   Focus, Redraw,        │    commands (first),          │
 │                              │   Shutdown              │    backend events,            │
 │  snapshot / text_dump        │                         │    timer: sync update 150 ms, │
 │  scroll / selection_*        │ ─ fair lock, briefly ─▶ │           throttled notices } │
 │  search / set_palette ...    │   (at most one chunk)   │  parse ≤ 16 KiB per lock      │
 │                              │                         │  side parser (OSC 7, X10)     │
 │ notify(Notice) ◀─────────────┼──── callback, no lock ──│  replies ─▶ backend.write     │
 └──────────────────────────────┘                         └──────────────▲────────────────┘
                                                                         │ BackendEvent
                                                                         │ (bounded: 16 chunks)
 ┌────────────────────────────────────────────────────────────────────────┴───────────────┐
 │ local PTY backend (pty::spawn)                                                         │
 │  control thread: start the shell, write input, resize, teardown                        │
 │  reader thread:  blocking read (Unix: poll + wake pipe) ─▶ Output; never locks          │
 │  waiter thread:  Child::wait() ─▶ exit code                                             │
 │  Windows only:   short-lived close thread (ClosePseudoConsole can block)               │
 └────────────────────────────────────────────────────────────────────────────────────────┘
```

- Nothing on the GUI thread waits for the program. The methods that lock take the fair lock, and the engine holds it for at most one 16 KiB chunk (well under 1 ms at about 110 MB/s).
- The engine calls `notify` from its own thread and never while the `Term` is locked. The callback must not block; the usual implementation queues onto the GUI thread (`CxxQtThread::queue`). Calling `Session` methods from it is allowed.

## 2. Starting a local terminal

```rust
use std::sync::Arc;
use opensesh_term::{backend::TermSize, pty, session::{Notice, Session, SessionConfig}, shell::ShellCommand};

let size = TermSize { columns: 120, lines: 32, cell_width: 9, cell_height: 18 }; // device px per cell
let (backend, events) = pty::spawn(ShellCommand::user_shell(), size)?;          // returns at once
let session = Session::start(backend, events, SessionConfig { size, ..SessionConfig::default() },
    Arc::new(move |notice: Notice| { /* queue onto the GUI thread */ }))?;
```

- `pty::spawn(command: ShellCommand, size: TermSize) -> Result<(Box<dyn TerminalBackend>, Receiver<BackendEvent>), BackendError>`. It only fails if a thread can't be created. A shell that can't start shows up as a red error line in the terminal, followed by `Notice::Exited(None)`.
- `ShellCommand { program: Option<PathBuf>, args: Vec<OsString>, cwd: Option<PathBuf>, env: Vec<(OsString, OsString)> }`. `ShellCommand::user_shell()` gives the user's shell in the home directory. There are builders: `ShellCommand::program(p).arg(a).cwd(d).env(k, v)`. Its `Debug` output never prints environment values.
- `Session::start(backend, events, config, notify) -> Result<Session, SessionError>`. Pass the size the backend was spawned with.
- `backend::replay(bytes)` is a backend with no program: it shows fixed bytes. Use it for the gallery, screenshots and benchmarks.

## 3. `Session` (cheap to clone; dropping every clone ends the session)

| Method | Blocks? | What it does |
|---|---|---|
| `write(&self, bytes: &[u8])` | no | Queues input for the program. It doesn't scroll; call `scroll(Scroll::Bottom)` first for typed input. |
| `resize(&self, size: TermSize)` | no | The engine resizes the `Term`, then the PTY (the other order corrupts tmux redraws). Clamped to ≥ 2 × 1, and bursts are coalesced. The next `Dirty` frame has the new size. |
| `focus_changed(&self, focused: bool)` | no | Hollow cursor while unfocused. Sends `CSI I` / `CSI O` if the program enabled mode 1004. |
| `shutdown(&self)` | no | Ends the program and the threads in the background. Idempotent. No notice is sent after it (one may already be in flight). The last screen stays readable. |
| `modes(&self) -> InputModes` | lock-free | `TermMode` plus `x10_mouse`, as of the last parsed chunk. When `x10_mouse` is set, X10 is the active mouse protocol and the mouse bits of `term` are stale. |
| `snapshot(&self, out: &mut Frame)` | fair lock | See §4. It clears the dirty flag **before** locking. |
| `scroll(&self, Scroll)` | fair lock | `Scroll::{Lines(i32), PageUp, PageDown, Top, Bottom}`. A positive `Lines` scrolls up into the history. |
| `selection_start(&self, ViewportPoint, Side, SelectionKind)` | fair lock | `SelectionKind::{Simple, Block, Semantic, Lines}` (drag, Alt+drag, double click, triple click). `Side::{Left, Right}` is the half of the cell under the pointer. |
| `selection_update(&self, ViewportPoint, Side)` | fair lock | Moves the end of the selection. |
| `selection_clear(&self)` | fair lock | |
| `selection_text(&self) -> Option<String>` | fair lock | `None` without a non-empty selection. `Lines` selections end with `\n`. |
| `search(&self, pattern: &str, forward: bool) -> Result<Option<(ViewportPoint, ViewportPoint)>, SearchError>` | fair lock, bounded | Regex, smart case. `forward` goes down (newer output), otherwise up. A new pattern starts at the viewport; repeating it moves from the current match and wraps. It scrolls the match into view and returns its first and last cell. An empty pattern clears the search. It scans at most `SessionConfig::search_max_lines` lines (10,000 by default, about 16 ms for 200 columns in release). |
| `search_clear(&self)` | fair lock | Removes the highlights. |
| `text_dump(&self) -> String` | fair lock | The visible screen, one `\n`-terminated line per row, trailing spaces trimmed. For tests and the smoke test. |
| `set_palette(&self, Palette)` | fair lock | For example when the app switches between light and dark. |
| `hyperlink_at(&self, ViewportPoint) -> Option<String>` | fair lock | The OSC 8 URI under the pointer. Validate the scheme before opening it (PLAN §8). |
| `set_link_highlight(&self, Option<(ViewportPoint, ViewportPoint)>)` | fair lock | Adds `flags::LINK` to a range (inclusive, reading order), for example a detected URL under the pointer. |

`ViewportPoint { row: u16, column: u16 }`: row 0 is the top visible line. Points outside the grid are clamped.

### Notices (`session::Notice`)

| Notice | When |
|---|---|
| `Dirty` | The content changed; take a snapshot. At most one is pending: the next is sent only after `snapshot()` cleared the flag. Output that went entirely into a synchronized update doesn't send it. |
| `Title(String)` / `ResetTitle` | OSC 0/2, and the title stack. Control characters are removed and the title is cut at 512 characters. At most one every 50 ms (the latest wins). On Windows the console host announces the shell's executable path as the first title. |
| `WorkingDirectory(String)` | OSC 7 for this machine, percent-decoded, as a local path (`C:/dir` on Windows). |
| `Bell` | BEL, at most one every 50 ms. |
| `CursorBlinking(bool)` | The program changed cursor blinking (DECSCUSR, mode 12). |
| `Exited(Option<i32>)` | The program ended, sent after all its output was parsed. `None` means it was killed by a signal or failed to start. Windows codes keep their bits (for example `0xC000013A` is `-1073741510`). |

## 4. Snapshots

`snapshot(&mut frame)` fills the contract types of `snapshot.rs` and reuses `frame`'s row allocations. Keep one `Frame` per item:

- `frame.damage == Damage::Full`: `rows` holds every row. This happens on the first frame, and after a resize, scroll, selection change, palette change, link-highlight change, or any frame while a search is active (highlights move with output).
- `Damage::Partial`: `rows` holds only the damaged rows (`Row::index` is the viewport row). The row with the cursor is always included. Keep your copy of the other rows.
- Colors are final `0xAARRGGBB`. Cells with the default background have `bg == frame.background`. Bold-as-bright, dim, inverse and hidden are already applied, and so are the selection (`flags::SELECTED`), search matches (`flags::MATCH`; the current match has its own colors) and OSC 8 links (`flags::LINK`).
- `WIDE` is followed by a `WIDE_SPACER` cell (draw only its background). `cluster != 0` indexes `frame.clusters[cluster - 1]`, the combining marks.
- The cursor is in viewport coordinates. Its shape is `Hidden` when DECTCEM is off or the cursor is scrolled out of view, and `HollowBlock` while unfocused. `blinking` is the program's wish; the GUI decides whether to blink (reduce motion).
- Only one consumer may take snapshots of a session: damage is consumed by each snapshot.

## 5. Writing a backend (SSH, serial...)

Implement `TerminalBackend` (`Send`; every method returns at once) and push `BackendEvent`s:

```rust
pub trait TerminalBackend: Send {
    fn write(&self, bytes: &[u8]) -> Result<(), BackendError>;   // queue input
    fn resize(&self, size: TermSize) -> Result<(), BackendError>; // queue a window change
    fn shutdown(&self);                                          // idempotent, never blocks
}
pub enum BackendEvent { Output(Vec<u8>), Exited(Option<i32>), Error(String) }
```

- Use a **bounded** channel for `Output`, so a fast producer is slowed down. Send every `Output` before `Exited`. Close the channel (drop every sender) once finished.
- `Error` text is shown in the terminal. Never put secrets or environment values in it.
- The engine answers terminal queries through `write` immediately. A backend must not reorder writes.
- `TermSize::pixel_width()` and `pixel_height()` give the whole text area for `TIOCSWINSZ` or SSH `window-change`.

## 6. Colors

`palette::Palette` holds the default foreground and background, the cursor, selection and search colors, and the 16 ANSI colors (`normal`, `bright`), plus `bold_is_bright`.

- `Palette::OPENSESH_DARK` is exactly PLAN §4.3, and `Palette::default()` returns it. `Palette::OPENSESH_LIGHT` has AA contrast for the foreground and the 7 non-black normal colors, and 3:1 for the bright ones.
- The engine derives the 256-color cube, the gray ramp and the dim colors itself. Programs can override colors with OSC 4, 10, 11 and 12, and those overrides win.
- `palette::Rgb::to_argb()` gives the snapshot format.

## 7. Tests and probes

```sh
cargo test -p opensesh-term                                   # unit tests + real shell on the real PTY
cargo test -p opensesh-term --test tui -- --include-ignored   # tmux, htop, less, nvim, mc, fzf (Unix; skipped if missing)
cargo test -p opensesh-term --test local_shell -- --include-ignored   # + the shutdown stress test
cargo test -p opensesh-term --test vttest -- --include-ignored   # vttest items 1, 2, 3, 6, 8 against goldens (Linux; cargo xtask vttest first)
cargo test --release -p opensesh-term --test perf -- --ignored --nocapture --test-threads=1
```

`OPENSESH_BLESS=1` rewrites the vttest goldens (review every changed screen before committing). The acceptance list and the deviations are in [`docs/testing/vttest.md`](../testing/vttest.md); the measured numbers are in [`docs/perf.md`](../perf.md).

`tests/common/mod.rs` is the harness. It drives a `Session` over `pty::spawn`, waits on `text_dump()` with timeouts (scale them with `OPENSESH_TEST_TIMEOUT_SCALE`) and checks exit codes through `Notice::Exited`.

## 8. Known limits

- vttest double-size lines, VT52, 132 columns, the UK character set, blink and DECSCNM are not supported ([`docs/testing/vttest.md`](../testing/vttest.md) has the full list). Grapheme clustering is per code point: VS16 emoji stay 1 cell wide.
- `alacritty_terminal` 0.26.0 bug: in origin mode, cursor up (CUU, CPL) adds the scroll region's top twice, so vttest screens 2-07 and 2-09 are wrong. It is still on alacritty master; the goldens record the current behaviour and `docs/testing/vttest.md` explains it.
- In searches, `^` and `$` don't match at each line, and `\b` is an ASCII word boundary.
- Windows: throughput and redraw quality are the console host's. The inbox ConPTY of Windows 10 is slow (a 100 MiB copy takes about a minute); the bundled one from `cargo xtask conpty` is much faster ([ADR 0014](../adr/0014-bundled-conpty.md), [`docs/perf.md`](../perf.md)). The console host's first title is the shell's executable path.
- Unix: a program that ignores `SIGHUP` outlives its tab, as in other terminals.
