# ADR 0013: The terminal is drawn by a C++ Qt Quick item with its own glyph atlas

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 2

## Context

PLAN §3.3 and the Sprint 2 checklist ask for a `TerminalItem` that draws the terminal grid from a snapshot of the damaged rows, with a glyph atlas aligned to the device pixel ratio and one batch per row. A `QQuickPaintedItem` MVP is allowed only if it meets the PLAN §9 budgets: render at the monitor's refresh rate, only what changed, and `yes` or a 100 MB `cat` must not freeze the UI.

The grid has to be exact. Every glyph sits on its cell, including CJK in two cells, combining marks, emoji from a fallback font, box drawing, Powerline symbols and missing glyphs. It also needs every SGR attribute: 256 colors and truecolor, bold, italic, dim, inverse, hidden, strikethrough, and single, double, curly, dotted and dashed underlines in their own color. On top of that come the selection, search matches, links, four cursor shapes and the input method preedit.

`cxx-qt-lib` 0.10 has no Qt Quick item or scene graph bindings, so any scene graph code is hand-written C++.

## Options

Measured with a probe on Windows 10, i7-12700KF, RTX 3060, a 180 Hz monitor, D3D11 and Qt 6.10.3. The grid was 200 x 60 cells with every row changing on every frame. "Sync" is the time the GUI thread is blocked per frame.

| Option | Frames/s | Sync (GUI blocked) | Render thread | Exact grid |
|---|---|---|---|---|
| **(a) C++ `QQuickItem`, own glyph atlas and material** | **180** (monitor rate) | **0.6 ms** | 0.7 ms | **yes** |
| (b) `QSGTextNode` per row (Qt 6.7+), distance fields | 31 | 11.2 ms | 19.9 ms | no |
| (b) `QSGTextNode` per row, native rendering | 31 | 10.5 ms | 20.8 ms | no |
| (b) `QSGTextNode` per row, curve rendering | 6.6 | 16 ms | 110 ms | no |
| (c) `QQuickPaintedItem`, `Image` target | 60 | 11.8 ms | upload of the whole image | no |
| (c) `QQuickPaintedItem`, `FramebufferObject` target (OpenGL, Qt 6.9+ only) | 127 | 7.0 ms | ~0 | no |
| (d) `QSGRenderNode` / `QQuickRhiItem` with raw QRhi | not measured | | | |

- **(a)** keeps the grid exact because glyph positions are ours. Untouched rows keep their geometry, so "only what changed" comes for free. With one row changing per frame (typing), the probe spent 0.08 ms on sync and 0.18 ms on the render thread.
- **(b)** positions glyphs with the shaper, so fallback fonts drift off the grid, and there is no API to place glyphs. It has only a plain underline. A changed row is rebuilt from scratch, and even with one changed row per frame the render thread needed 17 to 37 ms.
- **(c)** paints on the render thread with the GUI thread blocked, and uploads the whole image every frame (about 19 MB at 200 x 60 cells and DPR 1.5). The `FramebufferObject` target is ignored before Qt 6.9 and on every non-OpenGL backend. It fails PLAN §9.
- **(d)** needs the QRhi API, whose header says it has "limited compatibility guarantees". It gives nothing for textured quads that a `QSGMaterial` doesn't already give, and it loses the scene graph's batching.

## Decision

Option (a).

**Classes.** `TerminalItemBase` (`cpp/terminal_item.h`) is a `QQuickItem`, `QML_ANONYMOUS` in the `cc.caixa.opensesh` module. It owns the Qt side:

- font metrics and the grid size, exposed as `columns`, `lines`, `cellWidth`, `cellHeight` and `gridSizeChanged`;
- the cursor blink timer;
- key, mouse, wheel, hover, focus and input method events;
- the clipboard.

The Rust `TerminalItem` (`src/bridge/terminal_view.rs`, `#[base = TerminalItemBase]`) implements its pure virtual functions. It is named after PLAN §3.2, because `TerminalView` is already the terminal workspace view.

