# Sprint 16: importers and sync

**Goal:** move over from other programs in a minute, and sync between computers without a cloud of our own.

**Started:** 2026-10-02

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it. The owner asked on 2026-10-02 to keep going from sprint to sprint without waiting.
- **Real sample files:** PLAN asks to get real MobaXterm files from the owner for the fixtures. The owner is away, so the fixtures are written from the formats' public descriptions (below) and marked as such; the owner's real files are asked for in the report, to be added as fixtures when they come.
- **Importers** (`opensesh-import`, Qt-free), each giving hosts and groups with warnings for what is left out; never a password:
  - **MobaXterm:** `.mxtsessions` exports, single-session `.moba` files and the `[Bookmarks*]` sections of `MobaXterm.ini` (`%APPDATA%\MobaXterm`). The format (INI in Windows-1252, `#`- and `%`-separated fields, `__PIPE__`-style escapes, folders in `SubRep`) is from the public description in `Ruzgfpegk/sessionator` and its `.mxtsessions` notes (MobaXterm 23.6): SSH (type 0), RDP (4), VNC (5) and SFTP (7) are described field by field; other types (Telnet, Serial, Mosh...) aren't, so they are skipped with a warning until real files show their fields.
  - **PuTTY:** the Windows registry (`HKCU\Software\SimonTatham\PuTTY\Sessions`, read with `winreg`), a `.reg` export of it (to move sessions from another computer), and `~/.putty/sessions` or `~/.config/putty/sessions` on Unix. Names and values as PuTTY's `settings.c` and storage code write them (`HostName`, `PortNumber`, `Protocol`, `UserName`, `PublicKeyFile`, `Proxy*`, `Serial*`, ...): SSH, Telnet and serial sessions.
  - **Remmina:** `.remmina` files in `$XDG_DATA_HOME/remmina` (and the old `~/.remmina`), keys as Remmina's source reads them (`server`, `username`, `domain`, `ssh_tunnel_*`, `viewonly`, `disableclipboard`...): RDP, VNC, SSH and SFTP.
  - **CSV:** any delimiter (comma, semicolon, tab), quoted fields; a dialog maps columns to host fields, guessed from the headers.
- **Export:**
  - **An OpenSesh bundle** (`.opensesh`, TOML): groups and hosts, terminal profiles, themes and snippets; optionally the keychain's identities and keys with their secrets, sealed with an export password (Argon2id and XChaCha20-Poly1305, as the vault). Importing one asks for the password only when it has secrets.
  - **`ssh_config`:** SSH hosts as `Host` blocks (`HostName`, `User`, `Port`, `IdentityFile`, `ProxyJump`, `ForwardAgent`, `Compression`).
- **One import dialog** for every source (it replaces the `~/.ssh/config` one, which stays as one of its sources): choose the source and the file or folder, see what comes (with warnings), import into a new group.
- **Sync:**
  - **The settings folder** (`config.toml`, `hosts.toml`, profiles, themes, snippets, tunnels, keybindings, the keychain without its secrets) can live anywhere, e.g. in a Git repository or a Syncthing folder: a pointer in the default location, changed in Settings, used after a restart. Secrets (`vault.bin`), logs, recordings and the window's state stay on each computer.
  - **Merge-friendly files:** stable order, one value per line (checked by tests).
  - **Saving merges:** when a file changed on disk since this instance read it (another computer through Syncthing, another instance), the save merges both by record id (three-way, against what was read) instead of overwriting; a lock file keeps two instances on one computer from writing at once.
  - **Conflicts:** Syncthing's `*.sync-conflict-*` copies and Git's conflict markers are found and shown; a dialog says which hosts (or snippets, tunnels...) differ and lets you keep either side, then writes the result.
  - **A Git helper** in Settings for a folder that is a repository: status, commit, pull and push with the `git` program, only when asked (no network otherwise).

## Checklist

### Importers
- [x] MobaXterm: `.mxtsessions`, `.moba`, `MobaXterm.ini` (SSH, SFTP, RDP, VNC; folders, gateways, keys, proxies, comments)
- [x] PuTTY: the registry on Windows, `.reg` exports, the Unix sessions folder (SSH, Telnet, serial)
- [x] Remmina: `.remmina` files and folders (RDP, VNC, SSH, SFTP; SSH tunnels as jump hosts)
- [x] CSV: parser, column guesses, mapping to hosts
- [x] Fixtures for every importer, from the public format descriptions

### Export
- [x] The OpenSesh bundle: write and read; secrets sealed with an export password
- [x] `ssh_config` export

### The app
- [x] One import dialog for every source, with the CSV mapping and the bundle's password
- [x] Export dialog: bundle (with or without secrets) or `ssh_config`
- [x] Settings: the settings folder (move or use what is there; restart), the Git helper
- [x] Conflicts: found on start and on changes, shown in a dialog, resolved per record

### Sync engine (`opensesh-core`)
- [x] The settings folder pointer, read at start
- [x] Three-way merge of TOML files by record id, and the save that uses it; the cross-instance lock
- [x] Syncthing conflict copies and Git conflict markers parsed into two sides
- [x] Stable output checked by tests

### Quality
- [x] **Done when:** every importer's fixtures pass, and two instances on the same folder sync without corrupting anything (a test with two writers)
- [x] Smoke test and screenshots: the import dialog (each source), the CSV mapping, the export dialog, the sync settings, the conflict dialog

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, deny, audit
- [x] Build and smoke tests on Windows and Linux (CI); GitHub Actions green
- [x] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

