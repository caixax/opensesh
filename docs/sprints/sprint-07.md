# Sprint 7: SSH

**Goal:** robust SSH connections in the terminal.

**Started:** 2026-09-27

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it.
- **`russh` 0.63.3, and the MSRV moves to 1.89.**
  - 0.63.2 and 0.63.3 need Rust 1.89. They fix things a client needs: terminal modes in `pty-req`, sensitive data redacted from debug logs, writes split across a rekey, and inactivity timeouts during stalled writes.
  - 0.63.1 would miss them, so the MSRV rises to 1.89 (ADR, per ADR 0002).
- **The crypto backend is `ring`, not the default `aws-lc-rs`.** `aws-lc-sys` needs NASM or CMake on some platforms; `ring` builds everywhere with the C compiler we already have.
- **Two generations of the key crates.** `russh` uses `ssh-key` 0.7 (release candidate) and `rsa` 0.10 (release candidate); the vault stays on `ssh-key` 0.6. Keys cross the boundary in OpenSSH's encoding. RUSTSEC-2023-0071 is looked at again: the client now signs with RSA keys.
- **A new crate `opensesh-ssh`** (no Qt) holds the connection: host key checks, authentication, jump hosts, proxies and the terminal backend. The app runs one tokio runtime off the GUI thread.
- **Prompts live in the pane, not in modal dialogs:** the host key card, the password and keyboard-interactive prompts, and the key passphrase. Several panes can connect at once, and each asks in its own place. Answers go straight to the connection, never through files.
- **Known hosts:**
  - OpenSesh reads `~/.ssh/known_hosts` (hashed entries included) and its own file.
  - It writes only its own file, in OpenSSH's format.
  - A changed key blocks the connection until the user acts (PLAN §8).
- **Test servers:**
  - An in-process `russh` server in the tests.
  - OpenSSH and Dropbear in Docker in CI: one target and two jump hosts, and TOTP through PAM for MFA.
  - The same setup runs locally in a WSL distro without Docker.
- **X11 forwarding is a spike on Linux** (`spikes/x11-forwarding/`); its robustness is Sprint 15.

## Checklist

### Toolchain and crate
- [x] MSRV 1.89: `rust-toolchain.toml`, `rust-version`, an ADR; everything still builds and passes
- [x] `opensesh-ssh` with `russh` (ring, flate2, rsa) and tokio; a shared runtime in the app

### Connection
- [x] Modern algorithms by default; a per-host "legacy" toggle (SHA-1 key exchange and signatures, CBC ciphers, hmac-sha1)
- [x] Host key checks: `~/.ssh/known_hosts` (hashed names, `[host]:port`, wildcards, `@revoked`) and OpenSesh's own file; first connection shows the SHA256 fingerprint; a changed key blocks with an explicit choice
- [x] Jump host chains of any length (saved hosts or `user@host:port`), each hop with its own authentication
- [x] SOCKS5 and HTTP CONNECT proxies, and ProxyCommand (`%h`, `%p`, `%r`), for the first hop
- [x] Keepalive and compression from the host's options

### Authentication
- [x] Methods in a configurable order: public key (the identity's vault key, the key file, the agent's keys), keyboard-interactive, password
- [x] Passwords from the identity, else asked in the pane; key file passphrases asked in the pane
- [x] Keyboard-interactive prompts (MFA) in the pane, echo per prompt
- [x] Agents: `SSH_AUTH_SOCK`, the Windows OpenSSH agent and Pageant (through the `pageant` crate `russh` already uses)
- [x] OpenSSH user certificates (`<key>-cert.pub` next to a key file)

### Session
- [x] PTY with `TERM` (`xterm-256color`), the pane's size and resizes
- [x] Environment variables per host, and the locale (`LANG`, `LC_*`)
- [x] A remote command instead of the shell, or a startup snippet typed once the shell is ready
- [x] Agent forwarding (off by default, with a warning when turned on)
- [x] Reconnection: "Disconnected: press Enter to reconnect" in the pane; optional automatic reconnection with backoff
- [x] Optional session log (clean text or raw) under the data folder
- [x] OS detection over a separate exec channel (opt-out) for the host's icon
- [x] "Install my key on the server" (like `ssh-copy-id`), from the host menu
- [x] The OpenSSH backend stays available per host (`ssh.backend = "openssh"`)

