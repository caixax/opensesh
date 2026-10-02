# Manual test matrix

PLAN §10 asks for a manual pass on every Tier 1 environment in each sprint with UI changes. This file records **what was actually run**, when, and on which machine.

**Legend:** ✅ passed · ❌ failed · ⏳ not run yet (needs that environment) · — not applicable

## Sprint 16 (2026-10-03)

### Automated checks

- **Main window (`--smoke-test`):** the import dialog reads the MobaXterm, PuTTY (`.reg`), Remmina and CSV samples (6, 4, 3 and 3 hosts), a CSV without an address column can't be imported, the CSV import adds 3 hosts; the export dialog opens; a sample sync conflict lists its 4 differences in the conflict dialog; Settings > Data and sync opens.
- **Screenshots (native, Windows):** the import dialog (MobaXterm, with what is left out and the command it would run; CSV with the column mapping), the export dialog, Settings > Data and sync and the conflict dialog, in dark and light, comfortable and compact.
- **Importers:** fixtures for every format (CRLF and Windows-1252 for MobaXterm, UTF-16 `.reg`), the bundle written and read back, sealed keychains (a wrong password refused, nothing added twice), the OpenSSH config export read back by the importer.
- **Sync:** the three-way merge by record id, conflicts found and split, the settings folder pointer, the Git helper on a temporary repository, and two instances writing one folder at once (which loses hosts when the merge is taken out).

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) | Real servers |
|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ 730 + 5 (RDP helper) | ✅ / ✅ | ✅ `windows` | — |
| GitHub Actions: Ubuntu 24.04, Windows, Arch, Fedora and Debian 13 containers | aqt and distro | ✅ | ✅ / ✅ | — | ✅ OpenSSH, Dropbear, xrdp, TigerVNC, x11vnc, wayvnc, RustFS |

### Manual checks for the owner

| Check | Windows 10 | Linux |
|---|---|---|
| Import your own MobaXterm export (`.mxtsessions`) and `MobaXterm.ini`; say which sessions came wrong or were skipped (please send the files, with passwords removed, for the fixtures) | ⏳ | — |
| Import PuTTY's sessions from the registry, and a `.reg` export on Linux | ⏳ | ⏳ |
| Import Remmina's profiles (`~/.local/share/remmina`) | — | ⏳ |
| A CSV from a spreadsheet: map its columns, import | ⏳ | ⏳ |
| Export a bundle with its keychain on one computer and import it on the other (identities and keys work there) | ⏳ | ⏳ |
| Settings folder in Syncthing on two computers: edit different hosts on both, then the same host on both (conflict dialog) | ⏳ | ⏳ |
| Settings folder as a Git repository with a private remote: commit, push, pull on the other computer | ⏳ | ⏳ |

## Sprint 15 (2026-10-02)

### Automated checks

- **Main window (`--smoke-test`):** as before (no new steps: a test run never forwards X11 or starts waypipe).
- **Screenshots (native, Windows):** the host editor's X11 forwarding and Waypipe rows, in dark and light, comfortable and compact.
- **The SSH client:** the cookie swap (both byte orders, a wrong cookie refused), `DISPLAY` parsing, a forwarded X11 connection through the in-process server to a made-up display (which never sees the made-up cookie), a host without forwarding refusing the server's channel, the Waypipe command on the server.
- **Against OpenSSH in CI** (the `ssh` job): `xdpyinfo` through the built-in client's forwarding to Xvfb, trusted and untrusted; `wayland-info` through Waypipe to a headless sway, as a user without a login session (no `/run/user/<uid>`).

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) | Real servers |
|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ 691 + 5 (RDP helper) | ✅ / ✅ | ✅ `windows` | — |
| GitHub Actions: Ubuntu 24.04, Windows, Arch, Fedora and Debian 13 containers | aqt and distro | ✅ | ✅ / ✅ | — | ✅ OpenSSH (X11 to Xvfb, Waypipe to sway), Dropbear, xrdp, TigerVNC, x11vnc, wayvnc, RustFS |

### Manual checks for the owner

| Check | Windows 10 | Linux |
|---|---|---|
| `xclock` and `gedit` from an SSH host with X11 forwarding, untrusted then trusted, on Hyprland (Xwayland) and on KDE Plasma | — | ⏳ |
| The same on Windows with VcXsrv (access control off) or X410 | ⏳ | — |
| Waypipe: `gedit` or `foot` from a host with Waypipe on, on Hyprland or KDE (waypipe installed on both sides) | — | ⏳ |
| What a host says when waypipe is missing on the server, or X11 forwarding has no display | ⏳ | ⏳ |

## Sprint 14 (2026-10-02)

### Automated checks

- **Main window (`--smoke-test`, 557 steps offscreen with the software renderer and 569 native on Windows):** as before, plus a VNC desktop against the in-process RFB server:
  - VeNCrypt's certificate and the password asked in the pane, a wrong password first;
  - the desktop's pixels, Ctrl+Alt+Del as keysyms repainting a square, the clipboard both ways (a stand-in for the user's);
  - the desktop following the pane once asked to (SetDesktopSize), a disconnection, and connecting again.
- **Screenshots (native, Windows),** in dark and light, comfortable and compact: a VNC desktop fitted to its pane, and the host editor of a VNC host.
- **The VNC client against its in-process server:** every version and security type, the encodings in turn, keys, the pointer, the clipboard both ways, the cursor, a resize, read-only sessions; the same session driven as the app drives it.
- **The VNC client against real servers** in CI (`scripts/vnc-test-servers.sh`, Ubuntu 24.04):
  - TigerVNC 1.13.1 with VNC authentication: the desktop, keys, the pointer, the clipboard both ways (through the X session's clipboard), and a resize to 800x600;
  - TigerVNC with VeNCrypt X509Vnc and a certificate made for the run: the certificate's question, then the desktop over TLS;
  - x11vnc on Xvfb: the desktop, keys and the pointer;
  - wayvnc on a headless sway, VeNCrypt with a user name and password: the desktop and keys.

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) | Real servers |
|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ 686 + 5 (RDP helper) | ✅ / ✅ | ✅ `windows` | — |
| GitHub Actions: Ubuntu 24.04, Windows, Arch, Fedora and Debian 13 containers | aqt and distro | ✅ | ✅ / ✅ | — | ✅ TigerVNC 1.13.1 (two setups), x11vnc, wayvnc, xrdp, OpenSSH, Dropbear, RustFS |

