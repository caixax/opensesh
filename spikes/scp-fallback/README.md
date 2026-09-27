# SCP fallback spike

Sprint 8. Some servers run `scp` but have no SFTP subsystem: OpenSSH with no `Subsystem sftp`
line, Dropbear without an sftp-server next to it, small devices. Can the built-in client (russh
0.63.3) still copy files there? It can: this program speaks the SCP protocol over an exec
channel, both ways, with folders, times and modes.

```sh
# A server without SFTP: port 2225 of scripts/ssh-test-servers.sh.
SPIKE_KEY=/tmp/opensesh-ssh-servers/client_ed25519 \
    cargo run --release --manifest-path spikes/scp-fallback/Cargo.toml -- opensesh-test@127.0.0.1:2225 probe
SPIKE_KEY=... cargo run --release -- user@host:port download <remote path> <local folder>
SPIKE_PASSWORD=... cargo run --release -- user@host:port upload <local path> <remote folder>
```

It is a spike: it accepts any host key (printing its fingerprint), and it is not part of the
workspace (it has its own `Cargo.toml` and lock file).

## How it works

The protocol is the old rcp one, as OpenSSH's `scp.c` speaks it (OpenSSH's own `scp` uses SFTP
since 9.0, and the old protocol with `-O`; the server side is the same `scp` program either way).

1. **Download:** exec `scp -r -p -f -- '<path>'`; the server is the source, the client the sink.
   The client sends a 0 byte, then answers each record with a 0 byte (or `\x02` and a line to
   refuse it).
2. **Upload:** exec `scp -r -p -t -- '<folder>'`; the server is the sink and answers first.
3. **Records** are text lines:
   - `T<mtime> 0 <atime> 0`: the times of the next file or folder (`-p`)
   - `C<mode> <size> <name>`: a file, then `<size>` bytes and a 0 byte
   - `D<mode> 0 <name>` a folder begins, `E` it ends (`-r`)
   - A leading `\x01` is a warning line (a file that couldn't be sent: the copy goes on), `\x02`
     a fatal one.
4. **Paths** go through the remote shell: each one is single-quoted, and `--` ends the options.

## Findings

Tried on 2026-09-27 in the archlinux WSL distro against `scripts/ssh-test-servers.sh`: OpenSSH
10.5p1 without SFTP (port 2225, new for this spike), OpenSSH with SFTP (2221) and Dropbear
2026.94 (2222).

- **It works.** A folder tree (a name with spaces, a name with a quote, an empty file, a
  subfolder) went up and came back identical (`diff -r`), with its modification times (to the
  second) and modes. A 1 GiB random file went up at 154 MiB/s and came back at 621 MiB/s; the
  SHA-256 was the same on this side, on the server and back.
- **Errors are clear.** A missing file comes as a warning line ("No such file or directory",
  exit 1); a missing target folder as a fatal answer to the first record.
- **Probing.** `request_subsystem("sftp")` gets a failure right away where there is no SFTP (2225)
  and a success elsewhere (2221, and Dropbear, which hands it to the system's sftp-server). That is
  the moment to fall back: `command -v scp` says whether the program is there.
- **Each transfer is a command.** Every exec channel starts a session on the server: about 0.8 s
  with OpenSSH and PAM here, 0.01 s with Dropbear. Many small files should go in one `-r` command
  per folder, not one per file.
- **What SCP can't do:**
  - **No listing.** A file pane needs `ls` over exec; its output differs between GNU, BusyBox and
    the BSDs, and names with newlines can't be told apart. `LC_ALL=C ls -la --time-style=+%s` (GNU)
    or `stat -c` where it exists, `ls -la` parsed loosely elsewhere.
  - **No resuming.** A record carries the whole size; a partial file starts again (`dd` with
    `seek=` over exec could append, but only with a shell and coreutils).
  - **No renames, deletes or chmod** except through shell commands (`mv`, `rm -r`, `chmod`), which
    need a POSIX shell.
  - **Names with a newline** can't be sent (the record ends at the newline): they are skipped.
- **Safety.** The sink must refuse names that aren't plain names: `..`, `/`, `.` (CVE-2019-6111,
  CVE-2018-20685), as this spike does. A source never sends what the sink didn't ask for, but a
  hostile server can try.

## What the app would need

- In `opensesh-ssh::sftp`, an `Fs::Scp` next to `Fs::Remote`: transfers through `scp -f/-t`, the
  listing through `ls`, the other operations through shell commands; `Unsupported` for what isn't
  there. The file panes already show that error.
- The fallback only when the SFTP subsystem is refused, with a note in the pane ("This server has
  no SFTP: files go through scp, without resuming").
- The transfer queue keeps its per-file progress (the data of a `C` record is counted as it
  arrives); pausing becomes cancelling that file.

This is a later sprint's work, if servers without SFTP turn up; the app says "This server has no
SFTP" today.
