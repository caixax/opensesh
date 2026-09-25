# ADR 0003: Supported Qt versions and how developers and CI get Qt

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 0

## Context

PLAN §2 says: at least Qt 6.5, target Qt 6.8 LTS or newer. Facts verified on 2026-09-25:

| Source | Qt version |
|---|---|
| Arch (extra) | 6.11.2 |
| Fedora 44 / 43 | 6.11.2 / 6.10.3 |
| Debian 13 (trixie) | 6.8.2 |
| Ubuntu 24.04 LTS | **6.4.2** (below our minimum) |
| Ubuntu 26.04 LTS | 6.10.2 |
| aqtinstall 3.3.0, Windows | up to **6.10.3**. For 6.11 and newer, aqt asks for `qt6_6110/qt6_6110/Updates.xml`, which doesn't exist in Qt's new repository layout (aqtinstall issues #959 and #1007). The fix, PR #1000, is merged but not released yet. |
| aqtinstall 3.3.0, Linux | up to 6.12.0 |

Other facts:

- cxx-qt 0.10 supports "all versions of Qt 6".
- `QQmlApplicationEngine::objectCreationFailed`, which we use to detect a broken main QML file, needs Qt 6.4 or newer.
- Since Qt 6.10 the Wayland client plugin lives in qtbase. Before 6.10 it is in the `qt6-wayland` runtime package (on Debian 13).

## Options

1. Use only distro Qt everywhere. This doesn't work on Ubuntu 24.04 or Windows.
2. Use the official Qt online installer. It needs a Qt account and can't be scripted in CI without credentials.
3. **Use distro Qt where it's new enough, and aqtinstall where it isn't** (Windows, Ubuntu LTS, CI runners). jurplel/install-qt-action wraps aqtinstall in CI.

## Decision

Option 3.

- **Minimum supported Qt: 6.8.** CI tests it on the Debian 13 container (6.8.2). The code must not rely on APIs newer than 6.8 without a runtime or compile-time guard.
- **Reference Qt for aqtinstall: 6.10.3.** It is the newest version aqtinstall can install on Windows today, and CI's Ubuntu and Windows jobs use the same version. We move to 6.11+ once aqtinstall ships the fix (or earlier with the Qt online installer, locally only).
- Distro jobs in CI use the distro's own Qt: Arch and Fedora track the newest Qt, and Debian 13 covers the minimum.
- `qmake` is found through the `QMAKE` environment variable, or through `qmake6`/`qmake` on `PATH` (this is how cxx-qt's `qt-build-utils` looks for it). There is no CMake.

## Consequences

- Ubuntu 24.04 developers must use aqtinstall. `docs/dev-setup.md` explains how.
- On Windows, `cargo run` needs the Qt `bin` directory on `PATH` for the DLLs. Packaging (Sprint 18) will use `windeployqt`.
- Debian 13 users must install `qt6-wayland` (the runtime package, not only `-dev`) to run on Wayland.
- When the reference version changes, update this ADR, `QT_VERSION` in `.github/workflows/ci.yml` and `docs/dev-setup.md`.