### Manual checks for the owner

| Check | Windows 10 | Linux |
|---|---|---|
| A VNC server you use (TigerVNC, x11vnc, a NAS or a Raspberry Pi): the password from an identity, typing in your layout, the clipboard, view only | ⏳ | ⏳ |
| wayvnc on your Wayland desktop, with `enable_auth` and a certificate: the certificate question, then the desktop | — | ⏳ |
| A slow link: the picture quality at Low against Lossless | ⏳ | ⏳ |
| macOS Screen Sharing and RealVNC: expected to be refused with an explanation (they want Apple's or RealVNC's own security types unless a VNC password is allowed) | ⏳ | ⏳ |

## Sprint 13 (2026-10-02)

### Automated checks

- **Main window (`--smoke-test`, 533 steps offscreen and 537 native on Windows):** as before, plus remote desktops against the RDP test server (`cargo xtask rdp --test-server`) through the real helper:
  - the certificate and the password asked in the pane, a wrong password first;
  - the desktop's pixels, Ctrl+Alt+Del repainting a square, the clipboard both ways (a stand-in for the user's);
  - the desktop following a narrower pane after a split, a disconnection, and a new helper connecting again;
  - the same desktop through a jump host (the SSH test server), over the local tunnel.
- **Screenshots (native, Windows),** in dark and light, comfortable and compact: a connected desktop, one asking about its certificate, and the host editor of an RDP host.
- **The helper against its in-process server:** the certificate, a refused password then the right one, pixels (opaque), keys, a click, the wheel, the clipboard both ways, a resize, the end.
- **The helper against xrdp** in CI (`scripts/rdp-test-server.sh`): TLS without NLA, the desktop, keys and the mouse, the X session's clipboard, a resize.

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) | Real servers |
|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ 666 + 5 (helper) | ✅ / ✅ | ✅ `windows` | — |
| GitHub Actions: Ubuntu 24.04, Windows, Arch, Fedora and Debian 13 containers | aqt and distro | ✅ | ✅ / ✅ | — | ✅ xrdp 0.9.24 (desktop, keys, clipboard, resize), OpenSSH, Dropbear, RustFS |

### Manual checks for the owner

| Check | Windows 10 | Linux |
|---|---|---|
| Windows 11 (or Windows Server) with NLA: the certificate question, the password from an identity, typing in your keyboard layout, the clipboard both ways, resizing the pane, Ctrl+Alt+Del | ⏳ | ⏳ |
| xrdp on a Linux machine: logging in, a resize, copy and paste | ⏳ | ⏳ |
| Full screen (F11) with a desktop, and giving the keyboard back (Ctrl+Alt+Home) | ⏳ | ⏳ |
| A desktop through a jump host (a saved SSH host as the jump) | ⏳ | ⏳ |
| On Wayland: which shortcuts the compositor keeps (Super, Alt+Tab) while a desktop has the keyboard | — | ⏳ |
| A high-DPI screen (150 %): the desktop sharp at "Actual size", and following the pane | ⏳ | ⏳ |

## Sprint 12 (2026-10-02)

### Automated checks

- **Main window (`--smoke-test`, 511 steps offscreen and 504 native on Windows):** as before, plus:
  - a tab with a shell from the list (Windows PowerShell on Windows);
  - telnet to its in-process server: the warning, the window size the server was told, the end;
  - a serial port on the loopback device: an echo, the hexadecimal view, a break;
  - mosh: the questions answered in the pane, `mosh-server` started on the SSH test server, the key never shown;
  - containers: the commands of `podman://` and `kube://` targets, and the running containers offered in quick connect (samples);
  - S3 in the files view against the in-process S3 server: buckets, a bucket's folders, an upload, a temporary link, a folder downloaded, a folder made, renamed and deleted.
- **Screenshots (native, Windows),** in dark and light, comfortable and compact: the new tab menu with the shells, a telnet tab, a serial tab in hexadecimal, S3 in the files view, and the host editor of a serial and an S3 host.
- **S3 against a real server:** 1 GiB up in 32 MB parts and back down with the same SHA-256, against RustFS 1.0.0 in CI (`scripts/s3-test-server.sh`).

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) | Real servers |
|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ 653 | ✅ / ✅ | ✅ `windows` | — |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ build and clippy; tests stopped (low memory) | — | — | — |
| GitHub Actions: Ubuntu 24.04, Windows, Arch, Fedora and Debian 13 containers | aqt and distro | ✅ | ✅ / ✅ | — | ✅ RustFS 1.0.0 (1 GiB, same SHA-256), OpenSSH, Dropbear |

### Manual checks for the owner

