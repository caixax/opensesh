# Remote graphical programs

OpenSesh can show graphical programs that run on an SSH server on this computer's screen, in two ways. Both are host settings (Hosts > the host > Advanced), off by default, and both work with the built-in SSH client and with OpenSSH. See [ADR 0036](adr/0036-remote-graphics.md) for how they work.

## X11 forwarding

For X11 programs (`xclock`, `gedit` under X, most older programs).

- **Untrusted** (`ssh -X`): remote programs can't read what you type in other windows or what they show (the X SECURITY extension). Some programs misbehave under it.
- **Trusted** (`ssh -Y`): remote programs get full access to your display, including your keystrokes in other windows. Use it only with servers you trust, for programs that need it.

**On the server:** OpenSSH with `X11Forwarding yes` in `sshd_config` (Debian's and Ubuntu's set it; OpenSSH's own default is no) and `xauth` installed (`xauth` on Debian and Ubuntu, `xorg-xauth` on Arch, `xorg-x11-xauth` on Fedora).

**On Linux:** nothing to do on an X11 desktop or a Wayland desktop with Xwayland (GNOME, KDE Plasma, Hyprland, Sway): OpenSesh uses `DISPLAY`. Untrusted forwarding needs `xauth` here too.

**On Windows:** run an X server; OpenSesh connects to `localhost:0.0` unless `DISPLAY` says otherwise.
- **VcXsrv** (free, from its SourceForge page): start XLaunch, choose "Multiple windows", display number 0, and tick "Disable access control" (or put VcXsrv's `xauth.exe` on the `PATH`).
- **X410** (Microsoft Store): local connections work as it comes.
- **Xming** works the same way as VcXsrv.
- WSLg's X server only serves programs inside WSL; it can't show programs from an SSH server.

Then connect: `echo $DISPLAY` on the server shows `localhost:10.0`, and `xclock` opens a window here.

## Waypipe

For Wayland programs, on Linux with a Wayland desktop (GNOME, KDE Plasma, Hyprland, Sway). Waypipe sends the program's windows rather than a whole display, and works well over slow links.

**Install `waypipe` here and on the server** (it is packaged as `waypipe` on Debian, Ubuntu, Fedora and Arch). With the host's Waypipe setting on, the session runs inside `waypipe server`, so programs started in it (`gedit`, `foot`, `nautilus`) open here. The server needs OpenSSH's `AllowStreamLocalForwarding` (on by default). On a server without a login session for you (no `/run/user/<uid>`), a private folder in `/tmp` stands in for it during the session.

When something is missing (Waypipe here or on the server, or a Wayland desktop here), the terminal says so in yellow and the session starts without it.
