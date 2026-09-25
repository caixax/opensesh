// Render side of the terminal item (ADR 0013): cell metrics, the glyph atlas and the scene graph
// nodes. Everything here except `computeMetrics` runs on the scene graph render thread (or on the
// GUI thread with the basic render loop), inside TerminalItemBase::updatePaintNode.
#pragma once

#include <cstdint>
#include <vector>

#include <QtCore/QHash>
#include <QtCore/QPoint>
#include <QtCore/QRect>
#include <QtCore/QRectF>
#include <QtCore/QSizeF>
#include <QtCore/QString>
#include <QtGui/QFont>
#include <QtGui/QImage>
#include <QtGui/QRawFont>
#include <QtQuick/QSGGeometryNode>
#include <QtQuick/QSGMaterial>
#include <QtQuick/QSGNode>
#include <QtQuick/QSGVertexColorMaterial>

#include "rust/cxx.h"

class QQuickWindow;
class QSGRectangleNode;
class QSGTexture;

namespace opensesh {

// Shared structs defined by the cxx bridge (src/bridge/terminal_view.rs).
struct TerminalCell;
struct TerminalFrameInfo;

namespace terminal {

// Cell geometry for one font at one device pixel ratio. Every length except `pointSize` is in
// device pixels and, for the vertical ones, measured from the top of the cell.
struct Metrics
{
    QString family;
    qreal pointSize = 0.0;
    qreal dpr = 1.0;
    int cellWidth = 0;
    int cellHeight = 0;
    int baseline = 0;
    int lineThickness = 1;
    int underlineTop = 0;
    int strikeTop = 0;

    bool valid() const { return cellWidth > 0 && cellHeight > 0; }
    bool operator==(const Metrics &other) const;
    bool operator!=(const Metrics &other) const { return !(*this == other); }
};

// Metrics of `family` at `pointSize` and `dpr`. The cell is rounded to whole device pixels
// (ADR 0013). Never fails: a missing family falls back to Qt's default font.
Metrics computeMetrics(const QString &family, qreal pointSize, qreal dpr);

// The grid font in one of the four styles (bit 0 bold, bit 1 italic).
QFont terminalFont(const QString &family, qreal pointSize, int style);

// Per-frame counters, for OPENSESH_TERMINAL_STATS.
struct RenderStats
{
    int rowsBuilt = 0;
    int glyphsRasterized = 0;
    qint64 rasterNs = 0;
    int textureUploads = 0;
    int atlasSize = 0;
    int atlasResets = 0;
    bool software = false;
};

// What the item passes to the render node for one frame.
struct RenderInput
{
    Metrics metrics;
    QSizeF size;
    qreal padding = 0.0;
    // Qt Quick's software backend draws no custom geometry: only the background is drawn.
    bool software = false;
    // Blink phase: false while a blinking cursor is in its hidden half.
    bool cursorBlinkOn = true;
    bool focused = false;
    QString preedit;
    int preeditCursor = 0;
};

// One cell of the grid copy kept by the render node (a TerminalCell with a per-row cluster
// offset).
struct GridCell
{
    char32_t ch = U' ';
    std::uint32_t cluster = 0;
    std::uint32_t fg = 0;
    std::uint32_t bg = 0;
    std::uint32_t underline = 0;
    std::uint16_t flags = 0;
};

struct CursorInfo
{
    int row = 0;
    int column = 0;
    // TerminalCursorShape value; 4 = hidden.
    int shape = 4;
    bool blinking = false;
    bool wide = false;
    std::uint32_t color = 0;
    std::uint32_t textColor = 0;
};

// Glyphs rasterized once at device resolution and packed into one RGBA texture: white coverage
// masks for text and decorations, premultiplied colors for color glyphs (emoji).
class GlyphAtlas
{
public:
    struct Glyph
    {
        // Texels in the atlas; empty for blank glyphs.
        QRect rect;
        // Top-left of `rect` relative to the cell's top-left, in device pixels.
        QPoint offset;
        bool color = false;
        bool valid = false;
    };

    // Drops every glyph and prepares for `metrics`; pre-rasterizes printable ASCII.
    void reset(const Metrics &metrics);
    // A single code point, fast path. Returned by value: adding a glyph can clear the atlas.
    Glyph glyph(char32_t ch, int style, int span);
    // A grapheme cluster (base character plus combining marks, or any text).
    Glyph cluster(const QString &text, int style, int span);
    // Natural advance of `text` in cells (1 or 2), for the preedit.
    int spanOf(const QString &text) const;
    // Curly underline tile, one cell wide.
    const Glyph &curly() const { return m_curly; }
    // Texel in the middle of a solid white block, for filled rectangles.
    QPoint solidTexel() const { return m_solid.center(); }

    const QImage &image() const { return m_image; }
    bool isDirty() const { return m_dirty; }
    void markClean() { m_dirty = false; }
    // Changes whenever existing glyphs move or the texture size changes: every quad built
    // before must be rebuilt.
    int generation() const { return m_generation; }
    // Allows one clear-and-restart per frame when the atlas is full.
    void beginFrame() { m_clearedThisFrame = false; }

