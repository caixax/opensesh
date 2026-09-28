# Remote monitor fixtures

What the monitor's command (and the host info command) print on different systems, for the parser's tests.

- **Captured** (the owner's WSL distros, 2026-09-28):
  - `debian.txt`, `arch.txt`: Debian 13 and Arch.
  - `busybox.txt`: busybox 1.30.1's `sh` with only busybox's applets on `PATH`.
  - `info-debian.txt`, `info-busybox.txt`: the host info command in the same places.

  The host name, the user name and the addresses were replaced, and WSL's lines for the Windows drives removed.
- **Written from the tools' documented output formats** (`sysctl -n`, `route -n get default`, `netstat -ibn`, `vm_stat`, `ps -A -o %cpu=`, `df -Pk`, `who`), not captured:
  - `freebsd.txt`: FreeBSD.
  - `macos.txt`: macOS.

  They should be replaced by real captures when a FreeBSD or macOS machine is at hand.
