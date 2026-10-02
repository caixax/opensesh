# Threat model

This document says what OpenSesh protects, from whom, how, and where it stops. It is kept up to date as features arrive (PLAN §8). It was last reviewed in Sprint 11 (the remote monitor and host info).

## What is worth protecting

| Asset | Where it lives |
|---|---|
| Saved passwords (identities) | `vault.bin`, encrypted |
| Private SSH keys | `vault.bin`, encrypted |
| The master password | only in the user's head; typed into the unlock dialog |
| The vault key | the system keyring (without a master password, or "remembered"), or wrapped by the master password in `vault.bin` |
| Passphrases of imported or exported keys | typed once; never stored |
| Hosts, users, identities' names, public keys, known hosts | readable TOML files (`hosts.toml`, `keychain.toml`, `known_hosts`) |
| What remote sessions show | the terminal's memory; session logs when a host turns them on; recordings of the panes the user records |
| Snippets and macros | `snippets.toml`, readable; their secrets stay in the vault (`{{secret:identity}}`) |
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
- **Telnet has no protection at all:** everything, passwords too, crosses the network in clear, and nothing proves who answers. The pane says so in yellow before connecting, and the host editor does too. Use it only on networks the user trusts, for devices without SSH.
- **S3 over `http://`** (an endpoint the user typed that way, or `s3+http://`) sends requests in clear: the secret key itself never travels (requests are signed), but the objects and their names do, and a signed request can be replayed for a few minutes. `https://` is the default.
- **Remote desktops** always run over TLS (rustls on `ring`, no key logging, no session resumption).
  - **The server's certificate** is trusted on first use, like a host key: the first one is shown with its SHA-256 fingerprint and subject, and a changed one warns, defaulting to "Don't connect". Remembered certificates are in `trusted_certificates.toml`.
  - **Nothing is sent before the certificate is accepted.** Servers with NLA (Windows) then get the credentials through CredSSP; servers without it (xrdp) get them inside the TLS session.
  - **Through jump hosts** the desktop's TLS still ends at the server, and its certificate is checked against the server's name: the jump hosts carry it, they can't read it.
