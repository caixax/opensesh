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
- Selected rows draw their text on a translucent accent `selection`. With a fixed alpha (1/3 in dark mode), `textMuted` on the selection over the dark `surface` is **3.24:1**, and with a near-white custom accent even `text` falls under 4.5:1.

## Options

1. **Hardcode the §5.2 table in a QML singleton.** This is simple, but it fails AA in light mode and can't adapt to custom accents. The plan also asks for Rust.
2. **Keep the §5.2 base colors, and compute the colors whose job is readability.**

## Decision

Option 2, in `opensesh-core::theme` (pure Rust, unit-tested). `bridge/theme.rs` exposes it as the QML `Theme` singleton.

**Base colors:**
- The §5.2 base colors are kept **verbatim**: `bg`, `surface`, `surface2`, `border`, `text`, `textMuted`, and the default accents.
- Status colors keep their §5.2 value when it already reaches 3:1 on `bg`, `surface` and `surface2`. Only the light `warning` moves, from `#B7800F` to `#B37E0F`, because it is drawn on `surface2` (notices).

**Computed tokens:**

| Token | Rule |
|---|---|
| `accentText` | Text on an accent fill: the house ink with the better contrast (`#1A1406` dark or `#FFFFFF` light), falling back to pure black or white, so it always reaches **≥ 4.5:1**. Dark mode keeps §5.2 `#1A1406`. Light mode becomes `#1A1406` instead of `#FFFFFF`. |
| `accentFg` | The accent used **as** a text or icon color on `bg`/`surface`/`surface2`. It is moved towards `text` in 2.5 % steps until it reaches ≥ 4.5:1 on all three. |
| `borderStrong` | Outlines that identify controls: ≥ 3:1 on `surface`/`surface2`. |
| `focusRing` | Equal to `accentFg`. |
| `success`, `warning`, `danger`, `info` | Adjusted only if needed to reach ≥ 3:1 on `bg`, `surface` and `surface2`. Text on them uses `Theme.textOn(color)`. |
| `selection` | The accent, translucent. The scheme's alpha (`0x55` dark, `0x40` light) is only the upper bound: it is lowered until `text` and `textMuted` reach **≥ 4.5:1**, and `accentFg` (the icon of a selected item) **≥ 3:1**, on the selection composited over `bg`, `surface` and `surface2`. Qt blends in 8-bit sRGB and may round a channel one unit differently from us, so the check allows for that. The default accents get `0x28` (dark) and `0x1D` (light). |

**Overlay and state tokens:** `hover`, `pressed`, `selection` and `scrim` (translucent), plus `textDisabled`. These keep states, selection and modal dimming token-only.

**Accent contrast warning:** `accentLowContrast` is true when the accent is under 3:1 against `bg`, and Settings shows a warning.

**Metrics:** density (comfortable/compact) and UI scale drive every size. Spacing follows the 4 px scale; radii are 8 (cards) and 6 (controls); durations are 120 and 180 ms, and all durations become **0 with "reduce motion"**.

**Tests:**
- Every text token reaches AA on every surface, in both schemes and both densities.
- `text`, `textMuted` and `accentFg` reach their targets on the selection over every surface, even with every channel of the composite rounded one unit either way.
- **216 arbitrary accents plus the 7 accent presets, × 2 schemes,** all get a readable `accentText`, `accentFg` and selection, and the selection never drops to nothing.

## Consequences

- The light theme deviates from two §5.2 values (`accentText`, and slightly `warning`), on purpose and for accessibility.
- The selection is a lighter tint than a fixed alpha would give, most visibly in light mode. Selected items therefore don't rely on the fill alone: rows have an accent bar, the rail an indicator, and selected icons use `accentFg`.
- On a selection, `accentFg` is for icons only. Text on a selection uses `text` or `textMuted`.
- The hover and press overlays are not part of the selection check. Drawn on top of a selection they lower the contrast again (`textMuted` drops to about 3.9:1 on the dark `surface2` under the hover overlay), so a selected item may show them only as short-lived pointer feedback, never as a lasting state such as a "current item" highlight.
- There is one source of truth for colors. The QML lint rejects color literals, and new colors must be added as tokens with a contrast test.
- The Sprint 17 high-contrast theme becomes a new base palette. The same rules produce its derived tokens.
