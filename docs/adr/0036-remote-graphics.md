# ADR 0036: Remote graphics (X11 and Waypipe)

- **Status:** accepted
- **Date:** 2026-10-02
- **Sprint:** 15

## Context

PLAN Sprint 15 asks for remote graphical programs without friction:
- X11 forwarding made robust on Linux, untrusted and trusted;
- a Waypipe spike: `waypipe` run here with its socket forwarded (stream-local) or the OpenSSH backend; a per-host toggle;
- on Windows, finding a running X server (VcXsrv, X410, Xming, WSLg) and setting `DISPLAY` for the forwarding, documenting the installation, and no X server bundled in 1.0.

"Done when": remote `xclock` and `gedit` show on Hyprland (Xwayland) and KDE, and the waypipe toggle works for its use case.

Until now, hosts had an X11 setting (off, untrusted, trusted) that only the OpenSSH backend honoured (`-X`, `-Y`); the built-in client refused `x11` channels. Sprint 7's spike (`spikes/x11-forwarding`) showed `russh` 0.63.3 can do it the way OpenSSH does.

## Decision

### X11 in the built-in client (`opensesh_ssh::x11`)

As OpenSSH does it, and as the spike found:
- **A made-up cookie for the server:** the session channel gets `x11-req` with a random MIT-MAGIC-COOKIE-1 made for the connection. sshd stores it with its `xauth`; the real cookie never leaves this computer.
- **The real cookie for the display:** for each `x11` channel the server opens, the X client's first message is read, its made-up cookie checked (a channel without it is closed) and replaced by the real one before it goes to the display. Bytes then go both ways unchanged.
  - **Trusted:** the display's own cookie (`xauth list $DISPLAY`).
  - **Untrusted** (the default when forwarding is on): a cookie made for the connection with `xauth -f <its own file> generate $DISPLAY MIT-MAGIC-COOKIE-1 untrusted timeout 1200`, so the X SECURITY extension keeps remote programs from reading the other windows' input or contents. A display without that extension says so, and suggests trusted.
  - **No `xauth` here**, or a display without a cookie (WSLg's Xwayland, an X server started with `-ac`): the first message goes without authorization.
- **The display:** `DISPLAY` (`:N` is `/tmp/.X11-unix/XN`, `host:N` is TCP port 6000 + N, XQuartz's path form too). On Windows, without `DISPLAY`, `localhost:0.0`: VcXsrv, X410 and Xming listen there.
- **Only when asked:** `x11` channels are accepted only for a host with X11 forwarding on, and only on the target hop; never in test runs.
- **Problems said in the pane** (in yellow, the session goes on without forwarding): no display, a display without the SECURITY extension for untrusted, `xauth` failing.

### Waypipe (`opensesh_ssh::waypipe`)

As `waypipe ssh` does it, over the built-in client:
- **Here:** `waypipe --socket <a socket in XDG_RUNTIME_DIR> client`, for the session (ended, and its socket removed, with it).
- **There:** the session's command becomes `waypipe --socket /tmp/opensesh-waypipe-<random>.sock --unlink-socket server -- <the login shell, or the host's command>`. It runs in `sh` (whatever the user's shell is), and when the user's `XDG_RUNTIME_DIR` is missing (servers without a login session, as in CI) a private one is made with `mktemp -d` for the session and removed after it: `waypipe server` makes its Wayland display there.
- **Between them:** the server's socket is forwarded to this one with `streamlocal-forward@openssh.com` (`ssh -R remote:local`); channels for any other socket are refused (`russh` would accept them by default).
- **A per-host toggle** (`ssh.waypipe`, off by default). Linux only (it needs a Wayland session here); `waypipe` must be installed on both sides; what is missing is said in the pane, and the session goes on without it.

### Windows

No X server is bundled. The app uses `DISPLAY` when it is set, else `localhost:0.0`, where the usual servers listen; the docs say how to install one (VcXsrv or X410, access control off or `xauth` on the PATH). WSLg's X server only serves programs inside WSL, so it isn't offered.

## Consequences

- X11 and Waypipe work with the built-in client, as they did with the OpenSSH backend; the editor's notes say what each mode means.
- **Trusted X11** gives remote programs full access to the display (they can read keystrokes in other windows); the editor says so. Untrusted is the default when forwarding is turned on.
- **Tests:** the cookie substitution and `DISPLAY` parsing (unit), a forwarded X11 connection to a made-up display through the in-process server (it never sees the made-up cookie), and in CI: `xdpyinfo` through OpenSSH to Xvfb (trusted and untrusted), and `wayland-info` through Waypipe to a headless sway. "Done when" on Hyprland and KDE is in the manual matrix.
