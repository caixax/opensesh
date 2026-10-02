# Sprint 13: RDP

**Goal:** a Windows remote desktop in a tab.

**Started:** 2026-10-02

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it. The owner asked on 2026-10-02 to keep going from sprint to sprint without waiting.
- **Spike first** (`spikes/rdp-ironrdp`, then an ADR): IronRDP or FreeRDP 3.
  - **IronRDP:** Rust, MIT or Apache-2.0, MSRV 1.89 (ours). NLA through `sspi`, TLS through `ironrdp-tls` with rustls on `ring` (as SSH and S3). An `ironrdp-server` crate could be the in-process test server.
  - **FreeRDP 3:** C, Apache-2.0. Wider support (gateway, redirections), but a C build on Windows, distro packages on Linux, and hand-written bindings.
  - **To check in the spike:** the dependency tree (no `aws-lc`), the build on Windows and Linux, a connection to an in-process server with graphics, input and the clipboard, and what a real server (xrdp) needs.
- **Where the code lives:**
  - **`opensesh-rdp`** (new, as PLAN §3.1 names it, Qt-free): the connection (TLS, NLA, the server's certificate), the session (graphics into a framebuffer with the rectangles that changed, the pointer), input (scancodes, mouse, wheel), text clipboard, resizing.
  - **The app:**
    - **`FramebufferItem`:** a C++ Qt Quick item, shared with VNC in Sprint 14, that uploads only what changed, with fit, 1:1 and dynamic resize.
    - **The RDP pane,** in a tab like a terminal.
    - **The host editor's RDP rows.**
- **The server's certificate:** RDP servers mostly use self-signed certificates. The first one is shown with its fingerprint to trust once or remember, and a changed one warns, as for SSH host keys.
- **Credentials:** the host's identity (user, domain and password from the vault), else asked in the pane.
- **Through a jump host:** an automatic local tunnel over the built-in SSH client.
- **Tests:**
  - an in-process RDP server if the spike confirms it, for tests and the smoke test;
  - xrdp in CI;
  - Windows 11 in the manual matrix.

## Checklist

### Spike and decision
- [ ] `spikes/rdp-ironrdp`: dependencies, the Windows and Linux builds, a session with an in-process server (graphics, input, clipboard, resize)
- [ ] ADR: the RDP backend, with what each option supports (NLA, gateway, redirections), its license and the effort

### The engine (`opensesh-rdp`)
- [ ] Connecting: TCP, TLS, NLA (CredSSP), the server's certificate checked and remembered, a timeout, the reason it failed
- [ ] The session: graphics updates into a framebuffer with the changed rectangles, the pointer (shape, position, hidden), the end and its reason
- [ ] Input: Qt keys to scancodes (the keyboard layout respected, extended keys, the lock keys), mouse buttons and moves, the wheel
- [ ] Text clipboard both ways, and dynamic resize (display control) with a fallback when the server can't
- [ ] Ctrl+Alt+Del and other key combinations sent on request

### The app
- [ ] `FramebufferItem` (C++): a texture updated with the changed rectangles, fit, 1:1 and dynamic resize, the pointer, focus and input forwarding
- [ ] The RDP pane in a tab: connecting, the certificate and password questions, the reason it ended, reconnect
- [ ] Full screen, and a configurable key combination that gives the keyboard back to OpenSesh; on Wayland, keyboard shortcuts inhibited while focused (if Qt allows it)
- [ ] Saved hosts and `rdp://` quick connect; the host editor's RDP rows (domain, resolution, scaling, clipboard, the advanced options the backend supports)
- [ ] Through a jump host: a local tunnel over the built-in SSH client, started and stopped with the pane

### Quality
- [ ] **Done when:** it connects to Windows 11 and to xrdp, with the clipboard and resizing working
- [ ] Tests: scancodes, the framebuffer's rectangles, a session against the in-process server, xrdp in CI
- [ ] Smoke test: an RDP tab against the in-process server (an image, a key, the clipboard, a resize)
- [ ] Screenshots: an RDP tab, the certificate question, the host editor

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [ ] Build and smoke tests on Windows and Linux (WSL if memory allows, else CI); GitHub Actions green
- [ ] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

(Filled in at the end of the sprint.)
