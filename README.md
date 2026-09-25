# OpenSesh

> *"Open sesame" for your servers.*

OpenSesh is an open source, cross-platform and lightweight remote connections client. It is planned to cover SSH, SFTP, tunnels, local terminal, serial, telnet, mosh, RDP and VNC in a single native app built with **Rust** and **Qt 6 / QML** (through [cxx-qt](https://github.com/KDAB/cxx-qt)).

> **Status: pre-alpha (Sprint 0).** Right now the app is only a bootstrap window. Nothing here is ready for daily use yet. The roadmap is in [`docs/PLAN.md`](docs/PLAN.md) and sprint reports are in [`docs/sprints/`](docs/sprints/).

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

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

OpenSesh is free software, licensed under the **GNU General Public License v3.0 or later** ([`LICENSE`](LICENSE)). Third-party assets keep their own licenses (see [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)).
