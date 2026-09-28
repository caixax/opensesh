# Sprint 11: Remote monitor and host info

**Goal:** a live status bar, without installing anything on the server.

**Started:** 2026-09-28
**Finished:** 2026-09-28

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it.
- **No new crates** unless one is needed.
- **Where the code lives:**
  - `opensesh-ssh::monitor` (no Qt): the command, the parser and the runner. The command is a POSIX `sh` loop on its own exec channel of the pane's connection, like OS detection (Sprint 7), never in the user's shell.
  - The app: a monitor per SSH pane while its connection is up, its readings in the status bar, and the side panel's Info tab.
- **What is read, every N seconds (3 s by default):**
  - `/proc/stat` (CPU), `/proc/meminfo` (memory and swap), `/proc/net/dev` (network);
  - `df -P` (disks);
  - `/proc/uptime` or `uptime`, and the load average;
  - `who` (users).
- **Where it works:**
  - Linux first, busybox included.
  - FreeBSD and macOS as far as their tools allow (`sysctl`, `vm_stat`, `netstat`, `df`, `uptime`, `who`).
  - Anything else (a Windows server, a router's CLI) fails cleanly: no monitor, and the Info tab says why.
- **Rates are computed here,** from two readings: CPU use, and network bytes per second.
- **Settings:**
  - On by default.
  - Off globally in Settings > SSH, or per host and group in the editors.
  - Which metrics the status bar shows (CPU, memory, network, disk, uptime, load, users), and the interval, in Settings > SSH.
- **The host info** (side panel, Info tab): OS, kernel, architecture, host name, IP addresses, disks, logged-in users, uptime and load, with "Copy as text". Read once when the tab opens (and on request), on the same kind of exec channel.
- **Only the built-in SSH client** has a connection to use. For OpenSSH panes and local shells, the Info tab says so.
- **Privacy:** the monitor only runs on connections the user opened, and nothing is sent anywhere but that server; the readings stay in memory.

## Checklist

### Engine (`opensesh-ssh::monitor`)
- [x] The command: one POSIX `sh` loop, the sections marked, Linux (busybox too), FreeBSD and macOS branches, the interval given
- [x] The parser: CPU counters, memory and swap, network bytes per interface (loopback left out), disks, uptime, load, users; tolerant of missing sections
- [x] Rates from two readings (CPU %, bytes per second each way), and a reading's summary for the status bar
- [x] The runner: its own exec channel, a reading per loop, stops with the connection or when asked; "unsupported" when nothing useful comes back
- [x] The host info command and its parser (OS name, kernel, architecture, host name, IPs, disks, users, uptime)
- [x] Tests with real outputs: Debian, Alpine (busybox), Fedora, FreeBSD, macOS, and a Windows server
- [x] The in-process test server answers the monitor and info commands (canned readings)

### App
- [x] Settings: `[ssh] monitor`, the interval and the metrics (Settings > SSH); `ssh.monitor` per host and group in the editors
- [x] A monitor per SSH pane while its connection is up (and again after a reconnection); stopped when the pane closes
- [x] The status bar: the focused pane's readings (CPU, memory, network, disk, uptime...), as chosen; a tooltip with the details
- [x] The side panel's Info tab: the host info and live values, "Copy as text", refresh, and clear messages for unsupported servers, OpenSSH panes and local shells

### Quality
- [x] **Done when:** the CPU overhead is negligible on both ends (measured: the loop on the server, the parsing here, in `docs/perf.md`), it works on busybox, and it fails cleanly on unsupported systems
- [x] Busybox: the command run by `busybox sh` with busybox's applets, parsed
- [x] Real servers: the monitor and the info against OpenSSH in WSL (Arch) and in CI
- [x] Smoke test: an SSH pane's readings reach the status bar and the Info tab (against the in-process server)
- [x] Screenshots: the status bar with readings, the Info tab, Settings > SSH

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [x] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [x] ADR (the remote monitor), docs, CHANGELOG, report, commits pushed to GitHub

## Report

### What was done

- **The engine, `opensesh-ssh::monitor`** (no Qt, [ADR 0031](../adr/0031-remote-monitor.md)):
  - **The command:** one POSIX `sh` loop on its own exec channel prints a reading every few seconds.
    - **Linux:** `/proc` (CPU, memory, network, the default route, uptime, load), read through the shell's own `read`.
    - **FreeBSD and macOS:** `sysctl`, `route -n get default`, `netstat -ibn`, `vm_stat` and `ps`.
    - **Then** `df -Pk` and `who`.
    - It is one line without single quotes or `!`, so any login shell passes it to `sh -c` unchanged, and it ends by itself when its channel closes.
  - **The parser:**
    - CPU counters (or macOS's percentage), memory and swap, the default route's interface's traffic, disks (pseudo file systems and bind mounts left out, `/` always kept), uptime, load and users.
    - Tested on real outputs of Debian, Arch and busybox, and on FreeBSD and macOS outputs written from their documented formats.
  - **Rates** (CPU %, bytes per second) come from two readings; counters that go back give none.
  - **The host info:** a one-shot command adds the OS name (`/etc/os-release` or `sw_vers`), kernel, architecture, host name, CPUs and addresses (`ip -o addr` or `ifconfig`, loopback and link-local left out).
  - **Unsupported servers** (no POSIX `sh`, nothing readable, no answer in 20 s) are reported once, with the server's own error line.
  - **The backend** watches the server while each connection's session lasts, when the session asks for it.
  - **The in-process test server** answers both commands with a made-up Debian server whose counters grow, or refuses them as a server without `sh` would.
- **Settings:**
  - `[ssh] monitor` (on by default), `monitor_interval_secs` (1 to 60, 3 by default) and `monitor_metrics` (validated) in `config.toml`.
  - `ssh.monitor` per host and group, inherited like `ssh.detect_os`.
  - Settings > SSH has a Remote monitor group, and the host and group editors a Remote monitor row.
- **App:**
  - **The status bar** shows the focused pane's readings as chosen (CPU, RAM, network, the disk of `/`, uptime, load, users), with every detail in a tooltip; a click opens the Info tab.
  - **The side panel's Info tab** reads the host info when it shows a connected pane (again after a reconnection; Refresh reads again). It shows:
    - the system, kernel, architecture, CPUs, uptime and load;
    - live CPU, memory, swap and network bars while the monitor runs;
    - disks with their use, addresses and logged-in users;
    - "Copy as text".

    Local terminals, OpenSSH panes, a dropped connection and a server that can't be read each say why there is nothing.
- **Measured** ([perf.md](../perf.md#remote-monitor-sprint-11-2026-09-28)):
  - **On the server,** a reading costs 2.1 ms of CPU with dash and 1.7 ms with busybox, about 0.07 % of one core at the default interval.
  - **Here,** a parse takes 14 µs.
- **Documentation:** ADR 0031, the threat model (the monitor's command and its readings), the performance page, the SSH testing notes, CHANGELOG and README.

### "Done when" (PLAN)

- **Negligible CPU overhead on both ends: yes.** Server and client costs are as measured above.
- **Works on busybox: yes:**
  - The real command run by busybox 1.30.1's `sh` with only its applets gives full readings (a test on Linux, run in CI's Ubuntu job with busybox installed).
  - The captured busybox output is part of the parser's fixtures.
- **Fails cleanly on unsupported systems: yes.** A server that answers "command not found" (as Windows' OpenSSH or a router would) is reported once as unsupported, with its error line, and never retried on that connection. The status bar then shows nothing, and the Info tab says why.

### How it was verified

- **Windows (Qt 6.10.3), on the final code:**
  - fmt, clippy `-D warnings`, lint-qml, i18n, shaders, deny and audit.
  - Every test: 607 passed, 29 ignored (real servers, real agents and keyrings; the Unix-only monitor tests don't run here).
  - The smoke tests: offscreen (457 steps), native (449 steps) and the gallery. The new step: an SSH pane's monitor readings reach the status bar (CPU needs two readings), and the Info tab reads the host over the same connection.
  - Screenshots (native): the Info tab next to a real SSH tab on the test server, with the status bar's readings, and Settings > SSH, in dark and light, comfortable and compact.
- **WSL, on the final code** (one distro at a time, 3 build jobs):
  - Debian 13 (Qt 6.8.2), Fedora 43 (Qt 6.10.3) and Arch (Qt 6.11.2).
  - The build, clippy (with the Linux-only code), every test (611 passed, 36 ignored), and the smoke tests (offscreen, gallery, Wayland, X11), with `ssh-agent` holding a fixture key.
- **Real servers in WSL (Arch):** the 5 tests of `real_servers.rs`, the monitor and the host info on OpenSSH 10.5p1 and Dropbear 2026.94 among them.
- **Busybox:**
  - The real command with busybox 1.30.1's `sh` and applets in WSL (the measurement).
  - CI's Ubuntu job runs the same check with busybox 1.36.1, and now fails if busybox isn't found.
- **GitHub Actions:** green on every job:
  - Ubuntu 24.04 (with busybox), Windows, and the Fedora, Arch and Debian 13 containers;
  - the `ssh` job against OpenSSH and Dropbear;
  - formatting, lints, licenses and advisories.

### Deviations

- **FreeBSD and macOS** support rests on their tools' documented output formats, not on captures from real machines (none was at hand); the fixtures say so, and should be replaced by real captures.
- **Changing the interval or turning the monitor on or off** takes effect on the next connection, not on sessions already open.
- **English plurals show "(s)"** ("12 day(s)", "1 user(s)"), as elsewhere in the app, until an English translation with plural forms is added.

### Pending

- **Manual matrix** ([manual-matrix.md](../testing/manual-matrix.md)):
  - the monitor on the owner's own servers (a VPS, a Raspberry Pi with busybox or Alpine);
  - a FreeBSD or macOS server, if one is at hand;
  - a Windows server (the "unsupported" message);
  - the status bar and the Info tab during a reconnection.
- **Carried over:** the Sprint 6 to 10 manual checks, an Ubuntu package.

### Risks

- **One more exec channel per monitored session:** a server with a low `MaxSessions` has one fewer for SFTP and tunnels.
- **`df` can be slow** on servers with network mounts: a reading waits for it (the interval only starts after it).
