# ADR 0018: Workspace files and restoring the last session

- **Status:** accepted
- **Date:** 2026-09-26
- **Sprint:** 4

## Context

PLAN Sprint 4 asks to save a layout "with its sessions" and restore it, plus a "restore at startup" option (`general.restore_sessions` in `config.toml`, there since Sprint 1). Sessions are local terminals until SSH arrives in Sprint 7, and a running shell can't be saved: restoring means starting new shells where the old ones were. The format must hold several windows, each with tabs whose split trees (ADR 0017) can be of any depth, and it must stay readable and diff well in a synced folder, like the other settings files (ADR 0007, ADR 0016).

## Options

1. **JSON dumps of the QML state.** No schema, pane ids from one run leak into the file, and the file isn't pleasant to read.
2. **TOML with the split tree inline** and the panes as an array, pane ids in the tree being indexes into that array.
3. **A flat list of panes with parent links.** Easy to write, harder to read and to check.

## Decision

Option 2.

- **Format** (`opensesh-core::workspace`): `schema_version`, `name`, then `[[window]]` with `current_tab` and `[[window.tab]]` entries: `title` (the name the user gave), `color` (a tab color name), `pinned`, `focused` and `zoomed` (indexes), `layout` (the split tree as an inline table) and `[[window.tab.pane]]` entries with `kind` (`local` for now), `profile` and `directory`. The first window is the main one.
- **Pane ids are indexes** into the tab's pane list, so files don't depend on one run's ids. Opening a workspace gives every pane and tab a new id, which starts new shells in the saved directories (the shell's last OSC 7 directory, else the one it started in; a directory that no longer exists falls back to home). SSH and the other kinds will add their own fields to the pane entry.
- **Checks:** a tab whose layout doesn't use every listed pane exactly once is dropped with a warning; windows left without tabs are dropped; indexes and ratios out of range are fixed. A newer `schema_version` gives a warning, not a failure.
- **Saved workspaces** go to `workspaces/<id>.toml` in the config folder (the id is a slug of the name, made unique); saving under an existing name replaces that workspace. The `Workspaces` singleton lists them off the GUI thread, and saves, renames and removes them through the background writer.
- **The last session** goes to `last-session.toml` in the data folder (it is state, not a setting): written when the main window closes if "Restore sessions at startup" is on, and opened at startup with the same code as a saved workspace. The background writer is flushed at exit.
- **Test runs** never write workspace files; the smoke test checks a full save and open with `Workspaces.roundTrip()`, which converts to TOML and back in memory.

## Consequences

- A saved layout opens identically: the same trees, ratios, focused and maximized panes, tab names, colors and pins, profiles and directories (the smoke test compares them).
- What ran in a pane is not restored, only where it ran: a shell with a program running comes back as a new shell in that directory.
- Windows keep neither size nor position in the file; a restored detached window opens at a default size, and on Wayland the compositor places it.
- Unknown keys are not kept when a workspace is saved again, unlike the settings files; the files are generated.
