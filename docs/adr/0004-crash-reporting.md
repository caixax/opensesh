# ADR 0004: Panic hook with a crash dialog in a separate process

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 0

## Context

Sprint 0 asks for "a panic hook that logs and shows a dialog". Constraints:

- **No QtWidgets in the UI**, so `QMessageBox` is out.
- **Unknown state after a panic.** The panicking thread may be the GUI thread, and the Qt event loop may be blocked or half torn down.
- **Panics that unwind into cxx/cxx-qt FFI abort the process** right after the panic hook runs, so there is no chance to show anything later in the same process.
- **File logging is asynchronous** (a `tracing-appender` worker thread), so the last lines may not be flushed before an abort.

## Options

1. **Show a QML dialog from inside the panicking process.** This is unsafe: the event loop may be unusable, and it doesn't work when the panic leads to an abort.
2. **Use a native message box** (Win32 `MessageBoxW`, or zenity/kdialog on Linux). This needs per-platform code, extra dependencies or external binaries, and the result looks different on each platform.
3. **Write a crash report synchronously, then spawn our own binary in "crash dialog" mode.** The new process is fresh, loads a small QML window and shows the report.

## Decision

Option 3 (`crates/opensesh-app/src/crash.rs`).

1. The hook logs the panic through `tracing`. This is best effort, because the file log is asynchronous and the process may abort next.
2. **Only the first panic of a process** writes a new report and opens the dialog.
   - The report goes to `logs/crash-<unix time>-<pid>-<seq>.txt`. It is created with `create_new`, so a report is never overwritten.
   - It is written **synchronously**, and holds the version, OS and architecture, thread, location, message and a forced backtrace.
3. **Later panics are appended** to that same report as "followed by another panic" sections.
   - They open no new dialog.
   - This matters every time. A panic inside a QML → Rust call (any cxx/cxx-qt callback) always triggers a second one: cxx's unwind guard panics with `panic in ffi function ..., aborting`. Without this rule, the guard's report would replace the root cause and two dialogs would open. This was verified end to end in Sprint 0 (see `docs/testing/manual-matrix.md`).
4. The first panic spawns `opensesh-app --crash-report <file>` with `OPENSESH_NO_CRASH_DIALOG=1` set, so a crash inside the dialog process can't loop. It ignores stdio and doesn't wait.
5. Finally the default hook runs, which prints the panic to stderr.
6. The dialog (`qml/CrashDialog.qml`) shows the report and offers "Copy details", "Open logs folder" and "Close". The report path is shown as plain text, never as rich text.
7. **When the dialog is spawned:** only in the regular app mode. It is never spawned with `--smoke-test` or `--crash-report`, or when `OPENSESH_NO_CRASH_DIALOG` is set to a non-empty value other than `0`.
8. **Debug builds only:** `OPENSESH_DEBUG_PANIC=1` makes the Knock button panic inside a QML → Rust call. It exists to test this whole path by hand. Release builds don't contain it.

## Consequences

- The dialog works the same on every platform and needs no new dependency.
- Panic messages are shown to the user and stored on disk. As always (PLAN §8), secrets must never appear in panic messages or error strings.
- CI and headless runs set `OPENSESH_NO_CRASH_DIALOG=1`.
- Qt's own fatal errors (`qFatal`, for example a platform plugin that can't be loaded) are not Rust panics, and Qt aborts as soon as the message handler returns. The asynchronous file log may not be written in time. So the Qt message handler writes them **synchronously** as a `crash-*.txt` report (kind "Qt fatal error"), without a dialog: a dialog would usually hit the same fatal error.
- `--crash-report <file> --smoke-test` renders the dialog once and exits. CI runs it on every platform, so the dialog can't silently break.