| Check | Windows 10 | Linux |
|---|---|---|
| The new tab menu: each shell opens (PowerShell, cmd, Git Bash, a WSL distro; on Linux, zsh or fish) | ⏳ | ⏳ |
| A real serial device (a USB serial adapter, an Arduino, a router's console): settings, hexadecimal view, a break, unplugging it | ⏳ | ⏳ |
| Telnet to a real device or `telnet.example` service, and resizing the pane | ⏳ | ⏳ |
| Mosh to a server with mosh installed: typing while the network drops, and a server without it | — (no Windows client) | ⏳ |
| A Docker or Podman container and a Kubernetes pod: the running list, entering one, a container without bash | ⏳ | ⏳ |
| S3 on AWS and on your RustFS: saving the keys, a big upload, a temporary link opened in a browser | ⏳ | ⏳ |

## Sprint 11 (2026-09-28)

### Automated checks

- **Main window (`--smoke-test`, 449 to 461 steps):** as before, plus the remote monitor against the in-process test server:
  - the SSH pane's readings reach the status bar (CPU needs two readings, so the rates are checked too);
  - the side panel's Info tab reads the host over the same connection, and its "Copy as text" has the host name, the system, the address and the CPU.
- **Screenshots (native, Windows),** in dark and light, comfortable and compact:
  - the Info tab next to a real SSH tab on the test server, with the status bar's readings;
  - Settings > SSH with the Remote monitor group.
- **The monitor's command:**
  - run with the system's `sh` in every Linux job and distro;
  - with busybox's `sh` and applets in CI's Ubuntu job (busybox 1.36.1) and by hand in WSL (busybox 1.30.1);
  - its cost measured with dash and busybox ([perf.md](../perf.md)).
- **Real SSH servers:** two readings and the host info on OpenSSH 10.5p1 and Dropbear, with the Sprint 7 to 9 tests.

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) | Real SSH servers |
|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ 607 | ✅ / ✅ | ✅ `windows` | — |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ 611 | ✅ / ✅ | ✅ Wayland, ✅ X11 | — |
| Fedora 43 (WSLg) | 6.10.3 (distro) | ✅ 611 | ✅ / ✅ | ✅ Wayland, ✅ X11 | — |
| Arch Linux (WSLg) | 6.11.2 (distro) | ✅ 611 | ✅ / ✅ | ✅ Wayland, ✅ X11 | ✅ OpenSSH 10.5p1, Dropbear 2026.94 |
| GitHub Actions: Ubuntu 24.04, Windows, Arch, Fedora and Debian 13 containers | aqt and distro | ✅ | ✅ / ✅ | — | ✅ Ubuntu 24.04 packages |

### Manual checks

| Check | Windows 10 | Linux |
|---|---|---|
| The monitor on your own servers: a VPS, and a Raspberry Pi or an Alpine box (busybox) | ⏳ | ⏳ |
| A FreeBSD or macOS server, if one is at hand | ⏳ | ⏳ |
| A Windows server with OpenSSH: no readings, and the Info tab says why | ⏳ | ⏳ |
| The status bar and the Info tab during a reconnection, and switching between two SSH tabs | ⏳ | ⏳ |
| Turning the monitor off for one host in its editor, and globally in Settings > SSH | ⏳ | ⏳ |

## Sprint 10 (2026-09-28)

### Automated checks

- **Main window (`--smoke-test`, 433 to 439 steps):** as before, plus:
  - **Paste protection:** a `curl … | sh` paste waits for the review (cancelled); a plain one goes through at once.
  - **Snippets:** a snippet with a variable, its editor and the quick picker opened. It then runs through the run dialog in the two broadcast panes of a three-pane tab, and not in the pane that left.
  - **Macros,** against the in-process SSH server: one types, waits for the prompt and types again; one waiting for text that never shows stops on its timeout.
  - **Recordings:** the SSH session recorded, found in the History list, played in a tab to its end, then jumped back to the start.
  - **Closing:** closing a window whose shells run asks first; confirming closes it and ends its sessions.

  The user's snippets aren't loaded, and nothing is written outside the run's temporary folder.
- **Screenshots (native, Windows),** in dark and light, comfortable and compact:
  - the Snippets view, the editor on a macro and the quick picker;
  - the paste review of a two-line `curl | sudo bash`;
  - the player's bar, and History;
  - the close question, and Settings > About.
- **Windows executable:** `OpenSesh.exe` read back with PowerShell has its icon and "OpenSesh 0.1.3" in its version information. `cargo xtask icons` checks the `.ico`'s sizes and colors in a test.

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) | Real SSH servers |
|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ 589 | ✅ / ✅ | ✅ `windows` | — |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ 592 | ✅ / ✅ | ✅ Wayland, ✅ X11 | — |
| Fedora 43 (WSLg), before the owner's requests | 6.10.3 (distro) | ✅ 590 | ✅ / ✅ | ✅ Wayland, ✅ X11 | — |
| Arch Linux (WSLg), before the owner's requests | 6.11.2 (distro) | ✅ 590 | ✅ / ✅ | ✅ Wayland, ✅ X11 | — |
| GitHub Actions: Ubuntu 24.04, Windows, Arch, Fedora and Debian 13 containers | aqt and distro | ✅ | ✅ / ✅ | — | ✅ Ubuntu 24.04 packages |

### Manual checks

| Check | Windows 10 | Linux |
|---|---|---|
| A paste from a web page with hidden characters, and a real `curl … \| sh` line: the review shows them | ⏳ | ⏳ |
| A snippet with `{{secret:identity}}` on a real server (the password typed, never shown or saved) | ⏳ | ⏳ |
| A macro against a slow server or a network device (waits, a timeout) | ⏳ | ⏳ |
| A long recording played at several speeds and jumped around; the file opened with `asciinema play` | ⏳ | ⏳ |
| The macro recorder on a real session, reviewed and saved | ⏳ | ⏳ |
| Closing with sessions, tunnels and a transfer running: the question, Cancel, Close, Don't ask again | ⏳ | ⏳ |
| The installed app's icon in Explorer, the taskbar, the Start menu and Installed apps; the installer's icon | ⏳ | — |

## Sprint 9 (2026-09-27)

### Automated checks

