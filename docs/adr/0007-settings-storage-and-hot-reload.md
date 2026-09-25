# ADR 0007: Settings storage: lenient TOML, atomic background writes, hot reload

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 1

## Context

PLAN §4.2 asks for:

- `config.toml` with a `schema_version` and migrations;
- atomic writes (temporary file, `fsync`, `rename`) with 5 rotated backups;
- hot reload with `notify`;
- secrets never in clear text.

PLAN §0 rule 10 adds: never block the GUI thread.

Later sprints add more TOML files (`hosts.toml`, `snippets.toml`, ...) that users edit by hand or sync through Git or Syncthing (Sprint 16).

## Options

1. **serde derive with `#[serde(default)]`.** Short, but one invalid value fails the whole file (or silently disappears), and key order follows struct order.
2. **Explicit per-field reader over `toml::Table`.** More code, but every field is validated on its own and gets a precise warning.

For writing:

1. **Synchronous writes from the setter.** Simple, but it blocks the GUI thread on disk I/O.
2. **A background writer with a debounce.**

## Decision

**Parsing (`opensesh-core::config`):**
- Each field is read and validated on its own. An unknown or invalid value falls back to its default and produces a warning with the dotted key (for example `appearance.density: unknown value "huge"`). Unknown keys produce a warning too.
- A TOML syntax error fails the whole file, with the line and column. The app then runs on the defaults (or keeps its current settings, for an external edit) and **does not overwrite the file** until it parses again, so a typo in a hand edit is never replaced by the in-memory settings. An explicit, confirmed "Restore defaults" is the exception: it replaces the file, and the rotation keeps the broken one as `config.toml.bak.1`.

**Schema versions:**
- A missing `schema_version` is treated as the current version.
- A newer `schema_version` makes the file **read-only**: it is read as far as possible and never overwritten, so a downgrade can't destroy a newer config.
- Migrations run on the parsed table, oldest first, before fields are read. None exist yet.

**Output:** deterministic, with sections and keys in alphabetical order and one value per line (merge-friendly), plus a header comment.

**Atomic writes (`opensesh-core::fsutil::atomic_write`):**
1. A temporary file in the same directory, `fsync`ed.
2. Backups rotated as `<name>.bak.1..5`. The current file is *copied*, so it never disappears.
3. `rename` over the target: atomic on POSIX, `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` on Windows.
4. On Unix, the directory is `fsync`ed and files are created `0600`.

Writing identical content does nothing, so backups don't churn.

**Background writer (`opensesh-core::writer::FileWriter`):**
- One thread, with a 300 ms debounce per path, so dragging a slider writes once.
- `flush()` runs at exit.
- Errors come back to QML as `AppSettings.problem(message)`.

**Hot reload (`opensesh-core::watch::FileWatcher`):**
- Uses `notify` 8.2 on the **parent directory**, because atomic replacement by editors and by our own writer ends a watch on the file itself.
- Events are debounced (250 ms).
- The watcher ignores the echo of our own writes by comparing the file with the **recent** texts we queued (the last 16). Comparing with only the latest one is not enough: when two changes are saved within the debounce, the watcher can read the older write while the newer one is still pending, mistake it for an external edit and revert the newer value in memory.
- Invalid external edits keep the current settings and report the error.

**UI state:** window geometry, panels and the last view go to a separate `state.toml` in the **data** directory. It is machine-local, disposable and has no backups.

## Consequences

- A typo in `config.toml` costs one setting, not all of them, and the user sees exactly which one.
- Settings apply live both ways: from the UI to the file, and from the file (hand-edited or synced) back to the UI.
- The same `fsutil`, `writer` and `watch` building blocks will serve `hosts.toml`, `snippets.toml`, `tunnels.toml` and `keybindings.toml`.
- Adding a setting means extending the reader, the writer, the `AppSettings` bridge and the tests. That is more code than a derive, and accepted for the robustness.
- On Windows, a same-volume NTFS rename is treated as atomic. This is common practice, but Microsoft doesn't guarantee it.
