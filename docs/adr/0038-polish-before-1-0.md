# ADR 0038: Polish before 1.0: fuzzing, lazy dialogs, high contrast and error details

- **Status:** accepted
- **Date:** 2026-10-03
- **Sprint:** 17

## Context

PLAN Sprint 17 asks for what makes a 1.0: fuzzing (quick connect, the importers, the theme parsers, the paste analyzer, the remote monitor's parser), profiling against the §9 budgets (memory leaks, start-up time, lazy loading), a full accessibility pass (a high-contrast theme, automated AA checks over the tokens), a security review, a final i18n review, human error messages with collapsible technical details, and a first start that offers import, a new host or a local terminal in at most three steps.

"Done when": the fuzzers run for an hour without crashes, the §9 budgets are met, and the manual matrix is complete.

## Decision

### Fuzzing

- **cargo-fuzz and libFuzzer** in `fuzz/`, a workspace of its own: libFuzzer needs a nightly compiler and sanitizer flags, which the app's workspace (stable, the MSRV) must not need. Its lockfile started from the app's, so the crates under test are the versions the app ships.
- **Eleven targets**, one per parser of untrusted input: quick connect, the paste analyzer, the six terminal theme formats, the remote monitor's two parsers, `~/.ssh/config`, MobaXterm, PuTTY (`.reg` and a session file), Remmina, CSV (with the guessed and every mapping), bundles (read, turned into hosts, written again), and the sync merge with Git's conflict splitter (with the invariant that merging a result with itself changes nothing).
- **Seeds from the repository's own fixtures** (`fuzz/seed.sh`), rebuilt on each run rather than committed.
- **CI:** a `Fuzz` workflow runs every target for an hour, by hand and every Sunday, uploading the input of a crash. `fuzz/run-all.sh` runs them for a minute each locally.
- **What fuzzing found** is fixed with a regression test next to the code (see `docs/security-review.md`).

### Start-up and memory: lazy dialogs

The app had grown to 889 ms and 168 MB at start (Sprint 2: 343 ms and 110 MB), most of it QML created at start whether it was used or not. **Dialogs are created the first time they open** through `LazyPopup` (a `Loader` with `get()` and `close()`): seventeen shell dialogs, the pane's context menu and the tab's paste review. Start-up fell to 720 ms and the idle app to 137 MB. A leak check in the smoke test opens and closes tabs and dialogs twelve times and fails if memory grows by more than 30 MB.

Options weighed: `asynchronous` loaders for the views (they already load on first use; making the first view asynchronous would show an empty window first), and Qt's `QQmlIncubator` (more control, more code) for what the window needs at once.

### High contrast

A `contrast` setting (`system`, `high`, `standard`; `system` follows Qt 6.10's `QAccessibilityHints::contrastPreference`) feeds the theme, which in high contrast uses black or white surfaces, text at 7:1 (WCAG AAA), outlines, the focus ring and status colors at 4.5:1, and a 3 px focus ring. The contrast tests cover it with both schemes and every accent preset, as they cover the standard theme (AA).

### Error messages

`Toasts.show` takes the technical detail (a library's error, a path, a code) apart from the message: the toast says what happened in plain words and offers "Details", which shows the technical text in a dialog where it can be selected and copied, also from the notifications panel. The toasts that pasted raw errors into their text were changed.

### Accessibility checks

`cargo xtask lint-qml` asks every icon-only button for a name (`toolTip`, `text` or `Accessible.name`), so the pass done by hand stays done.

### The first start

The empty Hosts view welcomes the first start with the ways in, one click each: import hosts (every source), a new host, a local terminal, quick connect.

## Consequences

- Parsers of untrusted input are fuzzed for an hour every week; a crash fails the run with the input attached.
- A dialog's first opening costs its creation (a few tens of milliseconds for the largest); later openings don't.
- Start-up and memory are measured again in `docs/perf.md`; the memory budget with one terminal is met with the software renderer but not with D3D11 on the measuring machine's NVIDIA driver (see there).
- Messages read as sentences, and bug reports still get the exact error.
