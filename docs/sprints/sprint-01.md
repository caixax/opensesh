# Sprint 1: Design system and app skeleton

**Goal:** the app already "feels" like OpenSesh: tokens, icons, components and the full shell, with no network features.

**Started:** 2026-09-25 · **Finished:** 2026-09-25

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only, and the repository stays local (no push).
  - The plan asks for a complete Spanish translation in this sprint. Instead, the **i18n pipeline** (`cargo xtask i18n`: lupdate/lrelease, runtime translator, language selector) is built and tested with a generated **pseudo-locale** (debug builds only). Spanish comes when the owner asks.
- **"Notifications"** means in-app toasts plus a notification history in the status bar. Native OS notifications arrive with the first feature that needs them (the terminal bell in Sprint 3). This will be recorded in an ADR.
- **§5.2 accent text in light mode.** The plan's `accentText` (`#FFFFFF` on `#B7800F`) has a contrast of only about 3.4:1, which fails WCAG AA for text. Text-on-fill colors are therefore **computed for contrast** instead of fixed. This will be recorded in an ADR.

## Checklist

### Theme (§5.1, §5.2, §6.1)
- [x] `opensesh-core::theme`:
  - [x] Palettes (dark/light, §5.2 tokens)
  - [x] WCAG contrast
  - [x] Accent override with computed text colors
  - [x] Density metrics (comfortable/compact)
  - [x] Unit tests for AA contrast on every text token
- [x] `Theme` QML singleton fed by Rust:
  - [x] Live light/dark/system mode (`Application.styleHints.colorScheme`)
  - [x] Accent, density, UI scale, UI font, reduce motion
  - [x] No hardcoded color anywhere in QML

### Icons (§7)
- [x] `icons.toml` with the full §7 map (Lucide), plus the Tabler and Simple Icons sources pinned with sha256
- [x] `QQuickImageProvider` (`image://icon/<name>?color=&size=`) rendering with `QSvgRenderer`, cached by (name, color, size, dpr)
- [x] `OsIcon` component
- [x] `THIRD_PARTY_NOTICES.md` regenerated

### Fonts
- [x] `cargo xtask fonts`: Inter and JetBrains Mono from their official releases (version and sha256 pinned, OFL licenses copied)
- [x] Fonts registered with `QFontDatabase`, and Inter set as the UI font

### Components (§5.5) and Gallery
- [x] All 31 `Os*` components:
  - [x] Only `Theme` tokens
  - [x] `Accessible.name` / `Accessible.role`
  - [x] Keyboard navigation and a visible focus ring
- [x] `--gallery`: every component, with live theme and density switches; screenshots in all four combinations

### Shell (§5.3, §5.4)
- [x] Rail (left/right/hidden, optional labels, keyboard navigable)
- [x] Placeholder views with empty states
- [x] Tab bar in the title row
- [x] Status bar
- [x] Collapsible side panel (left/right)
- [x] Window decoration modes `auto | custom | native | none`:
  - [x] Tiling compositor detection
  - [x] `startSystemMove()` / `startSystemResize()`
- [x] Window size and state persistence

### Config
- [x] `config.toml`:
  - [x] `schema_version` and migrations
  - [x] Validation
  - [x] Atomic write (temp file, fsync, rename) with 5 rotated backups, written off the GUI thread
  - [x] Hot reload with `notify`
- [x] Settings > General and Settings > Appearance functional (every §6.1 option stored; the ones that affect the shell work live)

### i18n
- [x] `qsTr` on every string (lint)
- [x] `cargo xtask i18n` (lupdate/lrelease, pseudo-locale)
- [x] Runtime translator with live retranslation
- [x] Language selector

### Command palette, shortcuts, toasts
- [x] Central action registry (id, title, shortcut, handler), reused for shortcuts
- [x] Command palette (Ctrl+Shift+P) with fuzzy search
- [x] Toasts and a notification history

