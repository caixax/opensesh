# X11 forwarding spike

Sprint 7, Linux. Can the built-in client (russh 0.63.3) forward X11 the way OpenSSH does, onto
X.Org and onto Xwayland? It can: this program runs a command on a server with X11 forwarding and
carries each X11 connection back to the local display.

```sh
# On the server: X11Forwarding yes, and xauth installed (sshd stores the cookie with it).
SPIKE_KEY=~/.ssh/id_ed25519 cargo run --manifest-path spikes/x11-forwarding/Cargo.toml -- \
    user@host:22 "xdpyinfo | head -n 5; xeyes"
SPIKE_PASSWORD=... cargo run --manifest-path spikes/x11-forwarding/Cargo.toml -- user@host xeyes
```

It is a spike: it accepts any host key (printing its fingerprint), and it is not part of the
workspace (it has its own `Cargo.toml` and lock file).

## How it works

1. **A fake cookie for the server.** The session channel gets `x11-req` with a random 16-byte
   MIT-MAGIC-COOKIE-1 made for this run. sshd sets `DISPLAY=localhost:10.0` and stores the fake
   cookie with xauth. The real cookie of the local display never leaves this machine.
2. **The real cookie for the display.** For each `x11` channel the server opens (russh calls
   `Handler::server_channel_open_x11`), the client reads the X11 connection setup the remote
   program sends: a 12-byte header, then the authorization name and data, each padded to 4 bytes.
   A connection that doesn't carry the fake cookie is closed. The setup goes on to the local
   display with the real cookie (from `xauth list $DISPLAY`), or with no authorization when the
   display has no cookie. After the setup, bytes are copied both ways unchanged.
3. **The local display** is `$DISPLAY`: `:N` is the Unix socket `/tmp/.X11-unix/XN`, `host:N`
   is TCP port 6000 + N. The screen number goes in `x11-req`.

## Findings

Tried on 2026-09-27 in the archlinux WSL distro: the servers of `scripts/ssh-test-servers.sh`
(OpenSSH 10.5p1 on 127.0.0.1:2221, with `X11Forwarding yes`), xorg-xauth, xorg-xdpyinfo and
xorg-xeyes on the server side, and WSLg as the local display (`DISPLAY=:0`, Xwayland 24.1.6).

- It works. On the server `DISPLAY` was `localhost:10.0` and `xauth list` showed the fake cookie;
  `xdpyinfo` reported `vendor string: The X.Org Foundation`, `X.Org version: 24.1.6` through the
  forwarded channel (116 bytes up, 9,920 down), and `xeyes` drew its window on the Windows
  desktop until it was stopped (a second channel, 4 KB up, 15 KB down).
- **Xwayland without a cookie.** WSLg's Xwayland has no xauth entry (`~/.Xauthority` doesn't
  exist) and accepts local connections without authorization: the client strips the fake cookie
  and sends none. On X.Org and on Xwayland under GNOME or KDE, `xauth list $DISPLAY` gives the
  real cookie, which the client swaps in. Wayland sessions whose Xwayland starts on demand
  (GNOME) need `DISPLAY` set, which it is in a desktop session.
- **Each X11 connection is a channel.** Programs open several (xeyes opened one, xdpyinfo one);
  `single_connection` stays false, as in OpenSSH.
- **What the app needs (Sprint 15):**
  - `ClientHandler` accepts `x11` channels only when the host enables X11 forwarding. Today it
    refuses them, which is right while forwarding is never requested.
  - An `x11-req` after the PTY request, with `ssh.x11` (off, untrusted, trusted) from the host.
    OpenSSH's "untrusted" also generates an untrusted cookie with `xauth generate`, which limits
    what remote programs can do to the display (X SECURITY extension). That needs xauth on this
    machine; the built-in client would do "trusted" first and say so in the editor.
  - Reading `xauth list` off the GUI thread, once per connection.
  - Refusing channels when the display can't be reached, with a note in the pane.
  - Windows: no local X server to connect to unless the user runs one (VcXsrv, X410): TCP
    `localhost:0` works the same way; the app would ask for the display address.
