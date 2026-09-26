# Changelog

All notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project will follow [Semantic Versioning](https://semver.org/) from 1.0.

## [Unreleased]

### Added

- **Sprint 4: tabs, splits and workspaces.**
  - **Split panes ([ADR 0017](docs/adr/0017-tabs-panes-and-windows.md)):** a tab holds a tree of panes of any depth, each with its own shell. Split right or down (Alt+Shift+= / Alt+Shift+-), close a pane (Ctrl+Shift+W, the tab with its last pane), move the focus (Alt+arrows) and resize (Alt+Shift+arrows) while a tab has several panes, drag the dividers (double click centers one), swap panes, make them all the same size, and maximize one (Ctrl+Shift+Z). A new pane starts with the profile, directory and zoom of the one it splits.
  - **Tabs:** rename, eight colors that stay readable in dark and light, pin (pinned tabs come first), duplicate (Ctrl+Shift+D), close others, to the left or to the right, reopen closed tabs (Ctrl+Alt+Shift+T), drag to reorder or Ctrl+Shift+PgUp / PgDn, all from the tab's menu too. Activity and bell indicators cover every pane of a tab.
  - **Ctrl+Tab** switches to the tab used before; holding Ctrl shows the tabs in the order they were used. Ctrl+PgUp / PgDn keep the strip's order.
  - **Windows:** drag a tab out of the window, or use "Move to a new window", to give it its own window, and move it back from its menu or by dropping it on another window. Its shells keep running. Detached windows have the tabs and the status bar; the views open in the main window.
  - **Broadcast input (MultiExec, Ctrl+Shift+B):** what is typed in a receiving pane goes to every receiving pane of the tab, each key encoded for the program in that pane. Receiving panes, and only those, get a red border and a chip to leave or rejoin; the tab and the status bar show it too. A paste into several panes asks once per broadcast, and scrolling can follow along.
  - **Workspaces ([ADR 0018](docs/adr/0018-workspace-files.md)):** save the tabs of every window, with their layouts, profiles and directories, in `workspaces/*.toml`, and open, rename or delete them (the palette, or the Terminal view). "Restore sessions at startup" (Settings > General) now brings back the last session, with new shells in the same folders.
- **Sprint 3: terminal customization.**
  - **Profiles ([ADR 0016](docs/adr/0016-terminal-profiles-and-settings-files.md)):** every terminal option of PLAN §6.2 lives in profiles (`profiles/*.toml`) that inherit from the default one; the chain global, group, host, tab is in `opensesh-core` (groups and hosts use it once they exist). Each tab can switch profile from its menu, and new tabs use the profile chosen in Settings > Profiles. Every change reaches open terminals at once; files edited outside the app apply live too.
  - **Fonts:** family (monospaced fonts listed, all on request), fallback fonts, size, normal and bold weight, italics on or off, line height, letter spacing, antialiasing, hinting, and per-tab zoom (Ctrl+= / Ctrl+- / Ctrl+0). Programming ligatures are experimental and off by default ([ADR 0015](docs/adr/0015-programming-ligatures.md), with a spike in `spikes/ligatures/`).
  - **Themes:** the own TOML format, a dark and a light theme per profile, 15 built-in themes (OpenSesh Dark and Light, Catppuccin Mocha, Macchiato, Frappé and Latte, Dracula, Nord, Gruvbox Dark and Light, Tokyo Night, Storm and Day, Solarized Dark and Light) with their licenses, import from iTerm2, Windows Terminal, Alacritty, Kitty and base16 files, export to OpenSesh and Alacritty, and a visual editor.
  - **More options:** bold as bright, a minimum contrast, cursor and selection colors, cursor shape and blinking, hollow cursor without focus, padding, background opacity (the window gets an alpha channel when a profile uses it) and a background image with dimming and fit, scrollback, scroll speed and smooth scrolling, word separators, copy on select, right click pastes or opens the menu, the Linux primary selection, opt-in OSC 52 copying, the bell (flash, sound, notification or none), `TERM`, Backspace and Delete, Alt as Meta, legacy encodings through `encoding_rs`, an answerback and a pause between pasted lines.
  - **Keyword highlighting (PLAN §6.5):** regex rules with colors from the theme, bold and underline, in rule sets that profiles turn on (logs, network addresses, status words, paths and URLs built in, and your own in `highlights.toml`), applied as rows are drawn and switchable per tab.
  - **Settings pages:** Terminal (every option, inherited values marked and resettable, a live preview drawn by the real terminal), Profiles, Themes and Shortcuts (capture, conflicts, keys terminal programs need, restore defaults; changes in `keybindings.toml`).
  - **Tooling:** `cargo xtask notices` regenerates `THIRD_PARTY_NOTICES.md` (now with the themes) without the network; the Debian package needs `qml6-module-qtquick-dialogs` for the file dialogs.

### Changed

- Ctrl+Shift+W closes the focused pane (PLAN §6.4); "Close tab" has no default shortcut. Ctrl+Tab and Ctrl+Shift+Tab follow the order tabs were used in.
- The tab strip scrolls with the wheel instead of flicking, so tabs can be dragged.
- Screenshot runs add split terminal tabs with and without broadcast.
- Smoke tests and screenshot runs never write profiles, themes, rules or shortcuts.
- README: how to install OpenSesh (the Linux install script and its options, the Windows installer and portable zip), how updates work, how to check a download and how releases are made.

### Fixed

- The release script now keeps the blank line under a new version's heading in the changelog.

## [0.1.0] - 2026-09-26

### Added

- **Releases and updates:**
  - Windows: a portable zip and a per-user NSIS installer (no administrator rights) with Qt, the MSVC runtime and the bundled ConPTY (`cargo xtask dist windows`).
  - Linux: a `.deb` for Debian 13, an `.rpm` for Fedora and a pacman package for Arch, built against each distribution's Qt (`scripts/linux/build.sh`), and `install.sh` (`curl ... | bash`), which picks the right package, verifies it and installs it with the package manager.
  - `scripts/release.bat` cuts a release from a Windows machine with the WSL distros in minutes; a manual GitHub Actions workflow is the fallback.
  - Update checks (off by default, PLAN §6.1): at startup and daily when enabled, or with "Check now". The installed Windows app downloads the new installer, verifies it against `SHA256SUMS.txt` and restarts updated; portable copies and Linux packages link to the download.
- **Sprint 2: terminal engine and local terminal.**
  - **Engine (`opensesh-term`, [ADR 0012](docs/adr/0012-terminal-engine-and-session-threads.md)):** `alacritty_terminal` behind a `TerminalBackend` trait; a local PTY backend (your shell; PowerShell or cmd on Windows through ConPTY); one engine thread per session that parses in bounded chunks, answers terminal queries at once and sends damage-aware snapshots; OSC 7, X10 mouse, OSC 8 links, regex search; hostile output bounded (OSC strings, combining marks, synchronized updates).
  - **Input:** xterm key encoding (Alt, F1-F24, keypad and cursor modes, Windows AltGr and Alt codes), mouse reporting (X10, normal, button, any; SGR), bracketed paste with filtering, focus reports, URL detection.
  - **Renderer ([ADR 0013](docs/adr/0013-terminal-rendering.md)):** a scene-graph `QQuickItem` with a glyph atlas and custom shaders; truecolor, every underline style, wide and combining characters, emoji, box drawing and Powerline, all cursor shapes; only damaged rows are rebuilt, and new glyphs are rasterized within a per-frame budget.
  - **Local terminal tabs:** selection (character, word, line, block), copy and paste (Ctrl+Shift+C/V, Shift+Insert, context menu, primary selection on Linux), scrollback with a thin scrollbar, regex search (Ctrl+Shift+F), Ctrl+click links, titles from OSC 0/2, activity and bell indicators, the working directory in the status bar, an exit banner with Restart.
  - **Windows:** the modern ConPTY bundled by `cargo xtask conpty` ([ADR 0014](docs/adr/0014-bundled-conpty.md)), AltGr through Qt's `windows:altgr` option, a hardened DLL search order.
  - **Quality:** real-PTY tests, tmux/htop/less/nvim/mc/fzf tests, vttest goldens ([vttest.md](docs/testing/vttest.md)), performance against PLAN §9 ([perf.md](docs/perf.md)), IME notes ([ime.md](docs/testing/ime.md)); the smoke test runs a real shell.
  - **Project:** published at [github.com/caixax/opensesh](https://github.com/caixax/opensesh) with CI on every push.
- **Sprint 1: design system and app skeleton.**
  - **Theme (`opensesh-core::theme`, `Theme` QML singleton):**
    - Dark and light palettes from PLAN §5.2, following the system color scheme live.
    - Every text token is checked against WCAG AA in tests, for both schemes and 216 sample accents.
    - Text on accent and status fills is computed for contrast (`accentText`, `Theme.textOn()`), so any user accent stays readable; low-contrast accents raise a warning.
    - Comfortable and compact density, UI scale (80-150 %), UI font and reduce motion (all durations become 0).
  - **Settings (`config.toml`):**
    - A lenient per-field reader: an invalid value costs only that setting and produces a warning naming the key. Unknown keys and sections survive a save.
    - `schema_version` with a migrations hook. A file from a newer OpenSesh, or one with a syntax error, is never overwritten.
    - Atomic writes (temporary file, fsync, rename) with 5 rotated backups, on a background writer with a 300 ms debounce.
    - Hot reload with `notify`, which tells the app's own writes from external edits by sequence number.
    - On Windows, read-only backups can't block a save, and a symlinked `config.toml` stays a link.
    - Problems (a broken or newer file, a failed save) reach the UI as translated toasts, also at startup.
    - Window geometry, side panel and last view in a separate `state.toml`.
  - **Settings > General and Appearance:** every PLAN §6.1 option for them, applied live, with a preview card, language selector, restore defaults and the settings file location. The other sections show what sprint they arrive in.
  - **Component library:** 42 `Os*` QML files built on Qt Quick Templates, using only `Theme` tokens, with keyboard focus rings and accessible names and roles ([ADR 0008](docs/adr/0008-qml-component-library.md), [contract](docs/design/components.md)).
  - **Gallery (`--gallery`):** tokens with live contrast figures, typography and spacing, every icon, and every component in its states, with live dark/light, density, accent and reduce-motion switches that never write the user's settings.
  - **Shell:**
    - Custom title bar with tabs, and window decoration modes `auto`, `custom`, `native` and `none`. `auto` drops the window buttons on tiling compositors (Hyprland, Sway, niri, i3) ([ADR 0010](docs/adr/0010-window-decorations-and-notifications.md)).
    - Frameless move and resize through `startSystemMove()` / `startSystemResize()`. On Windows the frameless window keeps Win+arrows, taskbar minimize and the system menu.
    - Navigation rail (left, right or hidden, optional labels), placeholder views with empty states, collapsible side panel (left or right), status bar.
    - Window size, position and maximized state are restored, fitted to the screen, skipping positions that no longer fit any screen.
    - Switching views or tabs never leaves the keyboard focus on a hidden control, and popups give the focus back when they close.
  - **Command palette and shortcuts:** a central action registry drives the palette (Ctrl+Shift+P, fuzzy search, recent actions first) and the PLAN §6.4 default shortcuts, with conflict detection. Apart from F6 / Shift+F6, which move the focus between the window regions, no shortcut takes a combination terminal programs need. From Sprint 2 the terminal passes function keys to its programs, and Ctrl+F6 / Ctrl+Shift+F6 always move the focus ([ADR 0011](docs/adr/0011-focus-regions-and-function-keys.md)).
  - **Notifications:** in-app toasts plus a notification history in the status bar.
  - **Icons and fonts:**
    - Icons are rendered by a `QQuickImageProvider` (`image://icon/<name>?color=&size=`) from the pinned SVGs, recolored and cached.
    - Operating system logos come from pinned Tabler and Simple Icons packages.
    - Inter and JetBrains Mono are bundled from their pinned, sha256-verified releases (`cargo xtask fonts`) and set as the UI and monospace fonts.
  - **i18n ([ADR 0009](docs/adr/0009-i18n-pipeline.md)):** `cargo xtask i18n` runs lupdate and lrelease and generates a pseudo-locale (debug builds only). Changing the language retranslates the running UI. English is the only real language for now.
  - **Crash dialog** rebuilt with the component library.
  - **Quality:**
    - The smoke tests of the main window and the gallery visit every view and overlay, and fail on any QML warning (exit code 6).
    - `--screenshots <dir>` captures the main window, the gallery and the crash dialog in dark/light × comfortable/compact, and fails on a QML warning (exit code 6) or a failed capture (exit code 7).
    - CI runs the gallery smoke tests, uploads the screenshots and checks that translations are up to date.
  - **Docs:** ADRs 0006-0011, the component contract, developer setup and the manual test matrix.
- **Sprint 0: foundations.**
  - **Workspace:**
    - Cargo workspace (edition 2024) with the toolchain pinned to the MSRV (Rust 1.88.0).
    - Every dependency pinned in `[workspace.dependencies]` and locked by `Cargo.lock`.
    - Workspace lints deny `unwrap`/`expect` outside tests.
  - **`opensesh-core`:**
    - The application identity (`cc.caixa.OpenSesh`).
    - Data directory resolution: XDG on Linux, `%APPDATA%` / `%LOCALAPPDATA%` on Windows, and portable mode through a `portable` marker file.
    - Private (`0700`) directories on Unix.
  - **`opensesh-app`** (Qt 6 / QML bootstrap window, built with cxx-qt 0.10):
    - A Rust `SesameDoor` object driven from a QML button, and an `AppInfo` singleton that exposes startup data to QML.
    - Sets the Wayland `app_id` and the window icon.
    - Forwards Qt/QML log messages to `tracing`.
    - Placeholder Qt Quick Controls style (Fusion).
  - **Smoke tests:**
    - `--smoke-test` for headless CI. It checks that a frame renders, that the QML/Rust bridge works, and that non-ASCII text survives the build.
    - `--crash-report <file> --smoke-test` checks the crash dialog.
  - **Logging:** to stderr and to a daily-rotated file, 14 days kept. The filter comes from `OPENSESH_LOG`, and an invalid value falls back to the default.
  - **Crash reporting:**
    - **Panics** write a synchronous crash report and open a QML crash dialog in a separate process. Only the first panic of a process opens the dialog, and later panics (such as cxx's FFI unwind guard) are appended to the first report.
    - **Qt fatal errors** are written as crash reports before Qt aborts.
    - **Debug builds:** `OPENSESH_DEBUG_PANIC` makes the Knock button panic, to test the whole path.
  - **Windows release builds:** attach to the parent console for `--help`/`--version`, and show a message box when startup fails.
  - **Linux desktop entry** (`cc.caixa.OpenSesh.desktop`), and a placeholder logo generated from the pinned Lucide `door-open` glyph.
  - **`cargo xtask icons`:**
    - A reproducible icon pipeline that verifies the sha256 of each upstream package, over an HTTPS-only download.
    - Generates `THIRD_PARTY_NOTICES.md`.
  - **`cargo xtask lint-qml`:** rejects hardcoded colors and user-visible strings without `qsTr()` in QML.
  - **CI workflow:**
    - Checks: fmt, clippy, tests, QML lint, icon reproducibility, cargo-deny and cargo-audit.
    - Builds and smoke tests on Ubuntu (aqt Qt), in Arch, Fedora and Debian 13 containers, and on Windows (MSVC).
    - Actions and tools pinned; Dependabot configured.
  - **Docs:** developer setup per distro and for Windows, the manual test matrix, ADRs 0001–0005 and `CONTRIBUTING.md`.
