# ADR 0012: Terminal engine, PTY backend and session threads

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 2

## Context

PLAN §2 and §3.3 ask for an `alacritty_terminal` wrapper with its VTE parser, a `TerminalBackend` trait that SSH, serial and the rest can plug into later, and a local backend on `portable-pty`. PLAN §3.2 draws a tokio runtime with one task per session, a fair lock on the `Term` "as Alacritty does", and "dirty" notified to the GUI at most once per frame.

Facts verified for Sprint 2 (crate sources and probes on Windows 10 19045 and WSL Debian 13):

- **`alacritty_terminal` 0.26.0** (Apache-2.0, MSRV 1.85, builds on 1.88) provides `Term`, the `vte` 0.15 `Processor` and `FairMutex`. Its own `tty` + `EventLoop` exit the process when a resize fails on Unix (`die!`), assert on ConPTY errors, block in `Drop` on Unix and only drive a PTY, so SSH or serial would need a second pipeline.
- `Config::default()` enables OSC 52 clipboard writes (`OnlyCopy`); PLAN §6.2 wants them opt-in. With `kitty_keyboard` on, 4097 `CSI > 1 u` from any program panic the engine (reproduced; upstream won't fix).
- `Term` drops OSC 7 (working directory) and doesn't know X10 mouse mode (`CSI ? 9 h`). Synchronized updates (mode 2026) live in the `Processor` and need a caller-driven 150 ms timeout.
- **`portable-pty` 0.9.0** (MIT) only has blocking readers and writers. On Windows it creates the pseudoconsole with `PSEUDOCONSOLE_INHERIT_CURSOR`: the console host prints nothing until the terminal answers its `ESC[6n`. The reader sees no EOF when the shell exits. `clone_killer().kill()` returns `Err` when it succeeds. On Unix, dropping its writer types Enter and Ctrl+D into the program, and a reader thread blocked in `read()` keeps the PTY open.
- Copying Alacritty's read loop onto a blocking reader would keep the `Term` locked while waiting for the program, so the renderer would freeze until the shell prints something.

## Options

1. **`alacritty_terminal`'s `tty` + `EventLoop`.** Least code for a local terminal, but it has process-exiting error paths, and SSH would need a different pipeline.
2. **`portable-pty`, parsing on the reader thread.** It can't fire the synchronized-update timeout while blocked in `read()`, and it has no thread for search and other commands.
3. **`portable-pty` behind a push-model backend; a reader thread that never locks, and one engine thread per session that selects over output, commands and a timer.**
4. **tokio tasks per session.** `portable-pty` I/O is blocking (ConPTY pipes are synchronous), so every session would pin a blocking thread anyway.

## Decision

Option 3, with no tokio in Sprint 2. This deviates from PLAN §3.2; SSH (Sprint 7) will bridge its async channel into the same backend event channel.

**Engine (`session.rs`):**
- `Term` with 10,000 lines of history, `kitty_keyboard: false`, OSC 52 disabled and the default word separators. Sizes are clamped to at least 2 × 1. There is no `catch_unwind`: the crash hook fires on every panic anyway, and the kitty stack was the only known remote panic.
- One engine thread per session. It uses `crossbeam_channel::select!` over the backend's events, the handle's commands and a timer (the synchronized-update deadline and throttled notices). Commands are handled first, so typing stays responsive during floods.
- Already-received output is parsed under the `FairMutex` in chunks of at most 16 KiB per lock hold, so the renderer waits for at most one chunk. Synchronized updates (DEC mode 2026) are no exception: vte is given a timeout that never buffers, the content is parsed in the same chunks, and the engine only holds the redraw back (at most 150 ms). The bound is on bytes, not time: a few sequences cost O(screen) per byte (DECALN, erase), and resizing reflows the whole scrollback in one hold.
- OSC strings are capped at 8 KiB before either parser sees them, so a hostile title can't be cloned into gigabytes by the title stack (`CSI 22 t`, up to 4096 copies). The engine stops pulling output while 64 KiB wait, and the backend's bounded channel then slows the program down.
- Replies to terminal queries (`PtyWrite`, color queries, `CSI 14 t`) go to the backend at once and in order, from the engine thread. ConPTY's opening cursor query depends on this.
- A second raw `vte::Parser` (the side parser, `osc.rs`) reads the same bytes. It extracts OSC 7 (only for an empty host, `localhost` or this machine's name; payloads over 4 KiB are ignored) and tracks X10 mouse mode (the last mouse protocol set wins; RIS and any mouse-mode reset turn it off).
- The GUI gets `Notice`s through a callback, never while the lock is held. `Dirty` is coalesced with an atomic flag: it is sent on the clear-to-set transition, and `snapshot()` clears the flag first. Title, directory, bell and cursor-blink notices are sent at most every 50 ms, and the latest value wins.
- Snapshots resolve every color in Rust (theme table + OSC 4/10/11/12 overrides, bold-as-bright for the 8 normal colors, dim mixed 35 % toward the cell background, inverse, hidden, selection, search matches, cursor). They use `Term::damage()`, and force a full frame when the size, scroll position, selection, palette or link highlight changed, or while a search is active.
- Search runs on the caller's thread under the lock, bounded to 10,000 lines by default. Each scan uses a fresh copy of the compiled DFAs, because `alacritty_terminal` unwraps the lazy DFA's start state, which can only fail on a cache that has given up. `\b` and `\B` are rewritten to their ASCII forms, since the lazy DFA rejects Unicode word boundaries.

**Backend (`backend.rs`):** an object-safe, `Send` trait whose methods never block: `write`, `resize` and `shutdown`. Spawning returns the trait object and a `Receiver<BackendEvent>` (`Output`, `Exited`, `Error`). All output comes before `Exited`.

**Local PTY (`pty.rs`, `shell.rs`):**
- `pty::spawn` returns at once. A control thread resolves the shell, reads the environment (the registry on Windows, NSS on Unix) and starts the program, then writes input, applies resizes and runs the teardown. A reader thread and a waiter thread (blocked in `wait()`) complete it.
- The parent's copy of the slave is dropped right after the spawn.
- **Unix:** the terminal uses its own close-on-exec duplicates of the master: one to write to (no Enter and Ctrl+D on close) and one that the reader `poll`s together with a wake pipe. IUTF8 is set before the shell starts. Shutdown sends `SIGHUP` to the shell's process group and to the terminal's foreground group, wakes the reader and closes every master descriptor. There is no `SIGKILL` per tab.
- **Windows:** `SetConsoleCtrlHandler(NULL, FALSE)` runs once, so children don't inherit "ignore Ctrl+C". Shutdown closes the pseudoconsole on a short-lived thread, which ends every attached program the way closing a console window does. The shell is terminated only if it is still alive 3 s later, ignoring the inverted `Err` and waiting for the waiter's confirmation. The reader keeps draining until EOF. A cursor report is written before the close only when no input was ever written (the host may still be waiting for it). An unconditional one made the Windows 10 inbox host exit without ending its programs: a `ping ... >nul` survived 1 close in 8 (measured over 60 runs; 0 in 140 runs after the fix).
- **Shell:** Unix uses `$SHELL`, then the passwd entry, then `/bin/sh`, as a non-login shell in `$HOME`. Windows uses `pwsh.exe` from `PATH`, then Windows PowerShell, then `%ComSpec%`, always as an absolute path, in `%USERPROFILE%`.
- **Environment:** `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=OpenSesh` and `TERM_PROGRAM_VERSION` are set; on Windows they are added to `WSLENV`, on Unix `PWD` is set. Startup tokens, AppImage variables, `LINES`/`COLUMNS`, OpenSesh's own debug switches and a `QT_QPA_PLATFORM=windows:altgr` that OpenSesh set itself are removed.

## Consequences

- One pipeline serves every backend. The engine is Qt-free and tested headless: unit tests for the snapshot, palette, side parser and search; `/bin/sh`, `cmd.exe` and PowerShell through the real PTY; and opt-in tests for tmux, htop, less, nvim, mc and fzf. The opt-in tests pass on WSL Debian.
- A local session uses 4 long-lived threads (engine, control, reader, waiter), plus a short-lived close thread on Windows. They are mostly idle.
- Measured, release build, 200 × 60, with a thread snapshotting every 16 ms:

  | Measurement | Windows | WSL Debian |
  |---|---|---|
  | Engine throughput | 113 MB/s | 108 MB/s |
  | Snapshot wait, p99 | 1–2 ms | 0.4 ms |
  | `cat` of 105 MB through the PTY | ConPTY-bound (about 0.9 MB/s with `type` on the inbox host at 200x60; see [`docs/perf.md`](../perf.md)) | 1.0 s, snapshot p99 0.3 ms |

  A full 200 × 60 snapshot costs about 95 µs, or 230 µs while a search is active. A search step that finds nothing in 10,000 lines of 200 columns takes about 16 ms on the calling thread. `SessionConfig::search_max_lines` bounds it.
- `\b` means an ASCII word boundary in searches, and `^` and `$` don't match at every line (the search sees the buffer as one stream).
- Hostile streams can still grow memory through an unterminated OSC (vte's buffer is unbounded) and zero-width characters (fixed upstream after 0.26.0; the caret pin will pick it up). Both go to the threat model (Sprint 6).
- Programs that ignore `SIGHUP` survive a tab close on Unix, as in other terminals. A console host that never closes on Windows leaves its close and reader threads behind, logged.
- The Windows numbers are the inbox ConPTY's. Bundling a newer ConPTY is a separate decision.
