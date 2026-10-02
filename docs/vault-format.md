# The vault file (`vault.bin`), format 1

OpenSesh keeps passwords and private keys in `vault.bin`, in the data folder (`$XDG_DATA_HOME/opensesh/` on Linux, `%LOCALAPPDATA%\OpenSesh\` on Windows, `data/` in portable mode). This document describes the file so that it can be read without OpenSesh. The code is `crates/opensesh-vault/src/format.rs`; the reasons are in [ADR 0023](adr/0023-vault-and-key-holders.md).

## Primitives

- **Encryption:** XChaCha20-Poly1305 (256-bit key, 192-bit random nonce, 128-bit tag).
- **Key derivation from the master password:** Argon2id, version 0x13, 32-byte output. The costs are stored in the file. New vaults use RFC 9106's second recommended option: **64 MiB of memory, 3 passes, 4 lanes**, with a random 16-byte salt.
- **Randomness:** the operating system's generator (`getrandom`), for keys, salts and nonces. Every write uses fresh nonces.

## Keys

Two keys are involved:

- The **vault key**: 32 random bytes, made when the vault is created. It encrypts the secrets. It never changes, even when the master password does.
- **Who holds the vault key:**
  - **System keyring** (no master password): the keyring stores the vault key itself, as a binary secret with service `cc.caixa.OpenSesh` and account `vault-<vault id as a ULID>`.
  - **Master password:** Argon2id derives a key from the password and the salt; that key encrypts the vault key (the *wrapped key*) in the file header. "Remember on this computer" also stores the vault key in the keyring, under the same name as above.

## Layout

All integers are little-endian.

| Field | Size | Contents |
|---|---|---|
| magic | 8 | `OSVAULT\0` |
| version | 2 | `1` |
| vault id | 16 | random; names the keyring entry |
| holder | 1 | `1` system keyring, `2` master password |
| *master password only:* | | |
| kdf | 1 | `1` = Argon2id, version 0x13 |
| memory | 4 | KiB |
| passes | 4 | |
| lanes | 4 | |
| salt | 16 | |
| wrap nonce | 24 | |
| wrapped key | 48 | the vault key (32 bytes) sealed with the derived key, then its 16-byte tag |
| *always:* | | |
| body nonce | 24 | |
| body | rest | the payload sealed with the vault key, then its 16-byte tag |

**Associated data:**

- The wrapped key is sealed with the header bytes **before the wrap nonce** (magic through salt) as associated data.
- The body is sealed with the **whole header** (magic through body nonce) as associated data.

So any change to the header or the body makes decryption fail. A wrong master password and a changed header look the same.

**Limits when reading:**

- A version above 1 is refused as "written by a newer OpenSesh"; the file is never overwritten.
- Argon2 costs are accepted within 8×lanes to 4 GiB of memory, 1 to 64 passes and 1 to 64 lanes, so a crafted header can't make unlocking allocate without bound.

## Payload

The decrypted body:

| Field | Size | Contents |
|---|---|---|
| count | 4 | number of entries |
| *each entry:* | | |
| id | 16 | a ULID, referenced as `vault:<ULID>` from `keychain.toml` |
| kind | 1 | `1` password (UTF-8), `2` private key (OpenSSH's binary private key encoding, unencrypted), other values kept as they are |
| length | 4 | at most 1 MiB |
| data | length | |
| padding | | zeros up to a multiple of 256 bytes |

The padding means that the file size only hints at how much is stored. The payload is at most 64 MiB.

## Writing

The file is replaced atomically: a temporary file, `fsync`, then a rename. On Linux it is created with mode `0600`.

No backups are kept. A backup would keep secrets that were deleted, and a copy sealed under a master password that was since changed.

## Other files

These files hold no secrets:

- **`keychain.toml`** (config folder): identities (name, user, `password = "vault:<id>"`, `key = "<key id>"`) and keys (name, algorithm, public key, fingerprint, `private = "vault:<id>"`). A value that isn't a `vault:` reference where a secret belongs is dropped when the file is read, and never written back.
- **`vault-attempts.toml`** (data folder): wrong master passwords in a row and when the last one was; see the [threat model](threat-model.md).
- **`hosts.toml`**: a host or group names an identity by its id.

## Changing the format

A new layout gets a new version number. Older versions keep being read, and the file is rewritten in the new version on the next change.

## In bundles (Sprint 16)

An OpenSesh bundle exported with its keychain carries the identities' and keys' secrets as a file in this format, Base64-encoded in its `[keychain]` table (`sealed`), held by a password slot for the export password (a random vault id and key of its own). Opening it is the same as unlocking a vault with a master password; see [ADR 0037](adr/0037-importers-bundles-and-sync.md).
