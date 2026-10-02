# ADR 0032: The other terminal protocols, and local shells

- **Status:** accepted
- **Date:** 2026-10-02
- **Sprint:** 12

## Context

PLAN Sprint 12 asks for the rest of the terminal sessions:
- telnet, serial ports, mosh, and shells in Docker or Podman containers and Kubernetes pods;
- any local shell the computer has (PowerShell, cmd, Git Bash, MSYS2, Cygwin, WSL distros, `/etc/shells`).

Each must open from a saved host, from quick connect, and in tests without the network or a device. The terminal engine (ADR 0012) takes any backend that pushes `BackendEvent`s and takes input, sizes and a shutdown.

## Options and decisions

### Where the code lives

**Decision:** a Qt-free crate, `opensesh-proto-misc` (the name PLAN §3.1 gives it), with one module per kind:
- `telnet` and `serial` are terminal backends;
- `mosh` starts one, over the built-in SSH client;
- `containers` lists running containers and pods.

Local shells are found by `opensesh-term::shells`. The app only turns a host or a quick-connect target into a spec (`terminals.rs`).

### Telnet

**Options:** a crate (the telnet crates on crates.io are small, unmaintained, or blocking), or our own.

**Decision:** our own NVT, about 300 lines and tested:
- **Negotiation:** IAC handling and the options a terminal needs: ECHO and SGA, BINARY, NAWS (sent again on each resize) and TTYPE (the profile's `TERM`).
- **Refused:** every other option.
- **Encoding:** Enter as CR LF and `0xFF` doubled.
- **Local echo:** only while the server doesn't echo.

Telnet sends everything in clear, so the pane says so in yellow before connecting, and the host editor does too.

### Serial ports

**Decision:** `serialport` 4.10.1 without its default features. Without `libudev`, ports are found in `/sys/class/tty` on Linux, so the Linux packages don't build against libudev.

**The backend:**
- Reads and writes on threads of its own; a device that goes away ends the session with the reason.
- Enter can send CR (the default, as PuTTY), LF or CR LF; local echo for devices that don't echo.
- The session log works as for SSH.
- **Hexadecimal view:** received bytes as a dump, 16 to a line, with their text. A line is printed when full, or after a short quiet time.
- **Breaks:** sent from the pane's menu.

**The host editor** lists the detected ports with their descriptions, read again every 2 seconds while a serial host is edited.

**In test runs** a loopback "device" stands in for the port: it greets, then echoes what it gets.

### Mosh

**Options:** run the `mosh` Perl script (which needs `ssh` and Perl here), or do its job ourselves.

**Decision:** what the script does, with the built-in client:
1. **Connect** as for an SSH session: the same host keys, identities and questions in the pane.
2. **Start the server:** `mosh-server new -s -c 256 -l LANG=C.UTF-8` on an exec channel with a PTY.
3. **Read** `MOSH CONNECT <port> <key>`, then close the SSH connection.
4. **Run `mosh-client`** here in a PTY, with the server's address and port as arguments.

**The session key** goes only in `MOSH_KEY`: never on a command line, and never shown or logged (its `Debug` leaves it out).

**What is checked first:** `mosh-client` is looked for before connecting, so a missing one doesn't leave a server waiting. A missing `mosh-server` (exit 127, "not found") is reported with how to install mosh, and so is a missing client.

**Limits:**
- Mosh reaches the server straight over UDP: jump hosts and proxies carry only the SSH part, and the pane says so.
- Windows has no `mosh-client` of its own: the message points to Cygwin or a WSL tab.

### Containers and pods

**Options:** the Docker and Kubernetes APIs (bollard, kube-rs: big dependencies, and their own configuration handling), or the tools' own command lines.

**Decision:** the tools, in a local PTY:
- `docker exec -it` and `podman exec -it`, with `--user` from the host's user;
- `kubectl exec -it`, with the namespace, the container in the pod and the kubeconfig context.

Their configuration (Docker contexts, kubeconfig, credentials) applies as on the command line.

**The shell:** unset, a small `sh -c` script runs bash where the container has it, else sh. A host can name another.

**Running containers and pods** are listed through the same tools' JSON output (`docker ps --format '{{json .}}'`, `podman ps --format json`, `kubectl get pods -o json`), in the background, with a 15 s limit:
- the host editor offers them, with a Refresh button;
- quick connect offers them after `docker://`, `podman://` or `kube://`.

Test runs list samples and never run these programs.

**Quick connect:** `docker://[user@]name`, `podman://[user@]name` and `kube://[namespace/]pod[?container=&context=]`.

### Local shells

**On Linux and macOS:** `$SHELL` and `/etc/shells`, without `nologin`, `false` and other non-shells, and one entry per real file.

**On Windows:**
- PowerShell 7, Windows PowerShell and cmd;
- Git Bash, MSYS2 and Cygwin where installed;
- each WSL distribution from `wsl.exe -l -q`, whose UTF-16 output is decoded; `docker-desktop` is left out.

**Where to choose one:**
- a menu on the new tab button, and the command palette;
- a default shell in Settings > Terminal (a terminal setting, so a profile or a local host can set its own).

A pane keeps its shell in workspaces.

## Consequences

- Every kind of terminal session in the plan opens from hosts and quick connect, and is tested without the network or a device:
  - telnet against an in-process server;
  - serial on the loopback;
  - mosh up to the server's answer, on the in-process SSH server;
  - containers by their command lines and listings.
- **Real checks:** a real serial device, mosh to a real server, and Docker, Podman and kubectl are in the manual matrix.
- Mosh and the containers depend on programs the user installs. OpenSesh explains what is missing rather than bundling them.
- **New dependency:** `serialport` (MPL-2.0).
