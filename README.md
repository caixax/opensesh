# OpenSesh

> *"Open sesame" for your servers.*

OpenSesh is an open source, cross-platform and lightweight remote connections client. It is planned to cover SSH, SFTP, tunnels, local terminal, serial, telnet, mosh, RDP and VNC in a single native app built with **Rust** and **Qt 6 / QML** (through [cxx-qt](https://github.com/KDAB/cxx-qt)).

> **Status: pre-alpha** ([latest release](https://github.com/caixax/opensesh/releases/latest)). The app shell, the design system, settings and a fast local terminal (Windows ConPTY, Linux PTY) work, and the terminal is customizable: profiles, themes (with importers from other terminals), fonts, keyword highlighting and shortcuts. Tabs split into panes, move between windows and broadcast input; layouts save as workspaces. Saved hosts have groups, fuzzy search, quick connect and `~/.ssh/config` import, and SSH hosts connect through the system's OpenSSH until the built-in client arrives. SFTP and the other protocols are still to come. Nothing here is ready for daily use yet. Sprint reports are in [`docs/sprints/`](docs/sprints/), design decisions in [`docs/adr/`](docs/adr/) and the changes in [`CHANGELOG.md`](CHANGELOG.md).

## Install

### Linux

One command finds out which distribution you run, downloads the matching package of the latest release, checks it against the release's `SHA256SUMS.txt` and installs it with your package manager, which pulls in Qt:

```sh
curl -fsSL https://raw.githubusercontent.com/caixax/opensesh/main/install.sh | bash
```

| Distribution | Package |
|---|---|
| Debian 13 (trixie) or later | `.deb` (apt) |
| Fedora | `.rpm` (dnf) |
| Arch Linux and derivatives (Manjaro, EndeavourOS, CachyOS) | `.pkg.tar.zst` (pacman) |

On a Wayland session the script also installs Qt's Wayland plugin. Ubuntu and other distributions aren't packaged yet: the `.deb` is built against Debian 13's Qt, so [build from source](#building-from-source) there.

Running the script again updates OpenSesh. It asks before installing anything; to pass options through the pipe, add them after `bash -s --`:

```sh
curl -fsSL https://raw.githubusercontent.com/caixax/opensesh/main/install.sh | bash -s -- --dry-run
```

| Option | What it does |
|---|---|
| `--version X.Y.Z` | Installs that release instead of the latest |
| `--yes` | Doesn't ask before installing |
| `--dry-run` | Shows what it would do, and does nothing |
| `--uninstall` | Removes OpenSesh (your settings in `~/.config/opensesh` stay) |

You can also download the package from the [releases page](https://github.com/caixax/opensesh/releases/latest) and install it yourself (`sudo apt install ./opensesh_*.deb`, `sudo dnf install ./opensesh-*.rpm` or `sudo pacman -U opensesh-*.pkg.tar.zst`).

### Windows 10 and 11

From the [releases page](https://github.com/caixax/opensesh/releases/latest):

- **Installer** (`OpenSesh-X.Y.Z-windows-x64-setup.exe`): installs for your user only, without administrator rights, and adds OpenSesh to the Start menu and to "Installed apps" for uninstalling.
- **Portable** (`OpenSesh-X.Y.Z-windows-x64-portable.zip`): unzip it anywhere and run `OpenSesh.exe`. Settings and data stay in the `data` folder next to it, so it runs from a USB stick.

Both bundle Qt, the Microsoft C++ runtime and a modern ConPTY, so nothing else is needed. Windows SmartScreen may warn the first time, since the builds aren't code-signed yet.

### Updates

OpenSesh never connects to the internet on its own. In **Settings > General > Updates** you can turn on a check for new releases (at startup and once a day) or check now:

- the installed Windows app downloads the new installer, verifies it against `SHA256SUMS.txt` and restarts updated;
- the portable app and the Linux packages open the download page (on Linux, running the install script again updates).

### Checking a download

Every release has a `SHA256SUMS.txt`. On Linux, in the folder with the download:

```sh
sha256sum --ignore-missing -c SHA256SUMS.txt
```

On Windows, compare the output of `Get-FileHash .\OpenSesh-*-setup.exe` (PowerShell) with the file's line.

### Command line

The packages also install `opensesh`, which works with the running OpenSesh (starting it when needed):

```sh
opensesh list [--json]                  # saved hosts, with the ones linked from ~/.ssh/config
opensesh connect web-01                 # connect to a saved host, by name or id
opensesh open deploy@10.0.1.21:2222     # quick connect (also ssh://, rdp://...); OpenSesh asks first
```

On Windows it is `opensesh.exe` in the install or portable folder; add that folder to `PATH` to use it anywhere. Starting OpenSesh again while it runs brings the open window to the front instead of opening another one.

## Principles

- **Simple by default, powerful when you need it.**
- **Customizable down to the last terminal pixel:** fonts, colors, cursor, shortcuts, density and per-host profiles.
- **Keyboard first:** command palette and configurable shortcuts.
- **Native and light:** Qt Quick. No webviews and no Electron.
- **Local-first and private:** readable TOML files and secrets in an encrypted vault or the system keyring. **Zero telemetry.**
- **Wayland first:** built for Hyprland, Sway, KDE Plasma 6 and GNOME, plus X11 and Windows 10/11. See [`docs/testing/manual-matrix.md`](docs/testing/manual-matrix.md) for what has been verified so far.

## Building from source

You need Rust (the toolchain is pinned in `rust-toolchain.toml`), a C++17 compiler and **Qt 6.8 or newer**. See [`docs/dev-setup.md`](docs/dev-setup.md) for per-distro packages and Windows instructions.

```sh
cargo run -p opensesh-app
```

## Releasing

Releases are built on one Windows machine with the Linux packages made in its WSL distributions (Debian 13, Fedora and Arch), which takes minutes instead of the hours GitHub's runners need:

```bat
scripts\release.bat -Patch        :: or -Minor, -Major, -V 0.2.0
```

The script sets the version, moves the changelog's unreleased section under it, runs the tests, builds the Windows installer and portable zip and the three Linux packages, writes `SHA256SUMS.txt`, tags the release and publishes it on GitHub. The **Release (fallback)** workflow in GitHub Actions builds the same packages for an existing tag when that machine isn't available. Details are in [`docs/dev-setup.md`](docs/dev-setup.md#releasing).

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

OpenSesh is free software, licensed under the **GNU General Public License v3.0 or later** ([`LICENSE`](LICENSE)). Third-party assets keep their own licenses (see [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)).
