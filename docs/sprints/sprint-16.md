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
- [ ] MobaXterm: `.mxtsessions`, `.moba`, `MobaXterm.ini` (SSH, SFTP, RDP, VNC; folders, gateways, keys, proxies, comments)
- [ ] PuTTY: the registry on Windows, `.reg` exports, the Unix sessions folder (SSH, Telnet, serial)
- [ ] Remmina: `.remmina` files and folders (RDP, VNC, SSH, SFTP; SSH tunnels as jump hosts)
- [ ] CSV: parser, column guesses, mapping to hosts
- [ ] Fixtures for every importer, from the public format descriptions

### Export
- [ ] The OpenSesh bundle: write and read; secrets sealed with an export password
- [ ] `ssh_config` export

### The app
- [ ] One import dialog for every source, with the CSV mapping and the bundle's password
- [ ] Export dialog: bundle (with or without secrets) or `ssh_config`
- [ ] Settings: the settings folder (move or use what is there; restart), the Git helper
- [ ] Conflicts: found on start and on changes, shown in a dialog, resolved per record

### Sync engine (`opensesh-core`)
- [ ] The settings folder pointer, read at start
- [ ] Three-way merge of TOML files by record id, and the save that uses it; the cross-instance lock
- [ ] Syncthing conflict copies and Git conflict markers parsed into two sides
- [ ] Stable output checked by tests

### Quality
- [ ] **Done when:** every importer's fixtures pass, and two instances on the same folder sync without corrupting anything (a test with two writers)
- [ ] Smoke test and screenshots: the import dialog (each source), the CSV mapping, the export dialog, the sync settings, the conflict dialog

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, deny, audit
- [ ] Build and smoke tests on Windows and Linux (CI); GitHub Actions green
- [ ] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

(Filled in at the end of the sprint.)
