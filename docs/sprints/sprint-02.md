# Sprint 2: Terminal engine and local terminal

**Goal:** a real local terminal, fast and correct.

**Started:** 2026-09-25 · **Finished:** 2026-09-26

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only, and the repository stays local (no push).
- **Terminal settings UI is Sprint 3.** This sprint uses fixed defaults (bundled JetBrains Mono, the OpenSesh dark and light terminal colors derived from the Theme, block cursor). Only what the engine needs is configurable in code.
- **Function keys reach the program** when the terminal has the focus ([ADR 0011](../adr/0011-focus-regions-and-function-keys.md)); Ctrl+F6 / Ctrl+Shift+F6 leave the terminal.
- **Verification without real hardware:** TUI programs (nvim, tmux, htop, mc, less, fzf) and vttest are driven through a headless harness on the real PTY and engine, plus the GUI on Windows and WSLg with real key input where possible.

## Checklist

### `opensesh-term` crate
- [x] `alacritty_terminal` wrapper with its VTE parser (versions and MSRV verified, ADR if needed)
- [x] `TerminalBackend` trait (input, output, resize, events), ready for SSH and serial later
- [x] PTY backend with `portable-pty`: `$SHELL` on Unix, PowerShell or cmd on Windows (ConPTY)
- [x] Environment: `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=OpenSesh`
- [x] Session thread model per PLAN §3.2/§3.3: coalesced reads, fair lock on the `Term`, at most one "dirty" event per frame
- [x] Snapshot of damaged rows (cells with cluster, fg, bg, flags, underline color, hyperlink; cursor; selection)

### `TerminalItem` (render)
- [x] ADR: C++ `QQuickItem` with the scene graph and a glyph atlas, versus `QQuickPaintedItem`
- [x] Snapshot through cxx, glyph atlas aligned to the device pixel ratio, one batch per row
- [x] Truecolor and 256 colors
- [x] Bold, italic, underlines (single, double, curly, with color), strikethrough, inverse, dim, hidden
- [x] Wide characters and CJK, combining characters, emoji through font fallback
- [x] Cursor shapes and blinking (honouring reduce motion)

### Interaction
- [x] Selection: character, word (double click), line (triple click), block (Alt+drag); optional copy on select; primary selection on Linux
- [x] Bracketed paste
- [x] Mouse reporting: X10, normal, button, any; SGR encoding
- [x] Scrollback with a thin scrollbar; alternate screen
- [x] Search with regex (Ctrl+Shift+F)
- [x] Links: URL detection, Ctrl+click opens, OSC 8
- [x] OSC 0/2 (title), OSC 7 (cwd), focus reporting, bell
- [x] IME: basic preedit (ibus checked on X11 and Wayland; fcitx5 not tried)
- [x] Keyboard encoding (Alt/Meta, F1-F24, keypad and cursor modes, Ctrl+Space, ...) with unit tests

### Integration
- [x] "Local terminal" tabs from the rail, Hosts and Ctrl+Shift+T replace the placeholders
- [x] Tab title from OSC 0/2, closing a tab ends its session
- [x] Copy and paste shortcuts (Ctrl+Shift+C / Ctrl+Shift+V) and a context menu

### Performance
- [x] Coalesced reading and rendering limited to the display refresh
- [x] Benchmark in `docs/perf.md` (throughput, `yes` and 100 MB `cat` keep the UI responsive, memory with one terminal)

### Quality
- [x] Unit tests: key and mouse encoding, snapshot, selection, search, URL detection
- [x] Headless harness: nvim, tmux, htop, mc, less, fzf and vttest basic sections on the real PTY
- [x] Smoke tests cover a terminal tab (a hermetic shell starts, echoed text reaches the engine, closing the tab ends the session, `exit` closes the tab)

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, deny, audit
- [x] Build and smoke tests on Windows and Linux: GitHub Actions (Ubuntu 24.04, Arch, Debian 13 with Qt 6.8, Fedora, Windows MSVC), plus WSL runs during the sprint
- [x] ADRs, dev-setup, perf, CHANGELOG, report, commits pushed to GitHub

## Report

### What was done

- **`opensesh-term` crate** ([ADR 0012](../adr/0012-terminal-engine-and-session-threads.md), [design](../design/terminal-engine.md)):
  - `alacritty_terminal` 0.26 as the VT engine, behind a push-model `TerminalBackend` trait that SSH and serial will reuse.
  - Local PTY backend on `portable-pty` 0.9: the user's shell (Windows: pwsh, then powershell, then cmd), the `TERM*` variables, sanitised environment.
  - One engine thread per session: output parsed in 16 KiB lock holds (synchronized updates too), terminal queries answered at once (ConPTY waits for them), coalesced notices, damage-aware snapshots with resolved colors.
  - A side parser for OSC 7 and X10 mouse; OSC strings capped at 8 KiB before either parser.
  - OpenSesh Dark (PLAN palette) and Light palettes, regex search, OSC 8 hyperlinks.
  - Pure input encoders with tables of tests: xterm keys (Alt, F1-F24, keypad, cursor modes, Windows AltGr and Alt codes), mouse (X10, normal, button, any; default, UTF-8, SGR), paste filtering with bracketed paste, focus reports, URL detection.
