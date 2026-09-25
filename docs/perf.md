# Performance

PLAN §9 sets the performance budgets and asks for them to be measured and written down here. This page has the budgets, how each number is measured, the commands to repeat it, and the results.

Two layers are measured separately:

- **Engine** (`opensesh-term`: PTY backend, parser, `Term`, snapshots), headless. Measured in Sprint 2, below.
- **GUI** (the app with a terminal tab: render loop, event loop, start-up, the whole process's memory). [Still to measure](#gui-to-measure-after-integration), once the terminal item is integrated.

## Budgets (PLAN §9)

| Budget | Engine (measured) | GUI |
|---|---|---|
| Cold start to a usable window under 1 s on modest hardware (target 500 ms) | not an engine matter: a session starts in the background and returns at once | to measure |
| RAM at rest with 1 terminal under 150 MB (target 100 MB) | one idle session adds about 1 MB; a full 10,000-line scrollback adds 30 MB at 120 columns and 50 MB at 200 columns | to measure (the app with Qt) |
| `cat` of a 100 MB file or `yes` for 10 s: the UI doesn't freeze and input keeps responding | a renderer thread never waited more than 2.5 ms for a snapshot; Ctrl+C ends `yes` in 5 to 8 ms on Linux | to measure (frame intervals, event-loop lag) |
| Key-to-pixel latency comparable to native terminals; render at the display refresh, only with damage | input is handled before output; one `Dirty` per frame; a snapshot with nothing damaged costs 2 µs | to measure |
| Hosts view with 1000 entries: smooth scrolling, search under 16 ms | Sprint 5 | Sprint 5 |

## Environment

| | |
|---|---|
| Machine | Intel Core i7-12700KF (12 cores, 20 threads), 32 GB |
| Windows | Windows 10 Pro 22H2 (build 19045), inbox console host `conhost.exe` 10.0.19041.1; bundled ConPTY 1.24.260710001 ([ADR 0014](adr/0014-bundled-conpty.md)) |
| Linux | WSL2, kernel 6.18.33.2-microsoft-standard-WSL2, Debian 13 (15 GB for the VM) |
| Build | Rust 1.88.0, `--release` (thin LTO, `codegen-units = 1`), `alacritty_terminal` 0.26.0, `portable-pty` 0.9.0 |
| Code | commit `e79a0d2` plus the Sprint 2 verification probes (`crates/opensesh-term/tests/perf.rs`) |
| Date | 2026-09-25 |

## How to measure (engine)

The probes are `#[ignore]`d tests in [`crates/opensesh-term/tests/perf.rs`](../crates/opensesh-term/tests/perf.rs). They generate their data (nothing is committed), print their numbers and assert only that the programs ran.

```sh
P="cargo test --release -p opensesh-term --test perf -- --ignored --nocapture --test-threads=1"
$P engine_parse full_snapshot search_step        # parser, snapshots, search (no PTY)
$P pty_                                          # cat of 100 MiB and yes for 10 s through the PTY
$P --exact memory_idle_session_and_full_scrollback   # alone: other probes' allocations would skew it
```

- **Windows, both console hosts.** The test binaries live in `target/release/deps`, so by default they use the inbox ConPTY. `cargo xtask conpty --dest target/release/deps` puts the bundled `conpty.dll` and `OpenConsole.exe` next to them, and `cargo xtask conpty --remove --dest target/release/deps` takes them away again. Check which host runs with `Get-CimInstance Win32_Process -Filter "Name = 'OpenConsole.exe' OR Name = 'conhost.exe'"` (look for `--headless` in the command line).
- **Linux (WSL).** `wsl.exe -d Debian -- bash -lc 'cd /mnt/i/Projects/opensesh && export CARGO_TARGET_DIR=~/.cache/opensesh-target && <the commands above>'`. The `script(1)` baseline needs util-linux's `script` (Fedora: package `util-linux-script`); without it the probe skips that line.
- Run the probes on an otherwise idle machine, and repeat them: the tables give the range of 2 or 3 runs.

What each probe does:

- **`engine_parse_throughput_with_a_renderer`**: 64 MiB fed to a session at 200 x 60 through the replay backend (no PTY), twice: a mixed corpus (SGR colors, 256 colors, truecolor, bold, CJK, CR LF; about 90 bytes per line) and plain ASCII (79 columns, LF). A renderer-like thread takes a snapshot every 16 ms, as a 60 Hz renderer would.
- **`full_snapshot_cost`**: a 200 x 60 screen with a different 256-color foreground and background in every cell. Full snapshots (forced by a palette change), snapshots with nothing damaged, and full snapshots with a search highlighting 1 cell in 26.
- **`search_step_cost`**: 10,050 lines of 200 columns; one search step without a match (the whole 10,000-line bound), and steps that find a line 1,000 and 9,950 lines up.
- **`pty_cat_throughput_with_a_renderer`**: a 100 MiB file of the mixed corpus through the real PTY into a session at 200 x 60, with the renderer thread, until the program exits. Linux runs `/bin/cat`; Windows runs `cmd /c type` (it writes line by line) and a PowerShell stream copy (`Stream.CopyTo` with 64 KiB blocks, like `cat`).
- **`pty_yes_for_ten_seconds`**: `yes` (Windows: a PowerShell loop writing 12 KiB blocks of `y` lines) for 10 s through the PTY into a session, with the renderer thread; then Ctrl+C through the key encoder and the time until the program is gone.
- **Floors.** Next to each PTY number: the same program through our PTY backend with no engine (output read and dropped; the console host's opening queries answered), and on Linux `script(1)` (the kernel PTY with no terminal at all: `script -qfec "cat FILE" /dev/null` with output to `/dev/null`, and `script -qfc yes /dev/null` read by the probe).
- **"Snapshot" times** are what the renderer thread measured around `Session::snapshot`: the wait for the fair lock (the engine holds it for at most one 16 KiB chunk) plus building the frame, usually a partial one.
- **`memory_idle_session_and_full_scrollback`**: this process's memory (Linux `/proc/self/status`: `VmRSS`, `RssAnon`; Windows `Get-Process`: working set, private bytes) before any session, with one idle user shell at 120 x 40 (the shell's own memory is its own process and isn't counted), then with sessions whose 10,000-line scrollback is full, at 120 x 40 and 200 x 60.

## Engine results (2026-09-25)

### Parser throughput

Replay backend, 200 x 60, with a snapshot every 16 ms.

| Corpus (67.1 MB) | Windows | Linux (Debian 13) |
|---|---|---|
| Mixed: SGR, 256 colors, truecolor, CJK | 108 to 114 MB/s | 103 to 105 MB/s |
| Plain ASCII, 79 columns | 86 to 94 MB/s | 87 to 90 MB/s |
| Snapshot while parsing: p50 / p99 | 0.19 to 0.23 ms / 0.6 to 1.8 ms | 0.23 to 0.26 ms / 0.4 to 1.9 ms |

### Snapshots and search

| 200 x 60 | Windows | Linux |
|---|---|---|
| Full snapshot, every cell colored (mean / max) | 93 to 97 µs / 0.19 to 0.36 ms | 100 to 104 µs / 0.17 to 0.25 ms |
| Snapshot with nothing damaged (the cursor row) | 1.6 to 1.8 µs | 1.7 µs |
| Full snapshot while a search highlights 1 cell in 26 | 229 to 235 µs | 242 to 244 µs |
| Search step without a match, 10,000 lines of 200 columns | 15.5 to 16.5 ms | 16.3 to 18.1 ms |
| Search step finding a line 1,000 / 9,950 lines up | 1.6 to 1.7 ms / 14.0 to 15.3 ms | 1.7 to 1.9 ms / 15.6 to 16.0 ms |

A search step runs on the calling thread under the lock (ADR 0012), so a step over the full 10,000 lines holds the renderer for about one frame. `SessionConfig::search_max_lines` bounds it.

### `cat` of a 100 MiB file through the PTY

104.9 MB of the mixed corpus, 200 x 60. "From the PTY" is what the engine received: the inbox Windows host re-renders the output, so it sends more bytes than the file has.

| Platform and program | Time | Throughput (file) | From the PTY | Snapshot p99 / max | Floor: backend, no engine | Floor: `script(1)` |
|---|---|---|---|---|---|---|
| Linux, `cat` | 1.03 to 1.14 s | 92 to 101 MB/s | 105.9 MB | 0.33 to 0.39 ms / 0.6 to 1.0 ms | 1.10 to 1.12 s (93 to 96 MB/s) | 0.98 s (107 MB/s) |
| Windows inbox host, `cmd /c type` | 118.2 s | 0.9 MB/s | 120.1 MB | 0.22 ms / 1.3 ms | 120.2 s | |
| Windows inbox host, PowerShell copy | 57.4 s | 1.8 MB/s | 120.0 MB | 0.21 ms / 2.5 ms | 55.4 s | |
| Windows bundled host, `cmd /c type` | 49.3 to 49.5 s | 2.1 MB/s | 110.9 MB | 0.19 to 0.20 ms / 0.5 to 1.8 ms | 48.7 s | |
| Windows bundled host, PowerShell copy | 1.92 to 1.95 s | 54 to 55 MB/s | 110.9 MB | 0.33 to 0.40 ms / 0.4 to 0.6 ms | 1.88 s (56 MB/s) | |

- **Linux:** the engine keeps up with the kernel PTY (about 90 % of `script(1)`, and level with the backend alone).
- **Windows:** the console host is the limit in every case: the engine is as fast as the backend with no engine. `type` writes line by line, so even the bundled host manages only about 2 MB/s; block writes reach 55 MB/s with the bundled host and 1.8 MB/s with the inbox one.
- **Renderer:** a snapshot never waited more than 2.5 ms, with p99 under 0.4 ms, so the engine never blocks a frame.

### `yes` for 10 s through the PTY

200 x 60. Windows has no `yes`: a PowerShell loop writes 12 KiB blocks of `y` lines instead.

| Platform | Received in 10 s | Ctrl+C to exit | Snapshot p99 / max | Floor: backend, no engine | Floor: `script(1)` |
|---|---|---|---|---|---|
| Linux, `yes` | 70.9 to 72.3 MB (7.1 to 7.2 MB/s) | 4.9 to 8.1 ms | 0.22 ms / 0.27 to 0.47 ms | 6.8 to 7.0 MB/s | 6.2 to 6.3 MB/s |
| Windows inbox host | 1.4 MB | 292 ms | 0.21 ms / 0.36 ms | 1.4 MB | |
| Windows bundled host | 68 to 76 MB (6.8 to 7.6 MB/s) | 217 to 222 ms | 0.19 to 0.22 ms / 0.9 to 4.5 ms | 7.3 MB/s | |

- 2-byte lines make the PTY itself the limit (the line discipline on Linux, the console host on Windows); the engine is not slower than the floors.
- The inbox host doesn't forward every line: it repaints the screen, so only 1.4 MB arrive.
- Ctrl+C goes through the key encoder and the session's command queue, which the engine handles before output. On Windows most of the 220 to 290 ms is PowerShell stopping its loop (the Sprint 2 research measured 21 ms from Ctrl+C to exit for `ping` through ConPTY).

### Memory

`size_of::<alacritty_terminal::term::cell::Cell>()` is 24 bytes, and every row of the scrollback has all its cells, so a full scrollback costs `(10,000 + lines) x columns x 24` bytes: 28.9 MB at 120 x 40 and 48.3 MB at 200 x 60.

| This process | Linux RSS (anonymous) | Windows working set (private) |
|---|---|---|
| Before any session | 3.5 MB (0.4 MB) | 4.6 MB (1.0 MB) |
| + one idle session (user shell, 120 x 40) | 4.5 MB (0.8 MB): +1.0 MB | 5.7 MB (3.7 MB): +1.1 MB |
| + a session with a full scrollback at 120 x 40 | 34.2 MB (30.5 MB): +29.7 MB | 36.9 MB (38.2 MB): +31.2 MB |
| + a session with a full scrollback at 200 x 60 | 84.0 MB (80.4 MB): +49.8 MB | 87.1 to 87.3 MB (91.8 to 92.1 MB): +50.3 MB |

The engine's own cost is small; the scrollback dominates. With the 150 MB budget (target 100 MB) for the whole app, a wide terminal with a full 10,000-line history takes a third of it; several such tabs exceed it. The scrollback size becomes a user setting in Sprint 3.

## GUI (to measure after integration)

**Not measured yet.** These need the terminal item in the app. The method for each:

| Measurement | Method | Budget or proposed pass mark |
|---|---|---|
| Frame intervals under `yes` for 10 s and `cat` of 100 MB | Timestamps of `QQuickWindow::frameSwapped` in a terminal tab (behind a debug switch, for example `OPENSESH_PERF=1`): p50, p99, max and the number of frames longer than twice the refresh period. `QSG_RENDER_TIMING=1` and the `qt.scenegraph.time.renderloop` logging category for render times. | frames at the display rate while output flows; no stall over 100 ms |
| UI responsiveness under `yes` and `cat` | Event-loop lag: a 5 ms `QTimer` on the GUI thread records how late it fires (p99, max). Ctrl+C to prompt, opening a menu and switching tabs during the flood. | lag p99 under 16 ms and max under 50 ms; Ctrl+C to prompt under 250 ms on Linux (report Windows separately: the console host adds its own time) |
| Key-to-pixel latency | In-app timestamps: key event, PTY write, the echo parsed, the next `frameSwapped` (this leaves out OS input and scan-out). Optionally a camera or Typometer against Alacritty, Konsole and Windows Terminal on the same machine. | comparable to native terminals |
| Cold start | Process start to the first `frameSwapped` of the main window, logged by the app; median of 5 runs. Cold means after a reboot (Windows) or `echo 3 > /proc/sys/vm/drop_caches` (Linux); report warm starts too. | under 1 s (target 500 ms) |
| RAM of the app with one terminal | After 5 s idle with one local terminal: Linux `VmRSS` and `Pss` (`/proc/<pid>/smaps_rollup`, which shares the Qt libraries fairly); Windows working set and private bytes (`Get-Process`). Again after `yes` fills the scrollback. | under 150 MB (target 100 MB) |
| vtebench in a tab | [vtebench](https://github.com/alacritty/vtebench) (cloned inside Linux) in an OpenSesh tab, in Alacritty and in Windows Terminal on the same machine, with the `script(1)` floor next to them. | comparable to Alacritty |
