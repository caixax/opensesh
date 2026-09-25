# Contributing to OpenSesh

Thanks for your interest! OpenSesh is in an early stage (see [`docs/sprints/`](docs/sprints/) and [`docs/adr/`](docs/adr/)). This guide lists the rules every change follows.

## Getting started

1. Set up Rust, a C++ compiler and Qt 6 by following [`docs/dev-setup.md`](docs/dev-setup.md).
2. Build and run the app: `cargo run -p opensesh-app`.
3. Before opening a pull request, run the checks CI runs. They are listed under [Build, run and check](docs/dev-setup.md#build-run-and-check) in `docs/dev-setup.md`: fmt, clippy, tests, the QML lint, the translation check (`cargo xtask i18n --check`), `cargo deny` and `cargo audit`. If you added, changed or removed a `qsTr()` string, run `cargo xtask i18n` and commit the updated `.ts` and `.qm` files. If you changed `assets/icons/icons.toml`, also run `cargo xtask icons` and commit everything it generates. CI regenerates the icons and fails on any difference, including new untracked files.

## Ground rules

- **Language:** code, identifiers, commit messages, docs and UI source strings are in English.
- **No telemetry:** the app never makes network calls the user didn't ask for.
- **Secrets:** never log them, never write them to disk in clear text, never put them in error messages. Use `secrecy` / `zeroize`.
- **Errors:** no `unwrap()` or `expect()` in production code paths. Libraries use typed errors (`thiserror`); only binaries use `anyhow`. Tests may unwrap.
- **GUI thread:** never block it. Long work runs on the core runtime and reports back through events.
- **QML:** no hardcoded colors (use `Theme` tokens) and no user-visible string without `qsTr()`. `cargo xtask lint-qml` enforces this.
- **Icons:** never draw, generate or hand-edit SVG paths. Icons come only from the pinned Lucide / Tabler / Simple Icons packages through `cargo xtask icons`.
- **Dependencies:** check the latest stable version and its real API on crates.io / docs.rs, and pin it in `[workspace.dependencies]` in the root `Cargo.toml`. Licenses must pass `cargo deny check`.
- **Decisions:** any relevant architectural decision gets an ADR in `docs/adr/NNNN-title.md` with the sections Context, Options, Decision and Consequences.

## Commits

- Small commits following [Conventional Commits](https://www.conventionalcommits.org/): `feat(ssh): ...`, `fix(term): ...`, `docs: ...`, `ci: ...`, `chore: ...`.
- Use plain ASCII hyphens in commit messages (no em dashes).
- No AI-assistant attribution trailers (for example `Co-Authored-By: <AI tool>`).
- Never rewrite published history.

## License

By contributing, you agree that your contributions are licensed under the project license, **GPL-3.0-or-later** (see [`LICENSE`](LICENSE) and [ADR 0001](docs/adr/0001-license.md)).
