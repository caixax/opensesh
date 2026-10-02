# Sprint 15: remote graphics (X11 and Waypipe)

**Goal:** remote graphical programs without friction.

**Started:** 2026-10-02

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it. The owner asked on 2026-10-02 to keep going from sprint to sprint without waiting.
- **Where things stand:** hosts already have an X11 forwarding setting (off, untrusted, trusted), which only the OpenSSH backend honours (`-X`, `-Y`); the built-in client refuses `x11` channels.
- **X11 in the built-in client** (`russh` 0.63.3 has the pieces: `Channel::request_x11` and the handler's `server_channel_open_x11`):
  - **The request:** `x11-req` on the session channel with a made-up cookie (MIT-MAGIC-COOKIE-1), so the server never sees the real one.
  - **Each X11 channel:** connected to the local display (`DISPLAY`: a Unix socket `/tmp/.X11-unix/X<n>`, or TCP port 6000 + n), with the made-up cookie in the client's first message replaced by the real one, as OpenSSH does; a channel with the wrong cookie is refused.
  - **Untrusted:** the real cookie comes from `xauth generate <display> MIT-MAGIC-COOKIE-1 untrusted` (the X SECURITY extension limits what remote programs can do); **trusted:** from `xauth list`.
  - On Wayland desktops (Hyprland, KDE) the display is Xwayland's.
- **Waypipe** (a spike first): `waypipe client` here on a socket, `waypipe server` on the host for the session's command, and the host's socket forwarded back here (`streamlocal-forward@openssh.com`, which `russh` has). A per-host toggle; a clear message when `waypipe` is missing on either side.
- **Windows:** no X server is bundled for 1.0. The app finds a running one (VcXsrv, X410, Xming: TCP port 6000 + n on localhost, or `DISPLAY` when set) and uses it; the docs say how to install one. WSLg's X server only serves WSL's own programs.
- **Tests:** the cookie substitution and `DISPLAY` parsing as unit tests; a forwarded X11 connection against the in-process SSH server; in CI, OpenSSH with `X11Forwarding yes` and Xvfb, running `xdpyinfo` through the forwarding.

## Checklist

### X11
- [x] `x11-req` with a made-up cookie; `x11` channels accepted only when the host asked for forwarding
- [x] The local display: `DISPLAY` parsed (Unix socket, TCP), the real cookie from `xauth` (untrusted or trusted), the made-up one replaced in the client's first message
- [x] Errors said in the pane: no display, no `xauth`, the server refusing forwarding
- [x] Windows: a running X server found (or `DISPLAY`), and the docs for installing one

### Waypipe
- [x] Spike: `waypipe client` here, `waypipe server` there, the socket forwarded back over the connection
- [x] A per-host toggle; what is missing explained (waypipe here or there, a non-Wayland session)

### Quality
- [ ] **Done when:** remote `xclock` and `gedit` show on Hyprland (Xwayland) and KDE, and the waypipe toggle works for its use case
- [x] Tests: cookies, `DISPLAY`, a forwarded connection against the in-process server; OpenSSH and Xvfb in CI
- [x] The host editor's X11 and Waypipe rows; screenshots

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, deny, audit
- [x] Build and smoke tests on Windows and Linux (CI); GitHub Actions green
- [x] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

**Closed:** 2026-10-02

### What was done

- **X11 forwarding in the built-in client** (`opensesh_ssh::x11`, [ADR 0036](../adr/0036-remote-graphics.md)), as Sprint 7's spike (`spikes/x11-forwarding`) found it can be done:
  - `x11-req` with a cookie made up for the connection (random, from the OS);
  - each `x11` channel's first message read, its cookie checked and replaced by the real one (or by none), then carried to the display both ways;
  - the real cookie from `xauth`: the display's own (trusted), or one made with `xauth generate ... untrusted` in a file of its own (untrusted);
  - the display from `DISPLAY` (Unix socket, TCP, XQuartz's path form), on Windows `localhost:0.0` by default;
  - `x11` channels refused unless the host asked for forwarding (the target hop only).
- **Waypipe** (`opensesh_ssh::waypipe`), as `waypipe ssh` does it: `waypipe client` here in `XDG_RUNTIME_DIR`, the session's command wrapped in `waypipe server` there (after checking the server has it), the server's socket forwarded here with `streamlocal-forward`; forwarded sockets nobody asked for are now refused.
- **Problems said in the terminal,** in yellow, and the session goes on without the forwarding: no display, `xauth` failing, no X SECURITY extension for untrusted, `waypipe` missing here or on the server, no Wayland session here.
- **Hosts:** a Waypipe setting next to X11 forwarding (both off by default), with notes on what untrusted and trusted mean; the editor's screenshot.
- **Tests:** unit tests for the cookie swap, `DISPLAY` and the Waypipe command; the in-process SSH server takes `x11-req` and opens an `x11` channel ("xclock" in its shell), carried to a made-up display; in CI, `xdpyinfo` through OpenSSH to Xvfb (trusted and untrusted) and `wayland-info` through Waypipe to a headless sway.
- **Documentation:** ADR 0036, a user guide (`docs/remote-graphics.md`: X servers on Windows, Waypipe), the threat model, ADR 0027's note, CHANGELOG, README and the manual matrix.

### "Done when" (PLAN)

- **Remote `xclock` and `gedit` show on Hyprland (Xwayland) and KDE:** not checked here (no Linux desktop on this machine; WSL's WSLg is neither). CI shows the path works end to end (`xdpyinfo` through OpenSSH to an X display, trusted and untrusted); the desktops are in the manual matrix for the owner.
- **The waypipe toggle works for its use case:** in CI, a session with Waypipe on runs `wayland-info` on the server, which reaches the headless sway here through `waypipe server`, the forwarded socket and `waypipe client`. A real desktop is in the manual matrix.

### How it was verified

- **Windows (Qt 6.10.3), on the final code:** fmt, clippy `-D warnings`, lint-qml, i18n; every test (691 in the app's workspace); the offscreen smoke test; the screenshots.
- **GitHub Actions:** green on every job of the final code:
  - Ubuntu 24.04, Windows, and the Fedora, Arch and Debian 13 containers: build, clippy, tests and the smoke tests;
  - formatting, lints, licenses and advisories; the `s3`, `rdp` and `vnc` jobs;
  - **the `ssh` job:** the new real-server tests, X11 forwarding to Xvfb (trusted and untrusted) and Waypipe to a headless sway, with the earlier ones.

### Problems found and fixed

- **Waypipe on a server without a login session:** `waypipe server` makes its Wayland display in `XDG_RUNTIME_DIR`, which pointed at a `/run/user/<uid>` that didn't exist (CI's test user has no logind session, as many servers reached over SSH). The remote command now makes a private folder with `mktemp -d` when the user's is missing, and removes it after the session; it also runs in `sh` whatever the user's shell is (fish). A unit test runs the command with a stand-in `waypipe`.
- **Translations not regenerated** after the screenshot page's sample name; `cargo xtask i18n --check` caught it in CI.

### Deviations

- **Windows:** the app doesn't look for a running X server's process (VcXsrv, X410, Xming): it uses `DISPLAY`, else `localhost:0.0`, where all three listen, and says what went wrong when nothing answers. WSLg's X server isn't offered (it only serves WSL's programs).
- **The desktops of "done when"** (Hyprland, KDE) are for the owner's manual check.
