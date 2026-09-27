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
- [ ] `russh-sftp` 3.0.0 pinned; an SFTP session on a new channel of a connection, and a connection of its own
- [ ] The file system interface, remote and local: list (with symlink targets), stat, create a folder or an empty file, rename, delete (recursive), chmod, symlink, read link, disk usage where the server says (`statvfs`)
- [ ] The transfer engine:
  - [ ] upload and download, recursive, with a parallel limit
  - [ ] progress, speed and ETA
  - [ ] pause, resume, cancel and retry
  - [ ] resuming partial files
  - [ ] the overwrite policy
  - [ ] keeping times and permissions
- [ ] Copy within a remote host (`cp -R` over an exec channel when there is a shell, else through the client)
- [ ] The in-process SFTP server for the tests and the smoke test

### Sessions
- [ ] The SSH backend shares its live connection; the side panel opens an `sftp` channel on it, and again after a reconnection
- [ ] The SFTP view connects by itself (host or quick-connect text), with the prompt cards of the terminal pane

### App
- [ ] `SftpBrowser` (one per file pane): path, listing (a model for a virtualized list), sorting, hidden files, errors, busy state
- [ ] File pane component: breadcrumbs and an editable path, a list with name, size, modified, permissions and owner, keyboard navigation, multi-selection, the context menu
- [ ] Operations: new folder, new file, rename, delete (with confirmation), permissions dialog (rwx grid and octal), new symlink, properties, copy, cut and paste
- [ ] Side panel in SSH tabs (Ctrl+Shift+E), following the terminal's folder (OSC 7); the shell integration offer
- [ ] SFTP view: two panes, local or remote each; a host picker; swap panes
- [ ] Drag and drop: from the file manager (upload), to the file manager (download first), between panes
- [ ] Transfer queue panel: jobs with progress, speed and ETA; pause, resume, cancel, retry and clear; the overwrite question
- [ ] Editing remote files: temporary copy, the system editor or a command, upload on save with the conflict check, "save with sudo"
- [ ] Quick preview of text and images
- [ ] Settings > SFTP: parallel transfers, the overwrite policy, keeping times and permissions, hidden files, the editor command, following the terminal, confirming deletes

### SCP (spike)
- [ ] Download and upload with the SCP protocol over an exec channel against a server without SFTP; findings in `spikes/scp-fallback/`

### Quality
- [ ] Unit tests: path handling (remote POSIX paths, local Windows paths), sorting, the overwrite decisions, ETA, resuming offsets, the permissions dialog's conversions
- [ ] In-process server tests: every operation, a recursive transfer both ways, pause and resume, an interrupted transfer resumed, times and permissions kept, a 10,000-entry folder
- [ ] Real servers (OpenSSH): a 1 GB transfer both ways with checksums, and an interrupted one resumed (in CI and WSL)
- [ ] **Done when:** a 1 GB transfer is stable, resuming works, and a folder with 10,000 entries stays smooth (measured)
- [ ] Smoke tests: the SFTP view and the side panel against the in-process server (list, create, rename, upload, download, delete, the queue)
- [ ] Screenshots: the SFTP view, the side panel, the transfer queue, the permissions dialog, Settings > SFTP

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [ ] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [ ] ADR (SFTP), docs, CHANGELOG, report, commits pushed to GitHub

## Report

(Filled in at the end of the sprint.)
