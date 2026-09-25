# Changelog

All notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project will follow [Semantic Versioning](https://semver.org/) from 1.0.

## [Unreleased]

### Added

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