    int glyphsRasterized = 0;
    qint64 rasterNs = 0;
    int resets = 0;

private:
    Glyph rasterize(const QString &text, char32_t single, int style, int span);
    bool place(const QImage &source, const QRect &area, bool color, QRect *placed);
    bool allocate(const QSize &size, QPoint *position);
    void clearContents(int size);
    void addFixedTiles();

    Metrics m_metrics;
    QFont m_fonts[4];
    QRawFont m_rawFonts[4];
    // Design box of U+2588 FULL BLOCK per style (empty if the font has none).
    QRectF m_cellBox[4];
    QImage m_image;
    int m_shelfX = 0;
    int m_shelfY = 0;
    int m_shelfHeight = 0;
    bool m_dirty = false;
    int m_generation = 0;
    bool m_clearedThisFrame = false;
    QHash<quint32, Glyph> m_fast;
    QHash<QString, Glyph> m_clusters;
    Glyph m_curly;
    QRect m_solid;
};

// Material shared by every glyph node: samples the atlas texture (glyph shaders in
// shaders/terminal_glyph.*).
class GlyphMaterial final : public QSGMaterial
{
public:
    GlyphMaterial();
    QSGMaterialType *type() const override;
    QSGMaterialShader *createShader(QSGRendererInterface::RenderMode renderMode) const override;
    int compare(const QSGMaterial *other) const override;

    QSGTexture *texture = nullptr;
};

// Root of the item's scene graph subtree. It owns the render state, so the scene graph frees it
// (texture included) on the render thread when the item goes away or the graph is invalidated.
//   child 0: background rectangle (also drawn by the software backend)
//   child 1: one background geometry node per row (vertex colors)
//   child 2: one glyph geometry node per row (glyphs, then underlines and strikethrough)
//   child 3: cursor and input method preedit (glyph material)
class RootNode final : public QSGNode
{
public:
    explicit RootNode(QQuickWindow *window);
    ~RootNode() override;

    // Whether the next frame must hold every row (nothing received yet, or another grid size).
    bool needsFullFrame(int columns, int lines) const;
    // Copies a frame from Rust into the grid; invalid data is ignored, never trusted. The
    // `requested*` arguments are what fillFrame was asked for.
    void applyFrame(const TerminalFrameInfo &info, const ::rust::Vec<std::uint16_t> &rows,
                    const ::rust::Vec<TerminalCell> &cells,
                    const ::rust::Vec<std::uint32_t> &clusters, int requestedColumns,
                    int requestedLines, bool requestedFull);
    // Brings the nodes up to date with the grid.
    void sync(QQuickWindow *window, const RenderInput &input, RenderStats &stats);

    bool hasFrame() const { return m_hasFrame; }
    const CursorInfo &cursor() const { return m_cursor; }
    int frameColumns() const { return m_columns; }
    int frameLines() const { return m_lines; }

private:
    struct Quad
    {
        // Device pixels relative to the grid origin.
        float x0;
        float y0;
        float x1;
        float y1;
        // Atlas texels.
        float u0;
        float v0;
        float u1;
        float v1;
        std::uint32_t color;
        // 0: coverage mask tinted with `color`; 1: color glyph.
        float kind;
    };

    void ensureRowNodes(int lines);
    void buildRow(int row, const RenderInput &input);
    void buildOverlay(const RenderInput &input);
    void appendGlyph(std::vector<Quad> &out, const GlyphAtlas::Glyph &glyph, int x, int y,
                     std::uint32_t color) const;
    void appendSolid(std::vector<Quad> &out, int x, int y, int width, int height,
                     std::uint32_t color) const;
    void appendDecorations(std::vector<Quad> &out, const GridCell &cell, int x, int y,
                           int width) const;
    GlyphAtlas::Glyph glyphForCell(int row, const GridCell &cell, int span);
    void writeGlyphGeometry(QSGGeometryNode *node, const std::vector<Quad> &quads,
                            const RenderInput &input) const;
    void uploadAtlas(QQuickWindow *window, RenderStats &stats);

    QSGRectangleNode *m_background = nullptr;
    QSGNode *m_backgroundLayer = nullptr;
    QSGNode *m_glyphLayer = nullptr;
    QSGGeometryNode *m_overlay = nullptr;
    QSGVertexColorMaterial m_backgroundMaterial;
    GlyphMaterial m_glyphMaterial;
    QSGTexture *m_texture = nullptr;
    GlyphAtlas m_atlas;
    Metrics m_builtMetrics;
    int m_builtGeneration = -1;
    qreal m_builtPadding = 0.0;
    std::vector<QSGGeometryNode *> m_backgroundNodes;
    std::vector<QSGGeometryNode *> m_glyphNodes;
    std::vector<Quad> m_scratchBackgrounds;
    std::vector<Quad> m_scratchGlyphs;
    std::vector<Quad> m_scratchDecorations;

    int m_columns = 0;
    int m_lines = 0;
    std::vector<GridCell> m_grid;
    // Per row: combining characters, [count, code point...]; GridCell::cluster is 1 + offset.
    std::vector<std::vector<char32_t>> m_rowClusters;
    std::vector<char> m_rowDirty;
    CursorInfo m_cursor;
    std::uint32_t m_frameBackground = 0;
    bool m_hasFrame = false;
    // Grid size of the last full frame that was asked for.
    int m_fullColumns = -1;
    int m_fullLines = -1;
};

} // namespace terminal
} // namespace opensesh