- **Main window (`--smoke-test`, 397 to 407 steps):** as before, plus the Tunnels view against the in-process test server (which now also serves remote forwards) and a small HTTP server of the test run: a local, a remote and a dynamic tunnel through the saved host H00000 on their own connection (host key and password answered in their rows, and the question dialog opened), a page fetched through the local and the remote one, their counters, a row that stays while they change, a tunnel tied to H00000 that runs with a terminal session and waits again when it closes, the editor (a problem found, `0.0.0.0` seen as exposed), the import dialog (a sample `~/.ssh/config`, not the user's), duplicate and delete. The user's tunnels aren't loaded, and nothing is written.
- **Screenshots (native, Windows):** the Tunnels view (running with traffic, waiting for a session, stopped with the warning, failed), the editor of a tunnel that listens on every interface, and the import dialog, in dark and light, comfortable and compact.
- **Tunnels in-process** (`tests/tunnel.rs`): HTTP through each kind, with `curl` where it is installed; the counters; forwards carried over to a new connection (a request made meanwhile waits for it); a stopped forward giving its port back; a server that forbids forwarding; an independent tunnel reconnecting by itself, and a wrong password ending without retrying.
- **Real SSH servers** (`scripts/ssh-test-servers.sh`, [notes](ssh-servers.md)): the Sprint 7 and 8 tests, plus `curl` through a local, a dynamic and a remote tunnel on OpenSSH 10.5p1, and a tunnel whose `sshd-session` is killed coming back by itself.

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) | Real SSH servers |
|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ 556 | ✅ / ✅ | ✅ `windows` | — |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ 559 | ✅ / ✅ | ✅ Wayland, ✅ X11 | — |
| Fedora 43 (WSLg) | 6.10.3 (distro) | ✅ 559 | ✅ / ✅ | ✅ Wayland, ✅ X11 | — |
| Arch Linux (WSLg) | 6.11.2 (distro) | ✅ 559 | ✅ / ✅ | ✅ Wayland, ✅ X11 | ✅ OpenSSH 10.5p1, Dropbear 2026.94 |
| GitHub Actions: Ubuntu 24.04, Windows, Arch, Fedora and Debian 13 containers | aqt and distro | ✅ | ✅ / ✅ | — | ✅ Ubuntu 24.04 packages |

### Manual checks

| Check | Windows 10 | Linux |
|---|---|---|
| A local tunnel to a database on one of your servers, used by a real client | ⏳ | ⏳ |
| A browser through the SOCKS proxy (names resolved by the server) | ⏳ | ⏳ |
| A remote tunnel with and without `GatewayPorts` on the server; the warning for `0.0.0.0` | ⏳ | ⏳ |
| Wi-Fi off and on with an independent tunnel running: it comes back by itself | ⏳ | ⏳ |
| A tunnel imported from `~/.ssh/config`, up with a terminal session and down when it closes | ⏳ | ⏳ |
| A tunnel that starts with OpenSesh and needs a password: the notification and the Answer button | ⏳ | ⏳ |

## Sprint 8 (2026-09-27)

### Automated checks

- **Main window (`--smoke-test`, 343 to 359 steps):** as before, plus SFTP against the in-process test server (which now serves a temporary folder over SFTP; a smoke run removes it at exit):
  - the side panel of an SSH pane lists the server's files on the pane's connection, follows the shell's folder (OSC 7 on `cd`), and works again after the connection drops and Enter reconnects;
  - the SFTP view with the local sample folder and a saved host (host key card and password in the pane): 10,000 files listed and sorted (timed), a folder made, a file uploaded, uploaded again (the question, "keep both"), renamed, made read-only and writable, downloaded, deleted; the queue cleared; the sides swapped;
  - a server's file edited: the private copy in the test folder (no editor is started in a test run), a save uploaded, the server's copy changed meanwhile (the conflict), "replace it with mine".
  Settings changes stay in memory; nothing is written to the user's folders.
- **Screenshots (native, Windows):** the SFTP view with the queue (a finished and a failed transfer), a file's permissions, the side panel following an SSH tab, and Settings > SFTP, in dark and light, comfortable and compact. The terminal renders in captures only on the native platform (offscreen, grabbing gives an empty terminal, as before).
- **Real SSH servers** (`scripts/ssh-test-servers.sh`, [notes](ssh-servers.md)): the Sprint 7 tests, plus SFTP on OpenSSH 10.5p1: 1 GiB up and down through the queue with the same SHA-256 here, on the server and back; an upload whose connection is closed partway, resumed on a new connection; a copy within the server (`cp -R -p`); and port 2225 (no SFTP subsystem) refused.
- **Shell integration:** the install command run twice by `sh` in a temporary home adds the lines once, and interactive bash 5.3 and zsh 5.9 then report their folder (OSC 7). The test runs wherever bash or zsh is installed (CI's Linux jobs have bash).
- **SCP spike** ([README](../../spikes/scp-fallback/README.md)): a folder tree and 1 GiB both ways against OpenSSH without SFTP, checksums equal.

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) | Real SSH servers |
|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ 539 | ✅ / ✅ | ✅ `windows` | — |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ 542 | ✅ / ✅ | ✅ Wayland, ✅ X11 | — |
| Fedora 43 (WSLg) | 6.10.3 (distro) | ✅ 542 | ✅ / ✅ | ✅ Wayland, ✅ X11 | — |
| Arch Linux (WSLg) | 6.11.2 (distro) | ✅ 542 | ✅ / ✅ | ✅ Wayland, ✅ X11 | ✅ OpenSSH 10.5p1, Dropbear 2026.94 |
| GitHub Actions: Ubuntu 24.04, Windows, Arch, Fedora and Debian 13 containers | aqt and distro | ✅ | ✅ / ✅ | — | ✅ Ubuntu 24.04 packages |

### Manual checks

| Check | Windows 10 | Linux |
|---|---|---|
| The side panel on one of your servers: it follows `cd` once the shell integration is added (or with a shell that already sends OSC 7) | ⏳ | ⏳ |
| A large transfer over a real network (a few GB): speed, pause and resume, and pulling the cable in the middle then Retry | ⏳ | ⏳ |
| Drag files from Explorer / Nautilus / Dolphin into a pane (Wayland and X11), and this computer's files out to them | ⏳ | ⏳ |
| Edit a server's file with VS Code (`code --wait {file}`) and with the system's editor; save twice; change it on the server meanwhile | ⏳ | ⏳ |
| "Save with sudo" on a root-owned file (with and without a sudo password) | ⏳ | ⏳ |
| A server with a non-UTF-8 file name, and one with thousands of files over a slow link | ⏳ | ⏳ |
| Two SFTP panes on two different servers, copying between them | ⏳ | ⏳ |