### Quality
- [x] QML lint in CI (already there), extended where needed
- [x] Smoke tests:
  - [x] Shell and gallery fail on any QML warning
  - [x] Offscreen screenshots of the shell and gallery (dark/light × comfortable/compact)

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, deny, audit
- [x] Build and smoke tests on Windows plus WSL Arch, Debian 13 (Qt 6.8 minimum), Fedora 43 and Ubuntu 22.04
- [x] ADRs, dev-setup, manual matrix, CHANGELOG, report, local commits

## Report

### What was done

- **Theme** ([ADR 0006](../adr/0006-design-tokens-and-contrast.md)):
  - `opensesh-core::theme` resolves the §5.2 palettes for the mode (system, dark, light), the accent and the density, plus metrics that follow the UI scale.
  - Contrast is computed, not assumed:
    - text on a fill uses the more readable ink (`accentText`, `Theme.textOn()`);
    - the selection tint is lowered until text and muted text stay at 4.5:1 on it;
    - status colors reach 3:1 on every surface.
  - Tests check every text token for both schemes, the 216 sample accents and the 7 accent presets.
  - `Theme` is a Rust-backed QML singleton. `ThemeBinder` feeds it from `AppSettings` and the OS color scheme, live.
  - QML has no color literal: `lint-qml` enforces it, and the accent presets come from `Theme.accentPresets`.
- **Icons:**
  - A `QQuickImageProvider` renders `image://icon/<name>?color=&size=` from the pinned SVGs, recolored and cached.
  - `OsIcon` wraps it.
  - The §7 map is complete, and the OS logos come from pinned Tabler and Simple Icons packages.
  - `THIRD_PARTY_NOTICES.md` names each logo's own license, with links.
- **Fonts:** `cargo xtask fonts` extracts Inter 4.1 and JetBrains Mono 2.304 from their pinned, sha256-verified releases with their OFL licenses. They are registered at startup, and Inter is the UI font.
- **Component library** ([ADR 0008](../adr/0008-qml-component-library.md), [contract](../design/components.md)):
  - 42 `Os*` files built on Qt Quick Templates.
  - Every one uses only Theme tokens and has accessible names and roles.
  - Every interactive one has keyboard operation and a focus ring.
  - Popups give the focus back to where it was (`OsFocusReturn`).
- **Gallery (`--gallery`):**
  - Sections: tokens with live contrast figures, typography and spacing, every icon, inputs, structure, and overlays including the command palette.
  - Live theme, density, accent and reduce-motion switches that never write the user's settings.
  - Keyboard focus scrolls into view.
- **Shell** ([ADR 0010](../adr/0010-window-decorations-and-notifications.md), [ADR 0011](../adr/0011-focus-regions-and-function-keys.md)):
  - Title bar with tabs. The decoration modes `auto | custom | native | none` detect tiling compositors, and frameless windows move and resize through `startSystemMove()` / `startSystemResize()`.
  - On Windows the frameless window keeps Win+arrows, the taskbar minimize and the system menu.
  - Rail on the left, on the right or hidden, with optional labels and keyboard navigation.
  - Placeholder views with empty states, a collapsible side panel on either side, and a status bar with the notification history.
  - Window size, position and maximized state are restored and fitted to the screen.
  - F6 / Shift+F6 and Ctrl+F6 cycle the focus regions, and the focus never stays on a hidden item.
- **Settings** ([ADR 0007](../adr/0007-settings-storage-and-hot-reload.md)):
  - `config.toml` has `schema_version` and a migrations hook. Each field is validated on its own, and unknown keys and sections survive a save.
  - Writes are atomic, with 5 rotated backups, on a background writer. On Windows, read-only backups can't block a save, and a symlinked file stays a link.
  - Hot reload tells our own writes from external edits by sequence number.
  - A newer or broken file is never overwritten; Restore defaults can replace a broken one and keeps it as a backup.
  - Problems reach QML as codes, and QML shows translated toasts, also for problems found at startup.
  - Window state lives in a separate `state.toml`.
  - Settings > General and Appearance store every §6.1 option, with a live preview. The shell applies them live.
