# Changelog

All notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project will follow [Semantic Versioning](https://semver.org/) from 1.0.

## [Unreleased]

### Added

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
