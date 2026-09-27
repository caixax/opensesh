# Sprint 8: SFTP

**Goal:** the best built-in remote file explorer.

**Started:** 2026-09-27

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it.
- **`russh-sftp` 3.0.0** (checked on crates.io on 2026-09-27, released 2026-09-08, Apache-2.0).
  - It runs over any stream, so it uses a channel of our `russh` connection; it doesn't depend on `russh` itself.
  - Reads and writes are pipelined (16 requests in flight by default), which a 1 GB transfer needs.
  - Its server half gives the tests and the smoke test an in-process SFTP server over a temporary folder.
- **Where the code lives:**
  - `opensesh-ssh::sftp` (no Qt): the remote file system, the local one behind the same interface, and the transfer engine.
  - The app: a `SftpBrowser` object per pane of files, a transfer queue shared by the windows, and the QML views.
- **Two ways in (PLAN):**
  - A side panel in SSH tabs, on a new channel of the tab's connection: no second login, and it follows the terminal's folder.
  - The SFTP view, with two panes (local | remote, or remote | remote). It opens its own connection, with the same prompts as a terminal pane.
- **Following the terminal's folder** uses OSC 7, which the terminal already understands.
  - For shells that don't send it, OpenSesh shows a shell integration snippet to copy.
  - It adds the snippet to the remote rc file only when the user asks for it on that host and confirms. Nothing is changed without consent (PLAN).
- **Transfers:**
  - One queue: a parallel limit, progress, speed and ETA, and pause, resume, cancel and retry.
  - Partial files are resumed from where they stopped, after checking the part that is already there.
  - When a file already exists, an overwrite policy applies (ask, overwrite, overwrite if newer, resume, skip, rename).
  - Times and permissions are kept when asked. Folders are copied recursively.
- **Editing remote files:**
  - The file is downloaded to a private temporary folder and opened with the system's editor or a configured command.
  - `notify` watches it; each save is uploaded after checking the remote modification time for conflicts.
  - "Save with sudo" is opt-in with a warning, and goes through `sudo tee`.
- **Drag and drop** with the system's file manager, both ways, and between panes. Dragging remote files out downloads them first. Wayland is checked.
- **SCP fallback:** a spike (`spikes/scp-fallback/`), for servers without the SFTP subsystem.

## Checklist

### Engine (`opensesh-ssh::sftp`)
- [x] `russh-sftp` 3.0.0 pinned; an SFTP session on a new channel of a connection, and a connection of its own
- [x] The file system interface, remote and local: list (with symlink targets), stat, create a folder or an empty file, rename, delete (recursive), chmod, symlink, read link, disk usage where the server says (`statvfs`)
- [x] The transfer engine:
  - [x] upload and download, recursive, with a parallel limit
  - [x] progress, speed and ETA
  - [x] pause, resume, cancel and retry
  - [x] resuming partial files
  - [x] the overwrite policy
  - [x] keeping times and permissions
- [x] Copy within a remote host (`cp -R` over an exec channel when there is a shell, else through the client)
- [x] The in-process SFTP server for the tests and the smoke test

### Sessions
- [x] The SSH backend shares its live connection; the side panel opens an `sftp` channel on it, and again after a reconnection
- [x] The SFTP view connects by itself (host or quick-connect text), with the prompt cards of the terminal pane

### App
- [x] `SftpBrowser` (one per file pane): path, listing (a model for a virtualized list), sorting, hidden files, errors, busy state
- [x] File pane component: breadcrumbs and an editable path, a list with name, size, modified, permissions and owner, keyboard navigation, multi-selection, the context menu
- [x] Operations: new folder, new file, rename, delete (with confirmation), permissions dialog (rwx grid and octal), new symlink, properties, copy, cut and paste
- [x] Side panel in SSH tabs (Ctrl+Shift+E), following the terminal's folder (OSC 7); the shell integration offer
- [x] SFTP view: two panes, local or remote each; a host picker; swap panes
- [x] Drag and drop: from the file manager (upload), to the file manager (download first), between panes (remote files are not dragged out: see Deviations)
- [x] Transfer queue panel: jobs with progress, speed and ETA; pause, resume, cancel, retry and clear; the overwrite question
- [x] Editing remote files: temporary copy, the system editor or a command, upload on save with the conflict check, "save with sudo"
- [x] Quick preview of text and images
- [x] Settings > SFTP: parallel transfers, the overwrite policy, keeping times and permissions, hidden files, the editor command, following the terminal, confirming deletes

### SCP (spike)
- [x] Download and upload with the SCP protocol over an exec channel against a server without SFTP; findings in `spikes/scp-fallback/`

