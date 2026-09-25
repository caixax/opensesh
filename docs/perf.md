# Performance

PLAN §9 sets the performance budgets and asks for them to be measured and written down here. This page has the budgets, how each number is measured, the commands to repeat it, and the results.

Two layers are measured separately:

- **Engine** (`opensesh-term`: PTY backend, parser, `Term`, snapshots), headless. Measured in Sprint 2, below.
- **GUI** (the release app with a real terminal tab: start-up, the whole process's memory, render loop, event loop, input). Measured in Sprint 2 after the terminal item was integrated, [below](#gui-results-2026-09-25).

## Budgets (PLAN §9)

| Budget | Engine (measured) | GUI (measured) |
|---|---|---|
| Cold start to a usable window under 1 s on modest hardware (target 500 ms) | not an engine matter: a session starts in the background and returns at once | **Pass.** Process start to the first frame: Windows median 343 ms (warm file cache: no reboot); Debian WSLg median 417 ms on Wayland and 445 ms on X11 after dropping the page cache, 179 ms and 193 ms warm |
| RAM at rest with 1 terminal under 150 MB (target 100 MB) | one idle session adds about 1 MB; a full 10,000-line scrollback adds 30 MB at 120 columns and 50 MB at 200 columns | **Windows: pass, target missed:** 117.5 MB working set (135 MB private) with one idle tab. **Debian WSLg: fail:** 212 MB RSS (201 MB PSS), about 115 MB of it Mesa's software renderer. A full scrollback adds 28 MB at 124 columns and 45 MB at 204 columns, which takes a maximized Windows tab to 167.5 MB |
| `cat` of a 100 MB file or `yes` for 10 s: the UI doesn't freeze and input keeps responding | a renderer thread never waited more than 4.5 ms for a snapshot; Ctrl+C ends `yes` in 5 to 8 ms on Linux | **Pass.** Frames at the display rate while output flows; event-loop lag p99 under 1 ms on Windows and under 4.5 ms on Linux; typed keys reach the program in 0.8 ms (Windows) and 1.9 to 3.1 ms (Linux) at the median during `yes`; Ctrl+C to the prompt 57 to 101 ms on Windows (`yes.exe`, bundled ConPTY) and at most 12 ms on Linux (median 7.5 ms). A few isolated stalls of 34 to 63 ms, none of 100 ms |
| Key-to-pixel latency comparable to native terminals; render at the display refresh, only with damage | input is handled before output; one `Dirty` per frame; a snapshot with nothing damaged costs 2 µs | **Pass on Windows.** Key to pixels p50 5.9 to 6.1 ms, against 8.1 ms for Windows Terminal and 13.8 ms for the console host with the same probe. 180 frames/s on the 180 Hz monitor while output flows, no frames when idle except the cursor blink. On WSLg the software renderer isn't throttled to the 60 Hz display (144 frames/s during floods) |
| Hosts view with 1000 entries: smooth scrolling, search under 16 ms | Sprint 5 | Sprint 5 |

## Environment

| | |
|---|---|
| Machine | Intel Core i7-12700KF (12 cores, 20 threads), 32 GB |
| Windows | Windows 10 Pro 22H2 (build 19045), inbox console host `conhost.exe` 10.0.19041.1; bundled ConPTY 1.24.260710001 ([ADR 0014](adr/0014-bundled-conpty.md)) |
| Linux | WSL2, kernel 6.18.33.2-microsoft-standard-WSL2, Debian 13 (15 GB for the VM) |
| Build | Rust 1.88.0, `--release` (thin LTO, `codegen-units = 1`), `alacritty_terminal` 0.26.0, `portable-pty` 0.9.0 |
| Code | engine: commit `e79a0d2` plus the Sprint 2 verification probes (`crates/opensesh-term/tests/perf.rs`); GUI: commit `a47d994` |
| GPU and display | NVIDIA GeForce RTX 3060 (driver 32.0.16.1074); 1920 x 1080 at 180 Hz, 100 % scale |
| GUI on Windows | Qt 6.10.3, D3D11, threaded render loop (Qt measures a 5.56 ms vsync); portable mode; the bundled ConPTY unless a row says "inbox". `yes.exe` and `cat.exe` from Git for Windows 2.55.0 (MSYS2 runtime 3.6.9); the shell is Windows PowerShell 5.1 |
| GUI on Linux | Debian 13 in WSLg 1.0.73.2 (Weston, Xwayland 24.1.6), Qt 6.8.2 from Debian, OpenGL, threaded render loop, 60 Hz vsync. Mesa 25.0.7 renders on the CPU: the process maps `libgallium` and `libLLVM` but no d3d12 library. The shell is bash 5.2 |
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
- **Renderer:** a snapshot never waited more than 4.5 ms (the bundled-host `yes` run), with p99 under 0.4 ms. The engine parses at most one 16 KiB chunk per lock hold, synchronized updates included (a resize of a full scrollback is the one longer hold: it reflows every line).

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

The engine's own cost is small; the scrollback dominates. With the 150 MB budget (target 100 MB) for the whole app, a wide terminal with a full 10,000-line history takes a third of it; several such tabs exceed it. The scrollback size becomes a user setting in Sprint 3. The GUI measurements below confirm it: one maximized tab with a full history takes the Windows app to 167.5 MB.

## How to measure (GUI)

Everything is measured from outside the app, on release builds in portable mode (an empty `portable` file next to the executable, deleted afterwards), so nothing in the app is instrumented beyond its existing logs. The probe scripts are not in the repository; this section says exactly what they do.

**Logs.** Every run sets `OPENSESH_LOG=info,qt=debug`, `QT_LOGGING_RULES=qt.scenegraph.time.renderloop.debug=true;qt.scenegraph.general.debug=true` and `OPENSESH_TERMINAL_STATS=1`, and keeps stderr. Qt's threaded render loop then logs every frame twice, with the log's microsecond UTC timestamp: on the render thread (`frame rendered in N ms, sync=, render=, swap=`) and on the GUI thread (`Frame prepared, ... blockedForSync=N ms`). The terminal item logs its own frames per second and snapshot (sync) times every 2 s ([ADR 0013](adr/0013-terminal-rendering.md)).

**Input.** Windows: `SendInput` (`keybd_event`) to the focused window; commands are typed with `WScript.Shell.SendKeys`. Linux: XTest into Xwayland, so the app runs with `QT_QPA_PLATFORM=xcb`; the WSLg window is first brought to the Windows foreground (`SetForegroundWindow`), and the first injected key after that is lost, so the probe sends a harmless key first. XTest can't reach Wayland clients, so on Wayland only start-up and the Hosts view's memory were measured. (Windows `SendInput` into the WSLg window does reach them, as in the [IME check](testing/ime.md), but its clock can't be matched to the VM's.)

| What | How |
|---|---|
| Start-up | Process creation (Windows `Get-Process` `StartTime`; Linux the probe's clock just before `exec`) to the first render-loop `frame rendered` line with a non-zero time. Linux "cold": `sync; echo 3 > /proc/sys/vm/drop_caches` as root before each run. Windows was not rebooted, so its runs are warm. |
| Memory | 5 s after opening one terminal tab, then after `seq 20000` twice (the 10,000-line scrollback is full after the first). Windows: `WorkingSet64` and `PrivateMemorySize64`; Linux: `VmRSS`, `RssAnon` and `Pss` from `/proc/<pid>/smaps_rollup`. The shell and the console host are separate processes, reported apart. |
| Floods | `yes` for 10 s, then Ctrl+C: GNU `yes` on Linux, Git for Windows' `yes.exe` on Windows. `cat` of a 100 MiB file of SGR-colored ASCII lines (about 85 bytes each): `/bin/cat` on Linux; on Windows the PowerShell stream copy of the engine probe (64 KiB block writes), `cmd /c type` and Git's `cat.exe`. Enter, Ctrl+C and the prompt are timestamped (below), so the frames and lag samples inside the flood can be picked from the logs. |
| Frame intervals | Differences between consecutive render-thread `frame rendered` lines inside the flood: p50, p99, max, and how many exceed two refresh periods (11.1 ms at 180 Hz), 33 ms and 100 ms. Output-free moments (a program starting) produce no frames and count as gaps. |
| Event-loop lag | Windows: another process calls `SendMessageTimeout(hwnd, WM_NULL, SMTO_NORMAL)` every 10 ms and times the return; the GUI thread only answers when it is back in its message loop. Linux: every 10 ms a `ClientMessage` of a private type goes to the window; with `qt.qpa.events.debug=true` added to `QT_LOGGING_RULES`, Qt's GUI thread logs each X event it handles, and the lag is that log time minus the send time. (Qt 6.8 answered no `_NET_WM_PING` round trip that a root-window listener could see in WSLg, so there is no round trip.) |
| Typing and Ctrl+C | The shell's prompt function (PowerShell `prompt`, bash `PROMPT_COMMAND` with `$EPOCHREALTIME`) appends its time to a file, on the probe's clock (QueryPerformanceCounter on Windows, `CLOCK_REALTIME` in the WSL VM). A reader in the tab (PowerShell `[Console]::ReadKey`, a raw-mode Python loop) logs when each key arrives, while `yes` writes to the same terminal in the background. "Ctrl+C to prompt" ends when the shell runs its prompt function; "on screen" adds the time to the next rendered frame. |
| Key to pixels (Windows) | The reader toggles a 40-cell `#` bar on row 5 at each key and parks the cursor at the top left. The probe sends `a` with `SendInput`, then grabs the bar's rectangle with `BitBlt` from the screen DC in a loop until its brightness moves past half-way; 40 keys, 150 to 250 ms apart. A grab takes 2 to 3 ms, so the numbers are good to about 3 ms; they include the compositor but not the monitor's scan-out. The same reader and probe run in the console host (`conhost.exe`) and in Windows Terminal 1.24.11911.0. |
| Key to frame (Linux) | Key sent (XTest) to the first `frame rendered` line after the reader got the key. |

The Windows flood runs used a maximized window (204 x 47 cells at 9 x 20 px); the memory runs used the default window (124 x 33 cells) and one maximized run. Linux used the default window (124 x 33). vtebench in a tab was not run.

## GUI results (2026-09-25)

### Start-up

Process creation to the first frame of the main window (Hosts view).

| Platform | Runs | Result |
|---|---|---|
| Windows, D3D11, warm file cache | 7 | 338 to 444 ms, **median 343 ms** |
| Windows, WARP (`QSG_RHI_PREFER_SOFTWARE_RENDERER=1`) | 2 | 228 and 236 ms |
| Debian WSLg, Wayland, after dropping the page cache | 5 | 409 to 465 ms, **median 417 ms** |
| Debian WSLg, X11, after dropping the page cache | 5 | 438 to 484 ms, **median 445 ms** |
| Debian WSLg, Wayland, warm | 5 | 168 to 183 ms, median 179 ms |
| Debian WSLg, X11, warm | 5 | 189 to 205 ms, median 193 ms |

Where the Windows time goes (log timestamps): about 20 ms to the first log line, 25 ms to create the `QGuiApplication`, 175 ms to load the QML and show the window, 106 to 114 ms to create the D3D11 device (`Creating QRhi` to `Created QRhi`; the GUI thread is blocked for sync for 118 to 147 ms on the first frame), and about 15 ms to the first frame. WARP skips the GPU driver's start-up, which is why it starts faster.

### Memory

Windows, the app's process (working set / private bytes). The shell (`powershell.exe`, 71 to 72 MB) and the console host (`OpenConsole.exe` 8.4 MB, inbox `conhost.exe` 6.6 MB) are not included.

| State | 124 x 33 (3 runs) | 204 x 47, maximized (1 run) |
|---|---|---|
| Hosts view, no tab | 109.8 to 110.4 MB / 126.1 to 126.8 MB | 111.9 MB / 135.8 MB |
| **One idle terminal tab** | **117.4 to 117.9 MB / 134.4 to 135.8 MB** | 122.2 MB / 144.5 MB |
| After filling the 10,000-line scrollback | 145.1 to 146.0 MB / 163.1 to 164.7 MB | 167.5 MB / 191.8 MB |

The scrollback costs what the engine section predicts: +28 MB at 124 columns, +45 MB at 204 columns. Most of the rest is the graphics stack, as the Hosts view with each Qt Quick backend shows (working set / private, 2 runs each): D3D11 110 / 127 MB, WARP 121 / 90 MB, the software backend 94 / 69 MB, OpenGL 166 / 196 MB, Vulkan 214 / 304 MB, D3D12 245 / 311 MB. So the NVIDIA D3D11 driver adds about 37 MB of private memory; D3D11 is the right default.

Debian WSLg, X11, 124 x 33 (3 runs; bash takes another 4.1 MB):

| State | RSS | RssAnon | PSS |
|---|---|---|---|
| Hosts view, no tab | 201.4 to 201.9 MB | 78.7 to 79.1 MB | 190.8 to 191.4 MB |
| **One idle terminal tab** | **211.5 to 212.6 MB** | 88.3 to 89.2 MB | **200.8 to 202.0 MB** |
| After filling the scrollback | 240.4 to 241.6 MB | 117.2 to 118.1 MB | 229.7 to 230.9 MB |

On Wayland the Hosts view takes 221 MB RSS (207 MB PSS). The largest mappings of the X11 Hosts view are `libLLVM.so.19.1` (54.7 MB), anonymous memory (43 MB), the heap (22 MB) and `libgallium` (10.7 MB): Mesa's CPU renderer. With `QT_QUICK_BACKEND=software` the same view takes 87.8 MB RSS (77.8 MB PSS). On Linux with a GPU driver the number will differ; only WSLg was measured.

### Floods on Windows

Bundled ConPTY, 204 x 47, except where noted. Lag is the `WM_NULL` round trip during the flood; "sync" is the terminal item's own snapshot and geometry time per frame.

| Program | Runs | Duration | Frames/s | Frame interval p99 / max | Event-loop lag p99 / max | Sync avg / max |
|---|---|---|---|---|---|---|
| `yes.exe` for 10 s | 4 | 10 s | 179 to 180 | 6.7 to 7.3 ms / 16.9 to 44.8 ms | 0.76 to 0.91 ms / 2.4 to 38.8 ms | 0.19 to 0.20 ms / 2.8 to 5.3 ms |
| PowerShell stream copy, 100 MiB | 4 | 1.05 to 1.07 s | 159 to 163 | 11.0 to 14.1 ms / 80 to 107 ms (1) | 0.8 to 4.3 ms / 4.8 to 5.9 ms | 0.40 ms / 1.7 to 3.1 ms |
| `cmd /c type`, 100 MiB | 1 | 49.9 s | 180 | 7.1 ms / 16.6 ms | 0.93 ms / 4.5 ms | 0.29 ms / 1.4 ms |
| Git `cat.exe`, 100 MiB, 124 x 33 | 1 | 225 s | 180 | 7.1 ms / 41.3 ms (2) | 0.80 ms / 46.2 ms (2) | 0.15 ms / 1.4 ms |
| Inbox ConPTY: `yes.exe` for 10 s | 2 | 10 s | 179 to 180 | 6.6 to 6.9 ms / 11.0 to 13.4 ms | 0.72 to 0.73 ms / 3.2 to 5.7 ms | 0.18 to 0.19 ms / 0.8 to 1.8 ms |
| Inbox ConPTY: PowerShell stream copy | 1 | 38.0 s | 179 | 6.8 ms / 107 ms (1) | 0.84 ms / 3.8 ms | 0.30 ms / 1.2 ms |

The `yes.exe` maxima (a 44.8 ms interval, a 38.8 ms lag) are one moment of one run, 3.3 s in; the other three runs stayed under 20.2 ms and 5.2 ms.

1. The long gap is at the start, while `powershell.exe` starts and nothing is written; once output flows, no interval exceeds 16.5 ms. The first output frames block the GUI thread for up to 11 ms while new glyphs are rasterized.
2. At 25 to 30 s into the run, five frames spent 15 to 21 ms in the scene graph's sync while the terminal item's own sync stayed under 0.35 ms, and the event loop stalled for 34 to 46 ms. The cause is not identified. The rest of the run is like the others.

The 100 MiB stream copy took 1.05 s here (99 MB/s), faster than the engine probe's 1.9 s at 200 x 60; the difference was not investigated. Git's `cat.exe` (MSYS2) needs 225 s through the bundled ConPTY although the app draws 180 frames/s all along: the time is the MSYS2 runtime's and the console host's, not ours.

### Floods on Linux (Debian WSLg, X11)

124 x 33. Lag is the `ClientMessage` delay during the flood.

| Program | Runs | Duration | Frames/s | Frame interval p99 / max | Event-loop lag p99 / max | Render thread per frame p50 / max |
|---|---|---|---|---|---|---|
| `yes` for 10 s | 3 | 10 s | 144 to 145 | 10.7 to 11.6 ms / 12.7 to 18.9 ms | 2.97 to 3.65 ms / 5.6 to 7.7 ms | 2 ms / 7 to 12 ms |
| `/bin/cat`, 100 MiB | 6 | 1.12 to 1.19 s (88 to 94 MB/s) | 140 to 144 | 8.2 to 12.6 ms / 10.9 to 19.8 ms | 1.94 to 4.05 ms / 3.7 to 6.3 ms | 3 ms / 7 to 18 ms |

`cat` through the app is close to the engine probe (1.03 to 1.14 s at 200 x 60); the kernel PTY alone, `script(1)`, takes 0.98 s. Mesa renders on the CPU here, 2 to 3 ms a frame, and the swap doesn't wait for the 60 Hz display, so the render loop runs at about 144 frames/s while output flows. The GUI thread is blocked for sync at most 8 ms.

### Typing and Ctrl+C during floods

| | Key to program: p50 / p99 / max | Key to the next frame p50 / max | Ctrl+C to the prompt | Ctrl+C to the prompt on screen |
|---|---|---|---|---|
| Windows, bundled ConPTY, keys during `yes` (2 runs x 32 keys) | 0.82 to 0.86 / 1.6 to 1.9 / 1.9 ms | 3.6 to 3.9 / 7.2 ms | `yes.exe`: 57 to 101 ms, median 77 ms (4 runs); `cat.exe`: 161 and 172 ms | 58 to 101 ms; 166 and 176 ms |
| Windows, inbox ConPTY (1 run) | 5.1 / 27.8 / 33.8 ms | | `yes.exe`: 133 and 142 ms; `cat.exe`: 73 ms | 148 and 157 ms; 76 ms |
| Linux, keys during `yes` (3 runs x 32 keys) | 1.9 to 3.1 / 4.8 to 7.0 / 7.0 ms | 7.4 to 9.9 / 15.1 ms | `yes`: 0.4 to 12 ms, median 7.5 ms (5 runs) | 5.7 to 16 ms, median 12.7 ms |

Every key arrived and Ctrl+C always ended the program. The Windows times include PowerShell running its prompt; on Linux the proposed 250 ms mark is met with a wide margin. With the inbox ConPTY, one run had a 63 ms event-loop stall as the flood started: 13 ms of QML polish and a 54 ms scene-graph sync, while the terminal item's own sync stayed under 1.5 ms. It did not happen with the bundled host.

### Key to pixels

Idle terminal, 40 keys per run.

| Terminal (Windows 10, 180 Hz) | Key to pixels p50 / p99 / max | Key to program p50 |
|---|---|---|
| **OpenSesh**, bundled ConPTY (2 runs, 204 x 47) | **5.9 to 6.1 / 8.1 to 9.8 / 10.6 ms** | 1.4 ms |
| OpenSesh, 124 x 33 (1 run) | 5.6 / 10.9 / 11.4 ms | 1.5 ms |
| OpenSesh, inbox ConPTY (1 run) | 19.7 / 24.3 / 25.5 ms | 1.4 ms |
| Windows Terminal 1.24.11911.0 | 8.1 / 10.7 / 10.9 ms | 1.7 ms |
| Console host window (`conhost.exe`) | 13.8 / 16.7 / 16.8 ms | 1.2 ms |

From the program's write to the pixels (p50): OpenSesh 4.4 to 4.6 ms, less than one 5.6 ms refresh; Windows Terminal 6.3 ms; the console host window 12.6 ms. The inbox console host repaints on a timer, so behind it the same step takes 18.4 ms: the key reaches the program as fast, but key to pixels triples. On Linux (WSLg, idle) a key reaches the program in 0.55 to 0.58 ms (p50, max 0.78 ms) and the next frame is rendered 11.8 to 12.5 ms after the key (p50, max 14.4 ms); WSLg forwards the window to Windows over RDP, so pixels on the real screen were not measured there.

### Idle

With no output, the only frames are the cursor blink (0.25 to 1.1 frames/s in the terminal item's statistics). On Linux the idle event-loop lag is 0.15 ms p50, 0.93 ms p99, 6.9 ms max.

### Found while measuring

- **Portable mode leaks Qt's pipeline cache.** With `portable=true` and `data_dir=...\target\release\data` in the log, Qt still reads and writes its shader pipeline cache in the per-user cache folder: `Attempting to seed pipeline cache ... from 'C:/Users/<user>/AppData/Local/OpenSesh/cache/qtpipelinecache-x86_64-little_endian-llp64/qqpc_d3d11'` on Windows and `Writing pipeline cache contents (26952 bytes) ... to '/home/<user>/.cache/OpenSesh/qtpipelinecache-x86_64-little_endian-lp64/qqpc_opengl'` on Linux (logged with `qt.scenegraph.general.debug=true`). PLAN §4.1 keeps everything under `<exe dir>/data` in portable mode.
- **The Linux event-loop probe can't use `_NET_WM_PING`** (see the lag method above): Qt 6.8 gets the ping, but no reply reached a root-window listener in WSLg.