- **VNC** is only as private as the server allows:
  - **VeNCrypt with a certificate** (X509None, X509Vnc, X509Plain): TLS like RDP's, with the same trust on first use; nothing is sent before the certificate is accepted.
  - **VNC authentication alone** proves the password with a DES challenge (the password itself doesn't cross), but **the session is not encrypted**: everything typed and shown can be read on the way, and the password's challenge can be attacked offline. The pane's bar says "Not encrypted" in the warning colour for such a session: use VeNCrypt, or reach the server through a jump host (an SSH tunnel).
  - Servers that offer only anonymous TLS (TLSNone, TLSVnc, TLSPlain) are refused with an explanation: anonymous TLS can't tell the server from someone in the middle.

### A malicious server

- **It sees what the session sends it,** like any SSH client: what is typed, pasted or broadcast.
- **It can't get the user's other secrets:** passwords are sent only to the host whose identity holds them, and only after its key was checked. A password is asked for with the host's name in the question.
- **Agent forwarding** is off by default. When a host turns it on, anyone with root on that host can ask the user's agent to sign while the session lasts (the host editor says so). Agent channels are refused unless the host asked for forwarding.
- **X11 forwarding** isn't implemented in the built-in client yet; `x11` channels are refused.
- **Output is untrusted:** escape sequences are parsed by the terminal engine (bounded, no command execution); clipboard writes (OSC 52) are shown with a toast; OS detection only matches `/etc/os-release` IDs to a fixed icon list.
- **The remote monitor and the host info** (on by default, off globally or per host) run a fixed read-only command on an exec channel of the user's own connection: a POSIX `sh` loop reading `/proc` (or `sysctl`, `netstat`, `vm_stat`), `df` and `who`, and once `uname`, `hostname`, `ip`/`ifconfig`.
  - What comes back is only parsed into numbers and names for the status bar and the Info tab; a server can lie in it, not make OpenSesh run or write anything.
  - The loop ends by itself when its channel closes, so nothing stays on the server.
  - A server that doesn't have `sh` just gets no monitor.
- **Macros that wait for text** decide only when to go on, never what to type: a server that prints the awaited text early makes the next step come sooner, and one that never prints it stops the run at the step's timeout. What a macro types is what the user wrote in it.
- **"Install my key"** sends only the public key, on the command's standard input.
- **File names are untrusted:** listings, symlink targets and the names in a recursive download come from the server. A download writes only under the folder the user chose: a name that isn't one plain name (empty, `.`, `..`, with a `/`, and on Windows with a `\` or `:`) is left out of listings and downloads, and a file with such a name isn't opened for editing, so a server can't place a file elsewhere (the CVE-2019-6111 kind of attack; the SCP spike refuses such names too). Symlinks to folders are not followed in recursive copies (no loops, nothing outside the tree).
- **Shell commands on the server** run only for what the user asked: a copy within the server (`cp -R -p`), the shell integration (added to `~/.bashrc` or `~/.zshrc` only after the user confirms), and "Save with sudo". Paths go in single quotes; nothing from a listing is run.
- **Following the terminal's folder** (OSC 7) only moves the side panel's listing: a server can make it show another folder, not run anything or write anywhere.
- **Remote forwards:** a server can open `forwarded-tcpip` channels at any time; the client only accepts them for a port it asked to forward, and connects them only to that tunnel's destination. Others are refused.
- **RSA signing** (RUSTSEC-2023-0071, see Dependencies): a server could time the client's RSA signatures, one per connection.

### The other terminal kinds (Sprint 12)

- **Serial ports** are local devices: whatever the device prints is terminal output, parsed like a server's (see above). The port list only reads names and descriptions; nothing is opened until the user connects.
- **Mosh** connects with the built-in SSH client (same host key checks and questions), starts `mosh-server` on the server and closes. The session key it prints goes to `mosh-client` in its environment (`MOSH_KEY`), never on a command line where other users could see it with `ps`, and is never shown or logged. Mosh then talks to the server over UDP, encrypted and authenticated with that key (AES-OCB), straight to the server's address: jump hosts and proxies don't carry it, and the pane says so.
- **Containers and pods:** `docker`, `podman` and `kubectl` run here with their own configuration and credentials, as the user would run them. The container, pod, namespace, context and shell come from the host and are passed as separate arguments (no shell here); names that start with `-` or hold spaces are refused, so a host can't add options. The running containers and pods are listed only when the host editor or quick connect asks (a background command with a time limit).
- **Local shells** come from `/etc/shells`, `$SHELL`, known install folders and `wsl.exe -l -q`; choosing one runs it like the default shell.

### S3 storage

- **The secret key** is the password of a keychain identity, in the vault (encrypted); `hosts.toml` holds only the endpoint, region, access key or identity id. Typed when connecting, it is used for that pane only and not stored. Requests are signed with it (SigV4); it is never sent, logged or put in an error.
- **No other credentials are read:** not `~/.aws`, the environment or instance metadata.
- **Temporary links** are bearer tokens: anyone with one can download that object until it expires (at most seven days; the menu offers an hour, a day or a week). They are made here without a request, and only copied to the clipboard when the user asks.
- **Object keys are untrusted** like file names (see above): a download writes only under the folder the user chose, and a key that isn't a plain name is left out.
- **The server** sees what is uploaded and can serve anything for a download, like an SFTP server.
- **Test runs** use an in-process S3 server with test keys, never the network.

### Remote desktops (Sprint 13)

- **A program of its own:** each remote desktop pane runs `opensesh-rdp` (ADR 0034), which talks only to OpenSesh, over its standard input and output.
  - Everything the server sends is decoded there, in Rust, so a decoder that fails ends that pane, not the app.
  - It never logs: IronRDP and `sspi` write NTLM messages to their debug logs, so the helper installs no log output at all, and its standard error goes nowhere.
- **The password** goes to the helper once, through that pipe (never on a command line or in the environment), and only when a connection needs it. IronRDP keeps it as a plain string for that connection, which isn't wiped (see Memory).
- **The server sees what is typed while the desktop has the keyboard,** OpenSesh's own shortcuts included. The "Give the keyboard back" combination (Ctrl+Alt+Home unless changed) is the one exception. On Wayland the compositor's shortcuts still go to the compositor (Qt has no shortcuts inhibitor yet).
- **The clipboard** is shared both ways unless the host turns it off.
  - This computer's text is handed to the helper when the desktop gets the keyboard, or when the clipboard changes while it has it. The server asks for it when something there pastes, but nothing stops a malicious server from asking at any moment while connected: with sharing on, it can read whatever text was copied here last.
  - The server can put text into this computer's clipboard at any time while connected.
- **The pointer pictures** the server sends are checked against their size before they are drawn.
- **The local tunnel through jump hosts** listens on `127.0.0.1` while the pane is open. Programs on this computer could connect to it, as with a local tunnel (see Tunnels), and reach the desktop's login.
- **Test runs** connect only to an RDP test server started next to the app, remember certificates in their temporary folder, and never touch the user's clipboard.
- **VNC** (Sprint 14) runs in the app rather than a helper, in Rust (`opensesh-vnc`): what the server sends is bounded before it is decoded (desktops up to 8192 pixels a side, cursors up to 256, compressed rectangles up to 64 MiB, texts up to 1 MiB), and a rectangle outside the desktop ends the session. The password goes to the session once, kept in a wiped string; VeNCrypt's user name and password are sent in one wiped buffer. The clipboard is shared as for RDP (a host setting), and a view-only host sends no keys, pointer or clipboard at all.

### Pasted text

A web page, a chat or a document that gives the user a command to paste.

- **Paste protection** (on by default; per profile, group and host): before a paste, the analyzer looks for:
  - lines that would run at once;
  - control and escape characters (an `ESC` can end a bracketed paste early and run what follows);
  - zero-width and bidirectional characters;
  - letters from another alphabet mixed into Latin words;
  - downloads or decoded text piped into a shell;
  - writes to shell profiles, `authorized_keys` and system files;
  - `sudo` in a pipe, and destructive commands.
- **When it finds any of these,** a dialog shows the text, editable, with the findings, and nothing is sent until the user chooses Paste. A paste into several broadcast panes always goes through the same dialog.
- **It works on patterns,** not by running or fully parsing the text, so obfuscated commands can get through. It is a second look, not a sandbox.
- **Text reaches the terminal only when the user pastes it:** OpenSesh never reads the clipboard by itself, and programs can't read it (OSC 52 reads are never answered).

### Tunnels

- **Listening beyond localhost** (PLAN §8): a tunnel listens on `127.0.0.1` unless the user sets another address. One that listens on anything but a loopback address is marked in the Tunnels view, and the editor asks once before saving it, saying who could use it: anyone on the network could reach the server's network through a local or dynamic tunnel, or reach this computer through a remote one (when the server's `GatewayPorts` allows it).
- **The SOCKS proxy** of a dynamic tunnel asks for no authentication, like `ssh -D`: anything that can reach its port can use it, which is why it listens on the loopback by default.
- **Programs on this computer** can use a running local or dynamic tunnel, as with OpenSSH: they are inside the boundary above.
- **`tunnels.toml`** holds no secrets: a tunnel names a saved host (whose identity holds them) or `user@host` text. Someone who can write the file can add a tunnel, but it starts only if it is marked to start with the app and its host's authentication succeeds; a tunnel added this way that listens beyond localhost is marked like any other.

