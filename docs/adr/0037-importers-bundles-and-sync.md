# ADR 0037: Importers, bundles and sync

- **Status:** accepted
- **Date:** 2026-10-02
- **Sprint:** 16

## Context

PLAN Sprint 16 asks to move over from other tools in a minute and to sync without a cloud of our own:
- importers for MobaXterm (`.mxtsessions` and the bookmarks of `MobaXterm.ini`), PuTTY (the Windows registry and `~/.putty/sessions`), Remmina (`.remmina` files) and any CSV file with a column mapping UI;
- export: an OpenSesh bundle (hosts, profiles, themes and snippets, the secrets optionally encrypted with an export password) and `ssh_config`;
- sync: a settings folder that can be a Git repository or a Syncthing folder, merge-friendly files, conflict detection with a UI to resolve conflicts, an optional `git commit`/`pull` helper.

"Done when": every importer's fixtures pass, and two instances against the same folder sync without corrupting anything.

PLAN also asks for real MobaXterm files from the owner for the fixtures. The owner was away; see "Fixtures" below.

## Decision

### The importers (`opensesh-import`)

Every importer gives an `Imported`: hosts, the folders they were in as groups (parents first), and warnings for what was left out. None reads a password. The app adds the hosts under a new group, the folders as groups inside it, and skips hosts saved already (same name, protocol and address) or not valid.

