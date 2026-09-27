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
- [x] Local forwarding (`-L`): a local listener, a `direct-tcpip` channel per connection
- [x] Remote forwarding (`-R`): `tcpip-forward` on the server (the port it picked when asked for 0), and its `forwarded-tcpip` channels routed to the local destination; cancelled on stop
- [x] Dynamic forwarding (`-D`): a SOCKS5 server (no authentication, CONNECT, IPv4, IPv6 and names resolved by the server)
- [x] Traffic counters: bytes each way, open and total connections
- [x] Running on a connection that can change: the listener stays while the connection is replaced; remote forwards are asked for again on the new one
- [x] The in-process test server: `tcpip-forward` and `cancel-tcpip-forward`

### Data
- [x] `tunnels.toml`: kind, name, the host (a saved host or `user@host` text), addresses and ports, independent or tied to the host, start with the app, reconnect; validation with warnings, unknown keys kept, a newer file never overwritten
- [x] `~/.ssh/config`: `LocalForward`, `RemoteForward` and `DynamicForward` read (with `[bind:]port`, IPv6 in brackets); what can't be used (Unix sockets, remote dynamic forwards) skipped with a warning

### App
- [x] The tunnel service: independent tunnels on their own connection with reconnection and backoff; tied tunnels following their host's terminal sessions; start with the app; stop everything at exit
- [x] `Tunnels` singleton: the list with state and counters, start, stop, add, edit, duplicate, delete, import; questions and their answers
- [x] Tunnels view: a list with state and a switch per tunnel, the route in words, counters, the warning mark, a menu (edit, duplicate, copy the address, delete), an empty state
- [x] Tunnel editor: kind, name, host picker or text, addresses and ports (checked as you type), independent or tied, start with the app, reconnect; the warning for binding outside localhost
- [x] Import from `~/.ssh/config`: the forwards found, per host, to pick
- [x] Command palette: start and stop each tunnel; the status bar shows how many run

### Quality
- [x] Unit tests: parsing and validating `tunnels.toml`, the ssh_config forward syntax, loopback detection, the SOCKS5 request parser
- [x] In-process server tests: each kind with real traffic, counters, a connection replaced under a running tunnel, stop releasing the port
- [x] **Done when:** `curl` through each kind of tunnel, and automatic reconnection works (in-process, and against OpenSSH in CI and WSL: its session killed, the tunnel back without help)
- [x] Smoke tests: a tunnel of each kind against the in-process server, from the Tunnels view's API; the editor and the import dialog open
- [x] Screenshots: the Tunnels view (running, stopped, reconnecting, a warning), the editor, the import dialog (waiting and failed instead of reconnecting: see Deviations)

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [x] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [x] ADR (tunnels), docs, CHANGELOG, report, commits pushed to GitHub

## Report

### What was done

- **The engine, `opensesh-ssh::tunnel`** (no Qt, [ADR 0029](../adr/0029-tunnels.md)), on what `russh` 0.63.3 already has:
  - **Local** forwarding: a listener here and a `direct-tcpip` channel per connection.
  - **Remote** forwarding: `tcpip-forward` on the server (the port it picked when asked for 0) and its `forwarded-tcpip` channels, which the client accepts only for ports it asked for; cancelled on stop.
  - **Dynamic** forwarding: a SOCKS5 server (no authentication, `CONNECT`, IPv4, IPv6 and names resolved by the server).
  - **Traffic counters:** bytes each way, open and total connections.
  - **A connection that can change:** a forward runs on a `watch` of it; local listeners stay bound while it is replaced (a connection waits for the new one up to 10 s) and remote forwards are asked for again.
  - **An independent tunnel's own connection,** kept up with backoff (1, 2, 4, 8, 16, then every 30 s); errors that need the user end it.
  - **The in-process test server** also serves `tcpip-forward` and its cancellation.
- **Data:** `tunnels.toml` (`opensesh-core::tunnels`): kind, name, a saved host or `user@host`, addresses and ports, tied or independent, start with the app, reconnect; validation with warnings, unknown keys kept, a newer file never overwritten. The `~/.ssh/config` reader takes `LocalForward`, `RemoteForward` and `DynamicForward` (`[bind:]port`, IPv6 in brackets) and skips Unix sockets and remote dynamic forwards with a warning.
- **App:**
  - **The tunnel service** (`crate::tunnels`): independent tunnels on their own connection; tunnels tied to a saved host run while a terminal session to it is connected, on that session's connection (the terminal registry reports sessions going up and down); autostart; everything stops at exit. A tunnel that can't listen, or whose remote forward the server refuses, stops its connection and says why.
  - **`Tunnels` singleton:** the list with state, port, counters and a waiting question; start, stop, save, check, duplicate, delete, answer, import. `tunnels.toml` is saved through the background writer and reloaded when it changes on disk.
  - **Tunnels view:** a switch per tunnel, its route in words, its state (running, connecting, waiting for a session, retrying, failed) with its traffic, a mark when it listens beyond localhost, an Answer button when it asks something, and a menu (start or stop, edit, duplicate, copy the address, delete). Keys: Up/Down, Space, Enter, Delete, Ctrl+N. The rows stay while their counters change.
  - **Tunnel editor:** kind, name, the host (a saved SSH host or `user@host`), independent or with the host's sessions, where it listens and where connections go (checked as you type), start with OpenSesh, reconnect; saving a tunnel that listens beyond localhost asks first.
  - **Import from `~/.ssh/config`:** the forwards found, by host; those of hosts not in OpenSesh can't be picked.
  - **The questions** of a tunnel's own connection show with the terminal pane's cards in a dialog; a tunnel that asks out of sight says so with a notification.
  - **Command palette:** "Start tunnel …" and "Stop tunnel …". **Status bar:** how many tunnels run.