- **i18n** ([ADR 0009](../adr/0009-i18n-pipeline.md)):
  - Every user string is in `qsTr()`. `lint-qml` also checks the component library's own text properties.
  - `cargo xtask i18n` runs lupdate and lrelease and generates the pseudo-locale.
  - `--check` also compares the committed `.qm` byte for byte.
  - The language switches live. The pseudo-locale is bundled in debug builds only.
- **Command palette and actions:**
  - One `ActionRegistry` of `OsAction`s drives the palette (Ctrl+Shift+P, fuzzy search, recent actions first) and the §6.4 default shortcuts, with conflict detection.
  - Quit is Ctrl+Shift+Q, because Ctrl+Q is XON in terminals.
- **Notifications:** in-app toasts plus a history in the status bar.
- **Crash dialog** rebuilt with the component library.
- **Quality:**
  - The smoke tests visit every view and overlay: 59 steps for the main window, 26 for the gallery.
  - They fail on any warning from our QML (exit code 6), including Qt Quick layout and polish loops that Qt logs without a location.
  - `--screenshots <dir>` captures the main window, Settings, every gallery page and the crash dialog in the four theme × density combinations, and fails on a warning (6) or a failed capture (7).

**Multi-agent work:**

- **Build:** three phases of parallel agents in isolated git worktrees, which the lead merged:
  1. core engine and bridges;
  2. the component library, fonts and i18n tooling;
  3. the shell, the Settings views and the gallery.
- **Review:** an adversarial review ran 6 lenses (Rust bridge, core, shell, components, settings and i18n, tooling and CI), and each lens's findings went to a skeptical verifier. 47 findings; **42 survived** (37 confirmed, 5 plausible; none high, 8 medium). A few were the same issue seen through two lenses. 5 were refuted.
- **Fixes:** all surviving findings were fixed, by four fix agents plus the lead. The fix agents hit the session limit halfway; new agents resumed from the worktrees they left. The most important fixes:
  - Two setting changes within the watcher's debounce could revert each other, and an editor undo could leave settings unsaved for the rest of the session. The echo detection is now sequence-based.
  - A `config.toml` with a syntax error was overwritten by the next change in the UI.
  - Focus stayed on controls in hidden views, and popups dropped the focus when they closed.
  - The frameless window lost Win+arrows, taskbar minimize and the system menu on Windows.
  - Muted text on a selected row was 3.2:1.
  - A read-only backup on Windows could block every later save.
  - Settings messages were built in Rust and couldn't be translated.

### Verification

| Check | Result |
|---|---|
| `cargo fmt --all --check` | ✅ |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ Windows and Ubuntu 22.04 on the final code; Arch, Debian 13 and Fedora 43 before the review fixes (see Pending) |
| `cargo test --workspace` | ✅ 151 tests on Windows (25 app, 63 core, 63 xtask). On Linux: all four distros before the review fixes, and the 65 core tests (with the Unix-only symlink tests) on Debian 13 after them (see Pending) |
| `cargo xtask lint-qml` / `cargo xtask i18n --check` | ✅ / ✅ |
| `cargo deny check` / `cargo audit` | ✅ / ✅ (177 crates) |
| actionlint on `ci.yml` | ✅ |
| Smoke tests (main, gallery, crash dialog), Windows 10 (Qt 6.10.3 MSVC) | ✅ offscreen and `windows` platform |
| Smoke tests (main, gallery, crash dialog), Arch (6.11.2), Debian 13 (6.8.2), Fedora 43 (6.10.3), Ubuntu 22.04 (aqt 6.10.3), WSLg | ✅ offscreen, wayland, xcb (36/36) |
| Screenshots reviewed: Windows (offscreen) and Debian 13 (native Wayland, Qt 6.8.2), all four theme × density combinations, pseudo-locale | ✅ |
| Windows real input: palette, actions, tabs, F6, rail keyboard navigation, quit; title bar drag, double-click maximize (fills the work area exactly), edge resize, close button | ✅ |
| Broken `config.toml`: the app starts on defaults, shows a toast, and leaves the file byte-for-byte unchanged | ✅ |
| Release build: the pseudo-locale isn't bundled | ✅ |

