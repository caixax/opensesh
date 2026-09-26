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
- [ ] `vault.bin`: versioned header, Argon2id (RFC 9106 parameters, stored in the header), XChaCha20-Poly1305 with the header as associated data, a wrapped random vault key, a documented binary payload
- [ ] Two key holders: master password (derived key) or system keyring (vault key); "remember on this computer"; switching, changing and removing the master password; resetting a vault whose password is lost
- [ ] A `KeyStore` trait with the system keyring and an in-memory store (tests never touch the user's keyring)
- [ ] Locked and unlocked states; lock now; lock after a configurable idle time
- [ ] Backoff after wrong master passwords, kept on disk across restarts
- [ ] Secrets in memory use `secrecy`/`zeroize`; nothing secret in logs, errors or `Debug`
- [ ] Atomic saves without backups (a changed password leaves no copy under the old one)

### Identities and keys
- [ ] `keychain.toml`: identities (name, user, password reference, key) and keys (name, algorithm, public key, fingerprint, comment, private key reference), lenient loading, atomic saves, hot reload
- [ ] Generate ed25519 (default), ECDSA (P-256, P-384, P-521) and RSA-4096
- [ ] Import OpenSSH private keys (with passphrase) and PuTTY PPK v2 and v3 (with passphrase), with fixtures made by `ssh-keygen` and `puttygen`
- [ ] Export the public key (OpenSSH line), copy it, export the private key (OpenSSH, optionally with a new passphrase)
- [ ] Hosts and groups: inheritable `identity`; OpenSSH uses the identity's user name when none is set

### Agents
- [ ] Agent protocol: list identities (request 11, answer 12)
- [ ] Transports: `SSH_AUTH_SOCK` on Unix, the Windows OpenSSH agent pipe, Pageant (WM_COPYDATA)
- [ ] Listing never blocks the GUI thread and has a timeout

### Known hosts (read-only)
- [ ] `~/.ssh/known_hosts` and OpenSesh's `known_hosts`: patterns (hashed ones shown as hashed), markers, key type and SHA256 fingerprint

### App
- [ ] `Keychain` singleton: vault state, identities, keys, agents, known hosts; every vault and key operation on a worker thread
- [ ] Keychain view: Identities, Keys, Agents, Known hosts; empty states; search
- [ ] Dialogs: identity editor, generate key, import key (file or pasted text, passphrase), export, unlock (with the backoff countdown), set, change and remove the master password, reset
- [ ] Settings > Security: how the vault key is kept, master password, remember on this computer, idle lock, lock now
- [ ] Host and group editors: identity picker with inheritance
- [ ] Status bar: vault lock state, click to lock or unlock
- [ ] Command palette: lock the vault, generate a key, new identity

### Documentation
- [ ] `docs/threat-model.md`
- [ ] `docs/vault-format.md`
- [ ] ADRs: the vault and its key holders; the crypto crates; SSH keys, PPK and agents

### Quality
- [ ] Unit tests: format round trip and tamper detection, wrong password, key holders, backoff with a fake clock, keychain file, key generation and import (fixtures), PPK v2/v3, agent protocol and a mock agent on a socket, known hosts
- [ ] **Done when:** a test that walks the data and config folders and shows no secret is there in clear (raw, base64, hex, UTF-16), and the backoff test
- [ ] Smoke tests: keychain view, create an identity with a password, generate a key, lock and unlock with a master password, the backoff message
- [ ] Screenshots of the Keychain view, the unlock dialog and Settings > Security
- [ ] Real keyring checked once on Windows (Credential Manager) and on Linux (Secret Service in a private D-Bus session), with a throwaway entry

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [ ] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [ ] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

(Filled in at the end of the sprint.)
