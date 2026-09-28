# Sprint 10: Snippets, macros and safe paste

**Goal:** automate without fear.

**Started:** 2026-09-27
**Finished:** 2026-09-28

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
- **Owner requests at the close (2026-09-28):**
  - Closing OpenSesh with sessions running asks first, on by default.
  - Settings links to the project on GitHub.
  - The Windows executable gets the app's icon.

## Checklist

### Paste protection
- [x] `opensesh-core::paste`: the analyzer (lines that run at once, control and escape characters, zero-width and bidirectional characters, homoglyphs mixed into Latin words, `curl`/`wget` piped to a shell, decoding piped to a shell, writes to shell profiles, `authorized_keys` and system files, `sudo` in a pipe, destructive commands), with positions and a severity
- [x] An exhaustive test battery (clean text stays clean; every finding with examples and near misses)
- [x] The paste review dialog: the text, editable and monospaced, the findings, Paste and Cancel; the broadcast confirmation folded into it
- [x] Settings > Terminal: paste protection on or off; per host (and group) in the editors

### Snippets
- [x] `snippets.toml`: name, folder, tags, description, shortcut, text or steps; validation with warnings, unknown keys kept, a newer file never overwritten, live reload
- [x] Variables: `{{name}}` asked once per run (last values offered), `{{secret:identity}}` from the vault
- [x] Running on the focused pane, every pane of the tab or the broadcast panes
- [x] Snippets view: folders, tags, search, the editor (text or steps), run, duplicate, delete
- [x] Quick picker (Ctrl+Shift+Space) and a shortcut per snippet
- [x] The side panel's Snippets tab: the snippets, to run in the current terminal

### Macros
- [x] Steps: send, delay, wait for a pattern (with a timeout); a failed wait stops that pane's run and says why
- [x] The recorder: start and stop in a pane, typed input with its pauses, review and save as a snippet

### Recordings
- [x] An output tap on terminal sessions; the asciicast v2 writer (output and resizes) on its own thread
- [x] Start and stop recording from the pane's menu, a mark while it records
- [x] The player: a tab that plays a recording with play, pause, speed and restart
- [x] History view: recent connections, recordings (play, show in folder, delete) and the session logs folder

### Quality
- [x] Unit tests: the paste battery, the snippet parser and variable expansion, macro steps, asciicast writing and reading
- [x] **Done when:** the paste analyzer passes its battery and snippets with variables work in broadcast (smoke test)
- [x] Smoke tests: a snippet with a variable run in broadcast on two panes, a macro with an expect step against the in-process server, the paste dialog, recording and playing a session
- [x] Screenshots: the Snippets view, the snippet editor, the quick picker, the paste review dialog, the History view

### Owner requests
- [x] Closing OpenSesh or a window with terminals, tunnels or transfers running asks first ("Confirm before closing with active sessions", on by default, "Don't ask again"); an update restart doesn't ask
- [x] Settings > About: GitHub, Report a problem and Release notes
- [x] The Windows executable's icon and version information (an `.ico` rendered by `cargo xtask icons`), also in the installer

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [x] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [x] ADR (snippets, macros and paste protection), docs, CHANGELOG, report, commits pushed to GitHub

## Report

### What was done

- **Paste protection** (`opensesh-core::paste`, no Qt, [ADR 0030](../adr/0030-snippets-macros-paste-protection-and-recordings.md)).
  - **The analyzer** is a pure function. Each finding has a kind, a severity, the byte range and the line. It looks for:
    - lines that run at once (only info when bracketed paste is on);
    - control and escape characters, zero-width and bidirectional characters;
    - letters from another alphabet in Latin words;
    - downloads and decoded text piped into a shell;
    - writes to shell profiles, `authorized_keys` and system files;
    - `sudo` in a pipe, and destructive commands.
  - **Tests:** 15, covering clean text and near misses for each rule.
  - **The paste review dialog:** the text, editable and monospaced, and the findings (a click selects their text). It opens for a warning or a danger, and for every paste into several broadcast panes: the Sprint 4 confirmation is now this dialog.
  - **The setting:** `paste_protection`, on by default, in Settings > Terminal and inherited through profiles, groups and hosts (their editors have it).
