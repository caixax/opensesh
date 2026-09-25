# Developer setup

OpenSesh is a Cargo workspace. The GUI (`opensesh-app`) links against Qt 6 through [cxx-qt](https://github.com/KDAB/cxx-qt), so building it needs:

1. **Rust through rustup.** The toolchain (1.88.0, the MSRV) is pinned in `rust-toolchain.toml` and installed automatically the first time you run `cargo` in the repository. Distro compilers are too old (Debian 13 ships 1.85, Ubuntu 24.04 ships 1.75).
2. **A C++17 compiler.** GCC or Clang on Linux, MSVC on Windows.
3. **Qt 6.8 or newer**, with the Base, Declarative (QML/Quick), SVG, Wayland and Tools modules. See [ADR 0003](adr/0003-qt-version-and-installation.md).

cxx-qt finds Qt through **qmake**. It uses the `QMAKE` environment variable if set, otherwise `qmake6` or `qmake` on `PATH`. There is no CMake step.

On 2026-09-25 the Arch, Debian 13, Fedora 43 and Ubuntu 22.04 package lists below were installed, and the app was built and smoke-tested with them, linking with `lld` (see [`testing/manual-matrix.md`](testing/manual-matrix.md)). Fedora 44 and Ubuntu 26.04 are expected to work but are untested.

## Rust

- **Linux:**

  ```sh
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

- **Windows:** `winget install Rustlang.Rustup`, or run `rustup-init.exe` from <https://rustup.rs>.

## Linux

`lld` is recommended on every distro. cxx-qt warns that linking Qt executables with GNU `ld.bfd` can fail, and its build script switches to `lld`, `gold` or `mold` automatically when one is installed.

### Arch Linux (Qt 6.11.2)

```sh
sudo pacman -S --needed base-devel git lld qt6-base qt6-declarative qt6-svg qt6-wayland qt6-tools
```

- **qmake:** `/usr/bin/qmake6`, found automatically.
- Since Qt 6.10 the Wayland platform plugin ships in `qt6-base`. `qt6-wayland` adds the client-side decorations used on GNOME.
- **Translation tools:** `lupdate` and `lrelease` (for `cargo xtask i18n`) come with `qt6-tools`, under `/usr/lib/qt6/bin/`.

### Debian 13 "trixie" (Qt 6.8.2, the minimum supported)

```sh
sudo apt install build-essential pkg-config git curl lld \
  qt6-base-dev qt6-declarative-dev qt6-svg-dev qt6-wayland-dev qt6-wayland qt6-tools-dev qt6-l10n-tools \
  qml6-module-qtquick qml6-module-qtquick-controls qml6-module-qtquick-layouts \
  qml6-module-qtquick-window qml6-module-qtquick-templates qml6-module-qtqml-workerscript
```

- **qmake:** `/usr/bin/qmake6`, from the `qmake6` package, which `qt6-base-dev` pulls in.
- **Translation tools:** `lupdate` and `lrelease` are in `qt6-l10n-tools`, under `/usr/lib/qt6/bin/`.
- **`qt6-wayland` is required to run on Wayland.** On Qt 6.8 the Wayland platform plugin lives in this runtime package, and `qt6-wayland-dev` does not depend on it.
- On trixie, `qt6-declarative-dev` already depends on the `qml6-module-*` packages. They are listed explicitly to document the QML runtime modules the app imports.

### Fedora 43 / 44 (Qt 6.10.3 / 6.11.2)

```sh
sudo dnf install gcc-c++ git lld \
  qt6-qtbase-devel qt6-qtdeclarative-devel qt6-qtsvg-devel qt6-qtwayland-devel qt6-qttools-devel qt6-linguist
```

- **qmake:** `/usr/bin/qmake6`, found automatically.
- **Translation tools:** `lupdate` and `lrelease` are in `qt6-linguist`, under `/usr/lib64/qt6/bin/`.

### Ubuntu

> **Ubuntu 24.04 LTS ships Qt 6.4.2, which is too old.** Use aqtinstall (below) to develop on it. Ubuntu 26.04 LTS ships Qt 6.10.2; the Debian package list above works there.

System libraries needed to build and run against an aqtinstall Qt (tested on Ubuntu 22.04):

```sh
sudo apt install build-essential pkg-config git curl lld python3-pip \
  libgl-dev libegl-dev libxkbcommon-dev libxkbcommon-x11-0 libfontconfig1 libdbus-1-3 \
  libxcb-cursor0 libxcb-icccm4 libxcb-keysyms1 libxcb-shape0 libxcb-xinerama0 libxcb-randr0 \
  libxcb-render-util0 libxcb-image0 libwayland-client0 libwayland-cursor0 libwayland-egl1
```

Then install Qt into `~/Qt`. On Ubuntu 22.04 (the tested setup), use pip:

```sh
python3 -m pip install --user aqtinstall
python3 -m aqt install-qt linux desktop 6.10.3 linux_gcc_64 -O ~/Qt
export QMAKE=~/Qt/6.10.3/gcc_64/bin/qmake     # add it to your shell profile
```

On Ubuntu 24.04, pip refuses to install into the system Python. Use pipx instead: `sudo apt install pipx`, then `pipx run aqtinstall install-qt linux desktop 6.10.3 linux_gcc_64 -O ~/Qt`.

When Qt lives outside the dynamic loader's default directories (aqtinstall, the online installer, a source build in `/usr/local/Qt-x.y.z`), `build.rs` embeds an rpath to its `lib` directory. So `cargo run` and `cargo test` work without setting `LD_LIBRARY_PATH`.

## Windows 10 / 11 (x64)

1. **MSVC Build Tools 2022**, with the "Desktop development with C++" workload:

   ```powershell
   winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   ```

   Visual Studio 2022 or 2026 with the same workload also works.

2. **Qt 6.10.3 for MSVC** with aqtinstall. It needs Python 3; `winget install Python.Python.3.14` installs it.

   ```powershell
   pip install aqtinstall
   aqt install-qt windows desktop 6.10.3 win64_msvc2022_64 -O C:\Qt
   ```

   > aqtinstall 3.3.0 can't install Qt 6.11 or newer on Windows yet, because Qt changed its repository layout (aqtinstall issues #959 and #1007). The fix is merged but not released. For a newer Qt, use the official Qt Online Installer (it needs a Qt account) and choose the MSVC 2022 64-bit component.

3. **Tell cxx-qt where qmake is, and put the Qt DLLs on `PATH`** so `cargo run` can start the app:

   ```powershell
   # Current PowerShell session
   $Env:QMAKE = 'C:\Qt\6.10.3\msvc2022_64\bin\qmake.exe'
   $Env:PATH  = 'C:\Qt\6.10.3\msvc2022_64\bin;' + $Env:PATH

   # Persistent, for new terminals (User scope). Don't use `setx` for PATH: it truncates at 1024 characters.
   [Environment]::SetEnvironmentVariable('QMAKE', 'C:\Qt\6.10.3\msvc2022_64\bin\qmake.exe', 'User')
   [Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';C:\Qt\6.10.3\msvc2022_64\bin', 'User')
   ```

   If `PATH` is missing the Qt `bin` directory, the program exits immediately with `0xC0000135` (DLL not found).

4. **Copy the bundled ConPTY next to the app** after the first build ([ADR 0014](adr/0014-bundled-conpty.md)):

   ```powershell
   cargo build -p opensesh-app
   cargo xtask conpty
   ```

Cargo builds always link the **release** Qt DLLs and the release MSVC runtime, even in debug builds. This is expected: see the cxx-qt book.

### ConPTY (Windows pseudoconsole host)

Local terminals on Windows run through ConPTY. OpenSesh bundles the modern ConPTY from the NuGet package `Microsoft.Windows.Console.ConPTY` 1.24.260710001 (MIT), because the one built into Windows 10 re-renders the output, is several times slower and can hang when a session closes ([ADR 0014](adr/0014-bundled-conpty.md)).

- **`cargo xtask conpty`** downloads the package from nuget.org (pinned in `assets/conpty/conpty.toml`, sha256 verified, cached in `target/xtask-cache/`), and copies `conpty.dll` and `OpenConsole.exe` (x64) into `target/debug` and `target/release`, whichever exist. It honours `CARGO_TARGET_DIR`. It also refreshes `assets/conpty/LICENSE-MIT.txt` and `THIRD_PARTY_NOTICES.md`.
- **`--dest <folder>`** copies them somewhere else, for example `--dest target/debug/deps` so that `cargo test` binaries (which live in `deps`) use the bundled host too. Without it, tests use the ConPTY built into Windows.
- **`--remove`** deletes the copies again, to test the built-in ConPTY. The app works without them.
- `cargo build` never deletes the files, but `cargo clean` does: run the task again afterwards.
- **Is it in use?** While a terminal is open, `Get-CimInstance Win32_Process -Filter "Name='OpenConsole.exe'" | Select-Object ProcessId, ParentProcessId, CommandLine` lists the bundled host (`...\target\debug\OpenConsole.exe --headless --inheritcursor ...`). With the built-in ConPTY the host is `conhost.exe --headless ...` instead.
- If the copy fails with "access denied", an OpenSesh (or a test) that uses the files is still running.

### AltGr

On Windows, the app starts Qt with `QT_QPA_PLATFORM=windows:altgr` unless you set `QT_QPA_PLATFORM` yourself. With that option, Qt reports AltGr as its own modifier instead of Ctrl+Alt, so the terminal can tell AltGr+Q (`@` on a German layout) from Ctrl+Alt+Q. The value is removed from the environment of shells started in local terminals. If you set `QT_QPA_PLATFORM=windows` (or anything else), it is left alone and AltGr arrives as Ctrl+Alt.

## Build, run and check

```sh
cargo run -p opensesh-app                                      # the app
cargo run -p opensesh-app -- --gallery                         # every Os* component in every state
cargo run -p opensesh-app -- --smoke-test                      # renders, visits every view, checks the bridges, exits
cargo run -p opensesh-app -- --screenshots shots/              # PNGs of the window in all 4 theme x density combos
cargo run -p opensesh-app -- --gallery --screenshots shots/    # same for the gallery
cargo xtask help                                               # developer tasks
```

`--smoke-test` and `--screenshots` use your normal display. On a headless machine, add `QT_QPA_PLATFORM=offscreen` (PowerShell: `$Env:QT_QPA_PLATFORM = 'offscreen'`). `--gallery --smoke-test` and `--crash-report <file> --smoke-test` check the gallery and the crash dialog the same way.

These are the checks CI runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask lint-qml          # QML: no hardcoded colors, no strings without qsTr()
cargo xtask i18n --check      # translations up to date; run cargo xtask i18n after changing QML strings (needs the Qt linguist tools)
cargo xtask icons             # regenerate icons; CI fails if the result differs from the committed files
cargo deny check              # licenses, advisories, bans, sources (cargo install cargo-deny@0.20.2)
cargo audit                   # RustSec vulnerabilities (cargo install cargo-audit@0.22.2)
```

The assets these tasks generate are committed, so normal builds work offline:

- **`cargo xtask icons`** regenerates the icons from the pinned Lucide, Tabler and Simple Icons packages ([ADR 0005](adr/0005-icon-pipeline-bootstrap.md)). Run it after editing `assets/icons/icons.toml`.
- **`cargo xtask fonts`** extracts Inter and JetBrains Mono from their pinned release zips.
- **`cargo xtask i18n`** updates the `.ts` files with `lupdate`, regenerates the pseudo-locale and compiles the `.qm` files with `lrelease` ([ADR 0009](adr/0009-i18n-pipeline.md)). With `--check` it changes nothing, and fails when a `.ts` file is out of date or a `.qm` file differs from what `lrelease` builds. CI runs `--check` with Qt 6.10.3, so commit `.qm` files built with that version.

- **`cargo xtask conpty`** (Windows) copies the bundled ConPTY next to the app; see [ConPTY](#conpty-windows-pseudoconsole-host). Its outputs in `target/` are not committed; the license copy and the notices are.
- **`cargo xtask vttest`** (Linux) builds the pinned vttest used by the terminal harness tests; see [vttest](#vttest).

`icons`, `fonts`, `conpty` and `vttest` download from the network; no other task does.

### Shaders

The terminal renderer draws its glyphs with a Qt Quick material whose shaders live in `crates/opensesh-app/shaders/` (Vulkan-style GLSL 440). The compiled `.qsb` files next to them are committed and bundled into the Qt resources, so normal builds need no shader tools ([ADR 0013](adr/0013-terminal-rendering.md)).

After editing a `.vert` or `.frag` file, run `cargo xtask shaders` and commit the regenerated `.qsb` files. It needs `qsb` from the **Qt Shader Tools** module, found through `QMAKE` (`<Qt>/bin`) or on `PATH`:

| Setup | Install |
|---|---|
| aqtinstall (Windows, Linux) | Add the module to the Qt you already have. `--noarchives` installs only the module: `aqt install-qt windows desktop 6.10.3 win64_msvc2022_64 -m qtshadertools --noarchives -O C:\Qt` (Linux: `linux desktop 6.10.3 linux_gcc_64 ... -O ~/Qt`) |
| Arch | `pacman -S qt6-shadertools` |
| Debian 13 | `apt install qt6-shader-baker` (qsb 6.8.2) |
| Fedora | `dnf install qt6-qtshadertools` (`/usr/lib64/qt6/bin/qsb`, also `qsb-qt6`) |

- `cargo xtask shaders --check` changes nothing. It fails when a committed `.qsb` differs from what qsb builds.
- qsb output is byte-for-byte reproducible with one Qt version, but other versions may produce other bytes. So build committed files with qsb **6.10.3**, the version CI uses. Qt 6.8 loads them (`.qsb` format version 9 in both).
- The vertex shader is compiled with `-b` (the batchable variant Qt Quick needs to merge the per-row nodes). It must not use vertex input location 7, which that variant takes.

Smoke-test exit codes:

| Code | Meaning |
|---|---|
| 0 | OK |
| 1 | Startup error (QML failed to load, data or log directory unusable); see the log |
| 2 | Invalid command-line arguments |
| 3 | No frame rendered in time |
| 4 | QML/Rust bridge broken |
| 5 | Non-ASCII text mangled by the build (e.g. MSVC without `/utf-8`) |
| 6 | The UI ran, but our QML logged a warning (binding error, unknown icon, layout loop, ...); see the log. Applies to `--screenshots` runs too. |
| 7 | `--screenshots`: a capture could not be taken or saved |
| 8 | The terminal smoke steps failed (no shell output, the echoed marker never came back, or a closed session stayed open); the log has a `smoke test: FAILED:` line |
| 134 / `0xC0000409` | Abort: a Qt fatal error (e.g. no usable display for `QT_QPA_PLATFORM`) or a panic across FFI. A `crash-*.txt` report is written. |

### Useful environment variables

| Variable | Effect |
|---|---|
| `OPENSESH_LOG` | Log filter with `RUST_LOG` syntax, e.g. `debug` or `opensesh_app=trace,qt=warn`. The default is `info`, and an invalid value falls back to it with a warning. |
| `OPENSESH_NO_CRASH_DIALOG=1` | Never open the crash dialog after a panic. Use it for headless runs. |
| `OPENSESH_DEBUG_PANIC=1` | **Debug builds only.** The "Debug: trigger a panic" command panics inside a QML → Rust call, to test the crash report and dialog ([ADR 0004](adr/0004-crash-reporting.md)). |
| `QT_QPA_PLATFORM` | Qt platform plugin: `wayland`, `xcb`, `windows`, `offscreen`, ... On Windows the app uses `windows:altgr` when it is unset (see [AltGr](#altgr)). |
| `QT_QUICK_BACKEND=software` | Software Qt Quick renderer. Use it in CI and on machines without a GPU. |
| `WAYLAND_DEBUG=1` | Prints the Wayland protocol traffic. Useful to check the `app_id`. |

### Where things are written

| | Linux | Windows |
|---|---|---|
| Config: `config.toml`, plus its 5 backups `config.toml.bak.N` | `$XDG_CONFIG_HOME/opensesh` | `%APPDATA%\OpenSesh` |
| Data (logs, vault, recordings) | `$XDG_DATA_HOME/opensesh` | `%LOCALAPPDATA%\OpenSesh` |
| Window and panel state (`state.toml`) | `<data>/state.toml` | same |
| Cache | `$XDG_CACHE_HOME/opensesh` | `%LOCALAPPDATA%\OpenSesh\cache` |
| Logs | `<data>/logs/opensesh.YYYY-MM-DD.log`, plus `crash-*.txt` | same |

- If a file named `portable` sits next to the executable, everything goes to `./data/` next to it instead.
- On Linux, directories the app creates get mode `0700`, the data directory is always kept private, and `config.toml` is written with mode `0600`.
- You can edit `config.toml` while the app runs: changes apply live. An invalid value is ignored, with a warning that names the key.
- A `config.toml` with a syntax error, or one written by a newer OpenSesh, is never overwritten: changes made in the app apply but aren't saved until the file is fixed (Settings > General shows why). "Restore defaults" replaces a broken file and keeps it as `config.toml.bak.1`.

## Releasing

Releases are cut from this Windows machine, with the Linux packages built in the WSL distros:

```bat
scripts\release.bat -Patch        :: or -Minor, -Major, -V 0.3.0; -SkipTests, -NoPublish, -SkipLinux
```

The script:

1. Checks that `main` is clean.
2. Sets the version in `Cargo.toml` and moves the `CHANGELOG.md` [Unreleased] section under it.
3. Runs the tests.
4. Builds the Windows packages with `cargo xtask dist windows`: `windeployqt`, the MSVC runtime from System32, the bundled ConPTY, the portable zip (with the `portable` marker) and the NSIS installer (`packaging/windows/opensesh.nsi`; NSIS 3 is needed, found in Program Files or through `NSIS_HOME`).
5. Runs `scripts/linux/build.sh` in `Debian` (.deb), `FedoraLinux-43` (.rpm) and `archlinux` (pacman package) at the same time. Each build copies the tree into the distro's own filesystem and links against the distro's Qt. The distros need the Qt development packages from [Linux](#linux) plus `dpkg-dev`, `rpm-build` or `base-devel`.
6. Writes `dist/SHA256SUMS.txt`, commits, tags `vX.Y.Z`, pushes and publishes the GitHub release with `gh`, using the version's changelog section as notes.

The **Release (fallback)** workflow in GitHub Actions builds the same packages for an existing tag when this machine isn't available (`gh workflow run release.yml -f tag=v0.1.0`).

Each Linux package asks for the Qt it was built against: the `.deb` targets Debian 13, and Ubuntu needs its own build (not packaged yet). `install.sh` (`curl -fsSL https://raw.githubusercontent.com/caixax/opensesh/main/install.sh | bash`) picks the package for the distribution, verifies it and installs it; running it again updates OpenSesh.

## Checking Wayland and X11

The app sets `QGuiApplication::desktopFileName` to **`cc.caixa.OpenSesh`**. On Wayland, Qt sends it as the xdg-toplevel `app_id`. On X11 it goes into `_GTK_APPLICATION_ID` and `_KDE_NET_WM_DESKTOP_FILE`, and `WM_CLASS` is `"opensesh-app", "OpenSesh"`.

```sh
QT_QPA_PLATFORM=wayland cargo run -p opensesh-app   # force native Wayland
QT_QPA_PLATFORM=xcb cargo run -p opensesh-app       # force X11 / XWayland
```

| Environment | How to see the app id |
|---|---|
| Any Wayland compositor | `WAYLAND_DEBUG=1 QT_QPA_PLATFORM=wayland opensesh-app 2>&1 \| grep set_app_id`, which should print `set_app_id("cc.caixa.OpenSesh")` |
| Hyprland | `hyprctl clients`: the `class:` of a native Wayland window is its `app_id` |
| Sway | `swaymsg -t get_tree \| grep app_id` |
| KDE Plasma 6 | `qdbus6 org.kde.KWin /KWin org.kde.KWin.queryWindowInfo`, then click the window (the binary is `qdbus-qt6` on Fedora) |
| GNOME | Alt+F2, `lg`, then the Windows tab: `wmclass` and `app` |
| X11 | `xprop WM_CLASS _GTK_APPLICATION_ID`, then click the window. The tools are in `xorg-xprop` (Arch), `x11-utils` (Debian/Ubuntu) or `xprop` (Fedora). |

The desktop entry is `crates/opensesh-app/data/cc.caixa.OpenSesh.desktop`. To try it locally, copy it to `~/.local/share/applications/`, and copy `crates/opensesh-app/data/icons/cc.caixa.OpenSesh.svg` to `~/.local/share/icons/hicolor/scalable/apps/`. Put `opensesh-app` on your `PATH`.

### WSL2 (WSLg)

WSLg provides a Wayland compositor and XWayland, so all three platforms (`wayland`, `xcb`, `offscreen`) can be tested from Windows.

- **Build from a Linux path.** Use `CARGO_TARGET_DIR=~/.cache/opensesh-target` so the Linux and Windows builds don't share `target/`.
- **Wayland fails in some distros.** If the systemd user session fails to start, `/run/user/$UID` stays empty and Qt reports `Failed to create wl_display`. Run with `XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir`.

## Terminal test programs

The terminal harness tests (Sprint 2) drive real TUI programs through the PTY and the engine, so they need these programs. Install them with the distro's package manager:

```sh
sudo pacman -S --needed neovim tmux htop mc fzf less time             # Arch (vttest: see below)
sudo apt install vttest neovim tmux htop mc fzf less time             # Debian 13, Ubuntu 22.04
sudo dnf install vttest neovim tmux htop mc fzf less time             # Fedora 43
```

In WSL, `wsl.exe -d <distro> -u root -- <command>` runs a command as root without `sudo`. Versions installed in the WSL test distros on 2026-09-25:

| Program | Arch | Debian 13 | Fedora 43 | Ubuntu 22.04 |
|---|---|---|---|---|
| vttest | AUR only: use `cargo xtask vttest` | 2.7+20241208-1 | 2.7.20241204-8.fc43 | 2.7+20210210-1 |
| neovim | 0.12.5-1 | 0.10.4-8 | 0.11.6-1.fc43 | 0.6.1-3 |
| tmux | 3.7_c-1 | 3.5a-3 | 3.7c-2.fc43 | 3.2a-4ubuntu0.2 |
| htop | 3.5.3-1 | 3.4.1-5 | 3.4.1-2.fc43 | 3.0.5-7build2 |
| mc | 4.8.33-1 | 3:4.8.33-1+deb13u1 | 4.8.33-2.fc43 | 3:4.8.27-1 |
| fzf | 0.74.4-1 | 0.60.3-1+b2 | 0.74.4-1.fc43 | 0.29.0-1ubuntu0.1 |
| less | 1:710-1 | 668-1 | 679-2.fc43 | 590-1ubuntu0.22.04.3 |
| GNU time (`/usr/bin/time -v`) | 1.10-1 | 1.9-0.2 | 1.9-27.fc43 | 1.9-0.1build2 |

Ubuntu 22.04 ships old neovim (0.6) and fzf (0.29) releases; they are good enough for smoke tests.

### vttest

Each distro packages a different vttest release (see the table), and the screens differ between releases. The harness goldens use one pinned upstream release, 20251205, built from source:

```sh
cargo xtask vttest            # Linux; add --force to rebuild
```

- It downloads `https://invisible-island.net/archives/vttest/vttest-20251205.tgz` (sha256 `cd6886f9aefe6a3f6c566fa61271a55710901a71849c630bf5376aa984bf77cc`, cached in `target/xtask-cache/`), unpacks it into `<target>/vttest/vttest-20251205/` and runs `./configure && make` there. It needs a C compiler and `make` (`build-essential`, `base-devel`, or `gcc` and `make` on Fedora).
- The program ends up in **`<target>/vttest/vttest`**, where `<target>` is `$CARGO_TARGET_DIR` when it is set, else `target/`. A stamp file next to it makes later runs skip the build.
- On Windows the task only prints a note: vttest needs a Unix terminal. From PowerShell, build it in a distro with the same target folder the tests use:

  ```powershell
  wsl.exe -d Debian -- bash -lc 'cd /mnt/i/Projects/opensesh && CARGO_TARGET_DIR=~/.cache/opensesh-target cargo xtask vttest'
  ```
- Run the goldens with `cargo test -p opensesh-term --test vttest -- --include-ignored`. `OPENSESH_VTTEST=<path>` points to another build of the same release, and `OPENSESH_BLESS=1` rewrites the goldens (review each changed screen before committing; see [`testing/vttest.md`](testing/vttest.md)).
- The performance baselines in [`perf.md`](perf.md) also use `script(1)` (Fedora package: `util-linux-script`).
