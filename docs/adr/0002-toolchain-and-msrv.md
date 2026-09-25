# ADR 0002: Rust toolchain pinned to the MSRV (1.88), edition 2024

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 0

## Context

PLAN §2 asks to fix the MSRV in Sprint 0 and to ship a `rust-toolchain.toml`. Facts verified on crates.io on 2026-09-25:

| Crate | Version | `rust-version` |
|---|---|---|
| cxx | 1.0.202 | 1.88 |
| cxx-qt, cxx-qt-lib, cxx-qt-build | 0.10.0 | 1.85.0 |
| toml, sha2, ureq | 1.1.6, 0.11.0, 3.4.2 | 1.85 |
| tracing, tracing-subscriber, tracing-appender | 0.1.44, 0.3.23, 0.2.5 | 1.65 or lower |

Two more facts:

- With edition 2024 (resolver 3), Cargo picks the newest dependency versions that are compatible with `rust-version`. With `rust-version = "1.85"` it picks cxx 1.0.198; with 1.88 or newer it picks the current cxx 1.0.202.
- The code uses let-chains (`if let ... && ...`), which were stabilized in Rust 1.88 for edition 2024.

The current stable release is 1.98.1. Distro compilers are older: Debian 13 ships rustc 1.85.1 and Ubuntu 24.04 ships 1.75. For that reason `docs/dev-setup.md` uses rustup everywhere.

## Options

1. **Toolchain file = latest stable (1.98.1), `rust-version` = MSRV, plus a separate CI job for the MSRV.** Everyday builds get the newest compiler and lints, and one extra job proves the MSRV.
2. **Toolchain file = MSRV (1.88.0).** Every local and CI build proves the MSRV. The compiler and clippy are a bit older.
3. **No toolchain file** (`stable`). Builds are not reproducible, and a new stable clippy can break CI without any code change.

## Decision

Option 2.

- **MSRV = 1.88** (`workspace.package.rust-version`). It is the lowest version that fits both cxx 1.0.202 and let-chains.
- **`rust-toolchain.toml` pins `channel = "1.88.0"`** with `rustfmt` and `clippy`. It is the same version on purpose, so the MSRV can't rot.
- **Edition 2024**, resolver 3.
- Versions in `[workspace.dependencies]` are full semver requirements (for example `"0.10.0"`), and the committed `Cargo.lock` locks the exact versions. Updates are deliberate: `cargo update`, review, commit.

## Consequences

- Contributors need rustup. It installs 1.88.0 automatically the first time `cargo` runs in the repository.
- A dependency that raises its MSRV above 1.88 stays on its last compatible version (resolver 3) until we bump. **Bumping the MSRV means changing `rust-toolchain.toml`, `rust-version` and this ADR (or a new one superseding it) together.**
- Newer clippy lints only show up after a bump. We accept this in exchange for reproducible CI.
- Rust 1.90 changed the default linker on `x86_64-unknown-linux-gnu` to LLD. We don't get that yet.
  - cxx-qt recommends lld/mold/gold over GNU ld.bfd, and `qt-build-utils` switches to one of them automatically when it finds it on `PATH`. That's why `docs/dev-setup.md` lists `lld`.
  - First Sprint 0 builds, before `lld` was installed everywhere: Debian 13 and Fedora 43 linked with ld.bfd despite cxx-qt's warning, Ubuntu 22.04 used gold, and Arch used gold.
  - Final Sprint 0 builds: all four distros linked with lld (`-fuse-ld=lld`).
