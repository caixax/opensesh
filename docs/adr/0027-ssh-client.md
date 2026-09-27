# ADR 0027: The built-in SSH client

- **Status:** accepted (the built-in client becomes the default of [ADR 0022](0022-openssh-until-the-built-in-client.md))
- **Date:** 2026-09-27
- **Sprint:** 7

## Context

PLAN §2 and §3 put SSH on `russh`, off the GUI thread, with prompts, jump hosts, proxies, agents,
reconnection and the system `ssh` as an optional backend. Sprint 5 connected hosts through
OpenSSH in a PTY (ADR 0022) and Sprint 6 built the vault and the keychain (ADR 0023, 0024,
0025). This ADR records how the client is put together.

## Options and decisions

### The library and its crypto

`russh` 0.63.3 (ADR 0026) with `default-features = false` and the features `ring`, `flate2`,
`rsa` and `des`.

- **`ring`, not the default `aws-lc-rs`:** `aws-lc-sys` needs NASM or CMake on some platforms,
  and `ring` builds with the C compiler every target already has.
- **`des`** only for the legacy `3des-cbc` cipher, offered when a host turns legacy algorithms on.
- **Modern algorithms by default:** no SHA-1 key exchange or `ssh-rsa` signatures, no CBC, no
  `hmac-sha1`. A per-host "legacy algorithms" switch adds them back for old servers.

### Two generations of the key crates

`russh` uses `ssh-key` 0.7 (a release candidate, with `rsa` 0.10.0-rc); the vault stays on
`ssh-key` 0.6 (ADR 0024). Options: move the vault to the release candidate, or keep both and
convert at the boundary. **Both, with keys crossing in the OpenSSH binary encoding**
(`PrivateKey::from_bytes`), in zeroized buffers. The vault moves when `ssh-key` 0.7 is final.

### Where it runs

A crate without Qt, `opensesh-ssh`, and **one tokio runtime for every connection** (two worker
threads, started on first use). The terminal backend implements the engine's `TerminalBackend`,
so an SSH pane is a local pane with another backend: same rendering, scrollback, search,
broadcast and splits. Output goes through the engine's bounded channel.

### Questions go to the pane

Host keys, passwords, key passphrases and keyboard-interactive prompts could be modal dialogs or
part of the pane. **The pane:** several panes connect at once and each asks in its own place, a
split or a background tab keeps its question until the user gets to it, and nothing blocks the
window. The connection asks through a callback (`Asker`); the pane's registry entry keeps the
questions, and the answer goes straight back to the waiting connection. Typed secrets become
`SecretString` at once; a question that is dropped (a closed pane) counts as cancelled.

### Secrets when they are needed

A host names an identity. Its password and key are not copied into the connection: the
connection asks the keychain worker for them when it authenticates (`SecretSource`). A vault
that is locked stays locked until a connection needs it; then, if the keyring holds its key, it
opens, and otherwise the pane says "the vault is locked" with **Unlock and connect**.

### Host keys

Checked against OpenSesh's `known_hosts` first, then `~/.ssh/known_hosts` (hashed names,
`[host]:port`, wildcards, negation and `@revoked`; a file that knows the host decides). Keys the
user trusts go to OpenSesh's file, in OpenSSH's format; `~/.ssh/known_hosts` is never written.
A revoked key is refused. A changed key stops the connection with a card that says so (PLAN §8):
"Don't connect" is the default; "Connect once" and "Replace the saved key" are explicit. A key
trusted once is kept in memory for that connection's reconnections, never written.

### Authentication

The methods in the host's order (default: public key, keyboard-interactive, password), after a
`none` request that learns what the server offers. Public keys in this order: the identity's
key from the vault, the key file (with its `-cert.pub` certificate if there is one), the agents,
then OpenSSH's default key files. Agents: `SSH_AUTH_SOCK` on Unix; on Windows a named pipe from
the settings or `SSH_AUTH_SOCK`, else the Windows OpenSSH agent and then Pageant. Partial
success (a key, then a one-time code) goes on with the methods the server still wants.

### Sessions

A PTY with `TERM` and the pane's size, the host's environment and the locale, the shell or a
remote command, and a startup snippet typed once the shell has printed something. OS detection
and "install my key" run on channels of their own beside the shell. When the connection is
lost (not when the shell exits), the pane says why and **Enter reconnects**; hosts can reconnect
by themselves with backoff (1, 2, 4, 8, 16 seconds). Failures retrying won't fix (host key,
authentication, a cancelled question, a locked vault) never retry by themselves.

### The app's defaults

Settings > SSH (`[ssh]` in `config.toml`) sits between the built-in values and the groups: a
host takes its own value, else its nearest group's, else the app's, else the built-in one.
Quick connections, which have no group, take the app's.

### RSA signatures and RUSTSEC-2023-0071

The `rsa` crate (0.9 in the vault, 0.10.0-rc in `russh`) has no release that fixes the Marvin
timing attack. The client now signs with RSA keys from the vault and key files, which a server
can time. Options: refuse RSA keys in-process (only agents sign with them), or accept.
**Accepted, and reviewed each sprint:** the client never decrypts with RSA (Marvin's target is
PKCS#1 v1.5 decryption), each connection makes one signature over data that includes the
server's session id, new keys are Ed25519 by default, and keys in an agent (the OpenSSH agent,
Pageant) are signed outside OpenSesh. `deny.toml` and `.cargo/audit.toml` carry this reason.

### Testing against real servers

Docker in CI, as the sprint plan said, or the servers themselves. **The servers themselves:**
`scripts/ssh-test-servers.sh` starts OpenSSH and Dropbear on 127.0.0.1 with their own keys and
configuration, the same way on the CI runner and in a WSL distro, where Docker isn't installed.
See `docs/testing/ssh-servers.md`.

## Consequences

- SSH hosts connect with the built-in client unless they (or their group, or Settings > SSH)
  choose `openssh`, which still runs the system `ssh` in a PTY (ADR 0022) for Kerberos, smart
  cards and `Match exec`.
- X11 forwarding is not in the built-in client yet: `ClientHandler` refuses `x11` channels. The
  spike in `spikes/x11-forwarding` shows how (Sprint 15).
- Two `ssh-key` and two `rsa` versions are built until `ssh-key` 0.7 is final.
- Proxy passwords are not asked for yet: a proxy user name is sent as written.
