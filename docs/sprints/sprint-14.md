# Sprint 14: VNC

**Goal:** VNC in a tab.

**Started:** 2026-10-02

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it. The owner asked on 2026-10-02 to keep going from sprint to sprint without waiting.
- **Spike first** (then an ADR): libvncclient, `vnc-rs` or our own RFB client.
  - **libvncclient:** C, GPL-2.0-or-later (compatible, ADR 0001), the most complete; but a CMake build with its own TLS, zlib and libjpeg on Windows, which our build doesn't have (ADR 0033, ADR 0034).
  - **`vnc-rs` 0.6.0:** Rust, MIT or Apache-2.0, async on tokio. Raw, CopyRect, Tight and ZRLE, VNC authentication, the cursor and desktop size pseudo-encodings. No Hextile (PLAN asks for it) and no VeNCrypt.
  - **Our own client** (`opensesh-vnc`): RFB 3.3 to 3.8 is small and documented (RFC 6143, the community `rfbproto`); everything PLAN asks for, on crates the app already has (`des`, `flate2`, `tokio-rustls` on `ring`) plus a JPEG decoder.
  - **To check in the spike:** what TigerVNC, x11vnc and wayvnc offer by default (security types and encodings), and whether rustls can do what VeNCrypt needs (the X509 subtypes; the anonymous TLS ones need anonymous Diffie-Hellman, which rustls doesn't have).
- **The pane:** the VNC session speaks the same messages as the RDP helper (`opensesh-rdp-protocol`), as a task in the app rather than a program, so Sprint 13's pane serves both.
- **Where the code lives:**
  - **`opensesh-vnc`** (Qt-free, as PLAN §3.1 names it): the connection (TCP, the security types, VeNCrypt's TLS with the server's certificate decided by the app), the session (encodings into a framebuffer with the changed rectangles, the cursor, the desktop size), input (keysyms, the pointer, the wheel), the clipboard, resizing, and an in-process test server.
  - **The app:** a VNC item on Sprint 13's `FramebufferItem`, the remote desktop pane (`DesktopView`) shared with RDP, the host editor's VNC rows.
- **Keys:** VNC sends X keysyms, not scancodes: Qt keys and text become keysyms (Latin-1 and Unicode keysyms for characters).
- **Through a jump host:** the local tunnel of Sprint 13, shared.
- **Tests:** the in-process server for tests, the smoke test and screenshots; TigerVNC, x11vnc and wayvnc in CI.

## Checklist

### Spike and decision
- [x] Spike: the options above against the three servers' defaults; the dependencies and the build on Windows and Linux
- [x] ADR: the VNC client, with the security types and encodings it supports

### The engine (`opensesh-vnc`)
- [x] Connecting: TCP with a timeout, RFB 3.3, 3.7 and 3.8, no authentication, VNC authentication, VeNCrypt (X509 with VNC or plain authentication; TLS with the server's certificate trusted on first use), the reason it failed
- [x] The session: Raw, CopyRect, Hextile, ZRLE and Tight (zlib, palette and gradient filters, JPEG), the cursor, the desktop size (and the extended one), a framebuffer with the changed rectangles
- [x] Input: keysyms from Qt keys and text, the pointer's buttons, the wheel; a read-only mode that sends nothing
- [x] The clipboard both ways in Latin-1 (the extended UTF-8 clipboard is left for later, see Deviations)
- [x] Resizing: the server's desktop size followed, and asked for (SetDesktopSize) where the server allows it
- [x] An in-process RFB server for tests

### The app
- [x] The VNC pane, sharing `DesktopView` with RDP: the questions (certificate, password), the state, scaling, full screen, reconnecting
- [x] Saved hosts and `vnc://` quick connect; the host editor's VNC rows (read-only, scaling, the quality and compression)
- [x] Through a jump host: the local tunnel, shared with RDP

### Quality
- [x] **Done when:** it connects to TigerVNC, x11vnc and wayvnc (CI)
- [x] Tests: the decoders, the security types, a session against the in-process server, the three servers in CI
- [x] Smoke test: a VNC tab against the in-process server (an image, a key, the clipboard, read-only)
- [x] Screenshots: a VNC tab, the password question, the host editor

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [x] Build and smoke tests on Windows and Linux (CI); GitHub Actions green
- [x] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

**Closed:** 2026-10-02

### What was done

- **The decision** ([ADR 0035](../adr/0035-vnc-client.md), spike in `spikes/vnc-client`): an RFB client of our own. `vnc-rs` has no Hextile or VeNCrypt (read from its source), and libvncclient needs a CMake build with its own TLS, zlib and libjpeg.
- **`opensesh-vnc`** (Qt-free, in the app's workspace):
  - **Connecting:** TCP with a timeout; RFB 3.3, 3.7 and 3.8; no authentication, VNC authentication (DES from `des` 0.9, already locked), VeNCrypt 0.2 with X509None, X509Vnc and X509Plain (rustls with `ring`, the certificate's fingerprint, subject and key read with `x509-cert` 0.3 for the app's question). Anonymous TLS is refused with an explanation; a refused password and a refused certificate are told apart.
  - **The session:** 32-bit pixels; CopyRect, Tight (four zlib streams, copy, palette and gradient filters, JPEG through `zune-jpeg` 0.5), ZRLE, Hextile and Raw into a canvas, handing out the rectangles that changed; the cursor, the desktop size and the extended desktop size (SetDesktopSize where the server takes it, kept until the server says it can); the clipboard in Latin-1; read-only sessions; limits on everything the server sends.
  - **Keys:** Qt keys and their text as X keysyms; a key goes up with the keysym it went down with.
  - **`drive`:** a session as the app drives a remote desktop, in the RDP helper's messages (the protocol gained a keysym control and VNC's connect options).
  - **An in-process RFB server** (`testing`): every version and security type, a test certificate, its updates in Tight, ZRLE, Hextile and Raw in turn, the cursor, SetDesktopSize, the clipboard both ways.
- **The app:**
  - A VNC pane is the remote desktop pane of Sprint 13, with its session as a task on the SSH runtime instead of the helper program: the questions, the bar (a View only tag for view-only hosts, and "Not encrypted" in the warning colour for a session without TLS), full screen, scaling (fit by default, following the pane where the server resizes), jump hosts, reconnecting.
  - **Hosts:** saved VNC hosts and `vnc://` quick connect (`vnc://host:1` is display 1); the host editor's rows: scaling, picture quality, view only, clipboard, other viewers.
- **Tests:** decoders, VNC authentication against OpenSSL's DES-ECB, VeNCrypt's negotiation, keysyms, messages; sessions and the drive against the in-process server; the smoke test and the screenshots; TigerVNC, x11vnc and wayvnc in a new CI job.
- **Documentation:** ADR 0035, the threat model, the developer setup, CHANGELOG, README and the manual matrix.

### "Done when" (PLAN)

- **It connects to TigerVNC, x11vnc and wayvnc: yes,** in CI (`vnc` job, Ubuntu 24.04, `scripts/vnc-test-servers.sh`):
  - **TigerVNC 1.13.1** with VNC authentication (the desktop, keys, the pointer, the clipboard both ways through the X session's clipboard, a resize to 800x600 through SetDesktopSize) and with VeNCrypt X509Vnc (the certificate's question, the desktop over TLS);
  - **x11vnc** on Xvfb (the desktop, keys, the pointer);
  - **wayvnc** on a headless sway, VeNCrypt with a user name and password (the desktop, keys).

### How it was verified

- **Windows (Qt 6.10.3), on the final code:**
  - fmt, clippy `-D warnings`, lint-qml, i18n, deny and audit, on both workspaces.
  - Every test: 686 in the app's workspace and 5 in the RDP helper's.
  - The smoke tests: offscreen with the software renderer (557 steps), native (569 steps) and the gallery.
  - Screenshots (native): the whole series, with the VNC pages.
- **GitHub Actions:** green on every job of the final code:
  - Ubuntu 24.04, Windows, and the Fedora, Arch and Debian 13 containers: build, clippy, tests, and the smoke tests with the VNC steps;
  - formatting, lints, licenses and advisories; the `ssh`, `s3` and `rdp` jobs;
  - **the new `vnc` job:** the four real-server tests against TigerVNC, x11vnc and wayvnc.
- **A flaky test made sturdier:** sorting 10,000 SFTP entries took 502 ms against a 500 ms bound in the Arch container (a debug build on a shared runner); the bound is now 2 s, which still catches a sort gone quadratic.

### Problems found and fixed

- **A size asked for too early:** the app asks a VNC server for the pane's size as soon as it connects, before the server's first update says whether it takes SetDesktopSize; the request was dropped. It is now kept until the server says it can.
- **The VNC servers script** broke on an apostrophe inside a parameter expansion's message (bash parses quotes there) before wayvnc started; fixed, and sway runs without Xwayland.
- **The disk filled up** during the sprint (the debug build folder had grown to 102 GB of incremental build output); it was cleaned and the build redone.

### Deviations

- **The extended clipboard** (UTF-8, RFB's 0xC0A1E5CE) isn't asked for: text goes as Latin-1, so other characters arrive as `?`.
- **RSA-AES (RA2) and anonymous TLS** aren't offered (see ADR 0035): TigerVNC's default list also has VNC authentication; wayvnc and TigerVNC take certificates.
- **Apple's (ARD) and RealVNC's own security types** aren't supported; those servers work when they also allow a VNC password.
- **No H.264** (wayvnc's open-h264) and no QEMU extended key events.
