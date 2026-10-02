# Sprint 12: More protocols and shells

**Goal:** cover the rest of the terminal sessions, and S3 storage in the file views.

**Started:** 2026-10-02

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it.
- **Where the code lives:**
  - `opensesh-term::shells`: the local shells this computer has (Qt-free).
  - `opensesh-proto-misc` (new, as PLAN §3.1 names it, Qt-free): telnet, serial, mosh and the container listings, each a terminal backend or what starts one.
  - `opensesh-s3` (new, Qt-free): the S3 client, and an S3 file system for the file views.
  - The app: new tabs with a chosen shell, the sessions of every new kind, S3 sources in the file views, the host editor.
- **New crates:**
  - `serialport` 4.10.1 without its default features: no `libudev` to build against; ports are found in `/sys/class/tty` on Linux.
  - `aws-sdk-s3` 1.122.0: the newest that builds with Rust 1.89 (1.123 needs 1.91). Its TLS is rustls on `ring`, not `aws-lc`, which needs CMake and NASM on Windows. Without `aws-config`: credentials come from the vault, never from `~/.aws` or the environment.
- **Local shells:**
  - **Linux and macOS:** `$SHELL` and `/etc/shells`.
  - **Windows:** PowerShell 7, Windows PowerShell, cmd, Git Bash, MSYS2, Cygwin, and the WSL distros (`wsl.exe -l -q`, whose output is UTF-16).
  - **Where:** a menu on the new tab button, the command palette, a default shell in Settings > Terminal (a terminal setting, so a profile or a local host can choose its own).
- **Telnet:** a small NVT of our own (option negotiation with NAWS and TTYPE, ADR). A warning says the protocol sends everything in clear.
- **Serial:**
  - **Settings:** the port, speed, data bits, parity, stop bits, flow control, what Enter sends, and local echo.
  - **Ports:** the port list refreshes while the editor is open.
  - **In the pane:** a hexadecimal view can be turned on and off; the session log works as for SSH.
- **Mosh:**
  1. The built-in SSH client runs `mosh-server new` on its own channel and reads `MOSH CONNECT <port> <key>`.
  2. `mosh-client` runs here in a PTY with `MOSH_KEY`.
  3. A missing `mosh-server` or `mosh-client` is explained, with how to install it.
- **Containers:** `docker` or `podman exec` and `kubectl exec` run in a PTY. Running containers and pods are listed through the same tools (their JSON output).
- **S3 (the owner's request):**
  - A new connection kind for AWS and compatible servers (MinIO, RustFS…), with an endpoint, a region (`us-east-1` by default) and path-style addressing (on by default).
  - The access key and the secret key are a keychain identity (user and password): the secret key lives in the vault, never in clear.
  - **In the file views** (the SFTP view's panes and the transfer queue): buckets, folders with `ListObjectsV2` and the `/` delimiter, upload (multipart in 32 MB parts), download, delete, rename (copy and delete), new folder (an empty object ending in `/`), and temporary links (presigned URLs, with how long they last).
  - **Tests:** a small in-process S3 server (as for SSH: tests and the smoke test never reach the network), and a real RustFS server for the 1 GiB test.

## Checklist

### Local shells
- [ ] `opensesh-term::shells`: discovery on Linux, macOS and Windows (WSL's UTF-16 list), a command line splitter
- [ ] A `shell` terminal setting (Settings > Terminal, profiles, local hosts)
- [ ] The new tab button's menu and command palette entries for each shell; the pane remembers its shell in workspaces

### Telnet
- [ ] The NVT: IAC handling, option negotiation (ECHO, SGA, NAWS, TTYPE, BINARY), window size changes, tests
- [ ] A terminal backend over TCP, with connecting and the reason it ended in the pane
- [ ] Saved hosts and `telnet://` quick connect open it; the insecure-protocol warning

### Serial
- [ ] The backend: open, settings, read and write, Enter as CR, LF or CR LF, local echo, a port that goes away
- [ ] Port listing (with descriptions) and its refresh in the editor
- [ ] Saved hosts and `serial://` open it; the pane's hexadecimal view and the session log

### Mosh
- [ ] `mosh-server new` over the built-in client, parsing `MOSH CONNECT`, its errors
- [ ] `mosh-client` found here and run in a PTY with `MOSH_KEY`; clear messages when either side is missing

### Containers
- [ ] `docker`/`podman exec -it` and `kubectl exec -it` (namespace, container, context), with a shell that falls back to `sh`
- [ ] Listing running containers and pods for the host editor and quick connect

### S3
- [ ] `opensesh-s3`: the client (endpoint, region, path-style, credentials), buckets and listings, read and write (multipart), delete, copy, new folder, presigned links
- [ ] `Fs::S3` in the file views and the transfer queue (no resume into S3: a part-written object starts over)
- [ ] The S3 host kind: editor fields, credentials saved as a keychain identity, quick connect `s3://`
- [ ] The SFTP view's sources and menus for S3 (no permissions or links; "Copy a temporary link")
- [ ] The in-process S3 test server; a real RustFS server for the 1 GiB multipart test (script, WSL and CI)

### Quality
- [ ] **Done when:** every kind has tests or a passed manual check in `manual-matrix.md`, and the S3 view uploads and downloads a 1 GiB multipart file with the same SHA-256 against a local S3 server
- [ ] Tests: telnet against a loopback server, serial parsing and line endings, mosh's output parsing, container listings, the shell list (WSL's UTF-16), S3 against the in-process server
- [ ] Smoke test: a shell from the new tab menu, telnet to a loopback server, an S3 source in the SFTP view with an upload and a download
- [ ] Screenshots: the new tab menu, a telnet pane with its warning, a serial pane in hex, the S3 view, the host editor for serial and S3

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [ ] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [ ] ADRs (telnet and the other terminal protocols; S3), docs, CHANGELOG, report, commits pushed to GitHub

## Report

(Filled in at the end of the sprint.)
