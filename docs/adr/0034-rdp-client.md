# ADR 0034: The RDP client

- **Status:** accepted
- **Date:** 2026-10-02
- **Sprint:** 13

## Context

PLAN Sprint 13 asks for a Windows remote desktop in a tab:
- a spike and an ADR between IronRDP and FreeRDP 3 (NLA/CredSSP, gateway, redirections, license, effort);
- a `FramebufferItem` shared with VNC (Sprint 14) that uploads only what changed, with fit, 1:1 and dynamic resize;
- keyboard (scancodes, respecting the layout), mouse and wheel;
- text clipboard, dynamic resize, full screen, Ctrl+Alt+Del from a menu, and on Wayland the compositor's shortcuts inhibited while focused, with an escape combination;
- through a jump host, an automatic local tunnel;
- advanced options as the backend allows: drive redirection, audio, multi-monitor.

"Done when": it connects to Windows 11 and to xrdp, with the clipboard and resizing working.

## Options

1. **FreeRDP 3:** C, Apache-2.0, the most complete client (gateway, every redirection, H.264).
   - **Windows:** a CMake build with OpenSSL, and DLLs to ship. Our build has no CMake or NASM (ADR 0033 chose `ring` for the same reason).
   - **Linux:** distro packages (`freerdp3` in Debian 13, Fedora and Arch; not in Ubuntu 22.04).
   - **Bindings:** hand-written, for a large callback-driven API.
   - **Tests:** no in-process server, so they need xrdp or Windows.
2. **IronRDP** (Devolutions): Rust, MIT or Apache-2.0, MSRV 1.89 like ours.
   - NLA through `sspi` (NTLM, and Kerberos with a network client);
   - RemoteFX and bitmap codecs, display control, clipboard, device redirection (`rdpdr`), audio (`rdpsnd`), RD Gateway (`mstsgu`);
   - `ironrdp-server` for an in-process test server.

## Decision

**IronRDP 0.17**, with our own connection and session loop on `ironrdp-connector`, `ironrdp-session`, `ironrdp-tokio`, `ironrdp-input`, `ironrdp-cliprdr` and `ironrdp-displaycontrol`, in a Qt-free crate, `opensesh-rdp`.

The spike (`spikes/rdp-ironrdp`) connected with NLA to an in-process server in 7 ms:
- a wrong password was refused with its reason;
- the changed rectangle arrived exactly;
- keys (extended too) and mouse moves reached the server as sent.

### What the spike changed in the design

- **Our own TLS, not `ironrdp-tls`:**
  - Its rustls `upgrade` sets `KeyLogFile`, which writes the session's TLS secrets to disk when `SSLKEYLOGFILE` is set. That breaks our rule that secrets are never written in clear.
  - Ours uses rustls on `ring`, without key logging or resumption (CredSSP forbids resumption).
  - **The server's certificate is checked before any credential is sent:** a verifier keeps it, and CredSSP (`connect_finalize`) only runs after the app accepted it.
- **The server's certificate is trusted on first use,** as SSH host keys (ADR 0027):
  - **The first time:** the pane shows its SHA-256 fingerprint and subject, to trust once or remember.
  - **When it changes:** the pane warns, defaulting to not connecting.
  - **Where:** remembered certificates go in `trusted_certificates.toml` (host and port to fingerprint), which VNC's TLS can share.
- **No `aws-lc` in the app:** the client side doesn't pull it, while `ironrdp-server` does (with `zstd-sys`). So the in-process server is a test dependency, and the smoke test starts it as a separate test program (see Tests).
- **Codecs listed explicitly** (RemoteFX, and QOI only with its decoder), as feature unification can otherwise advertise codecs the session can't decode.
- **`picky-krb` pinned to 0.12.4:** 0.12.5 breaks `sspi` 0.21.3.

### The session

- **The loop:**
  - runs on a tokio task;
  - decodes into a `DecodedImage`;
  - hands the app each changed rectangle's pixels, never the whole frame;
  - also hands the pointer's shape, position and visibility, and the reason the session ended.
- **Resizing:**
  - through display control when the server opened it;
  - else by reconnecting at the new size after the resize settles, as Windows' client does.
- **Clipboard:** text both ways (`CF_UNICODETEXT`), never read by OpenSesh unless the session asks for it while focused.
- **Input:**
  - Qt key events become scancodes: from the native scan code where Qt has it (Windows, X11, Wayland), else from the key.
  - Lock keys are synchronized on focus, and every key is released on focus loss.

### Credentials

- **The identity:** user, domain (`DOMAIN\user`, `user@domain` or the host's `rdp.domain`), and the password from the vault.
- **When something is missing:** it is asked for in the pane.
- **What the connection holds:** IronRDP keeps the password as a plain `String` for that one connection. Our copies are `SecretString`s.

### Through a jump host

A local tunnel on 127.0.0.1 over the built-in SSH client (ADR 0029) is started with the pane and stopped with it; RDP connects through it.

### Later (deviations from the plan, revisited after 1.0)

- **Drive redirection:** `rdpdr` needs a file system backend of ours.
- **Audio:** `rdpsnd-native` brings `cpal`, and ALSA headers to the Linux build.
- **Multi-monitor.**
- **RD Gateway.**
- **Wayland:** the compositor's shortcuts are inhibited only if Qt gives access to `keyboard-shortcuts-inhibit`. The escape combination works everywhere.

### Tests

- **Unit tests:** scancodes, rectangles and the certificate store, in `opensesh-rdp`.
- **In-process server:** sessions against `ironrdp-server` in `opensesh-rdp`'s tests (it is a dev-dependency there).
- **Smoke test:** a small test program, `opensesh-rdp-test-server` (in the workspace, never shipped), runs the same server, and the app's smoke test connects to it when it is built next to the app.
- **CI:** xrdp.
- **Manual:** Windows 11, in the matrix.

## Consequences

- One language and toolchain, and tests without a network or a Windows machine.
- **Fewer features than FreeRDP today:** no H.264, gateway or redirections yet; they come with IronRDP's crates as they mature.
- **Dependencies:** the IronRDP crates, `sspi` (with `picky`, `winscard` and `libz-sys`, C built with `cc`) and `tokio-rustls`. All MIT or Apache-2.0.
