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

### Debian 13 "trixie" (Qt 6.8.2, the minimum supported)

```sh
sudo apt install build-essential pkg-config git curl lld \
  qt6-base-dev qt6-declarative-dev qt6-svg-dev qt6-wayland-dev qt6-wayland qt6-tools-dev \
  qml6-module-qtquick qml6-module-qtquick-controls qml6-module-qtquick-layouts \
  qml6-module-qtquick-window qml6-module-qtquick-templates qml6-module-qtqml-workerscript
```

- **qmake:** `/usr/bin/qmake6`, from the `qmake6` package, which `qt6-base-dev` pulls in.
- **`qt6-wayland` is required to run on Wayland.** On Qt 6.8 the Wayland platform plugin lives in this runtime package, and `qt6-wayland-dev` does not depend on it.
- On trixie, `qt6-declarative-dev` already depends on the `qml6-module-*` packages. They are listed explicitly to document the QML runtime modules the app imports.

### Fedora 43 / 44 (Qt 6.10.3 / 6.11.2)

```sh
sudo dnf install gcc-c++ git lld \
  qt6-qtbase-devel qt6-qtdeclarative-devel qt6-qtsvg-devel qt6-qtwayland-devel qt6-qttools-devel
```

- **qmake:** `/usr/bin/qmake6`, found automatically.

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

Cargo builds always link the **release** Qt DLLs and the release MSVC runtime, even in debug builds. This is expected: see the cxx-qt book.

## Build, run and check

```sh
cargo run -p opensesh-app                  # the app
cargo run -p opensesh-app -- --smoke-test  # renders one frame, clicks the button, checks, exits
cargo xtask help                           # developer tasks
```

`--smoke-test` uses your normal display. On a headless machine, add `QT_QPA_PLATFORM=offscreen` (PowerShell: `$Env:QT_QPA_PLATFORM = 'offscreen'`). `--crash-report <file> --smoke-test` does the same for the crash dialog.

These are the checks CI runs. The icon step is only needed after editing `assets/icons/icons.toml`:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask lint-qml          # QML: no hardcoded colors, no strings without qsTr()
cargo xtask icons             # regenerate icons; CI fails if the result differs from the committed files
cargo deny check              # licenses, advisories, bans, sources (cargo install cargo-deny@0.20.2)
cargo audit                   # RustSec vulnerabilities (cargo install cargo-audit@0.22.2)
```

`cargo xtask icons` regenerates the committed icons from the pinned upstream packages (see [ADR 0005](adr/0005-icon-pipeline-bootstrap.md)). It is the only task that uses the network.

Smoke-test exit codes:

| Code | Meaning |
|---|---|
| 0 | OK |
| 1 | Startup error (QML failed to load, data or log directory unusable); see the log |
| 2 | Invalid command-line arguments |
| 3 | No frame rendered within 15 s |
| 4 | QML/Rust bridge broken |
| 5 | Non-ASCII text mangled by the build (e.g. MSVC without `/utf-8`) |
| 134 / `0xC0000409` | Abort: a Qt fatal error (e.g. no usable display for `QT_QPA_PLATFORM`) or a panic across FFI. A `crash-*.txt` report is written. |

### Useful environment variables

| Variable | Effect |
|---|---|
| `OPENSESH_LOG` | Log filter with `RUST_LOG` syntax, e.g. `debug` or `opensesh_app=trace,qt=warn`. The default is `info`, and an invalid value falls back to it with a warning. |
| `OPENSESH_NO_CRASH_DIALOG=1` | Never open the crash dialog after a panic. Use it for headless runs. |
| `OPENSESH_DEBUG_PANIC=1` | **Debug builds only.** The Knock button panics inside a QML → Rust call, to test the crash report and dialog ([ADR 0004](adr/0004-crash-reporting.md)). |
| `QT_QPA_PLATFORM` | Qt platform plugin: `wayland`, `xcb`, `windows`, `offscreen`, ... |
| `QT_QUICK_BACKEND=software` | Software Qt Quick renderer. Use it in CI and on machines without a GPU. |
| `WAYLAND_DEBUG=1` | Prints the Wayland protocol traffic. Useful to check the `app_id`. |

### Where things are written

| | Linux | Windows |
|---|---|---|
| Config | `$XDG_CONFIG_HOME/opensesh` | `%APPDATA%\OpenSesh` |
| Data (logs, vault, recordings) | `$XDG_DATA_HOME/opensesh` | `%LOCALAPPDATA%\OpenSesh` |
| Cache | `$XDG_CACHE_HOME/opensesh` | `%LOCALAPPDATA%\OpenSesh\cache` |
| Logs | `<data>/logs/opensesh.YYYY-MM-DD.log`, plus `crash-*.txt` | same |

- If a file named `portable` sits next to the executable, everything goes to `./data/` next to it instead.
- On Linux, directories the app creates get mode `0700`, and the data directory is always kept private.

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
