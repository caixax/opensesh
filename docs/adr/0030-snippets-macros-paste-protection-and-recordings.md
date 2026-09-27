# ADR 0030: Snippets, macros, paste protection and recordings

- **Status:** accepted
- **Date:** 2026-09-27
- **Sprint:** 10

## Context

Sprint 10's goal is "automate without fear" (PLAN §5.4, §8):

- **Snippets:** commands with variables, in folders and tags, run in one pane or in many at once.
- **Macros:** steps that type, pause and wait for text in the output, and a recorder that makes them from what is typed.
- **Paste protection:** a review before a paste that could do harm.
- **Session recordings,** played in the app.

The terminal engine (ADR 0012) already runs each session on its own thread, and broadcast (Sprint 4) already types into several panes.

## Options and decisions

### Paste protection

**Options:**
- Parse the text as shell code.
- Look for known patterns.

Parsing can't be right for every shell a pane runs (`bash`, `fish`, PowerShell, a router's CLI), and a parser that gets it wrong feels precise while it misses things.

**Decision:** patterns. The analyzer (`opensesh-core::paste`) is a pure function with no Qt. It returns findings, each with:
- a kind and a severity (info, warning, danger);
- the byte range and the line;
- a detail (the character, the command, how many lines).

**What it looks for:**
- **Lines that run as soon as they are pasted.** It knows whether bracketed paste is on: with it, several lines wait for Enter, and that is only info.
- **Hidden characters:** control and escape characters (an `ESC` can end a bracketed paste early), zero-width and bidirectional characters.
- **Homoglyphs:** Cyrillic or Greek letters mixed into Latin words.
- **Downloads and decoded text piped into a shell.**
- **Writes to shell profiles, `authorized_keys`** and system files.
- **`sudo` in a pipe,** and destructive commands.

It has a test battery of examples and near misses (PLAN §8).

**When it asks:** only for a warning or a danger, and whenever a paste reaches several broadcast panes. The broadcast confirmation of Sprint 4 is now the same dialog: the text (editable, monospaced), the findings (a click selects their text), and Paste or Cancel.

**Settings:** `paste_protection` is a terminal setting, inherited like the others (app, profile, group, host), so a host whose pastes are known to be safe can turn it off.

### Snippets

**The file:** `snippets.toml` in the config folder, like the other TOML files (validation with warnings, unknown keys kept, a newer file never overwritten, live reload).

**The text:**
- `{{name}}` asks for a value when the snippet runs: once per run, also in broadcast. The last values are offered again during the session and never written.
- `{{secret:identity}}` types the password of a keychain identity:
  - The keychain worker gives it to the run when the run starts, as a wiped string.
  - The text with it filled in is built once, in a buffer of its final size that is wiped after typing.
  - It never goes through QML and never into `snippets.toml`.
  - A locked vault stops the run and says so.

**Running:** every run goes on the SSH runtime, one task per pane. The text is typed as input, as the keyboard would type it: a newline is Enter (`\r`), and bracketed paste is not used, so a multi-line snippet runs line by line as it would when typed.

**Where it runs:** the focused pane, every pane of the tab, or the broadcast panes, from the Snippets view, the side panel, the quick picker (Ctrl+Shift+Space) or the snippet's own shortcut.

### Macros

**Steps:**
- type text;
- pause;
- wait for a pattern in the output, with a timeout (at most 10 minutes).

**Waiting:** a run with waits adds a tap to the pane's session (below) and sees its output with escape sequences removed (the session log's cleaner), in a 64 KiB window. A wait looks after the previous match, and a failed wait stops that pane's run and says why.

**Patterns:**
- **Options:** glob patterns as in `expect`, or regular expressions.
- **Decision:** regular expressions, with the `regex` crate that is already a dependency. Its matching time is linear, so a pattern can't hang a run.

**The recorder** adds an input tap to a pane: what is typed becomes "type" steps, and a pause of 800 ms or more becomes a pause step. The result opens in the editor for review. The editor warns that anything typed, passwords included, was recorded as text, and suggests `{{secret:identity}}` instead.

### Session taps

A tap (`opensesh-term::session::Tap`) sees a session's output bytes (decoded, before parsing), its input and its resizes, on the engine thread. Macros, the recorder and recordings use it; nothing else changes in the engine.

### Recordings

**The format:**
- **Options:** a format of our own, or asciicast v2.
- **Decision:** asciicast v2, so `asciinema play`, its web player and other tools open the files too.

**What is written:** only the output and the size changes, never the input, so a password typed at a prompt that doesn't echo isn't recorded.

**The writer** runs on its own thread and flushes after half a second of quiet. It carries a UTF-8 character split across two output chunks over to the next one.

**Where:**
- **Files:** `recordings/` in the data folder, named `<date>_<time>_<name>.cast` (UTC).
- **Permissions:** on Linux, the folder is `0700` and the files are `0600`.
- **When:** only while a pane is recording. It starts and stops from the pane's menu, a chip shows it, and the recording ends with the session.

**The player** is a terminal backend (`opensesh-term::recording::player_file`):
- It reads the file on its own thread.
- It plays with pauses longer than 2 s shortened.
- Its controls: play, pause, restart, speed (0.25 to 16 times) and jumps. A jump back resets the terminal and prints everything up to that point again.
- It opens in a tab like any other pane. The History view lists the recordings with the recent connections and the logs folders.

## Consequences

- **Pastes:** a risky paste now needs one more click, and a paste into several panes still asks once per broadcast. Hosts where that is in the way can turn it off.
- **Patterns miss things:** an obfuscated command gets through. The review is a safety net, not a sandbox.
- **Secrets:**
  - A snippet's secret never leaves Rust until it is typed. What is typed into a remote shell is up to that shell (it may echo it, or keep it in its history).
  - The macro recorder does record typed passwords as text, which is why it warns before saving.
- **Recordings** keep whatever the screen showed, secrets printed there included, like session logs. They are off until asked for, per pane.
- **Test runs** keep snippets in memory and write recordings only to their own temporary folder.
