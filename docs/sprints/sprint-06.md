# Sprint 6: Keychain and vault

**Goal:** serious, comfortable secret management.

**Started:** 2026-09-26

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it.
- **One encrypted file, two ways to hold its key.** Secrets always live in `vault.bin`, encrypted with a random vault key. With a master password, the vault key is wrapped with a key derived from it (Argon2id). Without one, the vault key is kept in the system keyring (Secret Service, which includes KWallet's Secret Service interface, on Linux; Credential Manager on Windows). A keyring entry per secret doesn't fit: Windows credentials hold at most 2560 bytes, less than an RSA-4096 private key. "Remember on this computer" keeps the vault key in the keyring as well. An ADR records it.
- **Metadata stays readable.** Identities and keys are listed in `keychain.toml` in the config folder: names, users, public keys, fingerprints, and `vault:<ulid>` references. Passwords, passphrases and private keys only exist inside `vault.bin`.
- **Imported keys are stored decrypted inside the vault.** The vault protects them; exporting can set a new passphrase.
- **Hosts and groups get an inheritable `identity`** (an identity id). Until the built-in SSH client (Sprint 7), OpenSSH only uses the identity's user name; its password and key are for the built-in client.
- **Known hosts are read-only this sprint:** the list of `~/.ssh/known_hosts` and OpenSesh's own file, with fingerprints. Verifying and editing come with Sprint 7.
- **Crypto crates of one generation:** `ssh-key` 0.6 (stable) sets the RustCrypto generation (`rand_core` 0.6, `digest` 0.10, `cipher` 0.4), so the vault uses `argon2` 0.5 and `chacha20poly1305` 0.10, and the PPK reader `aes` 0.8, `cbc` 0.1, `hmac` 0.12, `sha1` 0.10 and `sha2` 0.10.
- **Pageant without the `pageant` crate:** it needs tokio and the `windows` crate. A small WM_COPYDATA client over `windows-sys` is enough to list keys.

## Checklist

### Vault (`opensesh-vault`)
- [x] `vault.bin`: versioned header, Argon2id (RFC 9106 parameters, stored in the header), XChaCha20-Poly1305 with the header as associated data, a wrapped random vault key, a documented binary payload
- [x] Two key holders: master password (derived key) or system keyring (vault key); "remember on this computer"; switching, changing and removing the master password; resetting a vault whose password is lost
- [x] A `KeyStore` trait with the system keyring and an in-memory store (tests never touch the user's keyring)
- [x] Locked and unlocked states; lock now; lock after a configurable idle time
- [x] Backoff after wrong master passwords, kept on disk across restarts
- [x] Secrets in memory use `secrecy`/`zeroize`; nothing secret in logs, errors or `Debug`
- [x] Atomic saves without backups (a changed password leaves no copy under the old one)

### Identities and keys
- [x] `keychain.toml`: identities (name, user, password reference, key) and keys (name, algorithm, public key, fingerprint, comment, private key reference), lenient loading, atomic saves, hot reload
- [x] Generate ed25519 (default), ECDSA (P-256, P-384, P-521) and RSA-4096
- [x] Import OpenSSH private keys (with passphrase) and PuTTY PPK v2 and v3 (with passphrase), with fixtures made by `ssh-keygen` and `puttygen`
- [x] Export the public key (OpenSSH line), copy it, export the private key (OpenSSH, optionally with a new passphrase)
- [x] Hosts and groups: inheritable `identity`; OpenSSH uses the identity's user name when none is set

### Agents
- [x] Agent protocol: list identities (request 11, answer 12)
- [x] Transports: `SSH_AUTH_SOCK` on Unix, the Windows OpenSSH agent pipe, Pageant (WM_COPYDATA)
- [x] Listing never blocks the GUI thread and has a timeout

### Known hosts (read-only)
- [x] `~/.ssh/known_hosts` and OpenSesh's `known_hosts`: patterns (hashed ones shown as hashed), markers, key type and SHA256 fingerprint

### App
- [x] `Keychain` singleton: vault state, identities, keys, agents, known hosts; every vault and key operation on a worker thread
- [x] Keychain view: Identities, Keys, Agents, Known hosts; empty states; search
- [x] Dialogs: identity editor, generate key, import key (file or pasted text, passphrase), export, unlock (with the backoff countdown), set, change and remove the master password, reset
- [x] Settings > Security: how the vault key is kept, master password, remember on this computer, idle lock, lock now
- [x] Host and group editors: identity picker with inheritance
- [x] Status bar: vault lock state, click to lock or unlock
- [x] Command palette: lock the vault, generate a key, new identity

### Documentation
- [x] `docs/threat-model.md`
- [x] `docs/vault-format.md`
- [x] ADRs: the vault and its key holders; the crypto crates; SSH keys, PPK and agents

### Quality
- [x] Unit tests: format round trip and tamper detection, wrong password, key holders, backoff with a fake clock, keychain file, key generation and import (fixtures), PPK v2/v3, agent protocol and a mock agent on a socket, known hosts
- [x] **Done when:** a test that walks the data and config folders and shows no secret is there in clear (raw, base64, hex, UTF-16), and the backoff test
- [x] Smoke tests: keychain view, create an identity with a password, generate a key, lock and unlock with a master password, the backoff message
- [x] Screenshots of the Keychain view, the unlock dialog and Settings > Security
- [x] Real keyring checked once on Windows (Credential Manager) and on Linux (Secret Service in a private D-Bus session), with a throwaway entry

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [ ] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [ ] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

### What was done

- **Vault** (`opensesh-vault`, [ADR 0023](../adr/0023-vault-and-key-holders.md), [format](../vault-format.md)): passwords and private keys live in `vault.bin`, sealed with XChaCha20-Poly1305 under a random vault key.
  - **Who holds the key:** the system keyring (Credential Manager, the Secret Service) or a master password (Argon2id, 64 MiB, 3 passes, 4 lanes, stored in the file) that wraps it. "Remember on this computer" keeps the vault key in the keyring too.
  - **Changing protection:** setting, changing and removing the master password only rewrap the key.
  - **Integrity:** the header is authenticated with the body.
  - **Writes and damage:** writes are atomic and keep no backups. A damaged or newer file is kept until the user resets it.
  - **Tests:** a `KeyStore` trait has the real keyring and an in-memory store for tests and test runs.
- **Locking and waits:**
  - Locking forgets the key: by hand, or after `security.lock_after_minutes` without input in any window (an application event filter).
  - After 3 wrong master passwords, attempts wait 5 s, doubling to 5 min, kept in `vault-attempts.toml`. A clock set back doesn't extend the wait.
- **Keychain** (`keychain.toml`): identities (user, password reference, key) and keys (public key, fingerprint, private key reference), readable, lenient, hot-reloaded. Values that look like secrets are dropped without being quoted. Hosts and groups get an inheritable `identity` (the editors' `none` turns it off). OpenSSH uses the identity's user name when the host has none.
- **SSH keys** ([ADR 0024](../adr/0024-crypto-crates-and-ssh-keys.md)):
  - Generate Ed25519, ECDSA P-256/P-384/P-521 and RSA-4096 with `ssh-key`.
  - Import OpenSSH keys (with a passphrase), and PuTTY `.ppk` versions 2 and 3 with our own reader (Argon2i/d/id, AES-256-CBC, the MAC checked first).
  - Old PEM keys are refused with advice to convert them.
  - Export the public key or the private key (optionally with a new passphrase).
- **Agents** ([ADR 0025](../adr/0025-ssh-agents.md)): the agent protocol, over a Unix socket, a named pipe and Pageant's WM_COPYDATA, with timeouts; no async runtime.
- **Known hosts:** `~/.ssh/known_hosts` and OpenSesh's own file, read-only (hashed names, markers, SHA256 fingerprints).
- **App:**
  - **Singletons:** `Keychain` (a worker thread owns the vault and the file; QML gets tokens and public snapshots), and `KeychainTasks` (callbacks and messages).
  - **The Keychain view:** the vault card; Identities, Keys, Agents and Known hosts with search, menus and empty states.
  - **Dialogs:** unlock (with the countdown), master password (create, set, change, remove), reset, identity editor, generate, import (file or paste) and export.
  - **Settings > Security.**
  - **Identity pickers** in the host and group editors.
  - **Vault lock:** in the status bar, and five palette actions.
  - **`OsDialog.closeOnAccept`** for dialogs that finish later.
- **Documentation:** [threat model](../threat-model.md), [vault format](../vault-format.md), ADRs 0023 to 0025, the component contract, README and CHANGELOG.

### "Done when" (PLAN)

- **A test walks the data and config folders and shows there are no secrets in clear: yes.** `crates/opensesh-vault/tests/no_plaintext.rs` does this:
  - It sets, replaces and clears identity passwords.
  - It generates every key type and imports encrypted OpenSSH and PuTTY keys.
  - It moves the vault from the keyring to a master password, changes the password, remembers and forgets it, and tries wrong passwords.
  - Then it searches every file for each secret: passwords, passphrases, the vault key, each key's private numbers and encoding. It looks for them raw, as hex in both cases, as base64 from each of the three alignments, and as UTF-16.
  - A second test plants a secret in each of those forms and checks that the search finds it.
- **The backoff works: yes.**
  - Unit tests with a fake clock cover the delays, a restart and a clock set back.
  - The vault's tests show that even the right password isn't tried while waiting.
  - The same test reopens the folders and finds the wait still running.
  - The smoke test gets the wait after three wrong passwords, through the app.

### How it was verified

- **Windows (Qt 6.10.3), on the final code:**
  - fmt, clippy `-D warnings`, lint-qml, i18n, shaders, deny and audit.
  - Every test: 481, including the "done when" test and the PuTTY fixtures.
  - The main smoke test offscreen and native (237 to 252 steps, with the keychain steps), and the gallery.
  - Screenshots of the four keychain sections, the unlock dialog and Settings > Security, in dark and light, comfortable and compact.
- **Real system services, with throwaway entries and keys:**
  - Credential Manager (the whole vault flow; no entry left afterwards).
  - Pageant 0.83 holding a fixture key.
  - On Debian, GNOME Keyring in a private D-Bus session, and `ssh-agent` with `ssh-add`.
  - On Fedora, `ssh-agent` with `ssh-add`.
  - A stand-in agent on a named pipe covers the Windows OpenSSH agent, whose service is disabled on this machine.
- **WSL:**
  - Debian 13 (Qt 6.8.2) passed clippy, tests and the smoke tests (offscreen, gallery, Wayland, X11) on the keychain as first committed.
  - Fedora 43 (Qt 6.10.3) passed them on everything but the smoke test's final unlock (a QML-only change).
  - Arch couldn't run: the drive that holds the WSL disks filled up, and the Arch and Debian disks went read-only with I/O errors. The CI containers (Arch, Fedora, Debian 13) and Ubuntu cover Linux on the final code (see the [matrix](../testing/manual-matrix.md)).

### Pending

- **Using identities to connect** is the SSH client's job (Sprint 7): with OpenSSH, only the identity's user name is used; its password and key aren't.
- **Known hosts** are read-only until Sprint 7 (verifying, adding, removing).
- **Not supported:** old PEM keys (the error says how to convert them) and hardware keys (FIDO, PKCS#11).
- **Host cards** show the host's or the group's user, not the identity's.
- **Manual matrix** ([manual-matrix.md](../testing/manual-matrix.md)):
  - the app with the real keyring through its dialogs (Windows, a GNOME and a KDE session);
  - the Windows OpenSSH agent service;
  - the idle lock in daily use;
  - importing a real user's keys.
- **Carried over:** "Confirm before closing with active sessions" is not wired; the open low-severity review items of Sprint 2; the Windows executable icon; an Ubuntu package.

### Risks

- **RUSTSEC-2023-0071** (`rsa`, no fixed release) is accepted because RSA is only used locally; Sprint 7 signs with it and must look again.
- **A keyring that asks the user to unlock it** holds the keychain worker until answered. The UI stays responsive and shows the operation as busy.
- **Memory:** what the user types into password fields lives in Qt strings, which aren't wiped (documented in the threat model).
- **No keyring (servers, WSL, minimal desktops):** secrets need a master password; the dialogs offer it.
