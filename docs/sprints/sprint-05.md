# Sprint 5: Hosts and sessions

**Goal:** the complete Hosts view, at the level of the best clients.

**Started:** 2026-09-26

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it.
- **Connecting before the SSH sprint.** The internal SSH backend arrives in Sprint 7. Until then an SSH host connects through the system OpenSSH client (`ssh`) in a terminal pane, which PLAN §2 lists as the optional alternative backend (`backend = "openssh"`). Its options (user, port, jump hosts, identity file, keepalive, compression, forwarding) become `ssh` arguments. Other protocols can be saved; connecting to them says which sprint brings them.
- **Secrets wait for the vault (Sprint 6).** Hosts can name an identity file; passwords and `vault:` references get their UI with the vault. Nothing secret is written.
- **The CLI is its own crate** (`opensesh-cli`, binary `opensesh`), without Qt. It talks to the running app over a local socket (`interprocess`), and starts the app when none runs.
- **Linked `~/.ssh/config` hosts are read-only.** They show in their own group; "Duplicate" makes an editable copy.

## Checklist

### Model (`opensesh-core`)
- [x] Hosts and nested groups: protocol, address, port, user, identity file, jump hosts, profile, tags, favorite, color, icon, markdown notes, SSH, SFTP and serial options, a partial terminal override; group defaults
- [x] `hosts.toml` with `schema_version`, lenient per-entry loading with warnings, atomic saves, hot reload
- [x] Inheritance: built-in defaults, then the group chain (outermost first), then the host; every resolved field knows where it comes from
- [x] Fuzzy search (`nucleo`) on name, address, user and tags, with sorting and filters; under 16 ms for 1000 hosts
- [x] Recent connections (saved hosts and quick-connect targets)

### Quick connect
- [x] Parser: `user@host:port`, IPv6, `ssh://`, `sftp://`, `telnet://`, `rdp://`, `vnc://`, `-J jump`, `-p`, `-l`, `serial:///dev/ttyUSB0?baud=115200` (and `serial://COM3`)
- [x] Popup with the parsed target, autocomplete against saved hosts, recents; "Connect to…" in the command palette

### Hosts view
- [x] Group column: all hosts, Favorites, Recent, the group tree with counts
- [x] Search bar, filters (protocol, tag), sort, list/cards toggle, "+ Host"
- [x] Cards and rows: OS icon, name, `user@host`, tags, open session indicator
- [x] Multi-selection (click, Ctrl, Shift, keyboard) and dragging hosts onto a group
- [x] Context menu: connect, connect in a split, duplicate, edit, copy the `ssh` command, favorite, move to group, delete
- [x] Group management: new, rename, color, delete, nest
- [x] Empty state with New host, Quick connect, Local terminal, Import

### Host editor
- [x] Sections Basic, Authentication, Advanced, Terminal, SFTP, Notes
- [x] Inline validation; inherited values shown as "inherited from <group>" with a way back to inheriting
- [x] Group editor with the same defaults

### Sessions
- [x] SSH hosts open in a pane through OpenSSH, with the host's and group's terminal profile layers
- [x] Workspaces remember which host a pane was connected to and reconnect on restore

### Import `~/.ssh/config` (`opensesh-import`)
- [x] Own parser: `Host`, `HostName`, `User`, `Port`, `IdentityFile`, `ProxyJump`, `Include` (globs, relative paths), comments, `=` and quotes; wildcard patterns and `Match` skipped with a warning; fixtures
- [x] Two modes: linked read-only (reloaded when the file changes) or an imported copy
- [x] Import dialog with the entries, the warnings and the mode

### CLI and single instance
- [x] `opensesh list`, `opensesh connect <name>`, `opensesh open <url>`
- [x] One running instance: a second start or the CLI hands its request to it over local IPC; the app starts when none runs
- [x] URLs that arrive from outside ask before connecting (PLAN §8)

### Quality
- [x] Unit tests: model, inheritance, search, parser, ssh_config fixtures, IPC messages
- [x] A 1000-host fixture: smooth scrolling and search timing measured (`docs/perf.md`)
- [x] Smoke tests: hosts view with the fixture, editor, quick connect, import, a host connected through a pane
- [x] Screenshots of the Hosts view (cards and list), the editor and quick connect

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [x] Packages ship the `opensesh` CLI
- [x] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [x] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

### What was done