- **Documentation:** ADR 0029, the threat model (tunnels and remote forwards), the SSH testing notes, CHANGELOG and README.

### "Done when" (PLAN)

- **`curl` through each kind of tunnel: yes.** In-process (our own HTTP and SOCKS5 clients, and `curl` where it is installed, as on Windows and in CI), and against OpenSSH 10.5p1: a local forward, a SOCKS5 proxy with the name resolved by the server (`--socks5-hostname`), and a remote forward with sshd listening on its loopback. In the app, the smoke test fetches a page through a local and a remote tunnel with `XMLHttpRequest`.
- **Automatic reconnection works: yes.** Against OpenSSH, the tunnel's `sshd-session` is killed (`kill -9 $PPID` on an exec channel): the connection is back after 1 s, the local port never went away, and the remote forward is asked for again on the new connection; `curl` works through both. In-process, the same with the connection closed from this side, and a wrong password ends without retrying.

### How it was verified

- **Windows (Qt 6.10.3), on the final code:**
  - fmt, clippy `-D warnings`, lint-qml, i18n, shaders, deny and audit.
  - Every test: 556 passed, 29 ignored (real servers, real agents and keyrings).
  - The main smoke test offscreen (399 steps) and native (407), and the gallery. Tunnel steps: a tunnel of each kind on its own connection (its questions answered in its row and in the question dialog), HTTP through the local and remote ones, the counters, a row kept while they change, a tunnel tied to a host that runs with a terminal session and waits when it closes, the editor (a problem found, `0.0.0.0` seen as exposed), the import dialog, duplicate and delete.
  - Screenshots (native): the Tunnels view (running with traffic, waiting for a session, stopped with the warning, failed), the editor of a tunnel that listens on every interface, and the import dialog, in dark and light, comfortable and compact.
- **Real servers in WSL (Arch):** `real_tunnels.rs` (2 tests) with the 8 tests of `real_servers.rs` and `real_sftp.rs`.
- **WSL, on the final code:** the build, clippy, every test (559 passed, 34 ignored) and the smoke tests (offscreen, gallery, Wayland, X11) in Debian 13 (Qt 6.8.2), Fedora 43 (Qt 6.10.3) and Arch (Qt 6.11.2); `ssh-agent` with a fixture key. The build script now reports its exit code and keeps its log (last sprint's stale Arch binary).
- **GitHub Actions:** running on the pushed commits (updated below once it finishes).

### Deviations

- **No "reconnecting" screenshot:** every connection of a test run goes to the in-process server, which doesn't drop by itself; the state is covered by the tests and the smoke test's texts. The screenshots show running, waiting, stopped and failed.
- **Not supported yet:** forwarding Unix sockets (`streamlocal`) and remote dynamic forwards (`RemoteForward port`); the importer says so.
- **Tied tunnels listen only while a session is up,** as OpenSSH's forwards do; a port that must always be there is an independent tunnel.

### Pending

- **Manual matrix** ([manual-matrix.md](../testing/manual-matrix.md)): a database through a local tunnel to one of the owner's servers, a browser through the SOCKS proxy, a remote tunnel with `GatewayPorts`, Wi-Fi off and on with a tunnel running, a tunnel from `~/.ssh/config` with a real session.
- **Carried over:** the Sprint 6, 7 and 8 manual checks, "Confirm before closing with active sessions", the Windows executable icon, an Ubuntu package.

### Risks

- **A SOCKS proxy has no authentication** (like `ssh -D`): it listens on the loopback by default, and anything else is marked and confirmed.
- **Remote forwards and `GatewayPorts`:** what a remote tunnel exposes depends on the server's configuration, which OpenSesh can't see; the warning says "if the server allows it".
- **One connection per independent tunnel:** several tunnels through the same host log in once each; a later sprint could share a connection between them.
- **Unix-only code** isn't linted by clippy on Windows: the WSL runs (or CI) catch it.
