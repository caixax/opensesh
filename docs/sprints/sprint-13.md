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
- [x] `spikes/rdp-ironrdp`: dependencies, the Windows and Linux builds, a session with an in-process server (graphics, input, clipboard, resize)
- [x] ADR: the RDP backend, with what each option supports (NLA, gateway, redirections), its license and the effort

### The engine (`opensesh-rdp`)
- [x] Connecting: TCP, TLS, NLA (CredSSP), the server's certificate checked and remembered, a timeout, the reason it failed
- [x] The session: graphics updates into a framebuffer with the changed rectangles, the pointer (shape, position, hidden), the end and its reason
- [x] Input: Qt keys to scancodes (the keyboard layout respected, extended keys), mouse buttons and moves, the wheel; the lock keys' state isn't synchronized (Qt doesn't report it)
- [x] Text clipboard both ways, and dynamic resize (display control) with a fallback when the server can't
- [x] Ctrl+Alt+Del and other key combinations sent on request

### The app
- [x] `FramebufferItem` (C++): a texture updated with the changed rectangles, fit, 1:1 and dynamic resize, the pointer, focus and input forwarding
- [x] The RDP pane in a tab: connecting, the certificate and password questions, the reason it ended, reconnect
- [x] Full screen, and a configurable key combination that gives the keyboard back to OpenSesh
- [ ] On Wayland, keyboard shortcuts inhibited while focused: Qt 6.10 doesn't allow it (see Deviations)
- [x] Saved hosts and `rdp://` quick connect; the host editor's RDP rows (domain, resolution, scaling, clipboard, the advanced options the backend supports)
- [x] Through a jump host: a local tunnel over the built-in SSH client, started and stopped with the pane

### Quality
- [x] **Done when:** it connects to xrdp with the clipboard and resizing working (CI); Windows 11 is in the manual matrix for the owner
- [x] Tests: scancodes, the framebuffer's rectangles, a session against the in-process server, xrdp in CI
- [x] Smoke test: an RDP tab against the in-process server (an image, a key, the clipboard, a resize)
- [x] Screenshots: an RDP tab, the certificate question, the host editor

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [x] Build and smoke tests on Windows and Linux (CI: WSL left alone after Sprint 12's memory trouble); GitHub Actions green
- [x] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

**Closed:** 2026-10-02

### What was done

- **The decision** ([ADR 0034](../adr/0034-rdp-client.md)): IronRDP 0.17, after a spike (`spikes/rdp-ironrdp`) that connected with NLA to an in-process IronRDP server.
  - **In a helper program** (`opensesh-rdp`, workspace `rdp/` with a lock file of its own): `sspi` pins pre-release cryptography that can't share the app's lock file with `russh`.
  - **The protocol** (`opensesh-rdp-protocol`, in the app's workspace): length-prefixed messages over the helper's standard input and output; JSON for control and events, raw messages for the password, clipboard text and pixels.
- **The helper:**
  - **Connecting:** TCP with a timeout, TLS on rustls with `ring` (no key logging, no resumption), the server's certificate reported to the app and waited on before CredSSP, NLA where the server has it and TLS alone where it doesn't (xrdp), a refused password reported as such.
  - **The session:** graphics decoded into an image, the changed rectangles sent with their pixels (opaque), the pointer, display control resizing with reconnecting as the fallback, the text clipboard both ways, Ctrl+Alt+Del, and every key released on request.
  - **It never logs:** IronRDP and `sspi` write NTLM messages to their debug logs.
  - **`cargo xtask rdp`** builds it optimized and puts it next to the app; `--test-server` adds the test server.
- **The app:**
  - **`FramebufferItem`** (C++, for VNC too): tiles of 256 by 256 pixels, each a texture uploaded again only when a changed rectangle touches it; follow the pane, fit or actual size; the keyboard with native scan codes, the mouse and the wheel in desktop pixels; the server's pointer.
  - **`RdpItem`** (cxx-qt, on that base): the helper per pane, kept by pane id so a moved tab keeps its session; the certificate (trusted on first use in `trusted_certificates.toml`), the password (the identity's, or asked) and a jump host's questions in the SSH overlay; reconnecting with a new helper.
  - **The pane** (`DesktopView`): a bar with what it connects to, its state, Ctrl+Alt+Del, full screen and a menu; a monitor icon on its tab; the status bar shows its connection.
  - **The keyboard:** while the desktop has it, every key goes to it, the app's shortcuts included, except "Give the keyboard back to OpenSesh" (Ctrl+Alt+Home, a shortcut setting).
  - **Through jump hosts:** an SSH connection to the last jump host, kept up, and a forward from 127.0.0.1 to the server, started and stopped with the pane.
  - **Hosts:** saved RDP hosts and `rdp://` quick connect open in tabs; the host editor has the domain, scaling, resolution and clipboard rows, and jump hosts.
  - **Packages:** the helper next to `OpenSesh.exe` (zip and installer) and in `/usr/lib/opensesh/` (.deb, .rpm, Arch).
- **Tests:**
  - **The helper against its in-process server** (`opensesh-rdp-testing`, never shipped): the certificate, a refused password then the right one, opaque pixels, keys, a click, the wheel, the clipboard both ways, a resize and the end.
  - **The smoke test** connects through the real helper to `opensesh-rdp-test-server` (a desktop of its own for each connection): the questions, a wrong password first, the desktop's pixels, Ctrl+Alt+Del repainting a square, the clipboard both ways, the desktop following a narrower pane, a disconnection and a new helper; then the same through a jump host over the SSH test server.
  - **Screenshots:** a connected desktop, one asking about its certificate, and the host editor of an RDP host.
  - **CI:** a new `rdp` job runs the helper's clippy and tests, then the helper against xrdp (`scripts/rdp-test-server.sh`); the checks job runs rustfmt, cargo-deny and cargo-audit on `rdp/`; every job that runs the full smoke test builds the helper and the test server first.
- **Documentation:** ADR 0034, the threat model, the developer setup, CHANGELOG, README and the manual matrix.

### "Done when" (PLAN)

- **It connects to xrdp, with the clipboard and resizing working: yes,** in CI (`rdp` job, xrdp 0.9.24 with its Xorg session on Ubuntu 24.04): TLS without NLA, the desktop's pixels, keys and the mouse, the text the X session copied arriving in OpenSesh, and a resize to 800x600 (by connecting again: xrdp accepts display control's layout and keeps its size).
- **It connects to Windows 11:** not checked here (no Windows 11 machine or server with RDP enabled); it is in the manual matrix for the owner. What Windows needs is covered by the in-process server, which is IronRDP's own: NLA (CredSSP with NTLM), the certificate, RemoteFX, display control and the clipboard channel.

### How it was verified

- **Windows (Qt 6.10.3), on the final code:**
  - fmt, clippy `-D warnings`, lint-qml, i18n, deny and audit, on both workspaces.
  - Every test: 666 in the app's workspace and 5 in the helper's.
  - The smoke tests: offscreen with the software renderer as CI runs it (533 steps), native (537 steps) and the gallery.
  - Screenshots (native): the whole series, with the three new remote desktop pages in dark and light, comfortable and compact.
- **WSL:** not run, so as not to run the computer out of memory again (Sprint 12); Linux ran in CI.
- **GitHub Actions:** green on every job of the final code:
  - Ubuntu 24.04, Windows, and the Fedora, Arch and Debian 13 containers: build, clippy, tests, the helper and its test server built next to the app, and the smoke tests with the remote desktop steps (directly and through a jump host);
  - formatting, lints, licenses and advisories, now on both workspaces;
  - the `ssh` and `s3` jobs;
  - **the new `rdp` job:** the helper's clippy and tests, then the helper against xrdp 0.9.24.

### Problems found and fixed

- **A crash when the first desktop arrived** (offscreen smoke test): the software renderer reads a texture node's texture as soon as the node joins the scene graph, and the tiles' nodes had none yet. Each node now gets its texture first.
- **A transparent desktop:** RemoteFX leaves the fourth byte of each pixel at 0, so the tiles were drawn fully transparent while the pixels were right. The helper sends the byte as 255, and the tiles are RGBX (drawn without blending).
- **xrdp kept its size:** it opens display control and accepts a monitor layout, then ignores it. When the desktop hasn't taken the size asked for within 5 seconds, the helper now connects again at that size (as for servers without display control); xrdp then resizes the session.
- **One client at a time:** the test server ran its connections one after the other, so a second desktop (the screenshots open two) waited for the first to end. Each connection now gets a server of its own.

### Deviations

- **Wayland's shortcuts inhibitor:** Qt 6.10's Wayland client doesn't implement `zwp_keyboard_shortcuts_inhibit_manager_v1`, and the window's `wl_surface` is only reachable through a private Qt interface. So the compositor keeps its own shortcuts (Super, Alt+Tab) while a desktop has the keyboard; the release combination works everywhere. Revisited when Qt adds it.
- **The lock keys** (Caps Lock, Num Lock) aren't synchronized when the desktop gets the keyboard: Qt doesn't report their state.
- **Advanced options not done** (as ADR 0034 decided): drive redirection, audio, multiple monitors and RD Gateway.
- **Windows 11** wasn't tested here (see "Done when").
