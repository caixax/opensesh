# Threat model

This document says what OpenSesh protects, from whom, how, and where it stops. It is kept up to date as features arrive (PLAN §8). It was last reviewed in Sprint 6 (the vault and the keychain).

## What is worth protecting

| Asset | Where it lives |
|---|---|
| Saved passwords (identities) | `vault.bin`, encrypted |
| Private SSH keys | `vault.bin`, encrypted |
| The master password | only in the user's head; typed into the unlock dialog |
| The vault key | the system keyring (without a master password, or "remembered"), or wrapped by the master password in `vault.bin` |
| Passphrases of imported or exported keys | typed once; never stored |
| Hosts, users, identities' names, public keys, known hosts | readable TOML files (`hosts.toml`, `keychain.toml`, `known_hosts`) |

Hosts and public keys are not secret, but they are private: they show where the user connects and as whom. They are kept readable on purpose (PLAN §0, local-first), like OpenSSH's own `~/.ssh/config` and `known_hosts`.

## Trust boundaries

- **The user's operating system account.** OpenSesh runs as the user and trusts the account, its file permissions and its keyring. Anything running as the same user is out of reach of the protections below, except where a master password is set and the vault is locked.
- **The config and data folders.** Their contents may be backed up, synced or copied elsewhere. They must be safe to lose without losing secrets in clear.
- **The system keyring.** On Windows, Credential Manager (protected by DPAPI with the user's login). On Linux, the Secret Service: GNOME Keyring, or KWallet through its Secret Service interface, usually unlocked at login.
- **The network.** Nothing in this sprint talks to the network. Host key checks and the SSH client arrive in Sprint 7.

## Adversaries and defenses

### Someone with a copy of the data folder

For example a backup, a synced folder or a stolen disk.

- Secrets are only in `vault.bin`, encrypted with XChaCha20-Poly1305. Nothing else holds them: this is checked by a test that exercises every kind of secret and then searches every file in the folders for each one, raw, hex, base64 and UTF-16 (`crates/opensesh-vault/tests/no_plaintext.rs`).
- **With a master password,** they must guess it. Each guess costs one Argon2id derivation: 64 MiB, 3 passes (RFC 9106, second recommendation). Weak passwords remain guessable offline. The dialog requires at least 8 characters.
- **Without a master password,** the vault key is not in the folder; it is in the system keyring. The copy alone is useless. With the user's keyring too (for example a full disk and the login password), it opens.
- The file is authenticated: a changed byte anywhere makes it unreadable, never silently different.

### Other users of the same computer

- On Linux, every file OpenSesh writes is created with mode `0600`, and the instance socket lives in the private `$XDG_RUNTIME_DIR`.
- On Windows, the files are in the user's profile. The keyring entries are per user.

### Programs running as the user (malware)

These are **not** stopped in general: they can read the keyring, the process memory, the keyboard and the screen.

- **A master password without "remember"** limits the exposure: while the vault is locked, the key is not on the computer at all.
- **The keyring and "remember" modes** trade that for convenience: the key is one keyring query away. On Windows that query needs no prompt. On Linux, an unlocked collection answers any program of the session.

Settings > Security says this in words.

### Someone at an unlocked computer

- **Idle lock:** a vault with a master password locks after a configurable time without input in any OpenSesh window (15 minutes by default). Locking forgets the vault key and the decrypted secrets.
- **Lock now:** from the status bar, the command palette and Settings.
- **Waits after wrong master passwords:** after 3 wrong passwords, each further one makes the next attempt wait 5 seconds, doubling up to 5 minutes. The count is kept in `vault-attempts.toml`, so restarting the app doesn't reset it. A clock set back doesn't extend the wait.
  - This slows guessing through the app only. Someone who can delete that file can also copy `vault.bin` and guess offline, where Argon2id is the limit.
- Changing or removing the master password asks for the current one, with the same waits.

### Tampering with files

- **`vault.bin`** is authenticated as a whole: header and body.
- **`keychain.toml` and `hosts.toml` are not.** Someone who can write them can rename entries, point a host at another identity, or change a host's address.
  - They can't read secrets this way. An identity's key is used from the vault, and its public half is derived from it, not trusted from the file.
  - Connecting to a changed address is caught by host key verification (Sprint 7).
- A value in `keychain.toml` that looks like a secret instead of a `vault:` reference is dropped when read, and never written back. Warnings about the file never quote its values.

### Leaks through the app itself

- **Logs, crash reports and error messages** never contain secrets. Errors carry codes and file names. Types that hold secrets print nothing in `Debug`.
- **The vault is written without backups:** a backup would keep deleted secrets, or a copy sealed under an old master password.
- **Exported private keys** are written by the keychain worker straight to the chosen file (`0600` on Linux), optionally with a passphrase. They never pass through the UI or the clipboard. Only public keys are copied to the clipboard.
- **Test runs** (smoke tests, screenshots) keep the vault in memory and never touch the user's keyring, agents or files.

## Memory

- Keys and decrypted secrets in Rust are wiped when dropped (`zeroize`). Buffers are sized once so that no reallocated copy is left behind.
- **Limits:**
  - Text typed into the UI (passwords, passphrases, a pasted key) lives in Qt strings for a while, which are not wiped.
  - The keyring libraries and the OS make their own copies.
  - Memory can be swapped to disk, and nothing is locked in RAM (`mlock`).
  - The cipher libraries keep round keys that aren't wiped everywhere.

## Agents

OpenSesh lists the keys of the agents it finds: `SSH_AUTH_SOCK`, the Windows OpenSSH agent's named pipe, and Pageant through WM_COPYDATA. It only reads public keys. The agents are trusted as the user's own, as OpenSSH trusts them. On Windows, a named pipe or a Pageant window could belong to another program of the same user, which is inside the boundary above.

## Dependencies

- Every version is pinned and locked. `cargo deny` and `cargo audit` run in CI.
- **RUSTSEC-2023-0071** (the `rsa` crate, the Marvin timing attack) has no fixed version. OpenSesh uses RSA only locally so far (generating, reading and writing keys), which the advisory considers safe ([ADR 0024](adr/0024-crypto-crates-and-ssh-keys.md)).
  - To revisit in Sprint 7, when the SSH client signs with RSA keys.

## Known gaps and future work

- Host keys are shown read-only; verification is Sprint 7.
- No hardware keys (FIDO, PKCS#11) yet. The OpenSSH backend covers them until then.
- The Windows instance pipe (ADR 0021) keeps the default security descriptor.
- Syncing the data folder between devices (Sprint 16) will need a look at `vault.bin` merges and keyring-held vaults, which don't open on another computer.
