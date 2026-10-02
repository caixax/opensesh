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
- [ ] `x11-req` with a made-up cookie; `x11` channels accepted only when the host asked for forwarding
- [ ] The local display: `DISPLAY` parsed (Unix socket, TCP), the real cookie from `xauth` (untrusted or trusted), the made-up one replaced in the client's first message
- [ ] Errors said in the pane: no display, no `xauth`, the server refusing forwarding
- [ ] Windows: a running X server found (or `DISPLAY`), and the docs for installing one

### Waypipe
- [ ] Spike: `waypipe client` here, `waypipe server` there, the socket forwarded back over the connection
- [ ] A per-host toggle; what is missing explained (waypipe here or there, a non-Wayland session)

### Quality
- [ ] **Done when:** remote `xclock` and `gedit` show on Hyprland (Xwayland) and KDE, and the waypipe toggle works for its use case
- [ ] Tests: cookies, `DISPLAY`, a forwarded connection against the in-process server; OpenSSH and Xvfb in CI
- [ ] The host editor's X11 and Waypipe rows; screenshots

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, deny, audit
- [ ] Build and smoke tests on Windows and Linux (CI); GitHub Actions green
- [ ] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

(Filled in at the end of the sprint.)
