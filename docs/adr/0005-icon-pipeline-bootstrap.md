# ADR 0005: Icon pipeline bootstrapped in Sprint 0, and the placeholder app logo

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 0

## Context

Sprint 0 needs a placeholder application icon for the window and the `.desktop` file. PLAN §7 sets a hard rule: icons are never drawn, generated or hand-edited. They may only come from pinned Lucide, Tabler or Simple Icons packages through `cargo xtask icons`. The complete pipeline (image provider, `OsIcon`, the full icon set) is Sprint 1 work.

The placeholder logo is specified as the `lucide:door-open` glyph in white, on a squircle filled with the accent color.

Verified facts:

- `lucide-static` 1.48.0 is the latest version on npm.
- Its tarball sha256 is `3c2ecda3d25f6a9692d83f8036d9a526f7da584a51af74cd16eda4498c5c33d8`. The npm sha1 and sha512 integrity fields match.
- `door-open` is ISC-licensed. It isn't among the Feather-derived (MIT) icons listed in the Lucide license.
- Qt SVG 6.10.3 renders `currentColor` as black unless the color is set explicitly.

## Options

1. Hand-write an SVG for the logo. **Forbidden** by §7.
2. Copy a PNG from somewhere. It isn't reproducible, and the license is unclear.
3. **Implement the core of the §7 pipeline now**, with one icon, and compose the logo in code from the pinned glyph.

## Decision

Option 3.

- **Manifest.** `assets/icons/icons.toml` pins the source and its sha256, lists the icons and holds the logo parameters.
- **Tarball.** `cargo xtask icons` downloads the tarball from `registry.npmjs.org`, verifies the sha256 before reading it, and caches it in `target/xtask-cache/`.
- **Icon files.** It extracts only the listed icons. The only change it makes is setting the root `stroke`/`fill` attributes to `currentColor` (Lucide already uses it). The results are written to `crates/opensesh-app/qml/icons/`, and `build.rs` compiles every SVG there into the Qt resources.
- **Licenses.** It copies the upstream license to `assets/icons/LICENSES/` and regenerates `THIRD_PARTY_NOTICES.md`.
- **Logo.** It composes `crates/opensesh-app/data/icons/cc.caixa.OpenSesh.svg`:
  - The background is a `<rect>` primitive with `rx`/`ry` computed from manifest numbers (256 px size, 56 px corner radius). There is no path data, and it is an approximation of a squircle.
  - The Lucide glyph children are copied **byte for byte** inside a `<g>` that translates and scales the 24×24 grid and sets `stroke="#FFFFFF"`. Setting the color explicitly avoids Qt's black `currentColor`.
  - The background color is the dark-theme accent `#E6B450` (PLAN §5.2).
- **Offline builds.** Generated files are committed, so builds don't need the network. CI re-runs the pipeline and fails if anything differs from the committed files.
- **Not in Sprint 0:** the `QQuickImageProvider`, the `OsIcon` component, and Tabler and Simple Icons sources. The manifest format already supports more sources.

## Consequences

- White on `#E6B450` has low contrast (about 1.9:1). That is acceptable for a placeholder only; a human-made logo replaces it later.
- Every Lucide file starts with a `<!-- @license lucide-static vX -->` comment. A version bump therefore changes every icon file even when the artwork is the same, and diffs are larger. That's expected.
- The logo composition is code (`xtask/src/icons.rs`) with unit tests. The tests assert that the glyph paths are copied verbatim.