## Sprint 7 (2026-09-27)

### Automated checks

- **Main window (`--smoke-test`, 260 to 272 steps):** as before, plus an SSH pane against the in-process test server (every SSH connection of a smoke test goes there, with no agent, key file or `known_hosts` of the user): the host key card, a wrong then a right password, the remote shell, a dropped connection and Enter to reconnect, and "Install my key" from its dialog. Keychain > Known hosts finds a hashed name. Settings > SSH opens. Nothing is written.
- **Real SSH servers** (`scripts/ssh-test-servers.sh`, [notes](ssh-servers.md)): OpenSSH with a key file, Dropbear with a password, a user certificate, agent forwarding and the environment, OpenSSH → Dropbear → OpenSSH with `ssh-agent` and a TOTP code through PAM, and the terminal reconnecting after `sshd-session` is killed. `stop` left no user, PAM block or process behind.
- **Pageant 0.83** (the official build, checked against PuTTY's SHA-256 list) holding a fixture key signed for the client, with the Windows OpenSSH agent service stopped.
- **X11 forwarding spike** onto WSLg's Xwayland (see [its README](../../spikes/x11-forwarding/README.md)).

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) | Real SSH servers | Real agent |
|---|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ 517 | ✅ / ✅ | ✅ `windows` | — | ✅ Pageant |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ 519 | ✅ / ✅ | ✅ Wayland, ✅ X11 | — | ✅ ssh-agent |
| Fedora 43 (WSLg) | 6.10.3 (distro) | ✅ 519 | ✅ / ✅ | ✅ Wayland, ✅ X11 | — | ✅ ssh-agent |
| Arch Linux (WSLg) | 6.11.2 (distro) | ✅ 519 | ✅ / ✅ | ✅ Wayland, ✅ X11 | ✅ OpenSSH 10.5p1, Dropbear 2026.94 | ✅ ssh-agent (the real-server tests) |
| GitHub Actions: Ubuntu 24.04, Windows, Arch, Fedora and Debian 13 containers | aqt and distro | ✅ | ✅ / ✅ | — | ✅ Ubuntu 24.04 packages | — |

### Manual checks

| Check | Windows 10 | Linux |
|---|---|---|
| Connect to one of your own servers: the host key card, trust and remember, and no question the next time | ⏳ | ⏳ |
| A server whose key changed (reinstalled): the warning, "Don't connect", then "Replace the saved key" | ⏳ | ⏳ |
| A host with an identity whose vault is locked: "Unlock and connect" | ⏳ | ⏳ |
| MFA on a real bastion (a key, then a code) and a jump host behind it | ⏳ | ⏳ |
| Wi-Fi off and on during a session: the banner, then Enter (and automatic reconnection on a host with it) | ⏳ | ⏳ |
| An old device with legacy algorithms (a switch or router) | ⏳ | ⏳ |
| A SOCKS5 or HTTP proxy, and a proxy command | ⏳ | ⏳ |
| The Windows OpenSSH agent service holding your key | ⏳ | — |
| "Install my key" on a server where it wasn't, then connecting with the key | ⏳ | ⏳ |
| Session logs (text and raw) in the chosen folder | ⏳ | ⏳ |

## Sprint 6 (2026-09-27)

### Automated checks

- **Main window (`--smoke-test`, 237 to 252 steps, most of them polling the wait):** as before, plus the Keychain view with the following, all in memory. Nothing is written, and the user's keyring isn't touched.
  - An identity saved with a password (which creates a keyring-held vault) and an Ed25519 key generated.
  - The key menu, the identity editor, and the generate and import dialogs.
  - A master password set, the vault locked, three wrong passwords, and the right one refused during the wait, then accepted after it.
  - The Agents and Known hosts sections.
- **The "done when" test** (`tests/no_plaintext.rs`): no secret in clear in the data and config folders, and the wait kept across a restart.
- **The real keyring,** with a throwaway service name (`cc.caixa.OpenSesh.selftest`), removed at the end (`cargo test -p opensesh-vault --lib system_keyring -- --ignored`):
  - Credential Manager on Windows (afterwards `cmdkey /list` shows no OpenSesh entry);
  - GNOME Keyring in a private D-Bus session on Debian.
- **Real agents** (`--lib real_agents -- --ignored --nocapture`):
  - Pageant 0.83 (the official build, checked against PuTTY's SHA-256 list) holding a fixture key;
  - `ssh-agent` with `ssh-add` on Debian and Fedora.
  Each listed the key with the fingerprint `ssh-add -l` prints. A stand-in agent on a named pipe covers the Windows OpenSSH agent's transport (its service is disabled on this machine).
- **PuTTY keys:** fixtures made by `puttygen` 0.83 (versions 2 and 3; Argon2i, Argon2d and Argon2id; Ed25519, ECDSA and RSA), each giving the public key `puttygen` prints.

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) | Real agent | Real keyring |
|---|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ | ✅ / ✅ | ✅ `windows` | ✅ Pageant | ✅ Credential Manager |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ | ✅ / ✅ | ✅ Wayland, ✅ X11 | ✅ ssh-agent | ✅ GNOME Keyring |
| Fedora 43 (WSLg) | 6.10.3 (distro) | ✅ | ✅ / ✅ | ✅ Wayland, ✅ X11 | ✅ ssh-agent | — |
| Arch Linux (WSLg) | 6.11.2 (distro) | ✅ | ✅ / ✅ | ✅ Wayland, ✅ X11 | — (no OpenSSH installed) | — |
| GitHub Actions: Ubuntu 24.04, Windows, Arch, Fedora and Debian 13 containers | aqt and distro | ✅ | ✅ / ✅ | — | — | — |

The drive that held the WSL disks filled up during the sprint, and the Debian and Arch disks went read-only with I/O errors in the middle of a run. With the owner's go-ahead, the four WSL distros moved to another drive (`wsl --manage <distro> --move`). Every distro came back read-write without file system errors, and the table above is the run on the final code after the move. Debian's clippy first failed to find the `opensesh_vault` crate, and passed after `cargo clean -p opensesh-vault`: build metadata truncated by the full disk.

