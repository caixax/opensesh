# ADR 0006: Design tokens resolved in Rust, with computed contrast

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 1

## Context

PLAN §5.2 defines the tokens (`bg`, `surface`, `surface2`, `border`, `text`, `textMuted`, `accent`, `accentText`, `success`, `warning`, `danger`, `info`) for the dark and light themes. It asks for a QML `Theme` singleton "fed from Rust", **no hardcoded color in QML**, a user-configurable accent, and contrast validation (WCAG AA for text).

Measured with the WCAG 2.1 formula (`opensesh-core::theme` tests):

- Light `accentText` `#FFFFFF` on light `accent` `#B7800F` is **3.44:1**. That fails AA for normal text (4.5:1).
- Light `accent` `#B7800F` used as text on `bg` `#F6F5F2` is **3.14:1**, which also fails.
- `border` `#2A2F3A` on the dark `surface` is about 1.4:1. That is fine for decorative hairlines but not for outlines that identify a control (WCAG 1.4.11 asks for 3:1).
- A user-chosen accent can be any color. A fixed `accentText` breaks on many of them.

## Options

1. **Hardcode the §5.2 table in a QML singleton.** This is simple, but it fails AA in light mode and can't adapt to custom accents. The plan also asks for Rust.
2. **Keep the §5.2 base colors, and compute the colors whose job is readability.**

## Decision

Option 2, in `opensesh-core::theme` (pure Rust, unit-tested). `bridge/theme.rs` exposes it as the QML `Theme` singleton.

**Base colors:**
- The §5.2 base colors are kept **verbatim**: `bg`, `surface`, `surface2`, `border`, `text`, `textMuted`, and the default accents.
- Status colors keep their §5.2 value when it already reaches 3:1 on `surface`.

**Computed tokens:**

| Token | Rule |
|---|---|
| `accentText` | Text on an accent fill: the house ink with the better contrast (`#1A1406` dark or `#FFFFFF` light), falling back to pure black or white, so it always reaches **≥ 4.5:1**. Dark mode keeps §5.2 `#1A1406`. Light mode becomes `#1A1406` instead of `#FFFFFF`. |
| `accentFg` | The accent used **as** a text or icon color on `bg`/`surface`/`surface2`. It is moved towards `text` in 2.5 % steps until it reaches ≥ 4.5:1 on all three. |
| `borderStrong` | Outlines that identify controls: ≥ 3:1 on `surface`/`surface2`. |
| `focusRing` | Equal to `accentFg`. |
| `success`, `warning`, `danger`, `info` | Adjusted only if needed to reach ≥ 3:1 on `surface`. Text on them uses `Theme.textOn(color)`. |

**Overlay and state tokens:** `hover`, `pressed`, `selection` and `scrim` (translucent), plus `textDisabled`. These keep states, selection and modal dimming token-only.

**Accent contrast warning:** `accentLowContrast` is true when the accent is under 3:1 against `bg`, and Settings shows a warning.

**Metrics:** density (comfortable/compact) and UI scale drive every size. Spacing follows the 4 px scale; radii are 8 (cards) and 6 (controls); durations are 120 and 180 ms, and all durations become **0 with "reduce motion"**.

**Tests:**
- Every text token reaches AA on every surface, in both schemes and both densities.
- **216 arbitrary accents × 2 schemes** all get a readable `accentText` and `accentFg`.

## Consequences

- The light theme deviates from one §5.2 value (`accentText`), on purpose and for accessibility.
- There is one source of truth for colors. The QML lint rejects color literals, and new colors must be added as tokens with a contrast test.
- The Sprint 17 high-contrast theme becomes a new base palette. The same rules produce its derived tokens.
