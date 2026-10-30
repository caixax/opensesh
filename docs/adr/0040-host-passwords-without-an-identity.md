# ADR 0040: A host's own password and key passphrase, without an identity

- **Status:** accepted
- **Date:** 2026-10-30
- **Issue:** [#23](https://github.com/caixax/opensesh/issues/23)

## Context

Since Sprint 6 a saved password lives only in a keychain identity: a user name with a password and/or a key, which hosts and groups name by id. To save one password for one server, a user had to open the keychain, create an identity, give it the password, then choose it in the host editor. Issue #23 asks for what other clients do: type the password for the host, or tick "remember" when it is asked for, and not be asked again.

The same applies to the passphrase of a host's key file: it was asked for on every connection, since the vault only held keys that were imported into it.

The constraints that shaped the keychain still hold (PLAN §0, ADR 0023):

- Secrets live only in `vault.bin`, encrypted; nothing else on disk holds them, and a test checks every file for them (`tests/no_plaintext.rs`).
- `hosts.toml` is readable and may be synced to other computers (ADR 0037), while the vault stays on this one.
- Connections ask the keychain worker for secrets when they authenticate, so a locked vault stays locked until it is needed.

## Options

1. **Create a hidden identity per host.** It needs no new storage, but the identities list fills with entries the user never made, a host can't have both an identity and its own password, and syncing `hosts.toml` would carry references to identities that don't exist elsewhere.
2. **A `password` field in `hosts.toml` holding a `vault:` reference.** That puts this computer's vault ids in a file other computers read, where they mean nothing.
3. **Per-host entries in `keychain.toml`, by host id, referring to the vault.** `keychain.toml` is local already; the host stays as it is, and the secrets stay on this computer.

## Decision

Option 3.

- **Storage:** `keychain.toml` gets `[[host]]` entries: `host = "<host id>"`, with `password` and/or `key_passphrase` as `vault:<id>` references (secret kind "password"). They follow the rules of the identities' references: a value that isn't one is dropped when the file is read, with a warning that never quotes it, and an entry with nothing left is dropped. The vault's garbage collection keeps their secrets.
- **Precedence:** a host's own password comes before its identity's. A saved key passphrase is tried on every encrypted key file the connection opens before asking (it may belong to another key file: that isn't counted as a wrong attempt).
- **Remembering:** the password and passphrase prompts of a saved host have a "Remember on this computer" check box, shown when the vault can take a secret without a master password being typed (unlocked, held by the keyring, or remembered). The SSH client (`AuthPlan.remember`, a `SecretSink`) saves a password only once the server accepted it, and a passphrase once it opened the key. The remote desktops save the password once the desktop is connected. A host typed in quick connect has no id, and nothing is offered.
- **The host editor** has Password and Key passphrase fields on the Authentication page, saved with the host, and Forget for what is saved. Deleting a host forgets its secrets; resetting the vault removes the entries.
- **Where it applies:** the built-in SSH client (terminals, SFTP, tunnels, Mosh, jump hosts, which are hosts too) and RDP and VNC. The system's OpenSSH client doesn't use them, as it doesn't use identities' passwords.

## Consequences

- One password for one server is one field, or one tick when connecting. Identities stay for what they are good at: one login shared by many hosts.
- `hosts.toml` doesn't change, so syncing and the older readers of it are unaffected.
- Host secrets stay on this computer: bundles carry identities and keys, not hosts' own secrets. A user moving to another computer types them again, or uses an identity.
- An older OpenSesh keeps the `[[host]]` entries it doesn't know when it writes `keychain.toml`, but its vault garbage collection doesn't know their secrets and removes them: after a downgrade, the hosts' passwords are asked for again.
- A server that changes its password makes the saved one fail: the prompt then comes back, and ticking "remember" replaces it.
