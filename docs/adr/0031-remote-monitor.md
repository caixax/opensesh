# ADR 0031: The remote monitor

- **Status:** accepted
- **Date:** 2026-09-28
- **Sprint:** 11

## Context

PLAN Sprint 11 asks for "a live status bar, without installing anything on the server":
- CPU, memory, network, disks, uptime and users of the server of the current terminal, every few seconds (3 by default);
- a host info drawer (OS, kernel, architecture, addresses, disks, users) with "copy as text";
- off globally or per host, with a choice of metrics;
- Linux first (busybox too), BSD and macOS as far as possible.

"Done when": the CPU overhead is negligible on both ends, it works on busybox, and it fails cleanly on unsupported systems. The built-in SSH client (ADR 0027) already opens exec channels beside the shell for OS detection and "install my key".

## Options and decisions

### What runs on the server

**Options:**
1. A small agent binary copied to the server.
2. One exec channel per metric, per reading.
3. One exec channel running a shell loop that prints every reading.

The first contradicts "without installing anything", and needs a build per server architecture. The second costs a channel open (and a process start on the server) for each metric every few seconds.

**Decision:** the third, a POSIX `sh` loop on its own exec channel, never in the user's shell.

**Keeping it cheap on the server:** on Linux, `/proc` files are read through the shell's own `read` in a function (`f`), so a reading starts no process for them. Only `df -Pk`, `who` and `sleep` run as programs.

**Ending it:** it ends by itself when the channel closes: the next `echo` fails and `sh` exits. So nothing is left running after a disconnection or when the monitor is stopped.

### Whatever the user's login shell is

The command reaches the server as `sh -c '...'`, run by the user's login shell. So the script is **one line, without single quotes or `!`**:
- bash, zsh and fish pass it to `sh` unchanged;
- csh does too: it would refuse a line break inside quotes, and expand `!`.

A test checks these rules.

### Parsing here, not there

**Options:** format the numbers on the server (`awk`), or send the raw sections.

**Decision:** raw sections marked with `@name` lines (`@cpu`, `@meminfo`, `@netdev`, `@route`, `@uptime`, `@loadavg`, `@df`, `@who`, `@end`). The server needs nothing beyond what every system has, and the parser (`opensesh-ssh::monitor`, no Qt) is tested against real outputs:
- captured on Debian, Arch and busybox;
- for FreeBSD and macOS, written from their tools' documented formats, until real captures replace them.

### Other systems

Without `/proc/stat`, the loop reads:
- `sysctl` for CPUs, memory, `kern.cp_time`, the load and the boot time;
- `route -n get default` and `netstat -ibn` for the network (columns found from the header, from the right: a row without an address has one field less);
- on macOS, `vm_stat`, and `ps -A -o %cpu=` for the CPU.

**When nothing can be read:** a server without a POSIX `sh` (Windows' OpenSSH, a router's CLI) prints an error and ends. It is reported once as unsupported, with the server's own error line; the Info tab shows it, and the status bar shows nothing.

### Rates and what counts

- **Rates:** CPU use and bytes per second come from two readings, so the first reading shows neither. A counter that goes back (a reboot, a new interface) gives no rate rather than a wrong one.
- **Network:** only the **interface of the default route** counts (`/proc/net/route`, `route -n get default`). Summing every interface would count the same bytes twice through a Docker bridge, a container's interface or a VPN.
- **Disks:** pseudo file systems (`tmpfs`, `overlay`, `devfs`...) and bind mounts are left out, except `/`, which is kept whatever it is (a container's root is `overlay`).

### Where it lives in the app

- **In the SSH backend:** the monitor is an option of the session, like OS detection. It starts with each connection, restarts after a reconnection, and its task is aborted when the session ends. Its readings go through the status sink the registry already listens to.
- **In the pane:** the pane keeps the latest one (`TerminalItem.monitor`).
- **The host info** is a one-shot command, run when the side panel's Info tab shows a connected pane (again after a reconnection, or with Refresh).

### Settings

- **On by default:** the readings go only between the user and a server they connected to, and stay in memory.
- **Where to turn it off:** Settings > SSH (all hosts), or per host and group (`ssh.monitor`, inherited like `ssh.detect_os`).
- **Global only:** the interval (1 to 60 s) and the metrics the status bar shows.
- **When a change applies:** on the next connection.

## Consequences

- **Nothing to install or clean up on servers.** The loop is gone when its channel is.
- **Cost:** a reading costs a few milliseconds of CPU on the server and microseconds here ([perf.md](../perf.md)).
- **One more exec channel per monitored session.** A server that limits sessions per connection (`MaxSessions`, 10 by default in OpenSSH) has one fewer for SFTP and tunnels.
- **What a server can do:** it can lie in its readings, as it can in anything it prints. It can't make the client run or write anything: the output is only parsed into numbers and names.
- **Still to capture:** FreeBSD and macOS support rests on documented formats until real captures are added. Other systems (Solaris, AIX) get only what `df` and `who` give.
