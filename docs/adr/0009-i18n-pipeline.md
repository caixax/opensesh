# ADR 0009: Translation pipeline, and a pseudo-locale while Spanish is postponed

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 1

## Context

PLAN §0 rule 9 and Sprint 1 ask for:

- `qsTr()` everywhere, with English as the source language;
- `lupdate`/`lrelease` through `cargo xtask i18n`;
- a complete Spanish translation;
- a language selector.

On 2026-09-25 the owner decided that everything stays **English only for now**, so Spanish is postponed.

Facts verified in Sprint 1 research:

- Installing a `QTranslator` does **not** update QML text by itself. `QQmlEngine::retranslate()` must be called.
- `lupdate` and `lrelease` ship in `qt6-tools` (Arch), `qt6-l10n-tools` (Debian), `qt6-linguist` (Fedora), and in aqtinstall's `bin/` (Windows).
- `QKeySequence::NativeText` is itself translated, so shortcuts must be stored as portable text.

## Decision

**`cargo xtask i18n`:**
- It finds `lupdate` and `lrelease` (in `qmake -query QT_INSTALL_BINS`, then on `PATH` under their distro-specific names).
- It updates `crates/opensesh-app/i18n/opensesh_<code>.ts` for every language in `assets/i18n/languages.toml`, which is empty for now.
- It **always** generates a **pseudo-locale** (`opensesh_pseudo`): accented, about 30 % longer text in brackets that keeps the placeholders. This makes untranslated strings and truncation visible.
- It compiles the `.qm` files with `lrelease`.
- `--check` fails when the `.ts` files are out of date.

**Build:** the `.qm` files are committed, compiled into the Qt resources (`build.rs` discovers them), and listed by `Platform.languages()`. The pseudo-locale is listed in **debug builds only**.

**Runtime:**
- The C++ shim installs the translator: "system" follows `QLocale::system().uiLanguages()`, and "en" means the source strings. It then calls `retranslate()`, so switching language in Settings updates the UI live.
- The initial language is applied before the first frame.
- Shortcuts are stored as portable text and shown with `NativeText`.

## Consequences

- The whole pipeline (extraction, compilation, runtime switching, retranslation) is built and exercised without a real translation.
- Adding Spanish later means adding `"es"` to `languages.toml`, translating `opensesh_es.ts` (Qt Linguist) and running `cargo xtask i18n`. No code changes.
- The pseudo-locale is a QA tool and never appears in release builds.
