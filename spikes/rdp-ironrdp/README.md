# Spike: an RDP client on IronRDP (Sprint 13)

**Question:** can OpenSesh's RDP client be built on IronRDP (pure Rust) rather than FreeRDP 3 (C, with bindings), on our toolchain (Rust 1.89, MSVC on Windows, no CMake or NASM), with an in-process server for tests?

**Program:** `src/main.rs` runs an `ironrdp-server` with NLA (CredSSP) on 127.0.0.1, in a thread of its own, and connects to it with a client made of `ironrdp-connector`, `ironrdp-session`, `ironrdp-tokio` and our own TLS (`tokio-rustls` on `ring`):
- a wrong password, then the right one;
- the first graphics update into a `DecodedImage`;
- a key, an extended key and a mouse move, checked on the server's side;
- a resize request, then a graceful shutdown.

```sh
cargo run --release
```

## Results (Windows 10, Rust 1.89, 2026-10-02)

```
wrong password refused: CredSSP: InvalidToken: ... NSTATUS code 0xc007030c
connected in 6.8ms: desktop 320x200, certificate 403 bytes
graphics: [InclusiveRectangle { left: 10, top: 20, right: 73, bottom: 51 }]; pixel (12, 22) = [141, 96, 45, 255]
server saw keys [(30, false, true), (30, false, false), (83, true, true), (83, true, false)] and moves [(100, 50)]
the server has no display control: a resize means reconnecting
all good in 33ms
```

- **NLA works** with NTLM: no Kerberos (`connect_finalize` takes a network client for the KDC; ours refuses, which is enough for NTLM).
- **Graphics come as the rectangle that changed,** exactly the server's block. RemoteFX is lossy: the colour is close, not equal.
- **Input:** scancodes with the extended flag, and mouse positions, reach the server as sent.
- **Display control** (resizing without reconnecting) isn't open right after the first image: its dynamic channel opens later, and a server may not offer it. A fallback (reconnecting at the new size) is needed, as `ironrdp-client` does.

## Findings that shape the design

- **TLS: our own, not `ironrdp-tls`.** Its rustls `upgrade` sets `KeyLogFile`: with `SSLKEYLOGFILE` set in the environment, the session's TLS secrets would be written to disk. It also accepts any certificate. Ours (40 lines) uses `ring`, no key log, no resumption (CredSSP forbids it), and a verifier that keeps the server's certificate so the app can check it (trust on first use, as SSH host keys) **before any credential is sent**: `connect_finalize`, which runs CredSSP, comes after the check.
- **No `aws-lc` on the client side.** `ironrdp-connector` pulls `sspi` with its default features, but `sspi`'s `aws-lc-rs` feature only reaches rustls through `rustls?/...`, which stays off without `network_client` or `tsssp`. **`ironrdp-server` does pull `aws-lc`** (it takes `tokio-rustls` with default features), and `zstd-sys` (C, built with `cc`). It compiles on Windows with MSVC alone, but it should not ship in the app: the in-process server belongs in tests only.
- **Feature unification:** with the server in the same build, `ironrdp-pdu`'s `qoi`/`qoiz` are on, so the client advertises QOI, which `ironrdp-session` can't decode without its own `qoi` feature ("Unsupported codec ID: 11"). The client must list its codecs explicitly.
- **`picky-krb` 0.12.5 breaks `sspi` 0.21.3** (a new enum variant in a patch release: `non-exhaustive patterns`). Pin `picky-krb = "=0.12.4"`.
- **`ironrdp-server`'s futures aren't `Send`:** it runs on a current-thread runtime with a `LocalSet`, on a thread of its own, and its handler traits are `async_trait`.
- **Credentials:** `ironrdp_connector::Credentials` holds the password as a plain `String`. It lives in memory for the connection only; the app's own copies are `SecretString`s and are zeroized.
- **Libraries:** all MIT or Apache-2.0. New C code: `libz-sys` (through `sspi` → `winscard` → `flate2`), built with `cc`.

## FreeRDP 3, for comparison (not built)

- C, Apache-2.0, the most complete client (gateway, every redirection, H.264).
- Windows: a CMake build with OpenSSL (or a vcpkg toolchain) and DLLs to ship, against the rule of no CMake/NASM in our build.
- Linux: distro packages (`freerdp3` in Debian 13, Fedora and Arch; not in Ubuntu 22.04).
- Bindings: hand-written (its callback-heavy API through `cxx`).
- Tests: no in-process server; xrdp or Windows only.

**Conclusion:** IronRDP. See ADR 0034.
