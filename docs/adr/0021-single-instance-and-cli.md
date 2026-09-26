# ADR 0021: One running instance and the `opensesh` CLI over local sockets

- **Status:** accepted
- **Date:** 2026-09-26
- **Sprint:** 5

## Context

PLAN Sprint 5 asks for a CLI (`opensesh list`, `opensesh connect <name>`, `opensesh open <url>`) and a single instance: a request goes to the running app through local IPC. PLAN §2 asks to evaluate `interprocess` against `QLocalServer` in an ADR. §8 says URL schemes that reach the app from outside always ask before connecting, and nothing listens beyond localhost without a warning.

## Options

1. **`QLocalServer` / `QLocalSocket`** (Qt Network). Native to the app, but the CLI would have to link Qt just to send one line, and Qt Network is one more module to deploy.
2. **`interprocess`** local sockets: Unix domain sockets on Linux, named pipes on Windows, synchronous or async, no Qt. License 0BSD or Apache-2.0.
3. **A TCP port on localhost.** Works everywhere, but any local user can connect, a port can be taken, and firewalls may ask.

## Decision

Option 2 (`interprocess` 2.4), with a separate CLI crate.

- **Protocol** (`opensesh-core::ipc`): one JSON object per line each way, a request (`activate`, `connect {host}`, `open {url}`) and a reply (`ok` or `error {message}`); requests are at most 64 KiB.
- **Endpoint:** on Linux a socket file `opensesh-<tag>.sock` in `$XDG_RUNTIME_DIR` (private to the user), else in the data folder (kept private); on Windows a named pipe `opensesh-<user>-<tag>`. The tag comes from the config folder's path, so a portable copy and an installed one are separate instances.
- **The app:** in normal runs it first sends its request (`--connect`, `--open`, or `activate`) to the endpoint; if an instance answers, it exits. Otherwise it listens in a background thread (one thread per client, so a client that hangs holds only itself; named pipes have no I/O timeouts, so the client side waits in a helper thread instead) and replaces a socket file left by a crash after checking nobody answers. Requests are checked (a target must parse) and queued; QML takes them once the window is ready. `connect` connects to a saved host at once; `open` shows what it would connect to and asks first. Test runs never listen.
- **The CLI** (`opensesh-cli`, binary `opensesh`, no Qt): `list` reads `hosts.toml` and the linked sources itself; `connect` checks the host exists, `open` parses the target, then both send the request, or start the app with it when no instance answers (`opensesh-app` next to the CLI, or `OpenSesh.exe` in the folder above it in the Windows packages). The packages install it: `/usr/bin/opensesh` on Linux, and `bin\opensesh.exe` under the app on Windows (next to `OpenSesh.exe` it would be the same file on a case-insensitive file system; the folder is also the one to add to `PATH`, without exposing Qt's DLLs).

## Consequences

- One window for all requests: a second start, the CLI and (later) `opensesh://` links all go through the same queue and the same confirmation.
- The CLI stays small and can run where Qt isn't loaded.
- On Linux only the user can reach the socket (a private directory). On Windows the pipe has the default security descriptor (only the user, administrators and the system can write to it); another user could create a pipe of that name first, which at worst receives the requests of the user's own CLI. The requests hold no secrets.
- The Windows installer doesn't add `bin\` to `PATH`; the README says how to use `opensesh.exe`.
- Two small dependencies of `interprocess` (`recvmsg`, `doctest-file`) are licensed 0BSD only, a license without conditions that is compatible with GPL-3.0; `deny.toml` now allows it.
