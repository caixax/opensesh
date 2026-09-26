# ADR 0024: Crypto crates, SSH keys and PuTTY keys

- **Status:** accepted
- **Date:** 2026-09-26
- **Sprint:** 6

## Context

PLAN §2 lists `keyring`, `argon2`, `chacha20poly1305`, `secrecy` and `zeroize` for secrets.

Sprint 6 adds SSH keys:
- generate Ed25519 (default), ECDSA and RSA-4096;
- import OpenSSH and PuTTY keys, with a passphrase ("verify the real support");
- export them and copy the public key.

The RustCrypto crates come in generations that don't mix (`rand_core`, `digest`, `cipher` major versions). The newest `argon2` (0.6) and `chacha20poly1305` (0.11) belong to a newer generation than `ssh-key` 0.6, the stable release. `ssh-key` 0.7 is still a release candidate.

## Options

**Keys:**
1. `ssh-key` 0.6: OpenSSH keys (read, write, encrypt with bcrypt-pbkdf and AES-256-CTR), key generation for every algorithm, fingerprints, `known_hosts` and `authorized_keys`.
2. Shell out to `ssh-keygen`: not always installed (Windows), and passphrases would travel through a subprocess.

**PuTTY keys:** `ssh-key` doesn't read `.ppk`, and no maintained crate does both versions.
1. Our own reader, from PuTTY's documented format (appendix C of its manual).
2. Ask users to convert with PuTTYgen first.

**Crypto versions:**
1. The generation of `ssh-key` 0.6 for everything: `argon2` 0.5, `chacha20poly1305` 0.10, `aes` 0.8, `cbc` 0.1, `hmac` 0.12, `sha1` 0.10, and `ssh_key::sha2`.
2. The newest crates for the vault, next to `ssh-key` 0.6's older ones: two copies of most primitives.

## Decision

**Keys:** `ssh-key` 0.6.7 with `ed25519`, `p256`, `p384`, `p521`, `rsa`, `encryption` and `getrandom`.
- Generation: Ed25519, ECDSA P-256, P-384 and P-521, and RSA 4096 bits (its default size).
- Import:
  - OpenSSH keys, encrypted or not.
  - The old PEM format (`BEGIN RSA PRIVATE KEY` and similar) is refused, with advice to convert it with `ssh-keygen -p`.
  - DSA and security keys are refused.
- In the vault, a key is OpenSSH's binary encoding without encryption: the vault encrypts it. An imported key's passphrase is only used to import it.
- Export writes an OpenSSH key file, encrypted with a new passphrase (AES-256-CTR and bcrypt-pbkdf, like `ssh-keygen`) unless the user leaves it empty.

**PuTTY keys:** our own reader, `opensesh_vault::ppk`.
- It reads versions 2 and 3, encrypted (AES-256-CBC) or not.
- Version 3 keys are derived with Argon2i, Argon2d or Argon2id, with the costs from the file. The MAC is HMAC-SHA-256 (version 3) or HMAC-SHA-1 (version 2), and is checked before anything is used; a wrong passphrase shows as a MAC mismatch.
- It rebuilds the key in OpenSSH's encoding and reads it back with `ssh-key`. An Ed25519 key is rebuilt from its seed and checked against the public key.
- Fixtures were made by `puttygen` 0.83:
  - version 2 and 3, with and without a passphrase;
  - Argon2i, Argon2d and Argon2id;
  - Ed25519, ECDSA and RSA.
  Each one must give the public key `puttygen` prints. Version 1 is refused.

**Crypto versions:** option 1, one generation. `argon2` 0.5.3 and `chacha20poly1305` 0.10.1 are the current stable releases of that generation. The vault's Argon2id is checked against the reference implementation's output.

**Debug builds:** the arithmetic crates (`num-bigint-dig`, `rsa`, `argon2`, `blake2`, `bcrypt-pbkdf`, `blowfish`, `sha2`) are optimized even in debug builds and tests. Unoptimized, generating an RSA-4096 key took over a minute.

**RUSTSEC-2023-0071** (`rsa`, the Marvin timing attack) has no fixed version, in 0.9 or in the 0.10 candidates.
- The advisory says local use on a non-compromised computer is fine. Sprint 6 uses RSA locally only: generating, reading and writing keys.
- It is accepted, with this reason, in `deny.toml` and `.cargo/audit.toml`.
- Sprint 7 signs with RSA keys in an SSH client: this must be looked at again then.

## Consequences

- One copy of each primitive in the build. Moving to `ssh-key` 0.7 later moves the whole group together.
- PuTTY users can import their keys without PuTTYgen. The reader is about 400 lines of our own code, so it has its own tests and real fixtures.
- Old PEM keys need a one-time conversion; the error message says how.
- The RSA advisory stays visible in the configuration until a fixed release exists.