### Quality
- [x] Unit tests: path handling (remote POSIX paths, local Windows paths), sorting, the overwrite decisions, ETA, resuming offsets, the permissions dialog's conversions (the overwrite decisions and offsets in the in-process tests; the dialog's conversions are not unit-tested: see Deviations)
- [x] In-process server tests: every operation, a recursive transfer both ways, pause and resume, an interrupted transfer resumed, times and permissions kept, a 10,000-entry folder
- [x] Real servers (OpenSSH): a 1 GB transfer both ways with checksums, and an interrupted one resumed (in CI and WSL)
- [x] **Done when:** a 1 GB transfer is stable, resuming works, and a folder with 10,000 entries stays smooth (measured)
- [x] Smoke tests: the SFTP view and the side panel against the in-process server (list, create, rename, upload, download, delete, the queue)
- [x] Screenshots: the SFTP view, the side panel, the transfer queue, the permissions dialog, Settings > SFTP

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [x] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [x] ADR (SFTP), docs, CHANGELOG, report, commits pushed to GitHub

## Report

### What was done

- **The engine, `opensesh-ssh::sftp`** (no Qt, [ADR 0028](../adr/0028-sftp.md)) on `russh-sftp` 3.0.0, over an `sftp` subsystem channel of the SSH connections of Sprint 7.
  - **One interface for both sides** (`Fs`: this computer or a server): list (symlinks resolved, 16 at a time), stat, create a folder or an empty file, rename, delete (recursive), chmod, times, symlinks, read link, free space (`statvfs@openssh.com`), and reading and writing at an offset. Windows drives sit under an empty root.
  - **The transfer queue:** one for every window, with a parallel limit shared by its jobs. Recursive copies between any two sides (server to server through this client), progress with a smoothed speed and an ETA, pause (keeping offsets), resume, cancel, and retry on the panes' new sessions. The overwrite policy (ask, replace, replace if newer, continue, skip, keep both), with "for every file". Continuing a partial file checks its last 64 KiB first. Times kept by default, permissions when asked.
  - **Within one server,** copies run `cp -R -p` there when the destination is free; without a shell they go through the client.
  - **Names from the server** that aren't one plain name (`..`, `/`, and on Windows `\` or `:`) are left out of listings and downloads and aren't opened for editing: a server can't place a file outside the chosen folder.
  - **The in-process test server** (`opensesh_ssh::testing`) serves a temporary folder over SFTP, for the tests and the smoke test.
- **Sessions:** the SSH backend shares its live connection; the side panel opens a channel on it (no second login) and again after each reconnection. The terminal reports the shell's folder (OSC 7) for any host.
- **App:**
  - **`SftpBrowser`** (one per file pane, a list model): path and breadcrumbs, listing, sorting (folders first, natural order), hidden files, errors and busy state, its own connection's questions, operations, a quick look at text and images, and the server's free space.
  - **The file pane:** breadcrumbs or a typed path (Ctrl+L), sortable columns (name, size, modified, permissions, owner), keyboard navigation and multi-selection, menus, and the dialogs for names, permissions (an rwx grid with setuid, setgid and sticky, and octal), properties and quick look.
  - **The SFTP view:** two sides, each this computer, a saved host or `user@host`, with the same prompt cards as a terminal pane; F5/F6 and a button copy or move to the other side; the sides swap. The transfer queue is below.
  - **The side panel's files (Ctrl+Shift+E):** the focused terminal's files, following its folder when "Follow the terminal" is on. A server's shell that doesn't report its folder gets the shell integration offer (the lines to copy, or added to `~/.bashrc`/`~/.zshrc` after confirming).
  - **The transfer queue panel** (the SFTP view, and a status bar button): progress, speed, ETA, pause, resume, cancel, retry and clear, and the question about a file already there.
  - **Drag and drop:** from the file manager (upload), between panes, and this computer's files out to other applications.
  - **Editing a server's file:** a private copy opened with the editor command or the system's application; each save uploaded after the conflict check; "Save with sudo" when the server refuses.
  - **Settings > SFTP** (`[sftp]` in `config.toml`).
- **SCP spike:** [`spikes/scp-fallback`](../../spikes/scp-fallback/README.md), against a new test server without SFTP (port 2225).
- **Fixes found on the way:** test runs no longer write `config.toml` (the guard hosts, profiles and keybindings had was missing for settings); breadcrumbs had no width; a refresh during a navigation went back to the previous folder; some string literals had real line breaks from the tooling.
- **Documentation:** ADR 0028, the threat model, the SSH testing notes, CHANGELOG and README.

### "Done when" (PLAN)