### Manual checks

| Check | Windows 10 | Linux |
|---|---|---|
| Save an identity with a password through the app: the vault appears in the keyring (Credential Manager, Seahorse or KWalletManager), and opens by itself after a restart | ⏳ | ⏳ GNOME, ⏳ KDE |
| Set a master password, restart: the unlock dialog; three wrong passwords and the countdown; "remember on this computer" skips the dialog next time | ⏳ | ⏳ |
| Leave the app alone past the idle time: the vault locks and the status bar shows it | ⏳ | ⏳ |
| Import your own OpenSSH key with a passphrase, and a .ppk saved by PuTTY on Windows | ⏳ | ⏳ |
| Export a private key with a passphrase and load it with `ssh-add` | ⏳ | ⏳ |
| The Windows OpenSSH agent service with keys added by `ssh-add` | ⏳ | — |
| A locked login keyring on Linux: the desktop's unlock prompt while the keychain shows it is busy | — | ⏳ |

## Sprint 5 (2026-09-26)

### Automated checks

- **Main window (`--smoke-test`, 131 to 134 steps):** as before, plus the Hosts view with 1000 generated hosts (cards and list, a fuzzy search, Favorites, a group with its subgroups, Shift extending the selection, the host menu, the host and group editors, the ssh_config import dialog), quick connect with a jump host and a bad port, the command palette offering a host, a saved host connected in a tab and in a split (the smoke test runs its hermetic shell instead of `ssh` and checks the command line), the Recent list, the open-session count, and a `connect` request as if from another process. Nothing is written.
- **Single instance and CLI, end to end** (portable copies, a saved host on 127.0.0.1 port 1): `opensesh list`, `opensesh connect`, a second start handing over its `--connect`, a bad target refused by the CLI, `ssh` starting in a pane and its exit shown; on Linux also a stale socket replaced on the next start.
- **Packages:** the Debian package ships `/usr/bin/opensesh` next to `opensesh-app`.

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) | Single instance + CLI |
|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ | ✅ / ✅ | ✅ `windows` | ✅ (named pipe) |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ | ✅ / ✅ | ✅ Wayland, ✅ X11 | ✅ (Unix socket) |
| Fedora 43 (WSLg) | 6.10.3 (distro) | ✅ | ✅ / ✅ | ✅ Wayland, ✅ X11 | ⏳ |
| Arch Linux (WSLg) | 6.11.2 (distro) | ✅ (GCC 16 warnings from Qt headers and cxx's generated code, as before) | ✅ / ✅ | ✅ Wayland, ✅ X11 | ⏳ |

### Manual checks

| Check | Windows 10 | Linux |
|---|---|---|
| Connect to a real SSH host with a jump host and a key file, and reconnect from the banner | ⏳ | ⏳ |
| Link a real ~/.ssh/config with includes; edit it and see the Hosts view follow | ⏳ | ⏳ |
| Drag hosts onto a group, and a group onto another; keyboard selection with Shift and Ctrl | ⏳ | ⏳ |
| Scroll 1000 hosts as cards on real hardware | ⏳ | ⏳ |
| `opensesh open` from a terminal: the confirmation, then the connection | ⏳ | ⏳ |
| Starting OpenSesh from the Start menu or the launcher while it runs brings the window forward | ⏳ | ⏳ (Wayland may refuse to raise it) |

## Sprint 4 (2026-09-26)

### Automated checks

- **Main window (`--smoke-test`, 105 to 108 steps):** as before, plus the workspaces dialog and the tab rename dialog. In real terminals it splits a tab into three panes, moves the focus in every direction, resizes, swaps and maximizes; broadcasts from one pane to a second while the third has left, and checks that the text reaches exactly the two receiving panes, that the third's own input stays there, and that a paste asks first; saves a tab (name, color, ratios, focused pane, profiles) through `Workspaces.roundTrip()`, opens it and compares it; moves the tab to a new window and back and checks that its sessions never ended; opens a workspace with two windows (as the last session is restored) and checks that closing the second window ends its sessions; then duplicates, pins, closes, reopens and switches tabs (Ctrl+Tab) and closes the others. Nothing is written.
- **Gallery (`--gallery --smoke-test`):** unchanged.
- **Screenshots:** a new series, `terminal-splits-*` and `terminal-broadcast-*`, reviewed on Windows with the real renderer (the panes show the renderer's demo frame; offscreen captures leave terminals blank, as in the gallery).

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) |
|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ | ✅ / ✅ | ✅ `windows` |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ | ✅ / ✅ | ✅ Wayland, ✅ X11 |
| Fedora 43 (WSLg) | 6.10.3 (distro) | ✅ | ✅ / ✅ | ✅ Wayland, ✅ X11 |
| Arch Linux (WSLg) | 6.11.2 (distro) | ✅ (GCC 16 warnings from Qt headers and cxx's generated code, as before) | ✅ / ✅ | ✅ Wayland, ✅ X11 |

### Manual checks

| Check | Windows 10 | Linux |
|---|---|---|
| Alt+Shift+= and Alt+Shift+- split on US, Spanish and German keyboard layouts | ⏳ | ⏳ |
| Alt+arrows reach the shell (word movement) while a tab has one pane, and move the focus with several | ⏳ | ⏳ |
| Dragging a divider, and the pane's terminal size following it (`stty size`) | ⏳ | ⏳ |
| Dragging a tab along the strip, onto another window, and out of every window (X11 places the new window at the pointer; Wayland lets the compositor place it) | ⏳ | ⏳ X11, ⏳ Wayland |
| Ctrl+Tab with Ctrl held shows the list; a quick press switches at once; releasing Ctrl outside the window switches | ⏳ | ⏳ |
| Broadcast to vim in one pane and a shell in another: arrows work in both (application cursor mode) | ⏳ | ⏳ |
| "Restore sessions at startup": quit with split tabs in two windows, start again, same layouts and folders (a shell that reports its directory with OSC 7) | ⏳ | ⏳ |

## Sprint 3 (2026-09-26)

### Automated checks

- **Main window (`--smoke-test`, 84 to 87 steps):** as before, plus Settings > Terminal (with its live preview), Profiles, Themes and Shortcuts, the highlighting rule editor and the theme editor. In a real terminal tab it creates a profile, checks that a profile edit reaches the open terminal (`fontSize`), zooms the tab, toggles highlighting, changes and resets a shortcut, and deletes the profile. Test runs keep these changes in memory: nothing is written.
- **Gallery (`--gallery --smoke-test`):** unchanged.

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) |
|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ | ✅ / ✅ |
| Debian 13 (WSL) | 6.8.2 (distro) | ✅ | ✅ / ✅ |
| Fedora 43 (WSL) | 6.10.3 (distro) | ✅ | ✅ / ✅ |
| Arch Linux (WSL) | 6.11.2 (distro) | ✅ (GCC 16 warnings from Qt headers, as in Sprint 1) | ✅ / ✅ |

Screenshots (`--screenshots`) of Settings > Appearance, Terminal, Profiles, Themes and Shortcuts were reviewed in dark/light × comfortable/compact on Windows with the real renderer, and the Terminal preview with a profile that turns on ligatures, the Dracula theme, highlighting and a 4.5:1 minimum contrast.

### Manual checks

| Check | Windows 10 | Linux |
|---|---|---|
| Translucent background (`background_opacity` below 1): the window starts with an alpha channel, without warnings | ✅ (started; the desktop showing through was not looked at) | ⏳ |
| The desktop shows through a translucent terminal, and only there (the panels stay opaque); blur from the compositor (KDE, Hyprland) | ⏳ | ⏳ (needs real hardware) |
| Bell "sound" | ⏳ | ⏳ X11 (Wayland has no standard beep: flashes) |
| Bell "notification" flashes the taskbar entry of an inactive window | ⏳ | ⏳ |
| File dialogs (background image, theme import and export): native on Windows, the portal or Qt's own on Linux | ⏳ | ⏳ |
| Fonts from the system font list, fallback fonts (a Nerd Font, a CJK font), hinting and antialiasing off | ⏳ | ⏳ |
| Ligatures with Fira Code and Cascadia Code | ⏳ | ⏳ |
| Legacy encodings against a real device or server (ISO-8859-15, Shift_JIS) | ⏳ (Sprint 7, SSH) | ⏳ |

## Sprint 1 (2026-09-25)

### Automated checks

Each smoke test fails on any warning from our QML (exit code 6), besides the Sprint 0 checks:

- **Main window (`--smoke-test`, 59 steps):** visits every view and every Settings section, opens and closes the command palette, the notifications, the side panel and the Restore defaults dialog, opens and closes tabs, cycles focus with F6, switches the layout, and checks that the keyboard focus never stays on a hidden item. It writes no settings.
- **Gallery (`--gallery --smoke-test`, 26 steps):** visits every section, opens and closes every dialog, drawer and menu, runs a command palette search, shows toasts, and flips theme, density, accent and reduce motion.
- **Crash dialog (`--crash-report <file> --smoke-test`):** unchanged.

**Host:** as in Sprint 0 (Windows 10 22H2; WSL2 with WSLg for Linux).

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery / dialog) | Native Wayland (main / gallery / dialog) | X11 `xcb` (main / gallery / dialog) |
|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ | ✅ / ✅ / ✅ | — (native `windows`: ✅ / ✅ / ✅) | — |
| Arch Linux (WSLg) | 6.11.2 (distro) | ✅ build; clippy and tests ✅ before the review fixes (1) (3) | ✅ / ✅ / ✅ | ✅ / ✅ / ✅ | ✅ / ✅ / ✅ |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ build; clippy and tests ✅ before the review fixes, core tests ✅ after (3) | ✅ / ✅ / ✅ | ✅ / ✅ / ✅ | ✅ / ✅ / ✅ |
| Fedora 43 (WSLg) | 6.10.3 (distro) | ✅ build; clippy and tests ✅ before the review fixes (3) | ✅ / ✅ / ✅ | ✅ / ✅ / ✅ (2) | ✅ / ✅ / ✅ |
| Ubuntu 22.04 (WSLg) | 6.10.3 (aqt) | ✅ build and clippy; tests ✅ before the review fixes (3) | ✅ / ✅ / ✅ | ✅ / ✅ / ✅ (2) | ✅ / ✅ / ✅ |

