# Sprint 10: Snippets, macros and safe paste

**Goal:** automate without fear.

**Started:** 2026-09-27

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it.
- **No new crates** unless one is needed; `regex` is already in the workspace (for expect-style waits).
- **Where the code lives:**
  - `opensesh-core::paste`: the paste analyzer, a pure function with an exhaustive test battery (PLAN §8).
  - `opensesh-core::snippets`: `snippets.toml` (PLAN §4.2) and the snippet language: text with `{{variables}}`, and macro steps.
  - `opensesh-term`: an output tap on sessions (for recordings and expect-style waits), the asciicast v2 writer, and a player backend.
  - The app: running snippets and macros on panes, the macro recorder, recordings, the Snippets view, the quick picker, the side panel's snippets, the paste review dialog and the History view.
- **Paste protection** (PLAN §8): every paste goes through the analyzer. When it finds something (several lines that would run at once, homoglyphs, zero-width or bidirectional characters, control characters, `curl … | sh`, writes to shell profiles or `authorized_keys`, `sudo` in a pipe, and more), a dialog shows the text, editable, with what was found. It can be turned off in Settings > Terminal and per host. The broadcast paste confirmation becomes part of the same dialog.
- **Snippets:**
  - `{{name}}` asks for a value when the snippet runs (once, also in broadcast); `{{secret:identity}}` types an identity's password from the vault, which never goes through QML or into `snippets.toml`.
  - Folders (a path like `Web/Ops`), tags, a description and an optional shortcut each.
  - Run in the focused pane, in every pane of the tab, or in the broadcast panes. Text is typed as input: a newline is Enter.
  - The quick picker (Ctrl+Shift+Space) finds a snippet by name, folder, tag or text.
- **Macros:** a snippet can be a list of steps: send text, wait some milliseconds, or wait for a pattern in the output (a regular expression, with a timeout). The recorder turns what is typed into a pane (with the pauses) into such a snippet, shown for review before it is saved, with a warning that typed passwords would be saved as text.
- **Recordings:** a pane can record its session as asciicast v2 in `recordings/` of the data folder (off by default). The History view lists them and plays them in a tab (play, pause, speed, restart). Test runs never write there.

## Checklist

### Paste protection
- [ ] `opensesh-core::paste`: the analyzer (lines that run at once, control and escape characters, zero-width and bidirectional characters, homoglyphs mixed into Latin words, `curl`/`wget` piped to a shell, decoding piped to a shell, writes to shell profiles, `authorized_keys` and system files, `sudo` in a pipe, destructive commands), with positions and a severity
- [ ] An exhaustive test battery (clean text stays clean; every finding with examples and near misses)
- [ ] The paste review dialog: the text, editable and monospaced, the findings, Paste and Cancel; the broadcast confirmation folded into it
- [ ] Settings > Terminal: paste protection on or off; per host (and group) in the editors

### Snippets
- [ ] `snippets.toml`: name, folder, tags, description, shortcut, text or steps; validation with warnings, unknown keys kept, a newer file never overwritten, live reload
- [ ] Variables: `{{name}}` asked once per run (last values offered), `{{secret:identity}}` from the vault
- [ ] Running on the focused pane, every pane of the tab or the broadcast panes
- [ ] Snippets view: folders, tags, search, the editor (text or steps), run, duplicate, delete
- [ ] Quick picker (Ctrl+Shift+Space) and a shortcut per snippet
- [ ] The side panel's Snippets tab: the snippets, to run in the current terminal

### Macros
- [ ] Steps: send, delay, wait for a pattern (with a timeout); a failed wait stops that pane's run and says why
- [ ] The recorder: start and stop in a pane, typed input with its pauses, review and save as a snippet

### Recordings
- [ ] An output tap on terminal sessions; the asciicast v2 writer (output and resizes) on its own thread
- [ ] Start and stop recording from the pane's menu, a mark while it records
- [ ] The player: a tab that plays a recording with play, pause, speed and restart
- [ ] History view: recent connections, recordings (play, show in folder, delete) and the session logs folder

### Quality
- [ ] Unit tests: the paste battery, the snippet parser and variable expansion, macro steps, asciicast writing and reading
- [ ] **Done when:** the paste analyzer passes its battery and snippets with variables work in broadcast (smoke test)
- [ ] Smoke tests: a snippet with a variable run in broadcast on two panes, a macro with an expect step against the in-process server, the paste dialog, recording and playing a session
- [ ] Screenshots: the Snippets view, the snippet editor, the quick picker, the paste review dialog, the History view

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [ ] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [ ] ADR (snippets, macros and paste protection), docs, CHANGELOG, report, commits pushed to GitHub

## Report

(Filled in at the end of the sprint.)