- **A 1 GB transfer is stable: yes.** 1 GiB through the queue to OpenSSH 10.5p1 and back, with the same SHA-256 here, on the server and back: 627 MiB/s up and 692 MiB/s down over the loopback in a release build, 223 and 124 MiB/s in a debug build (as CI runs it). In CI too.
- **Resuming works: yes.** An upload whose connection was closed after 98 to 157 MB of 256 MB was resumed on a new connection from the part that arrived (the first progress already counted it), and the checksum matched. In-process, pause and resume keep offsets on a 48 MiB file, a partial file that matches is continued and one that doesn't is copied again.
- **A folder with 10,000 entries stays smooth: yes (measured).** Listed over SFTP in 285 ms and sorted in 12 ms (engine, in-process server). In the app, with debug builds: listed into the pane in 85 to 127 ms on Linux and 304 to 359 ms on Windows, and sorted again in 34 to 43 ms. The list view is virtualized.

### How it was verified

- **Windows (Qt 6.10.3), on the final code:**
  - fmt, clippy `-D warnings`, lint-qml, i18n, shaders, deny and audit.
  - Every test: 539 passed, 27 ignored (real servers, real agents and keyrings).
  - The main smoke test offscreen (353 steps) and native (359), and the gallery. SFTP steps: see the [manual matrix](../testing/manual-matrix.md).
  - Screenshots (native): the SFTP view with the queue, a file's permissions, the side panel following an SSH tab, and Settings > SFTP, in dark and light, comfortable and compact.
- **Real servers in WSL (Arch):** `real_sftp.rs` (1 GiB, the interrupted upload, a copy within the server with `cp`, no SFTP on 2225) and the four tests of `real_servers.rs`.
- **Shell integration:** installed by `sh` twice (added once), then bash 5.3 and zsh 5.9 report their folder (Arch; zsh installed there for it).
- **WSL, on the final code:** the build, clippy, every test (542 passed, 32 ignored) and the smoke tests (offscreen, gallery, Wayland, X11) in Debian 13 (Qt 6.8.2), Fedora 43 (Qt 6.10.3) and Arch (Qt 6.11.2); `ssh-agent` with a fixture key.
  - The first Arch run tested the previous binary: its build step stopped, and the script cut the message off. Built again it was clean, and the whole run passed; the script now keeps the build log and its exit code.
  - One Arch Wayland run ended when WSLg's compositor broke while Qt read the clipboard ("The Wayland connection broke"); three more runs passed.
  - Arch's GCC 16 prints `-Wsfinae-incomplete` warnings from Qt's own headers while building the C++ of the bridges; they are Qt's, not ours.
- **GitHub Actions:** green on Ubuntu 24.04, Windows, the Arch, Fedora and Debian 13 containers, and the `ssh` job, where `real_servers.rs` and `real_sftp.rs` (4 and 4 tests, the SFTP ones in 36 s in a debug build) pass against the Ubuntu 24.04 OpenSSH.

### Deviations

- **Remote files can't be dragged out** to the file manager: the drop target needs file URLs when the drag starts, and downloading first would stall the drag for large files. "Download to…" and dragging to the local pane do it. Local files drag out.
- **The permissions dialog's conversions** (grid and octal) are JavaScript and not unit-tested; the permission text (`drwxr-xr-x`) is.
- **Drag and drop on Wayland** was not tried by hand (no file manager in the WSL distros); it is in the manual matrix.
- **SCP** stays a spike: servers without SFTP get "This server has no SFTP".

### Pending

- **Manual matrix** ([manual-matrix.md](../testing/manual-matrix.md)): the side panel on the owner's servers, a large transfer over a real network with the cable pulled, drag and drop with Explorer, Nautilus and Dolphin, VS Code as the editor, "Save with sudo" on a real server.
- **Carried over:** the Sprint 6 and 7 manual checks, "Confirm before closing with active sessions", the Windows executable icon, an Ubuntu package.

### Risks

- **`russh-sftp` 3 is new** (2026-09-08): a request without an answer fails after 60 s, which turns a stuck server into "disconnected" rather than a hang.
- **Edited files are in clear** in the cache folder while being edited, and after a crash until removed by hand (the threat model says so).
- **"Save with sudo"** relies on `sudo -n true` to know whether a password is needed; per-command sudo rules could make it guess wrong, and then nothing is written.
- **`DelegateModel::cancel: index out range`:** a Qt warning (not from our QML) seen now and then on Linux when a listing is replaced while the list view still creates rows; harmless, not counted by the smoke test.
- **cxx-qt incremental builds:** adding properties to a QObject once left stale generated C++ that crashed at startup; a clean build of the app fixed it.
- **Unix-only code** isn't linted by clippy on Windows: the WSL runs (or CI) catch it.