- **MobaXterm:** the format is an INI file in Windows-1252; `[Bookmarks]`, `[Bookmarks_N]` sections are folders (`SubRep`, `Parent\Child`); each other line is a session: `name=<flag>#<icon>#<type>%<fields>#<terminal>#<start>#<comment>#<color>`, with `__PIPE__`-style escapes. Field positions come from the public description in `Ruzgfpegk/sessionator` and its `.mxtsessions` notes (MobaXterm 23.6), which describe SSH (type 0), RDP (4), VNC (5) and SFTP (7) field by field: host, port, user, the SSH gateways (jump hosts), the key, the proxy (SOCKS 5, HTTP, a local command), agent forwarding, the comment. Other types aren't described there; they are skipped with a warning naming the type number.
- **PuTTY:** the registry (`HKCU\Software\SimonTatham\PuTTY\Sessions`, read with `winreg`, already in the lockfile through `embed-resource`), a `.reg` export of it (UTF-16 as `regedit` writes it, to bring sessions from another computer), and the Unix sessions folder (`~/.config/putty/sessions`, else `~/.putty/sessions`). Value names and meanings are PuTTY's `settings.c`; session names are `%XX`-escaped. SSH, Telnet and serial sessions are imported (raw, rlogin and SUPDUP are not).
- **Remmina:** `$XDG_DATA_HOME/remmina` (else the old `~/.remmina`); keys as Remmina's source reads them (`server`, `username`, `domain`, `ssh_tunnel_*`, `viewonly`, `disableclipboard`, `labels`, `notes_text`); groups nest with `/`; for VNC a port under 100 is a display number, as libvncclient takes it. RDP, VNC, SSH and SFTP profiles.
- **CSV:** our own parser (RFC 4180's form: quotes, doubled quotes, line breaks in quotes; comma, semicolon or tab, whichever the first line has most of). Columns are guessed from the headers and mapped in the dialog; the first row can be data.
- **PuTTY keys** (`.ppk`) referenced by sessions are left out with a warning: the Keychain converts them, SSH can't read them as files.
- **Text** is decoded as UTF-16 with a byte order mark, UTF-8, else Windows-1252 (`encoding_rs`, already a dependency).

### Bundles

A bundle is one TOML file (`.opensesh`): `format = "opensesh-bundle"`, `version = 1`, then `[hosts]` (the tables of `hosts.toml`, linked hosts left out), `[snippets]`, `[[file]]` entries for `profiles/*.toml` and `themes/*.toml` (paths checked: one plain file name in one of those two folders), and an optional `[keychain]`.

- **The keychain part** (`opensesh_vault::transfer`) keeps the identities' and keys' public parts readable, as `keychain.toml` does, and seals the secrets in a small file in the vault's own format (`docs/vault-format.md`) whose key is wrapped by the export password (Argon2id with the recommended costs, XChaCha20-Poly1305). The code that protects `vault.bin` protects the export.
- **Importing** asks for the password only when the bundle has a keychain; a wrong one is refused (the vault's `Decrypt`). Keys already here (same fingerprint) are reused; an identity with the same name, user and key is reused; new ones keep the bundle's ids when those are free here, so hosts synced from another computer find them.
- **Hosts get new ids** on import (references between them, jump hosts by id and groups, follow), identities are mapped, and references to identities that aren't here are dropped with a warning. Profiles and themes whose file names are taken here are kept as they are here; snippets are added by id.
- **The bundle is written by the keychain worker** (secrets never pass through the UI), after the background writer flushed what the app holds.

### `ssh_config` export

SSH, SFTP and Mosh hosts become `Host` blocks with what they resolve to (groups included): `HostName`, `User`, `Port`, `IdentityFile`, `ProxyJump` (jump references resolved), `ProxyCommand` (OpenSesh's `%h %p %r` are OpenSSH's), `ForwardAgent`, `ForwardX11`/`ForwardX11Trusted`, `Compression`. Names become patterns without OpenSSH's special characters, made unique. Keychain identities can't be written (their keys stay in the vault): the block says so in a comment. SOCKS and HTTP proxies have no OpenSSH setting and are left out with a warning.

### Sync

- **The settings folder** (`config.toml`, `hosts.toml`, profiles, themes, snippets, tunnels, keybindings, highlight rules, `known_hosts`, trusted certificates, workspaces) can live anywhere: `location.toml` in the default folder names it (`opensesh_core::sync::location`), and `AppPaths` follows it at start when the folder exists (an unplugged drive leaves the default in use). Changing it applies from the next start; the app doesn't restart itself (one running instance, ADR 0021). Moving can copy this computer's settings there first, never over a file that is there.
- **What stays on each computer:** the vault, logs, recordings, the window's state, and `keychain.toml`, which stays in the default folder because its secret references point into this computer's vault (another computer's vault has other ids, and a synced `keychain.toml` would point at secrets that aren't there). Identities and keys travel in a bundle.
- **Merge-friendly files:** the files are already written in a stable order (the user's order of hosts, snippets and tunnels; keys in a fixed order), one value per line, so Git and Syncthing see small diffs.
- **Saving merges instead of overwriting.** Settings files are read through `Baselines`, which remember what each file held when this instance read it. When the background writer is about to replace a file that changed on disk since (another computer through Syncthing, a `git pull`, another instance), it merges the three versions: arrays of tables are merged record by record by their `id` (or `path`), tables key by key, recursively; a record removed on one side and unchanged on the other goes; added records keep their place. Where both sides changed the same value, this instance's version is kept and the conflict is logged. After a merge the baseline stays what this instance wrote, so a second save before it reloads still merges; the file watcher then reloads the merged file. Files this instance never read are written as given.
- **A lock file** (`.opensesh.lock`, empty, in the folder being written) is held, with the standard library's `File::lock`, from reading the file to renaming the new one, so two instances on one computer never interleave. Across computers there is no lock: Syncthing or Git bring the other side, and the next save merges.
- **Conflicts:** Syncthing's `<name>.sync-conflict-<date>-<time>-<device>.<ext>` copies and files with Git's conflict markers (with `diff3`'s base when present) are found at start and whenever a settings file changes. The conflict dialog lists what differs by record (hosts, groups, snippets, tunnels, other settings) and lets the user keep either side of each; records on one side only are kept by default, so nothing is lost. Files that aren't TOML (`known_hosts`) are kept whole from one side. The result is written atomically, and Syncthing's copy removed.
- **The Git helper** (Settings, Data and sync) runs the `git` program in the folder: status, commit (everything), pull (a merge: conflicts become markers, then the dialog), push, and making the folder a repository with a `.gitignore` for OpenSesh's backups, temporary files and lock. Only when the user asks; pull and push reach the network. `git` runs with `GIT_TERMINAL_PROMPT=0`: credentials come from the user's own setup (an SSH agent, a credential helper) or the command fails and says so.

### Fixtures

The MobaXterm, PuTTY and Remmina fixtures were written from the public descriptions above (the format notes for MobaXterm, PuTTY's and Remmina's source code), not exported by the programs; `.gitattributes` keeps their bytes (CRLF, Windows-1252, UTF-16). The owner's real files are asked for in the sprint report, to be added as fixtures and to confirm the field positions (and to add the MobaXterm types that aren't described).

## Options considered

- **The `csv` crate** for CSV: mature, but a dependency for a few dozen lines of parsing; our parser has its own tests.
- **A ZIP bundle** (files as they are): more faithful for profiles and themes, but a binary file that diffs badly and needs a ZIP crate; one TOML file carries the same and stays readable.
- **Bundles sealed with `age`:** a well-known format, but a new dependency and a second crypto path next to the vault's; reusing the vault's format keeps one audited path.
- **Syncing the vault too:** one vault per computer is simpler and safer (a master password per computer, no concurrent writes to an encrypted file); identities travel in bundles.
- **Last writer wins** (no merge): simple, but two computers editing hosts before Syncthing syncs would lose one side silently. The three-way merge by record id is what makes "two instances on one folder" safe.
- **libgit2 (`git2`)** for the helper: no `git` program needed, but a large C dependency with its own TLS and SSH stacks for credentials; the user's `git` already has their credentials.

## Consequences

- Moving from MobaXterm, PuTTY, Remmina or a spreadsheet keeps folders as groups, jump hosts, keys and proxies where the format has them, and says what it left out.
- A bundle moves everything but the per-computer files; with its keychain it is as sensitive as the vault behind a password (see the threat model).
- Settings can be shared through Git or Syncthing; edits on two computers merge by record, and real conflicts are resolved in a dialog rather than by hand in TOML.
- The merge is generic over TOML: a future file with `id`ed records is covered without code.
- The MobaXterm types other than SSH, RDP, VNC and SFTP wait for real files.