**Closed:** 2026-10-03

### What was done

- **Importers** (`opensesh-import`, [ADR 0037](../adr/0037-importers-bundles-and-sync.md)), each giving hosts, their folders as groups and warnings for what was left out, never a password:
  - **MobaXterm:** `.mxtsessions`, `.moba` and `MobaXterm.ini` bookmarks in Windows-1252; SSH, SFTP, RDP and VNC sessions with host, port, user, SSH gateways (as jump hosts), keys, proxies (SOCKS 5, HTTP, a local command), agent forwarding and comments.
  - **PuTTY:** the Windows registry (`winreg`), `regedit` `.reg` exports (UTF-16) and the Unix sessions folder; SSH, Telnet and serial sessions.
  - **Remmina:** `.remmina` profiles (RDP, VNC, SSH, SFTP), SSH tunnels as jump hosts, labels as tags, notes, nested groups, VNC display numbers.
  - **CSV:** our own RFC 4180 parser (delimiter guessed), columns guessed from the headers and mapped in the dialog.
  - PuTTY keys (`.ppk`) are left out with a note pointing at the Keychain, which converts them.
- **OpenSesh bundles:** hosts and groups, snippets, profiles and themes in one TOML file; the keychain optionally sealed with an export password in the vault's own format (`opensesh_vault::transfer`); on import, new host ids with references followed, identities mapped (and kept with their ids when free, so synced hosts find them), keys reused by fingerprint.
- **`ssh_config` export** of every SSH host or the selected ones, with what they resolve to.
- **Sync** (`opensesh_core::sync`):
  - the settings folder anywhere, through a pointer in the default folder, used from the next start; the keychain file stays with this computer's vault;
  - saves merge instead of overwriting when the file changed on disk since it was read: three-way, records by `id`, values by key, under a folder lock;
  - Syncthing conflict copies and Git conflict markers found at start and on changes, resolved in a dialog per host, group or snippet;
  - a Git helper (status, commit, pull, push, init with a `.gitignore`), only when asked.
- **The app:** one import dialog for every source (with the CSV mapping and the bundle's password; `~/.ssh/config` keeps its own dialog), an export dialog, Settings > Data and sync, the conflict dialog, a notification with a "Resolve" action when conflicts appear, actions in the command palette and the hosts' menus.
- **Safety:** the import preview lists any command an import would run on this computer (a ProxyCommand, a local terminal's shell, a bundle profile's shell); the export password needs 8 characters, as the master password.
- **Documentation:** ADR 0037, a user guide (`docs/sync.md`), the threat model, the vault format, CHANGELOG, README and the manual matrix.

### "Done when" (PLAN)

- **The fixtures of every importer pass:** `tests/importers.rs` (MobaXterm export, `MobaXterm.ini` and a `.moba` file; a PuTTY `.reg` export and a sessions folder; Remmina profiles; two CSV files; several sources sharing folders), with the unit tests of each module, on Windows and on the three Linux distributions in CI.
- **Two instances on the same folder sync without corrupting anything:** `tests/two_instances.rs` runs two instances (each with its own background writer and baselines) adding 40 hosts each to one `hosts.toml` at once, saving and reloading as the watcher would; the file always parses and keeps all 80. With the merge taken out, the same test loses hosts (checked once by hand).

### How it was verified

- **Windows (Qt 6.10.3), on the final code:** fmt, clippy `-D warnings`, lint-qml, i18n, `cargo deny`, `cargo audit`; every test (730 in the app's workspace); the smoke tests offscreen with the software renderer (575 steps), native (573) and the gallery; the screenshots (224, 20 new).
- **GitHub Actions:** green on every job of the final code:
  - Ubuntu 24.04, Windows, and the Fedora, Arch and Debian 13 containers: build, clippy, tests (the importers' fixtures, the Git helper on a temporary repository and the two-instance test included) and the smoke tests with the import, export and conflict steps;
  - formatting, lints, licenses and advisories; the `ssh`, `s3`, `rdp` and `vnc` jobs.

### Problems found and fixed

- **A clashing property:** the import dialog's `header` (the CSV's first row) hid T.Dialog's own `header`; renamed.
- **Binding loops** in the new dialogs (a `ColumnLayout` with an explicit implicit width around wrapped text); they use a fixed-width `Column`, like the other dialogs.
- **A tall CSV mapping** pushed the dialog's last row under its buttons; the body scrolls inside the window now.
- **The fixtures' line endings:** Git turned their CRLF into LF; `.gitattributes` keeps their bytes.

### Deviations

- **No real MobaXterm files** (PLAN asks to get them from the owner, who was away): the fixtures were written from the public format notes and PuTTY's and Remmina's source code. MobaXterm sessions other than SSH, SFTP, RDP and VNC (Telnet, Serial, Mosh...) aren't described there and are skipped with a warning. **Owner: please send a real `.mxtsessions` export (passwords removed) with a few sessions of each type, and a PuTTY `.reg` export,** to add as fixtures.
- **PuTTY's port forwardings** aren't imported (a warning says to add them as tunnels).
- **A bundle's profiles and themes** whose file names exist here are kept as they are here.
- **Changing the settings folder takes effect at the next start** (the app doesn't restart itself).
- **Syncthing's ignore file** isn't written by OpenSesh (the folder is the user's); the guide gives the lines.