### X11 (Linux spike)
- [x] `x11-req` with a fake cookie, channels proxied to the local display (Xwayland included), the real cookie from `xauth`; findings in `spikes/x11-forwarding/` (tried onto WSLg's Xwayland, which has no cookie)

### App
- [x] Pane overlays: connecting, host key (new, changed, revoked), password, passphrase, keyboard-interactive, errors
- [x] Host and group editors: authentication order, legacy algorithms, proxy, environment, remote command, reconnection, session log, OS detection; the agent forwarding warning
- [x] Keychain > Known hosts: search, remove entries from OpenSesh's file, the file's path
- [x] Settings > SSH: defaults (auth order, keepalive, reconnection, OS detection, session logs folder)

### Quality
- [x] Unit tests: known hosts matching (hashed, ports, wildcards, negation, revoked), algorithm lists, proxy handshakes against fake servers, ProxyCommand parsing, the session log's text cleaner, OS release parsing
- [x] In-process server tests: every auth method, a new and a changed host key, keyboard-interactive, a 2-hop chain, agent authentication, reconnection
- [x] **Done when:** connects to OpenSSH and Dropbear, through 2 jump hosts with an agent, and MFA and reconnection work (in CI and in WSL, with the servers started natively by `scripts/ssh-test-servers.sh` instead of Docker: see ADR 0027)
- [x] Smoke tests: an SSH pane against the in-process server (host key card, password prompt, output), reconnection
- [x] Screenshots: the host key card, a prompt, the disconnected banner, Settings > SSH

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [x] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [x] ADRs, docs (threat model), CHANGELOG, report, commits pushed to GitHub

## Report

### What was done

- **`opensesh-ssh`** (no Qt, [ADR 0027](../adr/0027-ssh-client.md)) on `russh` 0.63.3 with `ring` (MSRV 1.89, [ADR 0026](../adr/0026-msrv-1.89-for-russh.md)), on one tokio runtime off the GUI thread.
  - **Algorithms:** modern ones by default; a per-host legacy set (SHA-1 key exchange and signatures, CBC, 3DES, `hmac-sha1`).
  - **Host keys:** OpenSesh's `known_hosts` first, then `~/.ssh/known_hosts` (hashed names checked with HMAC-SHA1 against real `ssh-keygen -H` output, `[host]:port`, wildcards, negation, `@revoked`). Trusted keys are written to OpenSesh's file only. A changed key stops the connection until the user decides. A key trusted once is kept for that connection's reconnections.
  - **Hops:** jump hosts of any length through `direct-tcpip`, each with its own host key check and authentication. SOCKS5 and HTTP CONNECT (with a user name) and ProxyCommand (`%h`, `%p`, `%r`) for the first hop.
  - **Authentication:** a `none` probe, then the host's order. Public keys in order: the identity's vault key, the key file (a passphrase asked in the pane, a `-cert.pub` certificate used), the agents, then OpenSSH's default key files. Keyboard-interactive and passwords follow, and partial success (MFA) goes on with what the server still wants. Secrets come from the keychain worker when a connection authenticates (`SecretSource`); a locked vault says so.
  - **Agents:** `SSH_AUTH_SOCK`; on Windows a named pipe, else the OpenSSH agent and then Pageant. Agent forwarding only when the host asks for it.
  - **The terminal backend:** PTY, `TERM`, resizes, environment and locale, a remote command or a startup snippet, and a session log (text without escape codes, or raw). A lost connection shows why and waits for Enter, or retries by itself with backoff (1, 2, 4, 8, 16 s). A remote exit ends the session with its code.
  - **OS detection** and **"install my key"** run on channels of their own beside the shell.
  - **A test server** (`opensesh_ssh::testing`) for the tests and the app's smoke test.
- **App:**
  - **SSH panes** use the built-in client unless the host chooses OpenSSH. `SshOverlay` shows a status chip while connecting, a card for each question (new or changed host key, password, passphrase, keyboard-interactive), and a banner when disconnected (Reconnect, Unlock and connect, the countdown of automatic reconnection). The status bar shows the connection's state.
  - **Hosts:** the detected OS (`detected-os.toml` in the data folder) shows while a host's icon is automatic. "Install my key…" in the host menu.
  - **Editors:** authentication order, agent socket, legacy algorithms, proxy, proxy command, environment (`NAME=value; ...`), language settings, remote command, startup snippet, reconnection, session log and OS detection, in the host and group editors.
  - **Settings > SSH** (`[ssh]` in `config.toml`): the defaults of every host and quick connection, between the groups and the built-in values, and the session logs folder.
  - **Keychain > Known hosts:** hashed names found by their full host name, entries of OpenSesh's file removable, and which file OpenSesh writes.
- **Real servers:** `scripts/ssh-test-servers.sh` and `tests/real_servers.rs` ([how](../testing/ssh-servers.md)), in a new CI job and in WSL.
- **X11 spike:** [`spikes/x11-forwarding`](../../spikes/x11-forwarding/README.md).
- **Documentation:** ADRs 0026 and 0027 (ADR 0022 updated), the threat model, the testing notes, CHANGELOG and README.

### "Done when" (PLAN)

- **Connects to OpenSSH and Dropbear: yes.** OpenSSH 10.5p1 and Dropbear 2026.94 in WSL (Arch), and the Ubuntu 24.04 packages in CI: a key file and a password, a wrong password refused, and a user certificate.
- **Through 2 jump hosts with an agent: yes.** OpenSSH, then Dropbear, then OpenSSH, every hop authenticated by `ssh-agent`; the in-process tests add three hops and OS detection through them.
- **MFA works: yes.** The target asks for a TOTP code through PAM (`pam_google_authenticator`) after the key; the code comes from `oathtool`.
- **Reconnection works: yes.** Through Dropbear to the MFA target, the session's `sshd-session` is killed: the backend reports the connection lost, Enter reconnects (with a new code), and `exit 0` ends with code 0. The in-process test covers it too, with the session log.

### How it was verified

- **Windows (Qt 6.10.3), on the final code:**
  - fmt, clippy `-D warnings`, lint-qml, i18n, shaders, deny and audit.
  - Every test: 517 passed, 23 ignored (real servers, real agents and keyrings).
  - The main smoke test offscreen (270 steps) and native (264), and the gallery. The smoke test drives an SSH pane against the in-process server: the host key card, a wrong then a right password, the remote shell, a dropped connection and Enter, and "install my key" from its dialog.
  - Screenshots: the host key card (new and changed), a one-time code, the disconnected banner and Settings > SSH, in dark and light, comfortable and compact.
  - Pageant 0.83 (the official build, checked against PuTTY's SHA-256 list) holding a fixture key signed for the client (the Windows OpenSSH agent service is stopped on this machine).
- **Real servers in WSL (Arch):** the four tests of `real_servers.rs` pass, and `stop` leaves no user, PAM block or process behind.
- **WSL, on the final code:** clippy, every test (519 passed, 28 ignored) and the smoke tests (offscreen, gallery, Wayland, X11) in Debian 13 (Qt 6.8.2), Fedora 43 (Qt 6.10.3) and Arch (Qt 6.11.2). `ssh-agent` with a fixture key in Debian and Fedora.
  - Arch's clippy build found a Qt bug: qmlcachegen 6.11 copies the carriage return of `"\r"` in `SshOverlay.qml` into the generated C++ as is, and GCC stops there. The file generated for clippy differs from the one of a plain build, which is why the build and CI didn't hit it. `String.fromCharCode(13)` avoids it.
- **GitHub Actions:** green on Ubuntu 24.04, Windows, the Arch, Fedora and Debian 13 containers, and the new `ssh` job against OpenSSH and Dropbear.
  - The first runs of the sprint failed on Ubuntu's clippy and then on rustfmt: Rust 1.89's `collapsible_if` in the Unix PTY code, which clippy on Windows doesn't build. Both were fixed.

### Pending

- **X11 forwarding** in the app (Sprint 15), from the spike's findings.
- **Proxy passwords:** a proxy user name is sent as written; no password is asked for yet.
- **Host certificates** (`@cert-authority`) are listed but don't make a host trusted yet.
- **Hardware keys and Kerberos:** through the OpenSSH backend.
- **Manual matrix** ([manual-matrix.md](../testing/manual-matrix.md)): real hosts of the owner (a Cisco-like device with legacy algorithms, a bastion), a proxy in daily use, the Windows OpenSSH agent service with keys.
- **Carried over:** the Sprint 6 manual checks, "Confirm before closing with active sessions", the Windows executable icon, an Ubuntu package.

### Risks

- **RUSTSEC-2023-0071:** the client signs with RSA keys in-process (vault keys and key files); accepted with its reasons in ADR 0027 and reviewed each sprint.
- **Two `ssh-key` and `rsa` generations** until `ssh-key` 0.7 is final; keys cross in OpenSSH's encoding.
- **Slow first logins** (a PAM session, systemd in WSL) can take seconds; OS detection waits up to 15 s beside the shell.
- **qmlcachegen and control characters:** a JavaScript string with a carriage return in a function qmlcachegen compiles to C++ breaks GCC builds with Qt 6.11. The one in `SshOverlay.qml` is gone; the smoke steps that type `\r` are not compiled to C++.
- **Unix-only code** isn't linted by clippy on Windows: the WSL runs (or CI) catch it, as they did this sprint.