**Snapshot.** Snapshots are taken in `updatePaintNode`, on the render thread while the GUI thread is blocked. `fillFrame` asks Rust for an `opensesh_term::snapshot::Frame`, flattened into cxx shared structs:

- `TerminalFrameInfo`;
- the damaged rows' `TerminalCell`s back to back;
- the combining characters as `[count, code point...]` entries.

Colors arrive resolved (`0xAARRGGBB`). C++ validates the data and never trusts it. It keeps a copy of the grid, so a full rebuild (a new atlas, for example) needs nothing from Rust. A new root node, such as after a scene graph invalidation, or a new grid size asks Rust for a full frame.

**Scene graph.** A root node owns every render resource, the atlas texture included, so the scene graph frees them on the render thread. It has four children, drawn in this order:

1. one background `QSGRectangleNode` in the frame's default background;
2. one `QSGVertexColorMaterial` geometry node per row, for cell backgrounds (runs of equal color; the selection and search colors are already in the cells);
3. one glyph geometry node per row: the glyphs, then that row's underlines and strikethrough, so decorations are drawn over the text;
4. one overlay node for the cursor and the input method preedit, rebuilt every frame (a few quads), so blinking never touches the rows.

All glyph nodes share one material instance and one texture, so the renderer merges them into a single batch. Quads are indexed: 4 vertices of 24 bytes and 6 `quint16` indices each. Only rows marked damaged, or every row after an atlas change, get new geometry.

**Material.** `GlyphMaterial` uses the shaders in `shaders/terminal_glyph.{vert,frag}`:

- Masks are tinted with the premultiplied vertex color. Color glyphs are drawn as they are, faded only by the vertex alpha.
- The vertex shader snaps every corner to the device pixel grid, as Qt's own native text shader does.
- The shaders are compiled to `.qsb` by `cargo xtask shaders`, with `qsb -b --glsl "100 es,120,150" --hlsl 50 --msl 12`. `-b` (batchable) is required for the vertex shader. Without it, Qt Quick silently drops geometry it merges into a batch.
- The `.qsb` files are committed and bundled in the Qt resources (`:/qt/qml/cc/caixa/opensesh/shaders/`). Their format is `QSB_VERSION` 9 in both Qt 6.8.2 and 6.10.3.
- qsb output is byte-for-byte reproducible with a given Qt version, so `--check` compares bytes, like the `.qm` files (ADR 0009).

**Glyph atlas.**

- **Rasterizing.** Each glyph or grapheme cluster is rasterized once at device resolution with `QPainter::drawGlyphRun`, which handles fallback fonts, combining marks and color fonts:
  - a character the grid font has takes a fast path through `QRawFont`, without text layout (about 7 µs per glyph);
  - everything else goes through `QTextLayout`.
- **Placing.** Wide glyphs are centered in two cells. Glyphs wider than their cells (emoji) are scaled down evenly and centered vertically. Box drawing, block elements and Powerline separators are stretched from the font's full-block box onto the cell, so neighbouring cells join without seams.
- **Anti-aliasing.** It is **grayscale**: a transparent canvas with a white pen gives coverage. Color glyphs are detected because their pixels are not gray, and stored as premultiplied RGBA in the same RGBA8 texture.
- **Packing.** Glyphs are packed on shelves with a one-texel gutter. The atlas starts at 512 x 512 and doubles up to 4096 x 4096. When that is full, it is cleared and refilled with what is on screen. Printable ASCII in the regular style is pre-rasterized.
- **Uploading.** A change uploads the whole atlas (`QQuickWindow::createTextureFromImage`), which is rare once the visible glyphs are in.

**Cell size.** The cell is **rounded to whole device pixels**:

- width: `round(advance('M') x dpr)`;
- height: `round((ascent + descent + leading) x dpr)`;
- baseline, underline, strikethrough and line thickness: rounded from `QFontMetricsF`.

So every cell starts on a pixel at any DPR. Measured cells: JetBrains Mono 11 pt is 9 x 20 device pixels at 100 %, 11 x 25 at 125 % and 14 x 30 at 150 %. Rounding makes the cell up to half a device pixel wider or narrower than the font's advance. Which rule looks best is revisited with the font settings (Sprint 3).

