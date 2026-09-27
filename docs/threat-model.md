# Threat model

This document says what OpenSesh protects, from whom, how, and where it stops. It is kept up to date as features arrive (PLAN §8). It was last reviewed in Sprint 8 (SFTP).

## What is worth protecting

| Asset | Where it lives |
|---|---|
| Saved passwords (identities) | `vault.bin`, encrypted |
| Private SSH keys | `vault.bin`, encrypted |
| The master password | only in the user's head; typed into the unlock dialog |
| The vault key | the system keyring (without a master password, or "remembered"), or wrapped by the master password in `vault.bin` |
| Passphrases of imported or exported keys | typed once; never stored |
| Hosts, users, identities' names, public keys, known hosts | readable TOML files (`hosts.toml`, `keychain.toml`, `known_hosts`) |
| What remote sessions show | the terminal's memory; session logs when a host turns them on |
| Files on servers | the servers; copies the user transfers; private copies of files being edited, in the cache folder |
| The user's SSH sessions themselves | the network, between OpenSesh and each server |

Hosts and public keys are not secret, but they are private: they show where the user connects and as whom. They are kept readable on purpose (PLAN §0, local-first), like OpenSSH's own `~/.ssh/config` and `known_hosts`.

## Trust boundaries

- **The user's operating system account.** OpenSesh runs as the user and trusts the account, its file permissions and its keyring. Anything running as the same user is out of reach of the protections below, except where a master password is set and the vault is locked.
- **The config and data folders.** Their contents may be backed up, synced or copied elsewhere. They must be safe to lose without losing secrets in clear.
- **The system keyring.** On Windows, Credential Manager (protected by DPAPI with the user's login). On Linux, the Secret Service: GNOME Keyring, or KWallet through its Secret Service interface, usually unlocked at login.
- **The network.** OpenSesh talks to the network only to connect where the user asks: the SSH servers (and the proxies and jump hosts on the way), and, when enabled, the update check. Everything on the path is untrusted until the server's key is checked.
- **The servers.** A server the user connects to sees what the session sends it. It is trusted with the session, not with the computer.

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

### Someone on the network path

A malicious Wi-Fi, a compromised router, a proxy or a jump host in the middle.

- **Host keys** are checked on every hop: against OpenSesh's `known_hosts` first, then `~/.ssh/known_hosts` (hashed names, ports, wildcards, `@revoked`). A key seen for the first time is shown with its SHA-256 fingerprint before anything is sent; trusting it is the user's choice.
- **A changed key stops the connection** (PLAN §8) with a card that says what that can mean. The default answer is "Don't connect"; "Connect once" keeps the key in memory for that pane only; "Replace the saved key" writes it to OpenSesh's file. A revoked key is refused without a question.
- A jump host carries the next hop's traffic, but the next hop's key is checked end to end: a malicious jump host can refuse or cut the tunnel, not read it.
- **Algorithms:** modern ones only by default (the ML-KEM and curve25519 hybrid, curve25519 and SHA-2 Diffie-Hellman groups for key exchange; AES-GCM, AES-CTR and ChaCha20-Poly1305; SHA-2 MACs and signatures). SHA-1, CBC and `hmac-sha1` only come back when a host turns "legacy algorithms" on, for that host.
- Proxies (SOCKS5, HTTP CONNECT, a proxy command) see where the user connects, not what is sent.

### A malicious server

- **It sees what the session sends it,** like any SSH client: what is typed, pasted or broadcast.
- **It can't get the user's other secrets:** passwords are sent only to the host whose identity holds them, and only after its key was checked. A password is asked for with the host's name in the question.
- **Agent forwarding** is off by default. When a host turns it on, anyone with root on that host can ask the user's agent to sign while the session lasts (the host editor says so). Agent channels are refused unless the host asked for forwarding.
- **X11 forwarding** isn't implemented in the built-in client yet; `x11` channels are refused.
- **Output is untrusted:** escape sequences are parsed by the terminal engine (bounded, no command execution); clipboard writes (OSC 52) are shown with a toast; OS detection only matches `/etc/os-release` IDs to a fixed icon list.
- **"Install my key"** sends only the public key, on the command's standard input.
- **File names are untrusted:** listings, symlink targets and the names in a recursive download come from the server. A download writes only under the folder the user chose: a name that isn't one plain name (empty, `.`, `..`, with a `/`, and on Windows with a `\` or `:`) is left out of listings and downloads, and a file with such a name isn't opened for editing, so a server can't place a file elsewhere (the CVE-2019-6111 kind of attack; the SCP spike refuses such names too). Symlinks to folders are not followed in recursive copies (no loops, nothing outside the tree).
- **Shell commands on the server** run only for what the user asked: a copy within the server (`cp -R -p`), the shell integration (added to `~/.bashrc` or `~/.zshrc` only after the user confirms), and "Save with sudo". Paths go in single quotes; nothing from a listing is run.
- **Following the terminal's folder** (OSC 7) only moves the side panel's listing: a server can make it show another folder, not run anything or write anywhere.
- **RSA signing** (RUSTSEC-2023-0071, see Dependencies): a server could time the client's RSA signatures, one per connection.

### Tampering with files

- **`vault.bin`** is authenticated as a whole: header and body.
- **`keychain.toml` and `hosts.toml` are not.** Someone who can write them can rename entries, point a host at another identity, or change a host's address.
  - They can't read secrets this way. An identity's key is used from the vault, and its public half is derived from it, not trusted from the file.
  - Connecting to a changed address is caught by host key verification: the new server's key is unknown or different.
- **`known_hosts`** (OpenSesh's file and `~/.ssh/known_hosts`) is not authenticated either. Someone who can write it can make a key trusted, as with OpenSSH. OpenSesh only ever writes its own file, atomically.
- A value in `keychain.toml` that looks like a secret instead of a `vault:` reference is dropped when read, and never written back. Warnings about the file never quote its values.

### Leaks through the app itself

- **Logs, crash reports and error messages** never contain secrets. Errors carry codes and file names. Types that hold secrets print nothing in `Debug`.
- **The vault is written without backups:** a backup would keep deleted secrets, or a copy sealed under an old master password.
- **Exported private keys** are written by the keychain worker straight to the chosen file (`0600` on Linux), optionally with a passphrase. They never pass through the UI or the clipboard. Only public keys are copied to the clipboard.
- **Test runs** (smoke tests, screenshots) keep the vault in memory and never touch the user's keyring, agents or files. Their SSH connections all go to an in-process test server.
- **Secrets reach a connection only when it authenticates:** the connection asks the keychain worker for the identity's password and key then, so a locked vault stays locked until a connection needs it. Typed answers become wiped strings at once; a dropped question counts as cancelled.
- **Files being edited** are downloaded into a private folder (the cache folder's `edit/<id>`, `0700` on Linux) while the edit lasts, in clear, like any file the user downloads; the folder is removed when the edit stops or OpenSesh quits (after a crash it stays until removed by hand). An editor may keep its own backups elsewhere.
- **"Save with sudo"** is offered only after the server refused a save, with a warning. The sudo password is typed in its dialog, kept as a wiped string, sent once on the command's standard input (never on the command line) and only when `sudo -n true` says a password is needed; otherwise a password line would end up in the file. If sudo asks for a password for `tee` but not for `true` (per-command rules), the first line of the file is taken as a wrong password attempt, and nothing is written.
- **Session logs** (off by default) keep what the screen showed, secrets printed there included. They go to `logs/sessions` in the data folder or a folder chosen in Settings > SSH, created `0600` on Linux; text logs leave out escape sequences. Passwords typed at a remote prompt are not echoed, so they are not in the log.

## Memory

- Keys and decrypted secrets in Rust are wiped when dropped (`zeroize`). Buffers are sized once so that no reallocated copy is left behind.
- **Limits:**
  - Text typed into the UI (passwords, passphrases, a pasted key) lives in Qt strings for a while, which are not wiped.
  - The keyring libraries and the OS make their own copies.
  - Memory can be swapped to disk, and nothing is locked in RAM (`mlock`).
  - The cipher libraries keep round keys that aren't wiped everywhere.

## Agents

OpenSesh lists the keys of the agents it finds: `SSH_AUTH_SOCK`, the Windows OpenSSH agent's named pipe, and Pageant. The SSH client asks them to sign the authentication of hosts that use public keys, and forwards them to hosts that turned agent forwarding on. The agents are trusted as the user's own, as OpenSSH trusts them. On Windows, a named pipe or a Pageant window could belong to another program of the same user, which is inside the boundary above.

## Dependencies

- Every version is pinned and locked. `cargo deny` and `cargo audit` run in CI.
- **RUSTSEC-2023-0071** (the `rsa` crate, the Marvin timing attack) has no fixed version. The vault uses RSA locally (generating, reading and writing keys), which the advisory considers safe ([ADR 0024](adr/0024-crypto-crates-and-ssh-keys.md)). The SSH client signs with RSA keys from the vault and key files: it never decrypts with RSA (the attack's target), it makes one signature per connection, new keys are Ed25519 by default, and keys held by an agent are signed outside OpenSesh. Accepted and reviewed each sprint ([ADR 0027](adr/0027-ssh-client.md)).

## Known gaps and future work

- No hardware keys (FIDO, PKCS#11) and no Kerberos yet. The OpenSSH backend covers them.
- Host certificates (`@cert-authority`) are listed but not used to trust a host yet: a certified host key is checked as a plain key.
- Proxy passwords are not supported yet.
- The Windows instance pipe (ADR 0021) keeps the default security descriptor.
- Syncing the data folder between devices (Sprint 16) will need a look at `vault.bin` merges and keyring-held vaults, which don't open on another computer.
