# ADR 0023: One encrypted vault, its key held by the keyring or a master password

- **Status:** accepted
- **Date:** 2026-09-26
- **Sprint:** 6

## Context

PLAN Sprint 6 and §8 set these requirements:

- The master password is optional.
- Without one, secrets go to the system keyring (Secret Service or KWallet on Linux, Credential Manager on Windows).
- "Remember on this computer" keeps the derived key in the keyring (opt-in).
- The vault uses Argon2id and XChaCha20-Poly1305, in a versioned, documented format.
- The vault locks after inactivity, and waits after wrong passwords.
- Secrets are never written in clear.

Facts that shaped the decision:

- Windows credentials hold at most 2560 bytes (`CRED_MAX_CREDENTIAL_BLOB_SIZE`). An RSA-4096 private key is about 3.3 KB in OpenSSH's encoding.
- A keyring can be locked, missing (a server, WSL, a minimal desktop) or slow to answer (it may prompt the user).
- The `keyring` crate (4.x) is maintained and uses the platform stores. It works without system libraries on Linux: it speaks D-Bus through `zbus`, with the Rust crypto for the Secret Service session.

## Options

1. **Each secret as its own keyring entry** without a master password, and an encrypted file with one. That makes two storage formats. RSA keys don't fit on Windows. Moving between the modes rewrites every secret, and a backup of the data folder misses the keyring half.
2. **One encrypted file always,** its random vault key held either by the keyring or wrapped by a key derived from the master password.
3. **The master password required.** It is simple, but PLAN makes it optional.

## Decision

Option 2.

- **`vault.bin`** in the data folder, format 1 ([vault-format.md](../vault-format.md)):
  - A header with the vault id and who holds the key.
  - For a master password: the Argon2id costs and salt, and the vault key sealed with the derived key.
  - The secrets sealed with the vault key.
  - The header is the associated data of both seals.
- **Two holders:**
  - **System keyring:** one entry per vault (service `cc.caixa.OpenSesh`, account `vault-<id>`) holds the 32-byte vault key.
  - **Master password:** Argon2id (64 MiB, 3 passes, 4 lanes; stored in the file) unwraps the vault key. "Remember on this computer" also stores the vault key in the keyring, so the vault opens without the password there. We remember the vault key, not the derived key: it survives a change of password, and unlocks directly.
- **Switching holders** only rewraps the vault key: setting, changing or removing the master password, and remembering or forgetting.
  - Changing or removing asks for the current password.
  - Removing it needs a working keyring.
  - Forgetting deletes the keyring entry. If that deletion fails, the vault says it is still remembered, and the error is shown.
- **The first secret** creates a vault held by the keyring. Where there is no keyring, the user is asked to set a master password first.
- **Locking** (a master password without "remember"):
  - Manually, from the status bar, the command palette or Settings.
  - After `security.lock_after_minutes` (default 15, 0 = never) without input in any OpenSesh window. A Qt application event filter notes the input.
  - Locking drops the key and the decrypted secrets.
- **Waiting after wrong passwords:**
  - 3 free attempts, then 5 s doubling to 5 min.
  - Kept in `vault-attempts.toml`, so restarting doesn't reset it.
  - While waiting, even the right password isn't tried.
- **Writes** are atomic, without backups (a backup would keep deleted secrets, or old passwords).
- **A vault that can't be read** is kept as it is: damaged, or from a newer OpenSesh. "Reset" deletes it, and its keyring entry, on request.
- **Identities and keys** are listed in `keychain.toml` (config folder), readable like `hosts.toml`. Secrets appear there only as `vault:<ulid>` references. Hosts and groups name an identity by id (inherited like the other fields).
- **Threads:** the vault and `keychain.toml` belong to one worker thread in the app. Every operation (Argon2id, RSA, the keyring) runs there; the GUI gets public snapshots.
- **Tests and test runs** use an in-memory key store and never touch the user's keyring.
  - A test that is `#[ignore]`d by default checks the real keyring with a throwaway entry. It was run on Windows and on Linux (GNOME Keyring in a private D-Bus session).

## Consequences

- One format to document, back up and later sync. A keyring-held vault doesn't open on another computer: the synced data alone isn't enough, by design. Sprint 16 will need a master password for syncing secrets.
- RSA keys and long secrets work on Windows.
- Without a master password, the protection is the keyring's: good against a copied data folder, not against programs running as the user. The threat model and Settings > Security say so.
- PLAN's "secrets go to the keyring" is met through the vault key rather than per secret.
