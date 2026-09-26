# ADR 0015: Programming ligatures by per-cell glyph substitution (experimental, off by default)

- **Status:** accepted
- **Date:** 2026-09-26
- **Sprint:** 3

## Context

PLAN §6.2 asks for programming ligatures as an option: experimental and off by default, decided with a spike and an ADR. Fonts such as JetBrains Mono (bundled), Fira Code and Cascadia Code draw `->`, `!=`, `===` or `<=>` as joined symbols when the text is shaped with their OpenType features.

The renderer ([ADR 0013](0013-terminal-rendering.md)) never shapes: each cell's glyph comes from a cmap lookup (`QRawFont::glyphIndexesForString`) or, for clusters and fallback fonts, from a `QTextLayout` of that single cell. The grid must stay a grid: a character belongs to one cell, selection and the cursor work per cell, and a partial redraw rebuilds whole rows.

The spike in [`spikes/ligatures/`](../../spikes/ligatures/) shapes samples with `QTextLayout` (HarfBuzz, the font's default features) and compares each shaped glyph with the glyph its character gets on its own. With JetBrains Mono 2.304 at 11 pt (cell 9 px) on Windows, Qt 6.10.3:

| Sample | Characters | Glyphs | One glyph per character, in order | Glyphs on the cell grid | Characters with another glyph |
|---|---|---|---|---|---|
| `->` | 2 | 2 | yes | yes | `-` `>` |
| `!=`, `=>`, `\|>`, `::` | 2 | 2 | yes | yes | both |
| `===`, `<=>` | 3 | 3 | yes | yes | all three |
| `<!--` | 4 | 4 | yes | yes | all four |
| `www`, `0xFF`, `fi fl` | | | yes | yes | none |
| `fn main() -> Result<(), Error> {` | 32 | 32 | yes | yes | `-` `>` |
| `if a != b && c >= d \|\| e == f {` | 31 | 31 | yes | yes | `! = & & > = \| \| = =` |

So these fonts form ligatures with **contextual alternates**: one glyph per character, each one cell wide, where the last glyph of `===` may reach back over the cells before it. Shaping a 120-column code line costs about **86 µs** (10,000 lines in 864 ms, release build), too much to do for every row of every frame.

## Options

1. **No ligatures.** Simple, but the option PLAN §6.2 asks for is missing.
2. **Shape whole rows and draw glyph runs.** The kitty and WezTerm way; it needs a shaping cache per row, and cursor, selection and partial redraws that understand multi-cell glyphs. A large change to the renderer.
3. **Per-cell glyph substitution:** shape only runs of plain ASCII in one style that contain two adjacent symbol characters, and when the shaped result keeps one glyph per character from the primary font, draw each cell with its shaped glyph instead of its own. Everything else about the grid stays as it is.

## Decision

Option 3, behind the profile option `ligatures` (default `false`).

- `RootNode::buildRow` finds the runs (`findLigatures`): cells without clusters, not wide, not hidden, printable ASCII, one style (bold and italic bits); a run is only shaped when two adjacent characters are ligature symbols (``! # $ % & * + - . / : ; < = > ? @ \ ^ _ | ~ [ ] { } ( )``).
- `GlyphAtlas::shapeRun` shapes the run text with the grid font of that style, and keeps a substitution only when the result is a single glyph run from the same family, with as many glyphs as characters and string indexes in order. Results are cached per style and text (at most 4096 entries per style, cleared when full).
- Shaping and the substitute glyphs (`glyphByIndex`, rasterized with a wider canvas so ink can reach three cells to the left) count against the renderer's per-frame rasterizing budget (4 ms, ADR 0013). Over budget, the row is drawn without ligatures and rebuilt on the next frame, as with deferred glyphs.
- Fonts that merge characters into one glyph, or that need a fallback font inside a run, simply get no ligatures there.

## Consequences

- JetBrains Mono, Fira Code and Cascadia Code show their ligatures, selection and the cursor keep working per cell, and nothing changes while the option is off (no shaping at all).
- Ligatures across a style change, a wide character or the soft wrap of a line are not formed; nor are ligatures of fonts that replace several characters with one glyph (a rare design for monospaced fonts).
- A screen full of new code lines costs about 86 µs per shaped row the first time; the cache makes redraws of the same text cheap. The option stays experimental until it has been used on more fonts and platforms.
- Letter spacing and line height apply to shaped glyphs as to the others: they are placed in the cell like the character they replace.
