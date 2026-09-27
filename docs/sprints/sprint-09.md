# Sprint 9: Tunnels

**Goal:** a clear port forwarding manager.

**Started:** 2026-09-27

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it.
- **No new crates.** Forwarding uses what `russh` 0.63.3 already has (`direct-tcpip`, `tcpip-forward` and the server's `forwarded-tcpip` channels). The SOCKS5 server for dynamic forwarding is small and written here, next to the SOCKS5 client of Sprint 7.
- **Where the code lives:**
  - `opensesh-core::tunnels`: `tunnels.toml` (PLAN §4.2), with `schema_version`, atomic writes, backups and live reload.
  - `opensesh-ssh::tunnel` (no Qt): the three kinds of forwarding, traffic counters, and the SOCKS5 server.
  - The app: a service that runs the tunnels (their connections and reconnection), a `Tunnels` singleton, and the Tunnels view.
- **"Tied to a host or independent" (PLAN):**
  - **Independent** tunnels open a connection of their own, through a saved host or `user@host` text. They can start with the app and reconnect by themselves, with backoff.
  - **Tied to a host,** a tunnel runs while a terminal session to that saved host is connected, on that session's connection (no second login), like OpenSSH's `LocalForward` in `~/.ssh/config`.
- **Binding outside localhost** (PLAN §8): a tunnel that listens on anything but a loopback address says so in the editor, asks once before saving, and keeps a mark in the list. Remote forwards that ask the server to listen on all its interfaces get the same warning.
- **Questions** (host key, password) of an independent tunnel show in its row and in a card, as in the file panes; tunnels that start with the app and need an answer say so with a notification instead of popping up a dialog.
- **Test runs** never start the user's tunnels, and every connection goes to the in-process test server.

## Checklist

### Engine (`opensesh-ssh::tunnel`)
- [ ] Local forwarding (`-L`): a local listener, a `direct-tcpip` channel per connection
- [ ] Remote forwarding (`-R`): `tcpip-forward` on the server (the port it picked when asked for 0), and its `forwarded-tcpip` channels routed to the local destination; cancelled on stop
- [ ] Dynamic forwarding (`-D`): a SOCKS5 server (no authentication, CONNECT, IPv4, IPv6 and names resolved by the server)
- [ ] Traffic counters: bytes each way, open and total connections
- [ ] Running on a connection that can change: the listener stays while the connection is replaced; remote forwards are asked for again on the new one
- [ ] The in-process test server: `tcpip-forward` and `cancel-tcpip-forward`

### Data
- [ ] `tunnels.toml`: kind, name, the host (a saved host or `user@host` text), addresses and ports, independent or tied to the host, start with the app, reconnect; validation with warnings, unknown keys kept, a newer file never overwritten
- [ ] `~/.ssh/config`: `LocalForward`, `RemoteForward` and `DynamicForward` read (with `[bind:]port`, IPv6 in brackets); what can't be used (Unix sockets, remote dynamic forwards) skipped with a warning

### App
- [ ] The tunnel service: independent tunnels on their own connection with reconnection and backoff; tied tunnels following their host's terminal sessions; start with the app; stop everything at exit
- [ ] `Tunnels` singleton: the list with state and counters, start, stop, add, edit, duplicate, delete, import; questions and their answers
- [ ] Tunnels view: a list with state and a switch per tunnel, the route in words, counters, the warning mark, a menu (edit, duplicate, copy the address, delete), an empty state
- [ ] Tunnel editor: kind, name, host picker or text, addresses and ports (checked as you type), independent or tied, start with the app, reconnect; the warning for binding outside localhost
- [ ] Import from `~/.ssh/config`: the forwards found, per host, to pick
- [ ] Command palette: start and stop each tunnel; the status bar shows how many run

### Quality
- [ ] Unit tests: parsing and validating `tunnels.toml`, the ssh_config forward syntax, loopback detection, the SOCKS5 request parser
- [ ] In-process server tests: each kind with real traffic, counters, a connection replaced under a running tunnel, stop releasing the port
- [ ] **Done when:** `curl` through each kind of tunnel, and automatic reconnection works (in-process, and against OpenSSH in CI and WSL: its session killed, the tunnel back without help)
- [ ] Smoke tests: a tunnel of each kind against the in-process server, from the Tunnels view's API; the editor and the import dialog open
- [ ] Screenshots: the Tunnels view (running, stopped, reconnecting, a warning), the editor, the import dialog

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [ ] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [ ] ADR (tunnels), docs, CHANGELOG, report, commits pushed to GitHub

## Report

(Filled in at the end of the sprint.)
