# Sprint 1: Design system and app skeleton

**Goal:** the app already "feels" like OpenSesh: tokens, icons, components and the full shell, with no network features.

**Started:** 2026-09-25

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only, and the repository stays local (no push).
  - The plan asks for a complete Spanish translation in this sprint. Instead, the **i18n pipeline** (`cargo xtask i18n`: lupdate/lrelease, runtime translator, language selector) is built and tested with a generated **pseudo-locale** (debug builds only). Spanish comes when the owner asks.
- **"Notifications"** means in-app toasts plus a notification history in the status bar. Native OS notifications arrive with the first feature that needs them (the terminal bell in Sprint 3). This will be recorded in an ADR.
- **§5.2 accent text in light mode.** The plan's `accentText` (`#FFFFFF` on `#B7800F`) has a contrast of only about 3.4:1, which fails WCAG AA for text. Text-on-fill colors are therefore **computed for contrast** instead of fixed. This will be recorded in an ADR.

## Checklist

### Theme (§5.1, §5.2, §6.1)
- [ ] `opensesh-core::theme`:
  - [ ] Palettes (dark/light, §5.2 tokens)
  - [ ] WCAG contrast
  - [ ] Accent override with computed text colors
  - [ ] Density metrics (comfortable/compact)
  - [ ] Unit tests for AA contrast on every text token
- [ ] `Theme` QML singleton fed by Rust:
  - [ ] Live light/dark/system mode (`Application.styleHints.colorScheme`)
  - [ ] Accent, density, UI scale, UI font, reduce motion
  - [ ] No hardcoded color anywhere in QML

### Icons (§7)
- [ ] `icons.toml` with the full §7 map (Lucide), plus the Tabler and Simple Icons sources pinned with sha256
- [ ] `QQuickImageProvider` (`image://icon/<name>?color=&size=`) rendering with `QSvgRenderer`, cached by (name, color, size, dpr)
- [ ] `OsIcon` component
- [ ] `THIRD_PARTY_NOTICES.md` regenerated

### Fonts
- [ ] `cargo xtask fonts`: Inter and JetBrains Mono from their official releases (version and sha256 pinned, OFL licenses copied)
- [ ] Fonts registered with `QFontDatabase`, and Inter set as the UI font

### Components (§5.5) and Gallery
- [ ] All 31 `Os*` components:
  - [ ] Only `Theme` tokens
  - [ ] `Accessible.name` / `Accessible.role`
  - [ ] Keyboard navigation and a visible focus ring
- [ ] `--gallery`: every component, with live theme and density switches; screenshots in all four combinations

### Shell (§5.3, §5.4)
- [ ] Rail (left/right/hidden, optional labels, keyboard navigable)
- [ ] Placeholder views with empty states
- [ ] Tab bar in the title row
- [ ] Status bar
- [ ] Collapsible side panel (left/right)
- [ ] Window decoration modes `auto | custom | native | none`:
  - [ ] Tiling compositor detection
  - [ ] `startSystemMove()` / `startSystemResize()`
- [ ] Window size and state persistence

### Config
- [ ] `config.toml`:
  - [ ] `schema_version` and migrations
  - [ ] Validation
  - [ ] Atomic write (temp file, fsync, rename) with 5 rotated backups, written off the GUI thread
  - [ ] Hot reload with `notify`
- [ ] Settings > General and Settings > Appearance functional (every §6.1 option stored; the ones that affect the shell work live)

### i18n
- [ ] `qsTr` on every string (lint)
- [ ] `cargo xtask i18n` (lupdate/lrelease, pseudo-locale)
- [ ] Runtime translator with live retranslation
- [ ] Language selector

### Command palette, shortcuts, toasts
- [ ] Central action registry (id, title, shortcut, handler), reused for shortcuts
- [ ] Command palette (Ctrl+Shift+P) with fuzzy search
- [ ] Toasts and a notification history

### Quality
- [ ] QML lint in CI (already there), extended where needed
- [ ] Smoke tests:
  - [ ] Shell and gallery fail on any QML warning
  - [ ] Offscreen screenshots of the shell and gallery (dark/light × comfortable/compact)

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, deny, audit
- [ ] Build and smoke tests on Windows plus WSL Arch, Debian 13 (Qt 6.8 minimum), Fedora 43 and Ubuntu 22.04
- [ ] ADRs, dev-setup, manual matrix, CHANGELOG, report, local commits

## Report

_Filled in at the end of the sprint._