### Decisions and deviations

- **Spanish is postponed (owner override).** The i18n pipeline is complete and is proven with a generated pseudo-locale instead ([ADR 0009](../adr/0009-i18n-pipeline.md)).
- **"Notifications" means in-app toasts and a history.** Native OS notifications come with the first feature that needs them, the terminal bell in Sprint 3 ([ADR 0010](../adr/0010-window-decorations-and-notifications.md)).
- **Text on fills is computed for contrast.** The plan's white `accentText` on `#B7800F` is only about 3.4:1 ([ADR 0006](../adr/0006-design-tokens-and-contrast.md)). The light `warning` moved from `#B7800F` to `#B37E0F` to reach 3:1 on every surface.
- **F6 / Shift+F6 are app shortcuts although they are not in §6.4.** From Sprint 2 the terminal takes function keys, and Ctrl+F6 always cycles ([ADR 0011](../adr/0011-focus-regions-and-function-keys.md)).
- **Quit is Ctrl+Shift+Q.** Ctrl+Q is XON and is used by editors such as nano.
- **Comments in `config.toml` are not preserved** when the app rewrites it. Unknown keys and sections are ([ADR 0007](../adr/0007-settings-storage-and-hot-reload.md)).
- **No new third-party dependency** apart from those recorded in the earlier Sprint 1 commits (`notify`, `toml`, `zip` for xtask).

### Pending

- **Manual matrix on real hardware:**
  - Hyprland, Sway, niri and i3 (does `auto` drop the window buttons?), KDE, GNOME, an X11 session, Windows 11 and fractional scaling.
  - Real key and mouse input on Linux.
  - Screen readers (Narrator, Orca).
- **Multi-monitor window fitting is approximate.** QML only exposes the free area of the whole desktop, so on setups whose screens have different heights the taskbar isn't subtracted. A `Platform` invokable returning `QScreen::availableGeometry()` would make it exact.
- **Terminal work for Sprint 2:**
  - Accept `ShortcutOverride` for function keys so they reach the program (ADR 0011).
  - Decide how the terminal shows selection (the hover overlay is already suppressed on selected rows).
- **Nice to have:** make `Os*` buttons escape `&` in user data, so a host name can never create a mnemonic. Our own strings no longer contain `&`.
- **Linux clippy and tests on the final code.** The last Linux run built the final code and passed all 36 smoke tests on the four distros, and clippy on Ubuntu. It was then stopped because the machine ran low on memory (four builds at once), before clippy on Arch, Debian and Fedora and the Linux test runs finished. They need one more run, one distro at a time.
- **CI can't run while the repository is local-only.** The workflow was checked with actionlint, and its jobs were reproduced by hand on Windows and the four WSL distros.

### Risks

- **Real tiling compositors are untested.** `auto` decorations rely on environment detection (Hyprland socket, `SWAYSOCK`, `NIRI_SOCKET`, `I3SOCK`, `XDG_CURRENT_DESKTOP`) that is unit-tested but not tried on a real session.
- **Frameless windows on Linux** depend on the compositor honouring `startSystemMove()` / `startSystemResize()`. This is verified on WSLg's Weston only.
- **cxx-qt 0.x** can break the bridge API on minor bumps.
- **GCC 16 build noise:** Arch prints a `-Wsfinae-incomplete` warning from Qt's `qchar.h` in the cxx-qt generated code. It is harmless.
- **The lighter selection tint** (needed for contrast) is subtler than §5.2 suggests. Selected rows and rail items keep the accent bar, so they stay identifiable.