**Decorations and cursor.**

- Single, double and dashed underlines, dotted underlines (square dots) and strikethrough are solid quads that sample a white block in the atlas. The curly underline is an anti-aliased tile, one sine period per cell.
- Links get a single underline.
- Cursor shapes are block (the character under it redrawn in the cursor text color), hollow block, beam and underline.
- Blinking uses a GUI-thread timer at half of `QStyleHints::cursorFlashTime()` (530 ms with the Windows default). It runs only while the item has the focus and `reduceMotion` is off. A platform setting of no blinking is honoured.

## Consequences

- **The software backend draws only the background.** Qt Quick's software backend skips every geometry node it doesn't know. It is used by `QT_QPA_PLATFORM=offscreen`, `QT_QUICK_BACKEND=software` and some VMs. The item detects it (`QSGRendererInterface::isApiRhiBased`), logs it once and draws only its background rectangle; it never crashes. The smoke tests therefore can't check terminal pixels. The terminal smoke test (Sprint 2 integration) checks the engine's text instead. A software fallback (per-row `QSGImageNode`s painted with `QPainter`) is possible later.
- **Grayscale anti-aliasing only.** Text can look lighter than ClearType or LCD text, especially dark text on a light background. It stays correct with the planned background opacity and images. LCD anti-aliasing needs dual-source blending, and its backend support is unverified. It is deferred with the other font settings (Sprint 3).
- **New build step for shader authors.** Editing `shaders/` needs Qt's `qsb` (the Qt Shader Tools module: `aqt install-qt ... -m qtshadertools`, `qt6-shadertools` on Arch, `qt6-shader-baker` on Debian). Everyone else uses the committed `.qsb`. CI runs `cargo xtask shaders --check` in the Windows job, with qsb 6.10.3 from the aqt `qtshadertools` module (the committed files are built on Windows). The vertex shader must never use input location 7, which the batchable rewrite takes.
- **Emoji on Qt 6.8 follow fontconfig.** Qt 6.8 has neither the emoji segmenter nor `addApplicationEmojiFontFamily` (6.9). So on Debian 13 a text-presentation fallback font can win over a color emoji font. Qt 6.10 has both and picks the color font. The distros tested in WSL have no CJK or emoji fonts, so those glyphs show the missing-glyph box there, still on the grid.
- **Qt 6.8 rejects revisioned QQuickItem properties on `TerminalItem` in QML.** For example, `activeFocusOnTab: false` fails with "is not available in cc.caixa.opensesh 255.255", while 6.10.3 accepts it. Such properties are set from C++ (the item takes Tab focus by default) or on a wrapping item.
- **Each item has its own atlas**, with 1 MiB of CPU memory and a texture of the same size to start. A shared per-window atlas and sub-rectangle uploads through `QSGTexture::commitTextureOperations` (QRhi, limited compatibility) are optimizations for later, if memory or first-frame time with many tabs needs them.
- **Costs measured** in the gallery demo (Windows 10, D3D11, 180 Hz monitor):
  - the benchmark rebuilds every row on every frame (Rust demo frame, conversion and geometry) and holds 180 frames/s, the monitor's rate;
  - the GUI thread is blocked for 0.82 ms per frame on average (1.0 ms at most) at 107 x 24 cells in a debug build, and 0.34 ms (0.9 ms at most) at 193 x 24 cells in a release build;
  - the first frame of the demo rasterizes 217 glyphs, including CJK, emoji, box drawing and the loading of fallback fonts, and blocks for 40 to 46 ms once. An ASCII-only item needs 1.4 ms. On Debian with FreeType the demo's first frame takes 8 ms.
- **The same output on every backend.** The gallery's terminal region is pixel-identical on D3D11, D3D12, Vulkan and OpenGL (Windows). The `.qsb` built with qsb 6.10.3 renders on Debian 13 (Qt 6.8.2, OpenGL, Wayland and X11 through WSLg).
- **The cursor is drawn by the renderer from `Frame::cursor`.** The engine must not bake the cursor into the cell colors, or blinking couldn't hide it.
