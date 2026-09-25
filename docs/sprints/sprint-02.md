# Sprint 2: Terminal engine and local terminal

**Goal:** a real local terminal, fast and correct.

**Started:** 2026-09-25

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only, and the repository stays local (no push).
- **Terminal settings UI is Sprint 3.** This sprint uses fixed defaults (bundled JetBrains Mono, the OpenSesh dark and light terminal colors derived from the Theme, block cursor). Only what the engine needs is configurable in code.
- **Function keys reach the program** when the terminal has the focus ([ADR 0011](../adr/0011-focus-regions-and-function-keys.md)); Ctrl+F6 / Ctrl+Shift+F6 leave the terminal.
- **Verification without real hardware:** TUI programs (nvim, tmux, htop, mc, less, fzf) and vttest are driven through a headless harness on the real PTY and engine, plus the GUI on Windows and WSLg with real key input where possible.

## Checklist

### `opensesh-term` crate
- [ ] `alacritty_terminal` wrapper with its VTE parser (versions and MSRV verified, ADR if needed)
- [ ] `TerminalBackend` trait (input, output, resize, events), ready for SSH and serial later
- [ ] PTY backend with `portable-pty`: `$SHELL` on Unix, PowerShell or cmd on Windows (ConPTY)
- [ ] Environment: `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=OpenSesh`
- [ ] Session thread model per PLAN §3.2/§3.3: coalesced reads, fair lock on the `Term`, at most one "dirty" event per frame
- [ ] Snapshot of damaged rows (cells with cluster, fg, bg, flags, underline color, hyperlink; cursor; selection)

### `TerminalItem` (render)
- [ ] ADR: C++ `QQuickItem` with the scene graph and a glyph atlas, versus `QQuickPaintedItem`
- [ ] Snapshot through cxx, glyph atlas aligned to the device pixel ratio, one batch per row
- [ ] Truecolor and 256 colors
- [ ] Bold, italic, underlines (single, double, curly, with color), strikethrough, inverse, dim, hidden
- [ ] Wide characters and CJK, combining characters, emoji through font fallback
- [ ] Cursor shapes and blinking (honouring reduce motion)

### Interaction
- [ ] Selection: character, word (double click), line (triple click), block (Alt+drag); optional copy on select; primary selection on Linux
- [ ] Bracketed paste
- [ ] Mouse reporting: X10, normal, button, any; SGR encoding
- [ ] Scrollback with a thin scrollbar; alternate screen
- [ ] Search with regex (Ctrl+Shift+F)
- [ ] Links: URL detection, Ctrl+click opens, OSC 8
- [ ] OSC 0/2 (title), OSC 7 (cwd), focus reporting, bell
- [ ] IME: basic preedit (ibus, fcitx5)
- [ ] Keyboard encoding (Alt/Meta, F1-F24, keypad and cursor modes, Ctrl+Space, ...) with unit tests

### Integration
- [ ] "Local terminal" tabs from the rail, Hosts and Ctrl+Shift+T replace the placeholders
- [ ] Tab title from OSC 0/2, closing a tab ends its session
- [ ] Copy and paste shortcuts (Ctrl+Shift+C / Ctrl+Shift+V) and a context menu

### Performance
- [ ] Coalesced reading and rendering limited to the display refresh
- [ ] Benchmark in `docs/perf.md` (throughput, `yes` and 100 MB `cat` keep the UI responsive, memory with one terminal)

### Quality
- [ ] Unit tests: key and mouse encoding, snapshot, selection, search, URL detection
- [ ] Headless harness: nvim, tmux, htop, mc, less, fzf and vttest basic sections on the real PTY
- [ ] Smoke tests cover a terminal tab (shell starts, output renders, exits cleanly)

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, deny, audit
- [ ] Build and smoke tests on Windows plus WSL Arch, Debian 13 (Qt 6.8 minimum), Fedora 43 and Ubuntu 22.04 (one distro at a time: memory)
- [ ] ADRs, dev-setup, manual matrix, CHANGELOG, report, local commits

## Report

_Filled in at the end of the sprint._
