# ADR 0014: Bundle the modern ConPTY (conpty.dll and OpenConsole.exe) on Windows

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 2

## Context

Local terminals on Windows run through ConPTY, the Windows pseudoconsole. portable-pty 0.9.0 (our PTY library) loads it this way (`src/win/psuedocon.rs`): it opens `kernel32.dll`, then tries `LoadLibraryW("conpty.dll")` with a bare name and uses that library when it loads. So an application can ship a newer ConPTY without code changes.

The ConPTY built into Windows only changes with major OS upgrades. On Windows 10 22H2 (build 19045, the owner's machine, conhost 10.0.19041.1) it is the host from before the 2024 rewrite (microsoft/terminal PR #17510, shipped in Windows Terminal 1.22). Measured on that machine with the Sprint 2 research probes (portable-pty 0.9.0; throughput at 120x40):

| | Built-in ConPTY (Windows 10 19045) | ConPTY 1.24.260710001 next to the exe |
|---|---|---|
| Output | Re-rendered: re-encoded SGR, `ECH` erase sequences, cursor moves | Passed through as the program wrote it |
| PowerShell writing 1 MiB blocks | **3.7 MB/s** (36.6 MB in 10 s, reads of about 69 bytes) | the whole 101.36 MB in under 3 s including PowerShell start-up: **at least 33.8 MB/s** (reads of about 62 KB) |
| `cmd /c type` of a 100 MB file (writes line by line) | 1.3 MB/s | 2.3 MB/s |
| Cursor query `ESC[6n` at start not answered | the shell never prints anything (tested for 20 s), and closing then **deadlocks** `ClosePseudoConsole` | about 3 s of silence, then the shell starts |
| `ClosePseudoConsole` with unread output | **blocks** until someone drains the pipe (documented for Windows before 11 24H2) | returns immediately (0 ms) |
| Ctrl+C to child exit | 21 ms | 20.6 ms |

With the bundled host, Windows reaches the same "large reads" regime as Linux, and the two hang paths of the old host go away. The engine must still answer the cursor query (portable-pty always sets `PSEUDOCONSOLE_INHERIT_CURSOR`) and must handle the focus (`?1004h`) and win32-input-mode (`?9001h`) requests the new host sends at start.

The DLL search order matters too. `LoadLibraryW` with a bare name uses the standard search order for desktop apps (Microsoft Learn, "Dynamic-link library search order"): after the known DLLs, **the folder the application was loaded from**, then the system folders, then the **current folder** and **`PATH`**. Windows has no `conpty.dll` in its system folders (checked on Windows 10 19045), so without a bundled copy any `conpty.dll` in the current folder or on `PATH` is loaded into OpenSesh: a DLL-planting path. Alacritty and wezterm load it the same way.

The package is `Microsoft.Windows.Console.ConPTY` on nuget.org, published by Microsoft with each Windows Terminal release (1.24.260710001 is the stable one, from 2026-07-13, attached to the GitHub release `v1.24.11911.0`). It is MIT-licensed and holds `conpty.dll` and `OpenConsole.exe` for x86, x64 and arm64. Both x64 files are Authenticode-signed by Microsoft Corporation (checked). The new `conpty.dll` exports the classic names (`CreatePseudoConsole`, `ResizePseudoConsole`, `ClosePseudoConsole`), which portable-pty looks up, and it starts the `OpenConsole.exe` found next to it (then in an `<arch>` subfolder, then falls back to the built-in `conhost.exe`). wezterm and VS Code ship the same pair.

## Options

1. **Use the built-in ConPTY only.** Nothing to ship, but Windows 10 users get the old host (the slow and re-rendered output, the hangs above), Windows 11 users get whatever their build has, and the DLL-planting path stays open.
2. **Keep the built-in ConPTY and close the planting path with `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS)` at start-up.** It fixes only the security part, and how it interacts with Qt's plugin loading is untested.
3. **Bundle `conpty.dll` and `OpenConsole.exe` from the pinned NuGet package, next to `opensesh-app.exe`.**

## Decision

Option 3.

- **`cargo xtask conpty`** reads `assets/conpty/conpty.toml`, downloads the package over HTTPS from nuget.org's package content endpoint (HTTPS only, redirects included; a 32 MB cap; the connect and total timeouts of the other xtask downloads), verifies its pinned sha256 before opening it, extracts the two x64 files, checks each against its own pinned sha256, and keeps them in `target/xtask-cache/conpty-<version>-x64/`. It then copies them next to the app in `target/debug` and `target/release` (whichever exist, in `CARGO_TARGET_DIR` when set), or into the `--dest` folders. Identical copies are left alone. `--remove` deletes them again.
- The package has no license file, so the task downloads the MIT text from the matching microsoft/terminal release tag (pinned sha256), stores it as `assets/conpty/LICENSE-MIT.txt` with LF line endings, and `THIRD_PARTY_NOTICES.md` gets a "Windows console host" section through `xtask/src/notices.rs`.
- The binaries are **not committed**: they are fetched and verified like the fonts and icons, but only into `target/`.
- CI runs `cargo xtask conpty` in the Windows job before the smoke tests, and fails if the license copy or the notices change.
- **No code change in the app or the engine:** portable-pty picks the bundled copy up. The app keeps working without it (for example in a fresh checkout before the task runs) through the built-in ConPTY, so both hosts must keep working.
- Only x64 is bundled, the only Windows target we build.

Verified on the owner's machine (Windows 10 19045) with a portable-pty 0.9.0 probe copied next to `target/debug/opensesh-app.exe`, started from another working folder, and `Get-CimInstance Win32_Process` on its children:

| Files in `target/debug` | `conpty.dll` + `OpenConsole.exe` in the working folder | Pseudoconsole host started |
|---|---|---|
| bundled (`cargo xtask conpty`) | no | `target\debug\OpenConsole.exe --headless --inheritcursor --width 80 --height 24 ...` |
| none (`cargo xtask conpty --remove`) | no | `C:\Windows\system32\conhost.exe --headless --inheritcursor ...` |
| none | yes | the working folder's `OpenConsole.exe`: the planting path is real |
| bundled | yes | `target\debug\OpenConsole.exe`: the application folder wins |

## Consequences

- Windows 10 and 11 get the same, current VT behaviour and throughput, and closing a terminal can't hang on the old `ClosePseudoConsole`.
- The application folder is searched first, so our `conpty.dll` always wins over a planted one. The Windows installer (Sprint 18) must put both files next to the executable, and must keep the application folder writable only by administrators (the usual `Program Files` rules); a per-user install in a user-writable folder is as safe as the executable itself.
- About 1.2 MB more in the Windows package, and a dependency on Microsoft's release cadence: updating means changing the version, the URLs and the three checksums in `conpty.toml`, running the task and checking a terminal. The ADR's measurements should be repeated then.
- `cargo test` binaries live in `target/debug/deps`, so by default they use the built-in ConPTY. `cargo xtask conpty --dest target/debug/deps` makes them use the bundled one. Both paths must pass the engine's Windows tests.
- `cargo clean` removes the copies; developers run the task again (dev-setup says so).
- A running OpenSesh keeps `conpty.dll` and `OpenConsole.exe` open, so the task fails with "access denied" until it exits.
