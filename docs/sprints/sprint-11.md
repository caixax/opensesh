# Sprint 11: Remote monitor and host info

**Goal:** a live status bar, without installing anything on the server.

**Started:** 2026-09-28

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
- [ ] The command: one POSIX `sh` loop, the sections marked, Linux (busybox too), FreeBSD and macOS branches, the interval given
- [ ] The parser: CPU counters, memory and swap, network bytes per interface (loopback left out), disks, uptime, load, users; tolerant of missing sections
- [ ] Rates from two readings (CPU %, bytes per second each way), and a reading's summary for the status bar
- [ ] The runner: its own exec channel, a reading per loop, stops with the connection or when asked; "unsupported" when nothing useful comes back
- [ ] The host info command and its parser (OS name, kernel, architecture, host name, IPs, disks, users, uptime)
- [ ] Tests with real outputs: Debian, Alpine (busybox), Fedora, FreeBSD, macOS, and a Windows server
- [ ] The in-process test server answers the monitor and info commands (canned readings)

### App
- [ ] Settings: `[ssh] monitor`, the interval and the metrics (Settings > SSH); `ssh.monitor` per host and group in the editors
- [ ] A monitor per SSH pane while its connection is up (and again after a reconnection); stopped when the pane closes
- [ ] The status bar: the focused pane's readings (CPU, memory, network, disk, uptime...), as chosen; a tooltip with the details
- [ ] The side panel's Info tab: the host info and live values, "Copy as text", refresh, and clear messages for unsupported servers, OpenSSH panes and local shells

### Quality
- [ ] **Done when:** the CPU overhead is negligible on both ends (measured: the loop on the server, the parsing here, in `docs/perf.md`), it works on busybox, and it fails cleanly on unsupported systems
- [ ] Busybox: the command run by `busybox sh` with busybox's applets, parsed
- [ ] Real servers: the monitor and the info against OpenSSH in WSL (Arch) and in CI
- [ ] Smoke test: an SSH pane's readings reach the status bar and the Info tab (against the in-process server)
- [ ] Screenshots: the status bar with readings, the Info tab, Settings > SSH

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [ ] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [ ] ADR (the remote monitor), docs, CHANGELOG, report, commits pushed to GitHub

## Report

(Filled in at the end of the sprint.)
