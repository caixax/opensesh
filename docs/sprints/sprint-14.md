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
- [ ] Spike: the options above against the three servers' defaults; the dependencies and the build on Windows and Linux
- [ ] ADR: the VNC client, with the security types and encodings it supports

### The engine (`opensesh-vnc`)
- [ ] Connecting: TCP with a timeout, RFB 3.3, 3.7 and 3.8, no authentication, VNC authentication, VeNCrypt (X509 with VNC or plain authentication; TLS with the server's certificate trusted on first use), the reason it failed
- [ ] The session: Raw, CopyRect, Hextile, ZRLE and Tight (zlib, palette and gradient filters, JPEG), the cursor, the desktop size (and the extended one), a framebuffer with the changed rectangles
- [ ] Input: keysyms from Qt keys and text, the pointer's buttons, the wheel; a read-only mode that sends nothing
- [ ] The clipboard both ways (Latin-1, and UTF-8 with the extended clipboard where the server has it)
- [ ] Resizing: the server's desktop size followed, and asked for (SetDesktopSize) where the server allows it
- [ ] An in-process RFB server for tests

### The app
- [ ] The VNC pane, sharing `DesktopView` with RDP: the questions (certificate, password), the state, scaling, full screen, reconnecting
- [ ] Saved hosts and `vnc://` quick connect; the host editor's VNC rows (read-only, scaling, the quality and compression)
- [ ] Through a jump host: the local tunnel, shared with RDP

### Quality
- [ ] **Done when:** it connects to TigerVNC, x11vnc and wayvnc
- [ ] Tests: the decoders, the security types, a session against the in-process server, the three servers in CI
- [ ] Smoke test: a VNC tab against the in-process server (an image, a key, the clipboard, read-only)
- [ ] Screenshots: a VNC tab, the password question, the host editor

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [ ] Build and smoke tests on Windows and Linux (CI); GitHub Actions green
- [ ] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

(Filled in at the end of the sprint.)