(1) GCC 16 prints a `-Wsfinae-incomplete` warning from Qt's own `qchar.h` while it compiles the cxx-qt generated code. It is not in our code and doesn't fail the build.

(2) With `XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir`, as in Sprint 0.

(3) The smoke tests were run on the final code. The final Linux clippy and test run was stopped because the host ran low on memory (four distros building at once); it is pending, one distro at a time.

Screenshots (`--screenshots`) of the main window, Settings, every gallery page and the crash dialog were reviewed in dark/light × comfortable/compact on Windows (offscreen) and on Debian 13 (native Wayland, Qt 6.8.2). The pseudo-locale was reviewed on Windows.

### Manual checks

| Check | Windows 10 | Linux (WSLg) |
|---|---|---|
| Real key presses (sent with `WScript.Shell.SendKeys` to the running window): Ctrl+Shift+P opens the palette, typing filters it, Enter runs the action (density switched to compact live), Ctrl+Shift+T opens a tab, F6 moves the focus to the rail with a visible focus ring, Ctrl+Shift+Q quits with exit code 0 | ✅ | ⏳ |
| Rail with real key presses: F6 twice reaches the rail (the first stop is the title bar), Down moves, Enter opens SFTP, End then Space opens Settings; the focused item shows its focus ring and label tooltip | ✅ | ⏳ |
| `config.toml` with a TOML syntax error: the app starts on defaults, warns, and leaves the file byte-for-byte unchanged | ✅ | ⏳ |
| An existing `config.toml` doesn't produce a "changed on disk" reload at startup | ✅ | ⏳ |
| Pseudo-locale (`language = "pseudo"`, debug build): every visible string is translated, long strings wrap without clipping | ✅ | ⏳ |
| Release build with `language = "pseudo"` in `config.toml`: the pseudo-locale isn't bundled, so the UI is English | ✅ | ⏳ |
| Real mouse (`SetCursorPos` + `mouse_event`), `custom` decorations: dragging the title bar moves the window, a double-click maximizes and restores it, dragging the bottom-right corner resizes it, the close button quits with exit code 0 | ✅ | ⏳ |
| Frameless maximize fills the work area exactly (1920×1040 on a 1920×1080 screen), so the taskbar stays visible | ✅ | — |
| Tiling compositor: `auto` decorations drop the window buttons (Hyprland, Sway, niri, i3) | — | ⏳ (needs real hardware) |
| Screen reader names (Narrator, Orca) | ⏳ | ⏳ |