- **Snippets** (`opensesh-core::snippets`: `snippets.toml`, the text and the steps).
  - **The file:** name, folder (a path), tags, description, shortcut, and text or steps; validation with warnings, unknown keys kept, a newer file never overwritten, live reload.
  - **The text:**
    - `{{name}}` is asked once per run; the last values are offered during the session, never written.
    - `{{secret:identity}}` is the password of a keychain identity, fetched by the keychain worker when the run starts and typed from a buffer that is wiped after.
  - **Where they run:** the focused pane, every pane of the tab, or the broadcast panes; one task per pane on the SSH runtime. Text is typed like keys (a newline is Enter).
  - **The Snippets view:** folders and tags on the left, a search, and rows with Run and a menu (edit, duplicate, delete). Keys: Up/Down, Enter, F2, Delete, Ctrl+N.
  - **The editor:** text or steps, what is missing said as you type.
  - **The run dialog:** the values, and where to run.
  - **Also:** the quick picker (Ctrl+Shift+Space), a shortcut per snippet, and the side panel's Snippets tab.
- **Macros.**
  - **Steps:** type, pause, and wait for a regular expression in the output with a timeout (at most 10 minutes). The output is seen through a session tap with escape sequences removed, in a 64 KiB window. A wait that runs out stops that pane's run with a notification.
  - **The recorder** (a terminal's menu, with a chip while it records) turns typed input and its pauses of 800 ms or more into steps, and opens them in the editor with a warning about typed passwords.
- **Recordings** (`opensesh-term`).
  - **Session taps:** output, input and resizes, on the engine thread.
  - **The asciicast v2 writer:** output and resizes only, on its own thread, carrying UTF-8 characters split across chunks. Files are `0600` in a `0700` folder on Linux.
  - **The player:** a terminal backend that reads the file on its own thread. It shortens pauses to 2 s, and has play, pause, speed (0.25 to 16 times), restart and jumps (back means reset and replay).
  - **In the app:**
    - "Record the session" in a terminal's menu, with a chip while it records; the recording ends with the session.
    - A player tab with a bar: play or pause, from the start, the position (drag to jump), the speed.
    - A recording played back never writes the clipboard.
- **The History view:** the recent connections (a click connects again; Clear asks first), the recordings (play, open the folder, delete, with a mark while one is still recording), and the session and app logs folders.
- **Owner requests at the close (2026-09-28):**
  - **Closing asks first** when terminals, tunnels or file transfers still run: the main window lists what would end in every window, a detached window lists its own sessions.
    - It has "Don't ask again", and is on by default. It is the "Confirm before closing with active sessions" setting of Sprint 1, which did nothing until now.
    - An update that restarts OpenSesh doesn't ask.
  - **Settings > About:** a Source code row with GitHub, Report a problem and Release notes, from the repository in `Cargo.toml`.
  - **The Windows executable icon:**
    - `cargo xtask icons` renders the composed logo into an `.ico` (16 to 256 px: bitmaps up to 64 px, PNG at 256 px), a rasterization of the pinned SVG.
    - The build script compiles it into `OpenSesh.exe` with its version information.
    - The NSIS installer and uninstaller use it.
- **Fixed on the way:**
  - Multi-line text in the host notes, the private key import and the paste review showed only its first line (a template `TextArea` keeps a one-line implicit height).
  - The text a snippet types is filled in once, in a buffer of its final size that is wiped after, with no unwiped copy of a secret left behind.
- **Documentation:** ADR 0030, the threat model (pasted text, secrets in snippets, recorded macros, recordings), CHANGELOG and README.

### "Done when" (PLAN)

- **The paste analyzer passes its battery: yes** (15 tests: every rule with examples and near misses, and clean text staying clean).
- **Snippets with variables work in broadcast: yes.** The smoke test saves a snippet with `{{word}}`, opens its editor and the quick picker, runs it through the run dialog in the two broadcast panes of a three-pane tab, and checks the text reached both and not the third.

### How it was verified

- **Windows (Qt 6.10.3), on the final code:**
  - fmt, clippy `-D warnings`, lint-qml, i18n, shaders, deny and audit.
  - Every test: 589 passed, 29 ignored (real servers, real agents and keyrings).
  - The smoke tests: offscreen (438 steps), native (439 steps) and the gallery. The new steps:
    - a snippet with a variable, run through its dialog in the broadcast panes (not in the pane that left);
    - its editor and the quick picker opened;
    - a macro against the in-process SSH server that types, waits for the prompt and types again;
    - one whose text never shows, stopped on its timeout;
    - the SSH session recorded, found in the History list, played in a tab to its end, then jumped back to the start;
    - the risky paste waiting for the review, and a plain one going through;
    - closing a window whose shells run: the question first, then the window closes and ends its sessions.
  - Nothing written to the user's folders.
  - Screenshots (native), in dark and light, comfortable and compact:
    - the Snippets view, the editor on a macro, the quick picker;
    - the paste review of a two-line `curl | sudo bash`;
    - the player's bar, and History;
    - the close question, and Settings > About.
  - `OpenSesh.exe` read back with PowerShell: its icon, and "OpenSesh 0.1.3" in its version information.
- **WSL:**
  - **Debian 13 (Qt 6.8.2), on the final code:** the build, clippy, every test (592 passed, 34 ignored), and the smoke tests (offscreen, gallery, Wayland, X11), with `ssh-agent` holding a fixture key.
  - **Fedora 43 (Qt 6.10.3) and Arch (Qt 6.11.2):** the same full runs (590 passed, 34 ignored) on the code before the owner's requests at the close.
  - **Why not again:** the reruns of Fedora and Arch on the final code were stopped twice because the computer ran out of memory (WSL's virtual machine kept about 14 GB). The owner chose to close with GitHub Actions, which builds, tests and runs the smoke test in Fedora, Arch and Debian containers on the final code.
- **Two timing problems found on the way, both fixed:**
  - **The paste and broadcast smoke steps on Debian and in CI:** a command that wrapped in a narrow pane wasn't found. The smoke test now reads a pane's rows joined.
  - **A Sprint 9 tunnel test on Arch:** a fixed 300 ms pause was too short for the test server to let go of a port on a busy machine. The test now waits for it.
- **GitHub Actions, on the final code:** green on every job:
  - Ubuntu 24.04, Windows, and the Fedora, Arch and Debian 13 containers;
  - the `ssh` job against OpenSSH and Dropbear;
  - formatting, lints, licenses and advisories.

### Deviations

- **Recordings aren't started automatically** for a host or a profile: a pane records when asked, from its menu. An "always record this host" setting can come later.
- **The recording and the player tab titles** show the file name; the recording's own title is in the History list.
- **Snippet values are kept only for the session,** not across restarts (they could be secrets typed as plain variables).

### Pending

- **Manual matrix** ([manual-matrix.md](../testing/manual-matrix.md)): the paste review with real clipboard contents (a page with hidden characters, a `curl | sh` line), a snippet with a secret on a real server, a macro against a network device or a slow server, a long recording played at several speeds, and a recording opened with `asciinema play`.
- **Also manual:**
  - Closing with sessions, tunnels and a transfer running (the question, Cancel, Close, Don't ask again).
  - The installed app's icon in Explorer, the taskbar, the Start menu, Installed apps and the installer.
- **Carried over:** the Sprint 6 to 9 manual checks, an Ubuntu package.

### Risks

- **Paste protection works on patterns:** obfuscated commands get through. It is a second look, not a guarantee, and the dialog says what it found, not that the rest is safe.
- **A macro's wait sees the output after escape sequences are removed:** a program that draws its screen with cursor movements (a full-screen program) may not print the awaited text in order.
- **Typed secrets** follow the terminal's input path, which isn't wiped, and the remote program may echo them or keep them in its history.
- **The executable's resources** need the Windows SDK's resource compiler, which every MSVC build already has. A build without it stops with a clear error rather than making an executable without its icon.
