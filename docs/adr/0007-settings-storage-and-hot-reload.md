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

For what the file holds beyond this version's settings (a newer OpenSesh's keys, comments):

1. **Regenerate the file from the known settings only.** Simplest, but the first save drops everything this version doesn't know, and a synced folder then spreads the loss.
2. **Carry unknown sections and keys through the round trip** in a `toml::Table`. No new dependency, but comments and formatting are still lost.
3. **Edit the parsed document in place with `toml_edit`**, which keeps comments too. It needs a new direct dependency and a second way of writing every setting.

## Decision

**Parsing (`opensesh-core::config`):**
- Each field is read and validated on its own. An invalid value falls back to its default and produces a warning with the dotted key (for example `appearance.density: unknown value "huge"`). The next save writes that default.
- Unknown sections and keys (top-level, or inside `[general]` and `[appearance]`) produce a warning too, but are **kept** in `Config::extra` and written back unchanged, in the same deterministic order (option 2 above). An older build sharing a synced folder therefore doesn't delete settings added by a newer one.
- A TOML syntax error fails the whole file, with the line and column. The app then runs on the defaults (or keeps its current settings, for an external edit) and **does not overwrite the file** until it parses again, so a typo in a hand edit is never replaced by the in-memory settings. An explicit, confirmed "Restore defaults" is the exception: it replaces the file, and the rotation keeps the broken one as `config.toml.bak.1`.

**Schema versions:**
- A missing `schema_version` is treated as the current version.
- A newer `schema_version` makes the file **read-only**: it is read as far as possible and never overwritten, so a downgrade can't destroy a newer config.
- Adding keys or sections doesn't need a new `schema_version`, since older builds keep them. A change that gives an existing key a value older builds reject (a new choice, for example) must bump it: otherwise an older build's next save writes its default back.
- Migrations run on the parsed table, oldest first, before fields are read. None exist yet.

**Output:** deterministic, with sections and keys in alphabetical order and one value per line (merge-friendly), plus a header comment. The header says that the file is rewritten when a setting changes in the app, and that comments and formatting are not kept.

**Atomic writes (`opensesh-core::fsutil::atomic_write`):**
1. A temporary file next to the file, `fsync`ed.
2. The current file is *copied* aside (so it never disappears).
3. `rename` over the file: atomic on POSIX, `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` on Windows.
4. Only after a successful rename are the backups rotated as `<name>.bak.1..5`: the oldest is deleted, the others shift, and the copy becomes `.bak.1`. A failed save leaves the backups as they were.
5. On Unix, the directory is `fsync`ed and files are created `0600`.

A problem with the backups is logged but never blocks the save. On Windows, `fs::copy` carries the read-only attribute over and a read-only file can't be replaced or deleted, so new backups get the attribute cleared, the oldest one is cleared before it is deleted, and every rename goes to a free name.

If the file is a **symbolic link** (dotfiles managed with GNU Stow, chezmoi or home-manager), the write goes through it: the temporary file and the rename happen next to the file the link points to, so the link survives. The backups stay next to the link, in the config directory, out of the dotfiles repository. If the target's directory is read-only (the Nix store), the save fails with a visible error and the link is left alone. Hard links are not kept: the file gets a new inode, as with any editor that saves atomically.

Writing identical content does nothing, so backups don't churn.

**Background writer (`opensesh-core::writer::FileWriter`):**
- One thread, with a 300 ms debounce per path, so dragging a slider writes once.
- `flush()` runs at exit.
- Problems reach QML as `AppSettings.problem(kind, detail)`. QML turns each `kind` into a translated sentence; `detail` is technical text (a path, an OS or parser error). A blocked save is reported once per protection state, and a repeated save error once until a save succeeds. Problems found at startup are reported on the next event-loop turn, once the shell is listening.

**Hot reload (`opensesh-core::watch::FileWatcher`):**
- Uses `notify` 8.2 on the **parent directory**, because atomic replacement by editors and by our own writer ends a watch on the file itself.
- When the file is a symbolic link, the directory of its target is watched too, so edits there (a `git pull` in a dotfiles repository) are noticed. The link is resolved once, when the watch starts.
- Events are debounced (250 ms).
- The watcher thread reads and parses the file; the GUI thread decides whether it is the echo of our own write. Each save gets a sequence number. The writer handles saves in order and may skip superseded ones, so when save `n` finishes, every earlier one is settled. The file is an echo only if it matches a save still in flight or the last one that landed. So:
  - two quick changes can't revert each other (the watcher may read the older write while the newer one is pending);
  - an external edit that restores an older text of ours (an editor undo, or a value flipped back) is applied, not mistaken for an echo;
  - while the file is protected nothing is ignored, so fixing a broken file clears the protection.
- After a successful save, the warnings are recomputed from the text that was written, so a notice about invalid values disappears once the app has rewritten them.
- "Restore defaults" keeps the settings this build doesn't know.
- Invalid external edits keep the current settings and report the error.

**UI state:** window geometry, panels and the last view go to a separate `state.toml` in the **data** directory. It is machine-local, disposable and has no backups. A file that doesn't parse (including `nan` or `inf` sizes) gives the defaults. Absurd values are replaced on load: sizes under the minimum or over 16384 px, and positions that are incomplete or 32000 px or more from the origin (where Windows parks minimized windows). The window shell then fits the restored size to the screen the window opens on.

## Consequences

- A typo in `config.toml` costs one setting, not all of them, and the user sees exactly which one.
- Settings apply live both ways: from the UI to the file, and from the file (hand-edited or synced) back to the UI.
- **Comments are not preserved.** A comment added by hand, and any custom formatting, disappears the next time a setting changes in the app. Unknown settings survive, but an invalid value of a known key is replaced by its default. If keeping comments becomes important, option 3 (`toml_edit`) can replace the writer without changing the file format.
- The same `fsutil`, `writer` and `watch` building blocks will serve `hosts.toml`, `snippets.toml`, `tunnels.toml` and `keybindings.toml`.
- Adding a setting means extending the reader, the writer, the `AppSettings` bridge and the tests. That is more code than a derive, and accepted for the robustness.
- On Windows, a same-volume NTFS rename is treated as atomic. This is common practice, but Microsoft doesn't guarantee it.
