# ADR 0008: Component library on QtQuick.Templates, Rust singletons, discovered QML module

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 1

## Context

PLAN §5.5 lists 31 `Os*` components. They must use only `Theme` tokens, be keyboard navigable with a visible focus ring, expose `Accessible.*`, and appear in the Gallery. The look must be OpenSesh's own (§5.1), identical on every platform, and work on Qt 6.8.

## Options

1. **Customize a stock style** (Basic, Fusion or Material) through palettes and attached properties. That's limited: the styles don't expose everything, and the result differs per platform.
2. **Build on `QtQuick.Templates`** (behavior, keyboard and accessibility, but no visuals) and draw everything from `Theme`.
3. **Draw everything from scratch on plain `Item`s.** That reimplements keyboard handling, popups and accessibility.

## Decision

Option 2:

- **Templates base.** Components import `QtQuick.Templates as T` and supply `background`, `contentItem`, `indicator`, popups and delegates. They follow the implicit-size patterns of Qt's Basic style, but none of its 6.9+/6.10-only APIs.
- **Fallback style.** `gui.rs` calls `QQuickStyle::setStyle("Basic")` before loading QML (cxx-qt-lib feature `qt_quickcontrols`). Anything that still loads a stock control, such as the attached `ToolTip`, gets the light, predictable Basic style instead of the platform style.
- **Singletons.**
  - Rust `#[qml_singleton]` objects: `Theme`, `AppSettings`, `UiState`, `Platform`, `AppInfo`. They are default-constructed by the QML engine, and their dependencies come from `services.rs`.
  - QML singletons: `ActionRegistry`, `Toasts`.
- **Build discovery.** `build.rs` finds every `qml/**/*.qml` file, bridge, C++ file and asset. Files under `qml/singletons/` are registered as QML singletons.
- **Checks.**
  - `--smoke-test` fails (exit 6) when our QML logs **any** warning.
  - `--gallery` shows every component in every state.
  - `--screenshots <dir>` captures the shell and the gallery offscreen in all four theme × density combinations, for review.

## Consequences

- The UI looks the same everywhere and follows the design tokens exactly.
- Every visual part is our responsibility, including ScrollView scrollbars, combo popups and dialog footers.
- Parallel work on components doesn't conflict in build files.
- Unknown icons and binding errors are caught by the headless smoke test.
