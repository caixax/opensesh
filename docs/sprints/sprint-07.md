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
- [ ] MSRV 1.89: `rust-toolchain.toml`, `rust-version`, an ADR; everything still builds and passes
- [ ] `opensesh-ssh` with `russh` (ring, flate2, rsa) and tokio; a shared runtime in the app

### Connection
- [ ] Modern algorithms by default; a per-host "legacy" toggle (SHA-1 key exchange and signatures, CBC ciphers, hmac-sha1)
- [ ] Host key checks: `~/.ssh/known_hosts` (hashed names, `[host]:port`, wildcards, `@revoked`) and OpenSesh's own file; first connection shows the SHA256 fingerprint; a changed key blocks with an explicit choice
- [ ] Jump host chains of any length (saved hosts or `user@host:port`), each hop with its own authentication
- [ ] SOCKS5 and HTTP CONNECT proxies, and ProxyCommand (`%h`, `%p`, `%r`), for the first hop
- [ ] Keepalive and compression from the host's options

### Authentication
- [ ] Methods in a configurable order: public key (the identity's vault key, the key file, the agent's keys), keyboard-interactive, password
- [ ] Passwords from the identity, else asked in the pane; key file passphrases asked in the pane
- [ ] Keyboard-interactive prompts (MFA) in the pane, echo per prompt
- [ ] Agents: `SSH_AUTH_SOCK`, the Windows OpenSSH agent and Pageant
- [ ] OpenSSH user certificates (`<key>-cert.pub` next to a key file)

### Session
- [ ] PTY with `TERM` (`xterm-256color`), the pane's size and resizes
- [ ] Environment variables per host, and the locale (`LANG`, `LC_*`)
- [ ] A remote command instead of the shell, or a startup snippet typed once the shell is ready
- [ ] Agent forwarding (off by default, with a warning when turned on)
- [ ] Reconnection: "Disconnected: press Enter to reconnect" in the pane; optional automatic reconnection with backoff
- [ ] Optional session log (clean text or raw) under the data folder
- [ ] OS detection over a separate exec channel (opt-out) for the host's icon
- [ ] "Install my key on the server" (like `ssh-copy-id`), from the host menu
- [ ] The OpenSSH backend stays available per host (`ssh.backend = "openssh"`)

### X11 (Linux spike)
- [ ] `x11-req` with a fake cookie, channels proxied to the local display (Xwayland included), the real cookie from `xauth`; findings in `spikes/x11-forwarding/`

### App
- [ ] Pane overlays: connecting, host key (new, changed, revoked), password, passphrase, keyboard-interactive, errors
- [ ] Host and group editors: authentication order, legacy algorithms, proxy, environment, remote command, reconnection, session log, OS detection; the agent forwarding warning
- [ ] Keychain > Known hosts: search, remove entries from OpenSesh's file, the file's path
- [ ] Settings > SSH: defaults (auth order, keepalive, reconnection, OS detection, session logs folder)

### Quality
- [ ] Unit tests: known hosts matching (hashed, ports, wildcards, negation, revoked), algorithm lists, proxy handshakes against fake servers, ProxyCommand parsing, the session log's text cleaner, OS release parsing
- [ ] In-process server tests: every auth method, a new and a changed host key, keyboard-interactive, a 2-hop chain, agent authentication, reconnection
- [ ] **Done when:** connects to OpenSSH and Dropbear, through 2 jump hosts with an agent, and MFA and reconnection work (Docker in CI, and WSL locally)
- [ ] Smoke tests: an SSH pane against the in-process server (host key card, password prompt, output), reconnection
- [ ] Screenshots: the host key card, a prompt, the disconnected banner, Settings > SSH

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [ ] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [ ] ADRs, docs (threat model), CHANGELOG, report, commits pushed to GitHub

## Report

(Filled in at the end of the sprint.)
