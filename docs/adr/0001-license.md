# ADR 0001: Project license is GPL-3.0-or-later

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 0

## Context

OpenSesh is an open source desktop client that will link against, or bundle, third-party code with different licenses:

| Component | License | Notes |
|---|---|---|
| Qt 6 (qtbase, qtdeclarative, qtsvg, qtwayland) | LGPL-3.0 (open source edition) | Dynamically linked. |
| cxx-qt, cxx-qt-lib, cxx-qt-build, cxx | MIT OR Apache-2.0 | |
| alacritty_terminal, russh, portable-pty and most Rust crates | MIT and/or Apache-2.0 | Apache-2.0 is compatible with GPLv3 but **not** with GPLv2-only. |
| libvncclient (candidate for the VNC backend, Sprint 14) | GPL-2.0-or-later | Can only be combined with a GPL-compatible program. |
| FreeRDP 3 (candidate for the RDP backend, Sprint 13) | Apache-2.0 | |
| Lucide icons | ISC (with an MIT notice for the Feather-derived icons) | |
| Tabler Icons | MIT | |
| Simple Icons | CC0-1.0 | Trademarks remain with their owners. |
| Inter, JetBrains Mono | SIL OFL 1.1 | Bundled fonts. OFL allows bundling with software. |

The owner wants the project to stay free software: forks and redistributed builds must keep the source available.

## Options

1. **MIT / Apache-2.0 (permissive).** Maximum reuse. But it can't be combined with GPL-only backends such as libvncclient, and it doesn't guarantee that derived builds stay open.
2. **GPL-3.0-only.** Copyleft. But it blocks moving to a future GPL version without asking every contributor again.
3. **GPL-3.0-or-later.** Copyleft, and it keeps the "or later" upgrade path open.
4. **AGPL-3.0.** Adds a network clause that makes little sense for a local desktop client.

## Decision

The project license is **GPL-3.0-or-later** (SPDX: `GPL-3.0-or-later`).

- The repository root contains the verbatim FSF text in `LICENSE` (from <https://www.gnu.org/licenses/gpl-3.0.txt>).
- Every crate declares `license = "GPL-3.0-or-later"` through `workspace.package`.
- New source files don't need per-file headers. The SPDX identifier in `Cargo.toml` and the `LICENSE` file are authoritative.

Why the dependencies are compatible:

- **LGPL-3.0 Qt:** a GPL-3.0 program can use an LGPL-3.0 library. We link Qt dynamically, so users can replace it.
- **Apache-2.0 and MIT crates:** compatible with GPLv3. This is one reason not to choose GPLv2.
- **libvncclient (GPL-2.0-or-later):** compatible, because the "or later" lets it be used under GPLv3.
- **ISC/MIT icons and OFL fonts:** compatible. Their notices must ship with the binaries (`THIRD_PARTY_NOTICES.md`).

## Consequences

- `cargo-deny` keeps an **allowlist** in `deny.toml` that contains only the GPL-3.0-compatible licenses our dependencies actually use today: MIT, Apache-2.0 (also with the LLVM exception), BSD-3-Clause, ISC, Zlib, Unicode-3.0, MPL-2.0 and CDLA-Permissive-2.0. Other compatible licenses, such as BSD-2-Clause, are added after review when a dependency needs them.
- Third-party assets (icons, fonts, themes) must have their license copied into the repository and listed in `THIRD_PARTY_NOTICES.md`.
- Binary distributions (AppImage, Flatpak, MSI, and so on) must offer the corresponding source and ship the license texts, including the Qt LGPL notices.
- Contributions are accepted under the same license (inbound = outbound). See `CONTRIBUTING.md`.