## Sprint 0 (2026-09-25)

### Automated checks

**Main window (`opensesh-app --smoke-test`) passes when:**

1. The main window renders a first frame.
2. The Knock button is pressed three times and the Rust `SesameDoor` object opens the door.
3. A typed, AOT-compiled QML probe confirms that non-ASCII text survived the build.

**Crash dialog (`--crash-report <file> --smoke-test`) passes when** the dialog renders a first frame.

**Host:** Windows 10 Pro 22H2 (build 19045), 20 threads. Linux runs happen in WSL2 with WSLg, where the Wayland compositor is Weston and X11 goes through XWayland. The Linux builds link with `lld`.

| Environment | Qt | Build | `offscreen` (main / dialog) | Native Wayland (main / dialog) | X11 `xcb` (main / dialog) | App id / WM_CLASS checked |
|---|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ debug + release | ✅ / ✅ | — | — | — |
| Arch Linux (WSLg) | 6.11.2 (distro) | ✅ | ✅ / ✅ | ✅ / ✅ | ✅ / ✅ | ✅ (1) |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ | ✅ / ✅ | ✅ / ✅ | ✅ / ✅ | ⏳ |
| Fedora 43 (WSLg) | 6.10.3 (distro) | ✅ | ✅ / ✅ | ✅ / ✅ (2) | ✅ / ✅ | ⏳ |
| Ubuntu 22.04 (WSLg) | 6.10.3 (aqt) | ✅ | ✅ / ✅ | ✅ / ✅ (2) | ✅ / ✅ | ⏳ |

(1) Checked on Arch:
- Wayland: `WAYLAND_DEBUG=1` shows `xdg_toplevel.set_app_id("cc.caixa.OpenSesh")`.
- X11 (`xprop`): `WM_CLASS = "opensesh-app", "OpenSesh"`, and `_GTK_APPLICATION_ID` and `_KDE_NET_WM_DESKTOP_FILE` are both `cc.caixa.OpenSesh`.

(2) These WSL instances couldn't always start the systemd user session. When that happens, `/run/user/1000` has no `wayland-0` socket and Qt fails with "Failed to create wl_display". It's a WSL environment issue, not an app issue: the runs passed with `XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir`.

The debug build also has a regression check for the smoke test itself: with `/utf-8` removed from `build.rs`, `--smoke-test` exits with code 5 ("text encoding BROKEN").

### Manual checks

| Check | Windows 10 | Arch | Debian 13 | Fedora 43 | Ubuntu 22.04 |
|---|---|---|---|---|---|
| Window renders (screenshot), window icon shown | ✅ | ⏳ | ⏳ | ⏳ | ⏳ |
| Keyboard: Space presses the focused Knock button, door opens after 3 knocks | ✅ | ⏳ | ⏳ | ⏳ | ⏳ |
| Panic across FFI (`OPENSESH_DEBUG_PANIC=1`, press Knock) (3) | ✅ | ⏳ | ⏳ | ⏳ | ⏳ |
| Qt fatal error (`QT_QPA_PLATFORM=nosuchplugin`) writes a "Qt fatal error" crash report | ✅ | ⏳ | ⏳ | ⏳ | ⏳ |
| Release build: `--version` printed; startup error shows a message box | ✅ | — | — | — | — |

(3) On Windows, pressing Knock with `OPENSESH_DEBUG_PANIC=1`:
- The process aborted (`0xC0000409`).
- **Exactly one** crash report was written. It starts with the root cause (`sesame.rs`), and cxx's "panic in ffi function ..., aborting" follows as an appended section.
- **Exactly one** crash dialog opened, showing that report.

### Not run yet (needs real hardware)

| Environment | Build | Native Wayland | X11 | App id | Window + keyboard |
|---|---|---|---|---|---|
| Hyprland (`hyprctl clients`), the Sprint 0 "done when" | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| Sway | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| KDE Plasma 6 | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| GNOME | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| X11 session (i3 / XFCE) | ⏳ | — | ⏳ | ⏳ | ⏳ |
| Windows 11 | ⏳ | — | — | — | ⏳ |
| Fractional scaling 125 % / 150 % | ⏳ | ⏳ | ⏳ | — | ⏳ |

The procedure for each is in [`../dev-setup.md`](../dev-setup.md#checking-wayland-and-x11).

## How to run the automated part

```sh
# Linux (repeat with QT_QPA_PLATFORM=wayland and QT_QPA_PLATFORM=xcb)
export OPENSESH_NO_CRASH_DIALOG=1 QT_QPA_PLATFORM=offscreen
cargo run -p opensesh-app -- --smoke-test; echo "exit=$?"
cargo run -p opensesh-app -- --gallery --smoke-test; echo "exit=$?"
printf 'test report\n' > /tmp/report.txt
cargo run -p opensesh-app -- --crash-report /tmp/report.txt --smoke-test; echo "exit=$?"
```

The exit codes are listed in [`../dev-setup.md`](../dev-setup.md#build-run-and-check).