### Tampering with files

- **`vault.bin`** is authenticated as a whole: header and body.
- **`keychain.toml` and `hosts.toml` are not.** Someone who can write them can rename entries, point a host at another identity, or change a host's address.
  - They can't read secrets this way. An identity's key is used from the vault, and its public half is derived from it, not trusted from the file.
  - Connecting to a changed address is caught by host key verification: the new server's key is unknown or different.
- **`known_hosts`** (OpenSesh's file and `~/.ssh/known_hosts`) is not authenticated either. Someone who can write it can make a key trusted, as with OpenSSH. OpenSesh only ever writes its own file, atomically.
- **`trusted_certificates.toml`** is the same for remote desktops' certificates: someone who can write it can make a certificate trusted.
- A value in `keychain.toml` that looks like a secret instead of a `vault:` reference is dropped when read, and never written back. Warnings about the file never quote its values.

### Leaks through the app itself

- **Logs, crash reports and error messages** never contain secrets. Errors carry codes and file names. Types that hold secrets print nothing in `Debug`.
- **The vault is written without backups:** a backup would keep deleted secrets, or a copy sealed under an old master password.
- **Exported private keys** are written by the keychain worker straight to the chosen file (`0600` on Linux), optionally with a passphrase. They never pass through the UI or the clipboard. Only public keys are copied to the clipboard.
- **Test runs** (smoke tests, screenshots) keep the vault in memory and never touch the user's keyring, agents or files. Their SSH connections all go to an in-process test server. Their snippets stay in memory, and their recordings go to the run's temporary folder.
- **Secrets reach a connection only when it authenticates:** the connection asks the keychain worker for the identity's password and key then, so a locked vault stays locked until a connection needs it. Typed answers become wiped strings at once; a dropped question counts as cancelled.
- **Files being edited** are downloaded into a private folder (the cache folder's `edit/<id>`, `0700` on Linux) while the edit lasts, in clear, like any file the user downloads; the folder is removed when the edit stops or OpenSesh quits (after a crash it stays until removed by hand). An editor may keep its own backups elsewhere.
- **"Save with sudo"** is offered only after the server refused a save, with a warning. The sudo password is typed in its dialog, kept as a wiped string, sent once on the command's standard input (never on the command line) and only when `sudo -n true` says a password is needed; otherwise a password line would end up in the file. If sudo asks for a password for `tee` but not for `true` (per-command rules), the first line of the file is taken as a wrong password attempt, and nothing is written.
- **Snippets with secrets:** `{{secret:identity}}` types a keychain identity's password.
  - It comes from the keychain worker when the run starts, as a wiped string, and never passes through QML or into `snippets.toml`. A locked vault stops the run instead.
  - The text with it filled in is built once, in a buffer of its final size that is wiped after it is typed.
  - From there it follows the path of a password typed on the keyboard, which isn't wiped (see Memory). The remote program may echo it or keep it in its history.
- **Values typed for a snippet's variables** are offered again during the session, in memory only, and never written.
- **The macro recorder** records what is typed as text, passwords included. The editor says so before saving and suggests `{{secret:identity}}` instead; a recorded macro is saved only when the user saves it.
- **Recordings** (off by default, per pane, started from the pane's menu, with a chip while they run) keep what the screen showed, secrets printed there included, but never the keys typed.
  - They go to `recordings/` in the data folder (`0700`, files `0600` on Linux) and end when the pane's session ends.
  - A recording played back is a file, not a program: its output is parsed like a server's, and it never writes the clipboard.
- **The monitor's readings and the host info** stay in memory, and leave only when the user copies them ("Copy as text").
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
- **RUSTSEC-2026-0253** (`lru` 0.16.4, unsound `pop()` when a key's code panics) comes with the AWS SDK, which uses it for its S3 Express session cache with `String` keys, whose code doesn't panic. No fixed 0.16 release exists; reviewed each sprint ([ADR 0033](adr/0033-s3-storage.md)).
- **RUSTSEC-2023-0071** (the `rsa` crate, the Marvin timing attack) has no fixed version. The vault uses RSA locally (generating, reading and writing keys), which the advisory considers safe ([ADR 0024](adr/0024-crypto-crates-and-ssh-keys.md)). The SSH client signs with RSA keys from the vault and key files: it never decrypts with RSA (the attack's target), it makes one signature per connection, new keys are Ed25519 by default, and keys held by an agent are signed outside OpenSesh. Accepted and reviewed each sprint ([ADR 0027](adr/0027-ssh-client.md)).
  - **The RDP helper** gets `rsa` too, through `sspi`'s `picky` (Kerberos and smart cards). It never decrypts with an RSA private key: its NLA is NTLM, and rustls on `ring` checks the server's TLS signature. Its workspace has a lock file of its own (`rdp/`), checked by `cargo deny` and `cargo audit` in CI ([ADR 0034](adr/0034-rdp-client.md)).

## Known gaps and future work

- No hardware keys (FIDO, PKCS#11) and no Kerberos yet. The OpenSSH backend covers them.
- Host certificates (`@cert-authority`) are listed but not used to trust a host yet: a certified host key is checked as a plain key.
- Proxy passwords are not supported yet.
- The Windows instance pipe (ADR 0021) keeps the default security descriptor.
- Syncing the data folder between devices (Sprint 16) will need a look at `vault.bin` merges and keyring-held vaults, which don't open on another computer.