- **Renderer** ([ADR 0013](../adr/0013-terminal-rendering.md)): a C++ `QQuickItem` on the Qt Quick scene graph with a glyph atlas rasterized at device resolution and a custom material (`.qsb` shaders, `cargo xtask shaders`). Only damaged rows are rebuilt; new glyphs are rasterized within a 4 ms budget per frame. All underline styles, strikethrough, wide and combining characters, emoji through fallback fonts, box drawing and Powerline fitted to the cell, every cursor shape.
- **App**: real local terminal tabs (rail, Hosts, Ctrl+Shift+T) backed by a Rust session registry, so a session outlives its view. Selection (character, word, line, block), copy and paste (Ctrl+Shift+C/V, Shift+Insert, context menu, primary selection on Linux), mouse reporting, scrollback with a thin scrollbar, regex search bar (Ctrl+Shift+F), Ctrl+click links, tab titles from OSC 0/2, activity and bell indicators, the working directory in the status bar, an exit banner with Restart.
- **Windows**: the modern ConPTY bundled next to the app by `cargo xtask conpty` ([ADR 0014](../adr/0014-bundled-conpty.md)); AltGr through Qt's `windows:altgr` option; the current directory removed from the DLL search order.
- **Tests and measurements**: 296 tests on Windows; real-PTY tests with cmd, PowerShell and sh; tmux, htop, less, nvim, mc and fzf tests; vttest items 1, 2, 3, 6 and 8 against reviewed goldens ([vttest.md](../testing/vttest.md)); IME with ibus ([ime.md](../testing/ime.md)); [perf.md](../perf.md).

### Against PLAN §9 ([perf.md](../perf.md))

| Budget | Windows (bundled ConPTY) | Debian WSLg (Qt 6.8.2, software GL) |
|---|---|---|
| Cold start under 1 s | 343 ms median | 417 ms (Wayland), 445 ms (X11) |
| RAM, 1 idle terminal, under 150 MB | 117.5 MB | 212 MB, of which about 115 MB is Mesa's software renderer |
| `yes` / `cat` 100 MB: no freeze | 180 fps, event-loop lag p99 under 1 ms | 144 fps, lag p99 under 4.5 ms |
| Key to pixel | 6 ms (Windows Terminal: 8.1 ms) | not measurable on WSLg |

### "Done when" (PLAN)

- nvim, tmux, htop, mc, less and fzf work: yes, with automated tests on four distros and real input on Windows and WSLg.
- vttest passes its basic sections: items 1, 2, 3, 6 and 8, with the deviations Alacritty shares and one upstream bug (cursor up in origin mode) listed in [vttest.md](../testing/vttest.md).
- `yes` and a 100 MB `cat` don't freeze the UI: yes (table above).

### How it was verified

- fmt, clippy `-D warnings`, 296 tests, lint-qml, i18n, shaders, cargo-deny and cargo-audit on Windows.
- GitHub Actions on every push: Ubuntu 24.04 (aqt Qt), Arch, Debian 13 (Qt 6.8.2), Fedora and Windows MSVC, each building the app and running the smoke tests.
- During the sprint, the four WSL distros (build, 36 smoke tests, clippy and tests) and real key and mouse input on Windows and WSLg.

### Review

An adversarial review found 37 issues (8 medium, none high). All medium ones and most low ones are fixed, among them: a 2 MiB lock hold in synchronized updates, a hostile title that could exhaust memory, match highlights left after a search, tab state flipping when a tab to the left closed, false activity dots, stale selection state, Windows Alt codes, unbounded glyph rasterization per frame, portable mode writing Qt's cache to the user folder, and a DLL-planting path.

### Pending

- **Review items left open (low):**
  - the Windows cursor-report race when closing a tab during startup with the inbox ConPTY;
  - soft-wrapped URLs;
  - search next/previous after output scrolls;
  - the Windows AltGr heuristic next to `windows:altgr`;
  - Shift+F10 on Windows;
  - atlas shrinking and per-vertex snapping;
  - the debug stats slot;
  - upstream notices of OpenConsole.
- **Manual matrix on real hardware:** Hyprland, Sway, KDE, GNOME, fcitx5, screen readers, fractional scaling.
- **Terminal settings** (font, colors, cursor, bell, copy on select, word separators) are Sprint 3.

### Risks

- `alacritty_terminal` bugs we can't fix here (origin-mode cursor up) and 0.x API changes.
- `portable-pty` 0.9 has had no release since 2025; its fixes live on master only.
- Custom scene-graph items draw nothing on Qt's software backend (documented in ADR 0013).
