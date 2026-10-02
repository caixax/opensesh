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
- [x] `opensesh-term::shells`: discovery on Linux, macOS and Windows (WSL's UTF-16 list), a command line splitter
- [x] A `shell` terminal setting (Settings > Terminal, profiles, local hosts)
- [x] The new tab button's menu and command palette entries for each shell; the pane remembers its shell in workspaces

### Telnet
- [x] The NVT: IAC handling, option negotiation (ECHO, SGA, NAWS, TTYPE, BINARY), window size changes, tests
- [x] A terminal backend over TCP, with connecting and the reason it ended in the pane
- [x] Saved hosts and `telnet://` quick connect open it; the insecure-protocol warning

### Serial
- [x] The backend: open, settings, read and write, Enter as CR, LF or CR LF, local echo, a port that goes away
- [x] Port listing (with descriptions) and its refresh in the editor
- [x] Saved hosts and `serial://` open it; the pane's hexadecimal view and the session log

### Mosh
- [x] `mosh-server new` over the built-in client, parsing `MOSH CONNECT`, its errors
- [x] `mosh-client` found here and run in a PTY with `MOSH_KEY`; clear messages when either side is missing

### Containers
- [x] `docker`/`podman exec -it` and `kubectl exec -it` (namespace, container, context), with a shell that falls back to `sh`
- [x] Listing running containers and pods for the host editor and quick connect

### S3
- [x] `opensesh-s3`: the client (endpoint, region, path-style, credentials), buckets and listings, read and write (multipart), delete, copy, new folder, presigned links
- [x] `Fs::S3` in the file views and the transfer queue (no resume into S3: a part-written object starts over)
- [x] The S3 host kind: editor fields, credentials saved as a keychain identity, quick connect `s3://`
- [x] The SFTP view's sources and menus for S3 (no permissions or links; "Copy a temporary link")
- [x] The in-process S3 test server; a real RustFS server for the 1 GiB multipart test (script, WSL and CI)

### Quality
- [x] **Done when:** every kind has tests or a passed manual check in `manual-matrix.md`, and the S3 view uploads and downloads a 1 GiB multipart file with the same SHA-256 against a local S3 server
- [x] Tests: telnet against a loopback server, serial parsing and line endings, mosh's output parsing, container listings, the shell list (WSL's UTF-16), S3 against the in-process server
- [x] Smoke test: a shell from the new tab menu, telnet to a loopback server, an S3 source in the SFTP view with an upload and a download
- [x] Screenshots: the new tab menu, a telnet pane with its warning, a serial pane in hex, the S3 view, the host editor for serial and S3

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [x] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [x] ADRs (telnet and the other terminal protocols; S3), docs, CHANGELOG, report, commits pushed to GitHub

## Report

**Closed:** 2026-10-02

### What was done

- **Local shells** (`opensesh-term::shells`, [ADR 0032](../adr/0032-terminal-protocols-and-shells.md)):
  - **Found:**
    - Windows: PowerShell 7, Windows PowerShell, cmd, Git Bash, MSYS2, Cygwin, and the WSL distributions (`wsl.exe -l -q`, UTF-16, `docker-desktop` left out);
    - Linux and macOS: `$SHELL` and `/etc/shells` (no `nologin` or `false`, one entry per real file).
  - **A command line splitter** (`opensesh-core::command_line`) for shells typed by hand.
  - **Where to choose one:**
    - a menu on the new tab button and the command palette;
    - a `shell` terminal setting (Settings > Terminal, profiles, local hosts).
  - A pane keeps its shell in workspaces.
- **The other terminal kinds,** in a new Qt-free crate, `opensesh-proto-misc`:
  - **Telnet:**
    - **Our own NVT:** IAC, ECHO, SGA, BINARY, NAWS (sent again on resize) and TTYPE; Enter as CR LF; `0xFF` doubled; local echo while the server doesn't echo.
    - **Where it opens:** saved hosts and `telnet://`, with a warning in the pane and in the host editor that it sends everything in clear.
  - **Serial ports** (`serialport` 4.10.1 without libudev):
    - **Settings:** speed, data bits, parity, stop bits, flow control, what Enter sends, local echo.
    - **Ports:** listed with their descriptions in the host editor, again every 2 s.
    - **In the pane:** a hexadecimal view and breaks from its menu; the session log works as for SSH; a device that goes away ends the session with the reason.
    - **Where it opens:** saved hosts and `serial://`.
  - **Mosh:**
    - The built-in SSH client connects (the same questions in the pane), starts `mosh-server new` on an exec channel with a PTY, and reads `MOSH CONNECT`.
    - `mosh-client` then runs here in a PTY, with the session key only in `MOSH_KEY`.
    - A missing `mosh-server` or `mosh-client` is explained with how to install it.
  - **Containers and pods:**
    - `docker`/`podman exec -it` and `kubectl exec -it`, with the namespace, container and context, and a shell that falls back from bash to sh.
    - **The running ones** are listed from the tools' JSON output, in the host editor and in quick connect.
    - **Quick connect:** `docker://`, `podman://` and `kube://`.