- **Model** (`opensesh-core::hosts`, [ADR 0019](../adr/0019-hosts-groups-and-inheritance.md)): hosts and nested groups with every §4.3 field (protocol, address, port, user, key file, jump hosts, profile, tags, favorite, color, icon, markdown notes, SSH, SFTP and serial options, a partial `[host.terminal]`), group defaults, and linked sources. Loading is lenient per entry, ids are ULIDs, unknown keys are kept, and a file that can't be read or is newer is never overwritten. Every inheritable field resolves from the host, else the nearest group, else a built-in default, and knows where it came from; validation returns field codes. Terminal options join the profile chain (global, groups, host, pane).
- **Search:** one line per host matched by `nucleo-matcher` (several words can match different fields; a name match, the text as typed and an exact name or address rank higher), scopes (all, favorites, recent, a group with its subgroups, no group, linked), protocol and tag filters, and orders with numbers in numeric order. Recent connections in `recent.toml`.
- **Quick connect parser and OpenSSH command line** (`hosts::target`): every form of the checklist, errors worded for the popup, and `ssh` arguments from a host's resolved options with jump hosts resolved to their addresses; anything that `ssh` would read as an option is refused.
- **`~/.ssh/config`** (`opensesh-import`, [ADR 0020](../adr/0020-ssh-config-import.md)): our own parser with OpenSSH's rules for `Host`, `HostName`, `User`, `Port`, `IdentityFile`, `ProxyJump` and `Include` (globs, loops, nesting); linked read-only (watched, with every included file) or copied into a new group.
- **Single instance and CLI** (`opensesh_core::ipc`, `opensesh-cli`, [ADR 0021](../adr/0021-single-instance-and-cli.md)): a JSON line protocol over a Unix socket or a named pipe; a second start or the CLI hands its request to the running app; `opensesh list`, `connect`, `open`, shipped in the packages. Targets from outside ask before connecting.
- **App:** the `Hosts` and `Instance` singletons; the Hosts view (group column with counts, search bar, filters, order, cards or list, multi-selection with mouse and keyboard, drag to groups, host and group menus, empty state, a notice while `hosts.toml` is read-only); the host editor (six sections, inline validation, inherited values with their group) and the group editor; quick connect (Ctrl+Shift+O) with suggestions; "Connect to <host>" in the command palette; the import dialog; SSH hosts in tabs and splits through OpenSSH ([ADR 0022](../adr/0022-openssh-until-the-built-in-client.md)) with the host's terminal levels, a Reconnect banner, and workspaces that remember the host; the open-session dot; `Platform.copyText` and `keyboardModifiers`; `TabColors`.

### "Done when" (PLAN)

- A 1000-host fixture is smooth: yes. The Hosts view keeps one list model updated in place and reuses its cards while scrolling; updating it for a new query takes 7 to 8 ms in a release build ([perf.md](../perf.md)).
- Search under 16 ms: yes. The slowest of ten queries over 1000 hosts takes 0.86 ms in Rust (release; the test checks the budget), and 7 to 8 ms from the query to updated cards in the app.
- The ssh_config import passes its fixtures: yes. Fixtures with includes by glob and inside a block, an include loop, a missing file, first-value-wins across files, wildcard patterns and `Match`, plus unit tests for every keyword and for bad values.

### How it was verified

- fmt, clippy `-D warnings`, lint-qml, i18n, shaders, deny and audit on Windows; clippy and every test (422 on Windows) on Windows (Qt 6.10.3) and in Debian 13 (Qt 6.8.2), Fedora 43 (Qt 6.10.3) and Arch (Qt 6.11.2).
- Main and gallery smoke tests offscreen everywhere, and the main one natively (Windows, Wayland and X11), with the new hosts steps (the fixture, search timing, selection, the menus and editors, quick connect, the palette, a host connected in a tab and a split, a request from another process).
- The single instance and the CLI end to end in portable copies on Windows (named pipe) and Debian (Unix socket, including a stale socket replaced on the next start); a real `ssh` started in a pane both times.
- The Debian package built with `scripts/linux/build.sh` ships `/usr/bin/opensesh`.
- Screenshots of the Hosts view (cards, list), the host editor and quick connect in dark and light, comfortable and compact, with the real renderer.

### Pending

- **Manual matrix** ([manual-matrix.md](../testing/manual-matrix.md)): a real SSH host with a jump host and a key, a real `~/.ssh/config` with includes followed live, dragging hosts and groups, 1000 cards on real hardware, `opensesh open`, bringing the window forward from the launcher.
- **Automatic OS icons** need a connection: `auto` shows the protocol's icon until the built-in client can ask the host (Sprint 7 or 11); an OS icon can be picked in the editor.
- **Passwords and vault identities** arrive with the vault (Sprint 6); **the built-in SSH client** in Sprint 7 (OpenSSH until then).
- Terminal overrides in the host editor cover the profile, font size and themes; other `[host.terminal]` keys are edited in the file.
- "Confirm before closing with active sessions" is still not wired. Carried over: the open low-severity review items of Sprint 2, the Windows executable icon, an Ubuntu package.

### Risks

- Behaviour follows the user's OpenSSH until Sprint 7 (its config, agent, prompts and host key checks).
- On Windows another user could create the instance pipe first and receive the requests of this user's CLI (no secrets travel on it); the pipe keeps the default security descriptor.
- 0BSD is new on the license allowlist (two dependencies of `interprocess`).
- Two power cuts during the sprint: one file was found zero-filled and restored from a copy, and every changed file was checked; local checkpoint commits now come earlier.
