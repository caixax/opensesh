# ADR 0025: Listing SSH agent keys without extra runtimes

- **Status:** accepted
- **Date:** 2026-09-26
- **Sprint:** 6

## Context

PLAN Sprint 6 asks to list the keys of the system's agent: `SSH_AUTH_SOCK`, the Windows OpenSSH agent over its named pipe, and Pageant. Sprint 7 will authenticate through them.

The agent protocol is small (draft-miller-ssh-agent): a length-prefixed request, "request identities" (11), and an "identities answer" (12) listing public key blobs and comments.

Pageant has two ways in:
- the old one, a WM_COPYDATA message pointing at a shared memory mapping;
- since 0.75, also a named pipe whose name depends on the user and on `CryptProtectMemory`.

The `pageant` crate (0.2) needs tokio and the `windows` crate. OpenSesh has neither and uses `windows-sys`.

## Options

1. **The `pageant` crate** for Pageant, plus our own code for the rest: it pulls in an async runtime for one synchronous message.
2. **Our own client for all three:**
   - `std::os::unix::net::UnixStream` for `SSH_AUTH_SOCK`;
   - `std::fs::File` for the OpenSSH pipe (`\\.\pipe\openssh-ssh-agent`, or `SSH_AUTH_SOCK` when it names a pipe);
   - WM_COPYDATA through `windows-sys` for Pageant.

## Decision

Option 2, in `opensesh_vault::agent`.

- **One protocol function** frames the request, reads an answer of at most 256 KiB, and parses it. Key types it doesn't know are skipped, and a failure answer is reported.
- **Timeouts:**
  - Unix sockets use read and write timeouts.
  - Named pipes have no I/O timeouts in `std`, so the exchange runs in a helper thread and the caller waits at most the timeout (3 s in the app).
  - Pageant uses `SendMessageTimeoutW` with `SMTO_ABORTIFHUNG`.
- **Pageant, as PuTTY's own client does it:**
  1. Write the request into a named 8 KiB file mapping. Its default security makes it owned by the user, which Pageant checks.
  2. Send the mapping's name with `dwData` 0x804e50ba.
  3. Read the answer from the mapping, checked against its size.
  The `unsafe` code is in one module, with a `SAFETY` note on each block.
- **In the app,** listing runs on the keychain worker thread when the Agents section opens or is refreshed. Test runs never ask the machine's agents.

## Verification

- A stand-in agent on a Unix socket (tests on Linux) and on a named pipe (tests on Windows, through `interprocess`).
- Parser tests: short answers, failures, unknown key types.
- By hand: Pageant 0.83 (the official build, checked against PuTTY's published SHA-256) holding a fixture key, and `ssh-agent` with `ssh-add` in the WSL distros. Each listed the key with the fingerprint `ssh-keygen -l` prints.

## Consequences

- No async runtime and no second Windows API crate for one message.
- Pageant's newer named pipe isn't used. WM_COPYDATA still works in current Pageant and is what PuTTY itself uses. If a future Pageant drops it, the pipe name can be added here.
- Sprint 7 reuses the same transports to sign ("sign request", 13).
