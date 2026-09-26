# ADR 0016: Terminal profiles, their inheritance and the terminal settings files

- **Status:** accepted
- **Date:** 2026-09-26
- **Sprint:** 3

## Context

PLAN §6.2 puts every terminal option in profiles with inheritance: global, then group, then host, then tab. PLAN §4.2 lists `profiles/*.toml`, `themes/*.toml` and `keybindings.toml`, and §4.3 shows a host's partial override (`[host.terminal]`) and a group that picks a profile (`[group.defaults] profile = ...`). Hosts and groups arrive in Sprint 5; the terminal options, themes, keyword highlighting rules (§6.5) and shortcuts (§6.4) arrive now, and every change must reach open terminals live. The settings files must stay readable and diff well in a Git or Syncthing folder (§4.2), like `config.toml` ([ADR 0007](0007-settings-storage-and-hot-reload.md)).

## Options

1. **All terminal options in `config.toml`**, one section per profile. One file, but hosts could not refer to profiles by file, and a synced folder would conflict on every change.
2. **Complete profiles**, each file holding every option. Simple to resolve, but changing a default means editing every profile, and nothing is inherited.
3. **Profiles as partial layers** over built-in defaults, applied in the chain's order, one file each.

## Decision

Option 3.

- **The model** (`opensesh-core::terminal::settings`): every option is declared once in a table (key, type, default, check). `TerminalSettings` holds a value for each; `TerminalOverrides` holds some of them (one layer). `resolve(layers)` applies layers in order over the defaults. Parsing is lenient per key: an invalid value is skipped with a warning naming the key, and unknown keys are kept and written back.
- **Profiles** (`profiles/<id>.toml`, `opensesh-core::terminal::profile`): the file name is the id, the file holds `schema_version`, `name` and a `[terminal]` table of overrides. `default.toml` is the **global** level; it always exists in memory, even without the file. `ProfileSet::resolve(&[group, host, tab])` builds the chain: the global profile, then for each level the profile it picks (if any) and its own overrides. Hosts and groups will pass their `[terminal]` tables and profile ids when `hosts.toml` exists (Sprint 5); today the app uses the global and tab levels (a tab picks a profile, and its font zoom is added on top).
- **The profile of new tabs** is `[terminal] profile` in `config.toml`.
- **Themes** (`themes/<id>.toml`, the §4.3 format, plus optional `search_*` colors): the built-in ones are compiled in. OpenSesh Dark and Light are own-format files; Catppuccin, Dracula, Nord, Gruvbox, Tokyo Night and Solarized are their upstream Alacritty files (pinned commits, licenses in `assets/themes/LICENSES/`), read through the Alacritty importer at start-up so the importer always runs on real files. A user theme with a built-in's id replaces it. Colors a theme leaves out are derived.
- **Keyword highlighting rules** live in `highlights.toml` (not in PLAN §4.2 yet): user rule sets `[[set]]` with `[[set.rule]]` entries; the built-in sets (logs, network, status, paths and URLs) are compiled in and can be copied, not changed. Profiles turn sets on by id (`highlight_sets`).
- **Shortcuts** (`keybindings.toml`): only the actions the user changed, `"action.id" = "Ctrl+Shift+X"` (empty for none). The defaults stay with the actions in QML (`OsAction.defaultShortcut`).
- **Live changes:** the `TerminalProfiles` and `Keybindings` QML singletons own the files: they load them at start-up, watch them (the profile and theme folders with a directory watcher), validate edits, and save through the background writer; a watcher event that arrives while our own saves are in flight is applied after they settle. The app keeps one immutable `Library` (profiles, themes, rules) that is swapped whole on every change, with a growing revision; each `TerminalItem` binds to the revision and re-resolves its profile: fonts and padding, colors, engine options (scrollback, cursor, word separators, OSC 52, encoding, answerback), key encoding and highlighting. Only `TERM` waits for a new terminal.
- **To QML** the lists go as JSON text (`JSON.parse` on the QML side) rather than item models: they are small, and one string per list keeps the bridge simple.
- **Test runs** (smoke tests, screenshots) read the files and never write them or create folders.

## Consequences

- A profile file only shows what differs from the defaults, so it diffs and merges well, and changing the default profile reaches every terminal that doesn't set the option itself.
- Removing a profile or a theme that something uses falls back to the default one, never to an error.
- The chain is tested in `opensesh-core` for all four levels; the host and group levels get their UI with hosts (Sprint 5).
- Four more files and two folders in the config directory; each has a header comment, and invalid content never discards the rest of the file.
- Unknown keys survive a save, but comments don't: the files are regenerated, as `config.toml` is.
