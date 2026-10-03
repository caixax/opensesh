# Security review (Sprint 17)

PLAN Sprint 17 asks for a security review against the [threat model](threat-model.md) and the ADRs before 1.0. This page says what was checked, how, what was found and what was done about it. It is a review by the project, not an outside audit.

**Date:** 2026-10-03. **Code:** the main branch at the end of Sprint 17.

## How

- **Each defense the threat model claims** was traced to the code that makes it true and to a test that would fail if it stopped being true; where there was no test, one was added or the gap is listed below.
- **The parsers of untrusted input** were fuzzed (`fuzz/`, cargo-fuzz and libFuzzer): quick connect, the paste analyzer, the terminal theme importers, the remote monitor's parsers, `~/.ssh/config`, MobaXterm, PuTTY, Remmina, CSV, OpenSesh bundles, and the sync merge with Git's conflict splitter. Results under [Fuzzing](#fuzzing).
- **The dependencies** were checked with `cargo deny` and `cargo audit`, and the accepted advisories read again.
- **Logs, files and processes:** every log call that names a credential was read; the permissions of what OpenSesh writes; what other local programs can reach (the instance socket, local tunnels).

## Findings

| # | Area | Finding | Severity | Status |
|---|---|---|---|---|
| 1 | Importers | An import read whatever the path named: `Include /dev/zero` in `~/.ssh/config`, or a FIFO named `x.remmina` in a Remmina folder, made the import read forever (the app's memory grew until it was stopped). | Low (needs a crafted file the user imports) | **Fixed:** importers read regular files only, up to 16 MiB (`~/.ssh/config` and its includes up to 1 MiB, bundles 64 MiB); a test reads `/dev/zero`. |
| 2 | Single instance | The instance socket's name came from the settings folder in use. Moving the settings (Sprint 16, effective at the next start) changed it, so a start before restarting didn't find the running instance and a second one could run with other settings. | Low (robustness) | **Fixed:** the name comes from the default settings folder, which never moves; a test checks it stays the same. |
| 3 | Imports from others | A host's ProxyCommand or a local terminal's shell, imported from someone else's file, runs a command here when used. | Medium (social engineering) | **Mitigated in Sprint 16:** the import preview lists every such command before importing. |
| 4 | Updates | The installer downloaded by an update is checked against `SHA256SUMS.txt` of the same GitHub release, over HTTPS: that catches a damaged download, not a release replaced by someone with access to the repository. | Medium | **Open:** code signing has its place in Sprint 18 (PLAN); until then authenticity rests on GitHub's account security and TLS. |
| 5 | Dependencies | `lru` 0.16.4 (through `aws-sdk-s3`) is unsound (RUSTSEC-2026-0253): `LruCache::pop()` can leave dangling pointers if a key's `Drop` panics inside `catch_unwind`. The SDK's keys are strings and plain values whose `Drop` doesn't panic, and OpenSesh doesn't catch panics around it. | Low | **Accepted** until the SDK moves to `lru` 0.18.2 or later; `cargo audit` reports it as a warning. |
| 6 | Dependencies | `rsa` (RUSTSEC-2023-0071, the Marvin timing attack) has no fixed release. OpenSesh signs with RSA keys (once per connection) and never decrypts with them. | Low | **Accepted** (ADR 0024, ADR 0027), as before. |
| 7 | Dependencies | `aws-smithy-json` 0.62.3 (through `aws-sdk-s3`) recurses without a limit when it skips unknown keys (GHSA-8ffr-xgwf-xj56, 2026-10-02): deeply nested JSON can exhaust the stack of a smithy-rs **server**. In OpenSesh, `aws-sdk-s3` uses that parser only for the AWS partition table compiled into the SDK (`endpoint_lib/partition.rs`); S3's responses are XML, and `aws-config` isn't used. No JSON from the network reaches it. The fix (0.62.7) needs Rust 1.91.1, above the 1.89 MSRV (ADR 0026). | None reachable | **Accepted** until the MSRV moves to 1.91.1 or later; the Dependabot alerts were dismissed as "vulnerable code is not actually used" (2026-10-03). When RustSec publishes it, `cargo deny` and `cargo audit` will need the same exception. |

## Repository settings (2026-10-03)

Checked with GitHub Security Lab's `gh-secure` (`gh secure status`), after 1.0.0:

- **On:** private vulnerability reporting (with `SECURITY.md`), secret scanning with push protection, Dependabot alerts and security updates (next to the weekly version updates of `.github/dependabot.yml`), and CodeQL's default setup (Rust, C++ and the GitHub Actions workflows).
- **Rulesets instead of classic branch protection** (which `gh-secure` would set for everyone), so that the maintainer keeps pushing to `main` and tagging releases from a script while everyone else goes through review:
  - **`main`:** for everyone but the repository's admin, changes come through a pull request with an approval of its latest push, every review conversation resolved, the ten CI jobs green, and no new CodeQL alert of high severity or above (or error). Nobody but the admin can delete `main` or force-push to it.
  - **Release tags (`v*`):** only the admin can create, move or delete them.
  - **Workflows of outside contributors** run only after the maintainer approves them (every outside contributor, not only first-time ones), so unknown code doesn't run on the project's runners before someone has read it.

### CodeQL's first analysis (2026-10-03)

CodeQL's first run raised 75 alerts in three rules. Each one was read, and none was a vulnerability:

- **Hard-coded cryptographic values (67):**
  - 60 are in tests, test servers and a Sprint 13 spike: made-up keys, nonces and passwords.
  - Four are the vault's header nonce, a placeholder: `format::seal_file` replaces it with a random one before encrypting. A test now checks that every seal takes a fresh, non-zero nonce.
  - One is a buffer filled from the system's random generator on the next line.
  - Two are PuTTY's PPK v2 format, which OpenSesh only reads: the empty passphrase of an unencrypted key's MAC, and the zero IV of its AES-256-CBC.
- **Cleartext logging (7):**
  - Three are in the SSH and S3 password prompts. There is no logging there: the query took the `secrets` binding that receives the user's answer for a log.
  - Two are a test that checks no secret reaches the files.
  - Two are the spike.
- **A weak algorithm (1):** DES in VNC authentication, which RFC 6143 defines that way. The pane says the session isn't encrypted, and VeNCrypt with TLS is offered (ADR 0035).

Each one was dismissed on GitHub with its reason. New alerts block pull requests (see the rulesets above).

## What was checked and held

- **Secrets never logged:** no log call prints a password, passphrase, key, cookie or token; the types that hold secrets print `[REDACTED]` or their size in `Debug` (the vault's tests check it), and the RDP helper installs no log output at all (ADR 0034).
- **Secrets never written in clear:** passwords and private keys live only in `vault.bin` (XChaCha20-Poly1305 under a key held by the keyring or a master password, Argon2id); bundles with a keychain reuse that format under an export password; `hosts.toml`, `keychain.toml` and `snippets.toml` hold `vault:` references only (the `no_plaintext` tests).
- **Private files:** everything OpenSesh writes goes through one function (`fsutil::atomic_write`), which creates files readable by the user only on Unix (`0600`); the data folder is `0700`. On Windows the files inherit the profile folder's permissions.
- **Host keys and certificates:** checked before anything is sent, on every hop; a changed key stops the connection (ADR 0027); RDP and VNC certificates are trusted on first use and then pinned (ADR 0034, ADR 0035).
- **Forwarding:** agent, X11 and Waypipe forwarding are off by default; `x11`, agent and forwarded-socket channels nobody asked for are refused (ADR 0036).
- **What the server sends:** escape sequences, file names, object keys, monitor output, pointer pictures and framebuffer rectangles are bounded or checked before use (threat model); the fuzzers found two crashes in the monitor's parsers, fixed (see [Fuzzing](#fuzzing)).
- **Commands run here:** only for what the user asked (shells, `docker`/`podman`/`kubectl`, `mosh-client`, `waypipe`, `git`), with arguments passed separately, never through a shell built from untrusted text.
- **No network without the user:** the update check is off by default; Git's pull and push run only from their buttons; tests never reach the network (CI's real servers are on the runner).

## Fuzzing

Each target of `fuzz/` ran for an hour on a GitHub runner (the `Fuzz` workflow), starting from seeds made of the repository's fixtures. The first run found three crashes and the second a fourth; each was fixed with a regression test holding the fuzzer's input (the first three fixes also ran under libFuzzer for a minute each locally) before the next hour-long run. **The last run, on the fixed code, went through every target for an hour without a crash.**

| Target | What it reads | Found |
|---|---|---|
| `quick_connect` | quick-connect text and URLs | nothing |
| `paste` | pasted text (the paste analyzer) | nothing |
| `themes` | iTerm2, Windows Terminal, Alacritty, kitty, base16 and OpenSesh theme files | nothing |
| `monitor` | the remote monitor's and the host info's server output | a panic on a macOS swap figure ending in a character of several bytes (split one byte before its end); an overflow when a server's counters (CPU ticks, memory pages, network bytes) add up past `u64::MAX` (every sum saturates now) |
| `ssh_config` | `~/.ssh/config`, with its `Include`s | `Include /*/*` read the whole disk: at most 256 files and 8 MiB in all now |
| `mobaxterm`, `putty`, `remmina`, `csv` | the importers' files | nothing |
| `bundle` | OpenSesh bundles: read, turned into hosts, written again | nothing |
| `sync_merge` | the three-way merge of settings files and Git's conflict splitter | a `nan` value was a conflict on every merge (`NaN != NaN`): values are compared as the same when both are NaN |

The workflow runs again every Sunday and uploads the input of any crash.

## Open items

- **Code signing** of the Windows packages and checksums signed with a release key (Sprint 18).
- **The RDP password** is held by IronRDP as a plain string for the connection's life and not wiped (threat model, Memory).
- **Windows named pipe permissions:** the instance pipe uses Windows' default security for named pipes (the creator and administrators); a request on it can only open windows and tabs (ADR 0021).