- **S3** ([ADR 0033](../adr/0033-s3-storage.md)):
  - **`opensesh-s3`:** the client on `aws-sdk-s3` 1.122.0 (rustls on ring, no `aws-config`):
    - buckets and listings with pages, reads with ranges;
    - a writer that sends small files in one request and bigger ones in 32 MB parts, and aborts the upload when dropped;
    - copies (in parts above 5 GiB), deletes, buckets and presigned links;
    - **an in-process S3 server** for tests and the smoke test.
  - **`Fs::S3`** puts the buckets in the files view and the transfer queue:
    - buckets at the top, folders as prefixes (with marker objects);
    - renames by copy and delete, copies inside the storage done by the server;
    - uploads that start over when paused, and leave nothing when cancelled.
  - **The S3 host kind:**
    - **Fields:** endpoint, region, path-style.
    - **The keys:** the access key, and the secret key saved encrypted as the password of the host's identity (or asked for when connecting).
    - **Quick connect:** `s3://` and `s3+http://`.
    - **Where it opens:** the files view, with buckets made at the top, no permissions or owners shown, and temporary links for an hour, a day or a week.
- **Documentation:** ADRs 0032 and 0033, the threat model, CHANGELOG, README and the manual matrix.

### "Done when" (PLAN)

- **Every kind has tests or a passed manual check:**
  - **Telnet:** the NVT's tests, and a session against an in-process server (smoke test).
  - **Serial:** parsing, line endings and the hex view, a loopback session, a port that isn't there (tests and smoke test).
  - **Mosh:** the server's output parsed, the server started over SSH on the in-process server, a missing server and a missing client (tests and smoke test). On Unix, a stand-in `mosh-client` gets the address, the port and the key.
  - **Containers:** the command lines and the listings' parsers (tests), the commands of quick-connect targets and the running ones in quick connect (smoke test).
  - **Local shells:** the shell list (WSL's UTF-16 too) and a tab with a shell from the list (smoke test).
  - **The real-world checks** (a serial device, mosh to a real server, Docker, Podman and Kubernetes) are in the manual matrix for the owner.
- **The S3 view uploads and downloads a 1 GiB multipart file with the same SHA-256 against a local S3 server: yes,** against RustFS 1.0.0 in CI (`real_s3`, through the transfer queue in 32 MB parts).

### How it was verified

- **Windows (Qt 6.10.3), on the final code:**
  - fmt, clippy `-D warnings`, lint-qml, i18n, shaders, deny and audit.
  - Every test: 653 passed.
  - The smoke tests: offscreen (511 steps), native (504 steps) and the gallery.
  - **The new smoke steps:** a shell from the list, telnet, a serial port in hexadecimal, mosh, containers, and S3 in the files view (buckets, an upload, a temporary link, a folder downloaded, a folder made, renamed and deleted).
  - Screenshots (native): the new tab menu, telnet, serial in hexadecimal, S3 in the files view, and the host editor of a serial and an S3 host, in dark and light, comfortable and compact.
- **WSL:** Debian 13 (Qt 6.8.2) built the app and passed clippy over the whole workspace (with the Linux-only code: serial ports from `/sys/class/tty`, the shell list from `/etc/shells`). Its test run was then stopped because the computer ran low on memory (the AWS SDK makes the build heavier). Fedora and Arch were not run, so as not to repeat it: Linux tests and smoke tests ran in CI instead, as in Sprint 10.
- **GitHub Actions:**
  - green on every job of the final code: Ubuntu 24.04, Windows, and the Fedora, Arch and Debian 13 containers (build, clippy, tests, smoke tests); formatting, lints, licenses and advisories; the `ssh` job against OpenSSH and Dropbear;
  - **the new `s3` job:** RustFS 1.0.0 in Docker, 1 GiB up in 6.8 s (multipart, 32 MB parts) and down in 1.4 s through the transfer queue, the same SHA-256, and a temporary link.

### Deviations

- **No `aws-config`** (PLAN §2 named it): credentials come only from the vault, so nothing is read from `~/.aws` or the environment.
- **No resume into S3:** a paused or interrupted upload into S3 starts over (S3 has no writing at an offset). Downloads from S3 resume as before.
- **Mosh on Windows** needs a `mosh-client` from Cygwin on the PATH (or a WSL tab): there is no Windows build of its own.
- **Not tested against real devices or servers here:** a serial adapter, mosh, Docker, Podman and Kubernetes (none on this machine or in the WSL distros); they are in the manual matrix.

### Pending

- **Manual matrix** ([manual-matrix.md](../testing/manual-matrix.md)):
  - each shell of the new tab menu;
  - a real serial device;
  - telnet to a real device;
  - mosh to a real server;
  - Docker, Podman and Kubernetes;
  - S3 on AWS and on the owner's RustFS, with a temporary link opened in a browser.
- **Carried over:** the Sprint 6 to 11 manual checks, an Ubuntu package.

### Risks

- **The AWS SDK** is a large dependency tree. Its newer releases need Rust 1.91 (ADR 0033): staying on 1.122.0 is fine until the MSRV moves.
- **`lru` 0.16.4** (RUSTSEC-2026-0253, unsound) comes with the SDK, with no fixed 0.16 release; reviewed each sprint.
- **Telnet** is unsafe on untrusted networks by design; the warnings say so.
