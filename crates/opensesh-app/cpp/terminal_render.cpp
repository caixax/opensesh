#include "opensesh-app/terminal_render.h"

// The bridge declares a #[qobject], so its .cxxqt.h must be included (not the .cxx.h): it defines
// TerminalCell, TerminalFrameInfo, TerminalCellFlag and TerminalCursorShape.
#include "opensesh-app/src/bridge/terminal_view.cxxqt.h"

#include <algorithm>
#include <cmath>
#include <cstring>
#include <string>
#include <utility>

#include <QtCore/QElapsedTimer>
#include <QtCore/QLoggingCategory>
#include <QtCore/QTextBoundaryFinder>
#include <QtGui/QFontMetricsF>
#include <QtGui/QGlyphRun>
#include <QtGui/QPainter>
#include <QtGui/QPainterPath>
#include <QtGui/QTextLayout>
#include <QtQuick/QQuickWindow>
#include <QtQuick/QSGGeometry>
#include <QtQuick/QSGMaterialShader>
#include <QtQuick/QSGRectangleNode>
#include <QtQuick/QSGTexture>

Q_DECLARE_LOGGING_CATEGORY(lcTerminal)

namespace opensesh::terminal {

namespace {

// Atlas texture: starts at 512 x 512 (1 MiB, enough for ASCII in four styles at 100 %) and
// doubles up to 4096 x 4096 (supported by every desktop GPU); when that is full it is cleared
// and refilled with the glyphs on screen.
constexpr int kInitialAtlasSize = 512;
constexpr int kMaxAtlasSize = 4096;
// A 16-bit index buffer addresses 65536 vertices, 4 per quad.
constexpr std::size_t kMaxQuadsPerNode = 16383;
// Grids larger than this are ignored (a 4K screen at a 4-pixel font is about 1 million cells).
constexpr std::size_t kMaxCells = 4u * 1024u * 1024u;
// Combining characters kept per cell.
constexpr std::uint32_t kMaxClusterLength = 32;
constexpr int kCursorHidden = 4;

constexpr std::uint16_t flag(TerminalCellFlag value)
{
    return static_cast<std::uint16_t>(value);
}

constexpr std::uint16_t kAnyUnderline = flag(TerminalCellFlag::Underline)
        | flag(TerminalCellFlag::DoubleUnderline) | flag(TerminalCellFlag::CurlyUnderline)
        | flag(TerminalCellFlag::DottedUnderline) | flag(TerminalCellFlag::DashedUnderline);

struct GlyphVertex
{
    float x;
    float y;
    float u;
    float v;
    unsigned char r;
    unsigned char g;
    unsigned char b;
    unsigned char a;
    float kind;
};
static_assert(sizeof(GlyphVertex) == 24, "unexpected glyph vertex size");

const QSGGeometry::AttributeSet &glyphAttributes()
{
    static const QSGGeometry::Attribute attributes[] = {
        QSGGeometry::Attribute::createWithAttributeType(0, 2, QSGGeometry::FloatType,
                                                        QSGGeometry::PositionAttribute),
        QSGGeometry::Attribute::createWithAttributeType(1, 2, QSGGeometry::FloatType,
                                                        QSGGeometry::TexCoordAttribute),
        QSGGeometry::Attribute::createWithAttributeType(2, 4, QSGGeometry::UnsignedByteType,
                                                        QSGGeometry::ColorAttribute),
        QSGGeometry::Attribute::createWithAttributeType(3, 1, QSGGeometry::FloatType,
                                                        QSGGeometry::UnknownAttribute),
    };
    static const QSGGeometry::AttributeSet set = { 4, int(sizeof(GlyphVertex)), attributes };
    return set;
}

// 0xAARRGGBB to premultiplied bytes.
void premultiply(std::uint32_t argb, unsigned char out[4])
{
    const unsigned a = (argb >> 24) & 0xffu;
    out[0] = static_cast<unsigned char>((((argb >> 16) & 0xffu) * a + 127) / 255);
    out[1] = static_cast<unsigned char>((((argb >> 8) & 0xffu) * a + 127) / 255);
    out[2] = static_cast<unsigned char>(((argb & 0xffu) * a + 127) / 255);
    out[3] = static_cast<unsigned char>(a);
}

// Box drawing, block elements and Powerline separators are designed to fill the line box; they
// are stretched to fill the cell exactly, so neighbouring cells join without seams.
bool fillsCell(char32_t ch)
{
    return (ch >= 0x2500 && ch <= 0x259F) || (ch >= 0xE0B0 && ch <= 0xE0BF);
}

// A Unicode scalar value, or U+FFFD.
char32_t sanitize(std::uint32_t codePoint)
{
    if (codePoint > 0x10FFFFu || (codePoint >= 0xD800u && codePoint <= 0xDFFFu))
        return char32_t(0xFFFDu);
    return static_cast<char32_t>(codePoint);
}

class GlyphShader final : public QSGMaterialShader
{
public:
    GlyphShader()
    {
        // Compiled by `cargo xtask shaders`, bundled by build.rs.
        setShaderFileName(VertexStage, QStringLiteral(":/qt/qml/cc/caixa/opensesh/shaders/"
                                                      "terminal_glyph.vert.qsb"));
        setShaderFileName(FragmentStage, QStringLiteral(":/qt/qml/cc/caixa/opensesh/shaders/"
                                                        "terminal_glyph.frag.qsb"));
    }

    bool updateUniformData(RenderState &state, QSGMaterial *, QSGMaterial *) override
    {
        // std140: mat4 modelViewMatrix, mat4 projectionMatrix, float qt_Opacity, float dpr.
        QByteArray *buffer = state.uniformData();
        if (buffer->size() < 136)
            return false;
        if (state.isMatrixDirty()) {
            const QMatrix4x4 modelView = state.modelViewMatrix();
            const QMatrix4x4 projection = state.projectionMatrix();
            std::memcpy(buffer->data(), modelView.constData(), 64);
            std::memcpy(buffer->data() + 64, projection.constData(), 64);
        }
        if (state.isOpacityDirty()) {
            const float opacity = state.opacity();
            std::memcpy(buffer->data() + 128, &opacity, 4);
        }
        // Written every time (so the buffer always changes): each batch may have its own
        // uniform buffer.
        const float dpr = state.devicePixelRatio();
        std::memcpy(buffer->data() + 132, &dpr, 4);
        return true;
    }

    void updateSampledImage(RenderState &state, int binding, QSGTexture **texture,
                            QSGMaterial *newMaterial, QSGMaterial *) override
    {
        if (binding != 1)
            return;
        auto *material = static_cast<GlyphMaterial *>(newMaterial);
        if (material->texture)
            material->texture->commitTextureOperations(state.rhi(), state.resourceUpdateBatch());
        *texture = material->texture;
    }
};

} // namespace

// ---- Metrics ------------------------------------------------------------------------------------

bool FontSpec::operator==(const FontSpec &other) const
{
    return family == other.family && fallbacks == other.fallbacks
            && pointSize == other.pointSize && weight == other.weight
            && boldWeight == other.boldWeight && italic == other.italic
            && lineHeight == other.lineHeight && letterSpacing == other.letterSpacing
            && antialiasing == other.antialiasing && hinting == other.hinting
            && ligatures == other.ligatures;
}

bool Metrics::operator==(const Metrics &other) const
{
    return font == other.font && dpr == other.dpr && cellWidth == other.cellWidth
            && cellHeight == other.cellHeight && baseline == other.baseline
            && lineThickness == other.lineThickness && underlineTop == other.underlineTop
            && strikeTop == other.strikeTop;
}

QFont terminalFont(const FontSpec &spec, int style)
{
    QFont font;
    QStringList families;
    if (!spec.family.isEmpty())
        families.append(spec.family);
    for (const QString &fallback : spec.fallbacks) {
        if (!fallback.isEmpty() && !families.contains(fallback))
            families.append(fallback);
    }
    if (!families.isEmpty())
        font.setFamilies(families);
    font.setStyleHint(QFont::Monospace);
    font.setPointSizeF(spec.pointSize > 0 ? spec.pointSize : 11.0);
    // Glyphs are shaped one cell (or cluster) at a time, so kerning could only shift them.
    font.setKerning(false);
    const int weight = (style & 1) != 0 ? spec.boldWeight : spec.weight;
    font.setWeight(QFont::Weight(std::clamp(weight, 100, 900)));
    font.setItalic(spec.italic && (style & 2) != 0);
    font.setStyleStrategy(spec.antialiasing ? QFont::PreferAntialias : QFont::NoAntialias);
    switch (spec.hinting) {
    case 1:
        font.setHintingPreference(QFont::PreferNoHinting);
        break;
    case 2:
        font.setHintingPreference(QFont::PreferVerticalHinting);
        break;
    case 3:
        font.setHintingPreference(QFont::PreferFullHinting);
        break;
    default:
        font.setHintingPreference(QFont::PreferDefaultHinting);
        break;
    }
    return font;
}

Metrics computeMetrics(const FontSpec &font, qreal dpr)
{
    Metrics metrics;
    metrics.font = font;
    metrics.dpr = dpr > 0 ? dpr : 1.0;
    const qreal d = metrics.dpr;
    const QFontMetricsF fm(terminalFont(font, 0));

    // Whole device pixels, so every cell starts on a pixel (ADR 0013). Letter spacing widens
    // (or narrows) the cell; glyphs stay centered in it.
    const qreal advance = fm.horizontalAdvance(QLatin1Char('M'));
    metrics.cellWidth = std::max(1, int(std::lround((advance + font.letterSpacing) * d)));
    const qreal ascent = std::max<qreal>(fm.ascent(), 1.0);
    const qreal descent = std::max<qreal>(fm.descent(), 0.0);
    const qreal leading = std::max<qreal>(fm.leading(), 0.0);
    const qreal natural = (ascent + descent + leading) * d;
    const qreal lineHeight = std::clamp<qreal>(font.lineHeight, 0.5, 3.0);
    metrics.cellHeight = std::max(2, int(std::lround(natural * lineHeight)));
    // Extra line height is shared above and below the text.
    const qreal extra = (metrics.cellHeight - natural) / 2;
    metrics.baseline = std::clamp(int(std::lround((leading / 2 + ascent) * d + extra)), 1,
                                  metrics.cellHeight - 1);

    const int thickness = std::max(1, int(std::lround(fm.lineWidth() * d)));
    metrics.lineThickness = thickness;
    const int lowest = std::max(0, metrics.cellHeight - thickness);
    metrics.underlineTop = std::clamp(
            metrics.baseline + std::max(1, int(std::lround(fm.underlinePos() * d))), 0, lowest);
    metrics.strikeTop = std::clamp(
            metrics.baseline - int(std::lround(fm.strikeOutPos() * d)) - thickness / 2, 0,
            lowest);
    return metrics;
}

// ---- GlyphAtlas ---------------------------------------------------------------------------------

void GlyphAtlas::reset(const Metrics &metrics)
{
    m_metrics = metrics;
    for (int style = 0; style < 4; ++style) {
        m_fonts[style] = terminalFont(metrics.font, style);
        m_rawFonts[style] = QRawFont::fromFont(m_fonts[style]);
        // The full block's design box is the font's cell: box drawing is stretched from it.
        m_cellBox[style] = QRectF();
        const QRawFont &raw = m_rawFonts[style];
        if (raw.isValid() && raw.supportsCharacter(0x2588u)) {
            const QList<quint32> block = raw.glyphIndexesForString(QString(QChar(0x2588)));
            if (block.size() == 1 && block.first() != 0) {
                const QRectF box = raw.boundingRect(block.first());
                if (box.width() > 0.5 && box.height() > 0.5)
                    m_cellBox[style] = box;
            }
        }
    }
    ++resets;
    clearContents(kInitialAtlasSize);
    // Printable ASCII in the regular style: most of what a shell shows first.
    for (char32_t ch = U'!'; ch <= U'~'; ++ch)
        glyph(ch, 0, 1);
}

void GlyphAtlas::clearContents(int size)
{
    m_image = QImage(size, size, QImage::Format_RGBA8888_Premultiplied);
    m_image.fill(Qt::transparent);
    // A one-texel gutter at the top and left edges too.
    m_shelfX = 1;
    m_shelfY = 1;
    m_shelfHeight = 0;
    m_fast.clear();
    m_clusters.clear();
    m_indexed.clear();
    for (auto &shaped : m_shaped)
        shaped.clear();
    ++m_generation;
    m_dirty = true;
    addFixedTiles();
}

void GlyphAtlas::addFixedTiles()
{
    // Solid white block: every filled rectangle samples its center.
    QPoint position;
    m_solid = QRect();
    if (allocate(QSize(4, 4), &position)) {
        m_solid = QRect(position, QSize(4, 4));
        for (int y = 0; y < 4; ++y)
            std::memset(m_image.scanLine(position.y() + y) + position.x() * 4, 0xff, 16);
    }

    // Curly underline: one sine period per cell, so neighbouring cells join.
    m_curly = Glyph();
    const Metrics &m = m_metrics;
    if (!m.valid())
        return;
    const qreal thickness = m.lineThickness;
    const qreal amplitude = 0.75 * (thickness + 1.0);
    const int height = int(std::ceil(2 * amplitude + thickness)) + 2;
    QImage tile(m.cellWidth, height, QImage::Format_ARGB32_Premultiplied);
    tile.fill(Qt::transparent);
    {
        QPainter painter(&tile);
        painter.setRenderHint(QPainter::Antialiasing, true);
        QPen pen(Qt::white);
        pen.setWidthF(thickness);
        pen.setCapStyle(Qt::FlatCap);
        painter.setPen(pen);
        QPainterPath path;
        const qreal center = height / 2.0;
        const qreal twoPi = 6.283185307179586;
        for (qreal x = -1.0; x <= m.cellWidth + 1.0; x += 0.25) {
            const QPointF point(x, center + amplitude * std::sin(twoPi * x / m.cellWidth));
            if (x == -1.0)
                path.moveTo(point);
            else
                path.lineTo(point);
        }
        painter.drawPath(path);
    }
    QRect placed;
    if (place(tile, tile.rect(), false, &placed)) {
        int top = m.underlineTop + m.lineThickness / 2 - height / 2;
        top = std::max(m.baseline, std::min(top, m.cellHeight - height));
        m_curly.rect = placed;
        m_curly.offset = QPoint(0, top);
        m_curly.valid = true;
    }
}

bool GlyphAtlas::allocate(const QSize &size, QPoint *position)
{
    // One transparent texel between entries.
    const int width = size.width() + 1;
    const int height = size.height() + 1;
    if (width > m_image.width() || height > m_image.height())
        return false;
    if (m_shelfX + width > m_image.width()) {
        m_shelfX = 1;
        m_shelfY += m_shelfHeight;
        m_shelfHeight = 0;
    }
    if (m_shelfY + height > m_image.height())
        return false;
    *position = QPoint(m_shelfX, m_shelfY);
    m_shelfX += width;
    m_shelfHeight = std::max(m_shelfHeight, height);
    return true;
}

bool GlyphAtlas::place(const QImage &source, const QRect &area, bool color, QRect *placed)
{
    QPoint position;
    if (!allocate(area.size(), &position)) {
        if (m_image.width() < kMaxAtlasSize) {
            // Grow: existing entries keep their texels, but normalized coordinates change.
            const int size = m_image.width() * 2;
            QImage grown(size, size, QImage::Format_RGBA8888_Premultiplied);
            if (grown.isNull()) {
                // Out of memory: keep the current atlas; this glyph stays blank.
                qCWarning(lcTerminal) << "could not grow the glyph atlas to" << size << "px";
                return false;
            }
            grown.fill(Qt::transparent);
            for (int y = 0; y < m_image.height(); ++y)
                std::memcpy(grown.scanLine(y), m_image.constScanLine(y),
                            std::size_t(m_image.bytesPerLine()));
            m_image = grown;
            ++m_generation;
            m_dirty = true;
        } else if (!m_clearedThisFrame) {
            // Full at the maximum size: start again with only what this frame needs.
            m_clearedThisFrame = true;
            ++resets;
            clearContents(m_image.width());
        }
        if (!allocate(area.size(), &position))
            return false;
    }
    for (int y = 0; y < area.height(); ++y) {
        const auto *in = reinterpret_cast<const QRgb *>(source.constScanLine(area.y() + y))
                + area.x();
        unsigned char *out = m_image.scanLine(position.y() + y) + position.x() * 4;
        for (int x = 0; x < area.width(); ++x) {
            const QRgb pixel = in[x];
            const auto alpha = static_cast<unsigned char>(qAlpha(pixel));
            if (color) {
                out[0] = static_cast<unsigned char>(qRed(pixel));
                out[1] = static_cast<unsigned char>(qGreen(pixel));
                out[2] = static_cast<unsigned char>(qBlue(pixel));
            } else {
                out[0] = alpha;
                out[1] = alpha;
                out[2] = alpha;
            }
            out[3] = alpha;
            out += 4;
        }
    }
    *placed = QRect(position, area.size());
    m_dirty = true;
    return true;
}

GlyphAtlas::Glyph GlyphAtlas::glyph(char32_t ch, int style, int span)
{
    style &= 3;
    span = span == 2 ? 2 : 1;
    ch = sanitize(ch);
    const quint32 key = (quint32(style) << 22) | (quint32(span == 2) << 21) | quint32(ch);
    const auto found = m_fast.constFind(key);
    if (found != m_fast.constEnd())
        return *found;
    if (m_frameNs >= kFrameBudgetNs) {
        ++m_deferred;
        return Glyph();
    }
    const Glyph result = rasterize(QString::fromUcs4(&ch, 1), ch, style, span);
    m_fast.insert(key, result);
    return result;
}

GlyphAtlas::Glyph GlyphAtlas::cluster(const QString &text, int style, int span)
{
    style &= 3;
    span = span == 2 ? 2 : 1;
    const QString key = QChar(u'A' + style * 2 + span - 1) + text;
    const auto found = m_clusters.constFind(key);
    if (found != m_clusters.constEnd())
        return *found;
    if (m_frameNs >= kFrameBudgetNs) {
        ++m_deferred;
        return Glyph();
    }
    const Glyph result = rasterize(text, 0, style, span);
    m_clusters.insert(key, result);
    return result;
}

QList<quint32> GlyphAtlas::shapeRun(const QString &text, int style)
{
    style &= 3;
    QHash<QString, QList<quint32>> &cache = m_shaped[style];
    const auto found = cache.constFind(text);
    if (found != cache.constEnd())
        return *found;
    if (m_frameNs >= kFrameBudgetNs) {
        ++m_deferred;
        return {};
    }
    QElapsedTimer timer;
    timer.start();
    QList<quint32> result(text.size(), 0);
    const QRawFont &raw = m_rawFonts[style];
    if (raw.isValid()) {
        QTextLayout layout(text, m_fonts[style]);
        QTextOption option;
        option.setWrapMode(QTextOption::NoWrap);
        layout.setTextOption(option);
        layout.beginLayout();
        const QTextLine line = layout.createLine();
        layout.endLayout();
        const QList<QGlyphRun> runs = line.isValid()
                ? layout.glyphRuns(0, text.size(),
                                   QTextLayout::RetrieveGlyphIndexes
                                           | QTextLayout::RetrieveStringIndexes)
                : QList<QGlyphRun>();
        // Only fonts that keep one glyph per character on the grid (JetBrains Mono, Fira Code,
        // Cascadia Code): anything else (fallback fonts, merged glyphs) stays unshaped.
        if (runs.size() == 1 && runs.first().rawFont().familyName() == raw.familyName()) {
            const QList<quint32> glyphs = runs.first().glyphIndexes();
            const QList<qsizetype> indexes = runs.first().stringIndexes();
            const QList<quint32> plain = raw.glyphIndexesForString(text);
            if (glyphs.size() == text.size() && indexes.size() == text.size()
                && plain.size() == text.size()) {
                for (qsizetype i = 0; i < text.size(); ++i) {
                    if (indexes[i] == i && glyphs[i] != 0 && glyphs[i] != plain[i])
                        result[i] = glyphs[i];
                }
            }
        }
    }
    m_frameNs += timer.nsecsElapsed();
    // Bounded: a long session of changing text can't grow it without end.
    if (cache.size() >= 4096)
        cache.clear();
    cache.insert(text, result);
    return result;
}

GlyphAtlas::Glyph GlyphAtlas::glyphByIndex(quint32 index, int style)
{
    style &= 3;
    const quint64 key = (quint64(style) << 32) | index;
    const auto found = m_indexed.constFind(key);
    if (found != m_indexed.constEnd())
        return *found;
    if (m_frameNs >= kFrameBudgetNs) {
        ++m_deferred;
        return Glyph();
    }
    const Glyph result = rasterize(QString(QLatin1Char(' ')), 0, style, 1, index);
    m_indexed.insert(key, result);
    return result;
}

int GlyphAtlas::spanOf(const QString &text) const
{
    if (!m_metrics.valid())
        return 1;
    const qreal advance = QFontMetricsF(m_fonts[0]).horizontalAdvance(text);
    const qreal cell = m_metrics.cellWidth / m_metrics.dpr;
    return advance > cell * 1.5 ? 2 : 1;
}

GlyphAtlas::Glyph GlyphAtlas::rasterize(const QString &text, char32_t single, int style, int span,
                                        quint32 glyphIndex)
{
    Glyph result;
    const Metrics &m = m_metrics;
    if (!m.valid() || text.isEmpty())
        return result;
    QElapsedTimer timer;
    timer.start();
    ++glyphsRasterized;

    // Rasterized at device resolution into a padded canvas, with the cell's top-left at
    // (pad, pad): room for italic overhang, accents and tall fallback glyphs.
    const qreal dpr = m.dpr;
    const int pad = std::max(2, m.cellHeight / 2);
    // A ligature's last glyph may draw over the cells before it (JetBrains Mono's `===`).
    const int padX = glyphIndex != 0 ? std::max(pad, 3 * m.cellWidth) : pad;
    QImage canvas(m.cellWidth * span + 2 * padX, m.cellHeight + 2 * pad,
                  QImage::Format_ARGB32_Premultiplied);
    canvas.fill(Qt::transparent);
    canvas.setDevicePixelRatio(dpr);
    const qreal cellWidth = qreal(m.cellWidth * span) / dpr;
    const qreal cellHeight = qreal(m.cellHeight) / dpr;
    const qreal left = padX / dpr;
    const qreal top = pad / dpr;
    const qreal baseline = (pad + m.baseline) / dpr;

    {
        QPainter painter(&canvas);
        painter.setRenderHint(QPainter::TextAntialiasing, m.font.antialiasing);
        painter.setPen(Qt::white);

        QList<QGlyphRun> runs;
        qreal advance = 0.0;
        qreal ascent = 0.0;
        qreal descent = 0.0;
        qreal runY = 0.0;
        // Fast path: a character the grid font has, without text layout.
        const QRawFont &raw = m_rawFonts[style];
        if (glyphIndex != 0 && raw.isValid()) {
            const QList<quint32> indexes { glyphIndex };
            QGlyphRun run;
            run.setRawFont(raw);
            run.setGlyphIndexes(indexes);
            run.setPositions({ QPointF(0, 0) });
            const QList<QPointF> advances = raw.advancesForGlyphIndexes(indexes);
            advance = advances.isEmpty() ? cellWidth : advances.first().x();
            ascent = raw.ascent();
            descent = raw.descent();
            runs.append(run);
        } else if (single != 0 && raw.isValid() && raw.supportsCharacter(uint(single))) {
            const QList<quint32> indexes = raw.glyphIndexesForString(text);
            if (indexes.size() == 1 && indexes.first() != 0) {
                QGlyphRun run;
                run.setRawFont(raw);
                run.setGlyphIndexes(indexes);
                run.setPositions({ QPointF(0, 0) });
                const QList<QPointF> advances = raw.advancesForGlyphIndexes(indexes);
                advance = advances.isEmpty() ? cellWidth : advances.first().x();
                ascent = raw.ascent();
                descent = raw.descent();
                runs.append(run);
            }
        }
        // Everything else goes through text layout: font fallback (CJK, emoji, symbols),
        // combining marks, variation selectors.
        QTextLayout layout;
        if (runs.isEmpty() && glyphIndex == 0) {
            layout.setText(text);
            layout.setFont(m_fonts[style]);
            QTextOption option;
            option.setWrapMode(QTextOption::NoWrap);
            layout.setTextOption(option);
            layout.setCacheEnabled(true);
            layout.beginLayout();
            QTextLine line = layout.createLine();
            if (line.isValid())
                line.setPosition(QPointF(0, 0));
            layout.endLayout();
            if (line.isValid()) {
                runs = layout.glyphRuns();
                advance = line.naturalTextWidth();
                ascent = line.ascent();
                descent = line.descent();
                runY = -line.ascent();
            }
        }

        const QRectF &cellBox = m_cellBox[style];
        if (!runs.isEmpty() && single != 0 && runY == 0.0 && fillsCell(single)
            && cellBox.isValid()) {
            // Map the font's full-block box exactly onto the cell.
            const qreal sx = cellWidth / cellBox.width();
            const qreal sy = cellHeight / cellBox.height();
            painter.translate(left - cellBox.left() * sx, top - cellBox.top() * sy);
            painter.scale(sx, sy);
            for (const QGlyphRun &run : std::as_const(runs))
                painter.drawGlyphRun(QPointF(0, 0), run);
        } else if (!runs.isEmpty() && advance > 0.0) {
            // Too wide for its cells (emoji, wide fallback glyphs): shrink evenly and center
            // vertically. Otherwise center horizontally on the baseline.
            qreal scale = 1.0;
            qreal y = baseline;
            if (advance > cellWidth * 1.05) {
                scale = cellWidth / advance;
                y = top + (cellHeight - (ascent + descent) * scale) / 2 + ascent * scale;
            }
            painter.translate(left + (cellWidth - advance * scale) / 2, y);
            painter.scale(scale, scale);
            for (const QGlyphRun &run : std::as_const(runs))
                painter.drawGlyphRun(QPointF(0, runY), run);
        }
    }

    // Bounding box of the ink, and whether it is a color glyph: a mask drawn with a white pen
    // has r == g == b == a in every pixel.
    int minX = canvas.width();
    int minY = canvas.height();
    int maxX = -1;
    int maxY = -1;
    bool color = false;
    for (int y = 0; y < canvas.height(); ++y) {
        const auto *line = reinterpret_cast<const QRgb *>(canvas.constScanLine(y));
        for (int x = 0; x < canvas.width(); ++x) {
            const QRgb pixel = line[x];
            if (pixel == 0)
                continue;
            minX = std::min(minX, x);
            maxX = std::max(maxX, x);
            minY = std::min(minY, y);
            maxY = std::max(maxY, y);
            const int alpha = qAlpha(pixel);
            if (!color
                && (std::abs(qRed(pixel) - alpha) > 2 || std::abs(qGreen(pixel) - alpha) > 2
                    || std::abs(qBlue(pixel) - alpha) > 2))
                color = true;
        }
    }

    result.valid = true;
    if (maxX >= 0) {
        const QRect ink(minX, minY, maxX - minX + 1, maxY - minY + 1);
        QRect placed;
        if (place(canvas, ink, color, &placed)) {
            result.rect = placed;
            result.offset = QPoint(minX - padX, minY - pad);
            result.color = color;
        } else {
            result.valid = false;
        }
    }
    const qint64 elapsed = timer.nsecsElapsed();
    rasterNs += elapsed;
    m_frameNs += elapsed;
    if (single == 0 || single > 0x7E)
        qCDebug(lcTerminal).nospace()
                << "glyph " << text << " (U+" << Qt::hex << uint(text.toUcs4().value(0))
                << Qt::dec << ") style " << style << " span " << span << ": "
                << (result.color ? "color" : "mask") << ", " << elapsed / 1000 << " us";
    return result;
}

// ---- GlyphMaterial ------------------------------------------------------------------------------

GlyphMaterial::GlyphMaterial()
{
    setFlag(Blending, true);
}

QSGMaterialType *GlyphMaterial::type() const
{
    static QSGMaterialType type;
    return &type;
}

QSGMaterialShader *GlyphMaterial::createShader(QSGRendererInterface::RenderMode) const
{
    return new GlyphShader;
}

int GlyphMaterial::compare(const QSGMaterial *other) const
{
    const auto *material = static_cast<const GlyphMaterial *>(other);
    const qint64 mine = texture ? texture->comparisonKey() : 0;
    const qint64 theirs = material->texture ? material->texture->comparisonKey() : 0;
    return mine == theirs ? 0 : (mine < theirs ? -1 : 1);
}

// ---- RootNode -----------------------------------------------------------------------------------

RootNode::RootNode(QQuickWindow *window)
{
    m_background = window->createRectangleNode();
    m_background->setColor(Qt::transparent);
    appendChildNode(m_background);
}

RootNode::~RootNode()
{
    // The child nodes use the materials below without owning them: delete them first.
    QSGNode *child = firstChild();
    while (child) {
        QSGNode *next = child->nextSibling();
        removeChildNode(child);
        delete child;
        child = next;
    }
    delete m_texture;
}

bool RootNode::needsFullFrame(int columns, int lines) const
{
    return !m_hasFrame || columns != m_fullColumns || lines != m_fullLines;
}

void RootNode::applyFrame(const TerminalFrameInfo &info, const ::rust::Vec<std::uint16_t> &rows,
                          const ::rust::Vec<TerminalCell> &cells,
                          const ::rust::Vec<std::uint32_t> &clusters, int requestedColumns,
                          int requestedLines, bool requestedFull)
{
    const int columns = info.columns;
    const int lines = info.lines;
    if (columns <= 0 || lines <= 0 || std::size_t(columns) * std::size_t(lines) > kMaxCells)
        return;
    if (info.full || columns != m_columns || lines != m_lines) {
        m_columns = columns;
        m_lines = lines;
        m_grid.assign(std::size_t(columns) * std::size_t(lines), GridCell());
        m_rowClusters.assign(std::size_t(lines), {});
        m_rowDirty.assign(std::size_t(lines), 1);
    }
    if (requestedFull) {
        m_fullColumns = requestedColumns;
        m_fullLines = requestedLines;
    }

    // Only whole rows are used.
    const std::size_t rowCount = std::min(rows.size(), cells.size() / std::size_t(columns));
    for (std::size_t i = 0; i < rowCount; ++i) {
        const int row = rows[i];
        if (row >= lines)
            continue;
        std::vector<char32_t> &rowClusters = m_rowClusters[std::size_t(row)];
        rowClusters.clear();
        const TerminalCell *source = cells.data() + i * std::size_t(columns);
        GridCell *target = m_grid.data() + std::size_t(row) * std::size_t(columns);
        for (int column = 0; column < columns; ++column) {
            const TerminalCell &in = source[column];
            GridCell &out = target[column];
            out.ch = sanitize(in.ch);
            out.fg = in.fg;
            out.bg = in.bg;
            out.underline = in.underline;
            out.flags = in.flags;
            out.cluster = 0;
            if (in.cluster == 0)
                continue;
            const std::size_t offset = std::size_t(in.cluster) - 1;
            if (offset >= clusters.size())
                continue;
            const std::uint32_t count = clusters[offset];
            if (count == 0 || count > kMaxClusterLength || offset + count >= clusters.size())
                continue;
            out.cluster = std::uint32_t(rowClusters.size()) + 1;
            rowClusters.push_back(char32_t(count));
            for (std::uint32_t k = 0; k < count; ++k)
                rowClusters.push_back(sanitize(clusters[offset + 1 + k]));
        }
        m_rowDirty[std::size_t(row)] = 1;
    }

    m_cursor.row = info.cursor_row;
    m_cursor.column = info.cursor_column;
    m_cursor.shape = static_cast<int>(info.cursor_shape);
    m_cursor.blinking = info.cursor_blinking;
    m_cursor.wide = info.cursor_wide;
    m_cursor.color = info.cursor_color;
    m_cursor.textColor = info.cursor_text_color;
    m_frameBackground = info.background;
    m_hasFrame = true;
}

void RootNode::ensureRowNodes(int lines)
{
    if (!m_backgroundLayer) {
        m_backgroundLayer = new QSGNode;
        m_glyphLayer = new QSGNode;
        m_overlay = new QSGGeometryNode;
        auto *geometry = new QSGGeometry(glyphAttributes(), 0, 0, QSGGeometry::UnsignedShortType);
        geometry->setDrawingMode(QSGGeometry::DrawTriangles);
        m_overlay->setGeometry(geometry);
        m_overlay->setFlag(QSGNode::OwnsGeometry);
        m_overlay->setMaterial(&m_glyphMaterial);
        appendChildNode(m_backgroundLayer);
        appendChildNode(m_glyphLayer);
        appendChildNode(m_overlay);
    }
    while (int(m_backgroundNodes.size()) < lines) {
        auto *background = new QSGGeometryNode;
        auto *backgroundGeometry = new QSGGeometry(QSGGeometry::defaultAttributes_ColoredPoint2D(),
                                                   0, 0, QSGGeometry::UnsignedShortType);
        backgroundGeometry->setDrawingMode(QSGGeometry::DrawTriangles);
        background->setGeometry(backgroundGeometry);
        background->setFlag(QSGNode::OwnsGeometry);
        background->setMaterial(&m_backgroundMaterial);
        m_backgroundLayer->appendChildNode(background);
        m_backgroundNodes.push_back(background);

        auto *glyphs = new QSGGeometryNode;
        auto *glyphGeometry =
                new QSGGeometry(glyphAttributes(), 0, 0, QSGGeometry::UnsignedShortType);
        glyphGeometry->setDrawingMode(QSGGeometry::DrawTriangles);
        glyphs->setGeometry(glyphGeometry);
        glyphs->setFlag(QSGNode::OwnsGeometry);
        glyphs->setMaterial(&m_glyphMaterial);
        m_glyphLayer->appendChildNode(glyphs);
        m_glyphNodes.push_back(glyphs);
    }
    while (int(m_backgroundNodes.size()) > lines) {
        QSGGeometryNode *background = m_backgroundNodes.back();
        m_backgroundNodes.pop_back();
        m_backgroundLayer->removeChildNode(background);
        delete background;
        QSGGeometryNode *glyphs = m_glyphNodes.back();
        m_glyphNodes.pop_back();
        m_glyphLayer->removeChildNode(glyphs);
        delete glyphs;
    }
}

void RootNode::sync(QQuickWindow *window, const RenderInput &input, RenderStats &stats)
{
    stats.software = input.software;
    m_background->setRect(QRectF(QPointF(0, 0), input.size));
    QColor background = QColor::fromRgba(m_frameBackground);
    background.setAlphaF(background.alphaF() * std::clamp<qreal>(input.backgroundOpacity, 0.0, 1.0));
    m_background->setColor(background);
    // The software backend skips custom geometry nodes (ADR 0013): keep only the background.
    if (input.software || !input.metrics.valid())
        return;

    if (input.metrics != m_builtMetrics) {
        m_builtMetrics = input.metrics;
        m_atlas.reset(input.metrics);
    }
    m_atlas.beginFrame();
    ensureRowNodes(m_lines);
    if (m_atlas.generation() != m_builtGeneration || input.padding != m_builtPadding) {
        std::fill(m_rowDirty.begin(), m_rowDirty.end(), 1);
        m_builtPadding = input.padding;
    }

    // Glyphs added while building can move the atlas (growth or a clear); the rows built before
    // that point then have stale texture coordinates and are built again. The second pass finds
    // every glyph cached, so it doesn't move the atlas unless the overlay adds one; if it still
    // does, `m_builtGeneration` stays behind and the next frame rebuilds everything.
    int generation = m_atlas.generation();
    for (int pass = 0; pass < 3; ++pass) {
        generation = m_atlas.generation();
        for (int row = 0; row < m_lines; ++row) {
            if (!m_rowDirty[std::size_t(row)])
                continue;
            const int deferredBefore = m_atlas.deferred();
            buildRow(row, input);
            // A row with glyphs left for later is built again next frame.
            m_rowDirty[std::size_t(row)] = m_atlas.deferred() > deferredBefore ? 1 : 0;
            ++stats.rowsBuilt;
        }
        buildOverlay(input);
        if (m_atlas.generation() == generation)
            break;
        std::fill(m_rowDirty.begin(), m_rowDirty.end(), 1);
    }
    m_builtGeneration = generation;

    uploadAtlas(window, stats);
    stats.glyphsRasterized += m_atlas.glyphsRasterized;
    stats.rasterNs += m_atlas.rasterNs;
    stats.atlasResets += m_atlas.resets;
    stats.atlasSize = m_atlas.image().width();
    m_atlas.glyphsRasterized = 0;
    m_atlas.rasterNs = 0;
    m_atlas.resets = 0;
}

GlyphAtlas::Glyph RootNode::glyphForCell(int row, const GridCell &cell, int span)
{
    const int style = ((cell.flags & flag(TerminalCellFlag::Bold)) ? 1 : 0)
            | ((cell.flags & flag(TerminalCellFlag::Italic)) ? 2 : 0);
    if (cell.cluster == 0)
        return m_atlas.glyph(cell.ch, style, span);
    const std::vector<char32_t> &clusters = m_rowClusters[std::size_t(row)];
    const std::size_t offset = cell.cluster - 1;
    if (offset >= clusters.size())
        return m_atlas.glyph(cell.ch, style, span);
    const std::size_t count = clusters[offset];
    std::u32string text(1, cell.ch);
    for (std::size_t k = 0; k < count && offset + 1 + k < clusters.size(); ++k)
        text.push_back(clusters[offset + 1 + k]);
    return m_atlas.cluster(QString::fromUcs4(text.data(), qsizetype(text.size())), style, span);
}

namespace {

// Characters that take part in programming ligatures (`->`, `!=`, `===`, `<=>`, `::`, ...): a row
// is only shaped where two of them are next to each other.
bool ligatureSymbol(char32_t ch)
{
    switch (ch) {
    case U'!': case U'#': case U'$': case U'%': case U'&': case U'*': case U'+': case U'-':
    case U'.': case U'/': case U':': case U';': case U'<': case U'=': case U'>': case U'?':
    case U'@': case U'\\': case U'^': case U'_': case U'|': case U'~': case U'[': case U']':
    case U'{': case U'}': case U'(': case U')':
        return true;
    default:
        return false;
    }
}

} // namespace

void RootNode::findLigatures(const GridCell *cells)
{
    m_scratchLigatures.assign(std::size_t(m_columns), 0);
    const auto style = [](const GridCell &cell) {
        return ((cell.flags & flag(TerminalCellFlag::Bold)) ? 1 : 0)
                | ((cell.flags & flag(TerminalCellFlag::Italic)) ? 2 : 0);
    };
    const auto plain = [](const GridCell &cell) {
        const std::uint16_t skip = flag(TerminalCellFlag::Wide) | flag(TerminalCellFlag::WideSpacer)
                | flag(TerminalCellFlag::Hidden);
        return cell.cluster == 0 && (cell.flags & skip) == 0 && cell.ch >= U' ' && cell.ch < 0x7f;
    };
    int column = 0;
    while (column < m_columns) {
        if (!plain(cells[column])) {
            ++column;
            continue;
        }
        // A run of plain ASCII in one style.
        const int start = column;
        const int runStyle = style(cells[column]);
        bool candidate = false;
        while (column < m_columns && plain(cells[column]) && style(cells[column]) == runStyle) {
            if (column > start && ligatureSymbol(cells[column].ch)
                && ligatureSymbol(cells[column - 1].ch))
                candidate = true;
            ++column;
        }
        if (!candidate)
            continue;
        QString text;
        text.reserve(column - start);
        for (int i = start; i < column; ++i)
            text.append(QChar(char16_t(cells[i].ch)));
        const QList<quint32> shaped = m_atlas.shapeRun(text, runStyle);
        for (qsizetype i = 0; i < shaped.size() && start + i < m_columns; ++i)
            m_scratchLigatures[std::size_t(start + i)] = shaped[i];
    }
}

void RootNode::appendGlyph(std::vector<Quad> &out, const GlyphAtlas::Glyph &glyph, int x, int y,
                           std::uint32_t color) const
{
    if (!glyph.valid || glyph.rect.isEmpty())
        return;
    const QRect &r = glyph.rect;
    const float x0 = float(x + glyph.offset.x());
    const float y0 = float(y + glyph.offset.y());
    out.push_back(Quad { x0, y0, x0 + float(r.width()), y0 + float(r.height()), float(r.x()),
                         float(r.y()), float(r.x() + r.width()), float(r.y() + r.height()),
                         color, glyph.color ? 1.0f : 0.0f });
}

void RootNode::appendSolid(std::vector<Quad> &out, int x, int y, int width, int height,
                           std::uint32_t color) const
{
    if (width <= 0 || height <= 0)
        return;
    const QPoint texel = m_atlas.solidTexel();
    // The middle of the 4 x 4 block: every sample is white, even with linear filtering.
    const float u = float(texel.x()) + 1.0f;
    const float v = float(texel.y()) + 1.0f;
    out.push_back(Quad { float(x), float(y), float(x + width), float(y + height), u, v, u, v,
                         color, 0.0f });
}

void RootNode::appendDecorations(std::vector<Quad> &out, const GridCell &cell, int x, int y,
                                 int width) const
{
    const std::uint16_t flags = cell.flags;
    const Metrics &m = m_builtMetrics;
    const int thickness = m.lineThickness;
    const int cellWidth = m.cellWidth;
    const int cells = std::max(1, width / std::max(1, cellWidth));
    const int top = y + m.underlineTop;
    const std::uint32_t color = cell.underline;

    if (flags & flag(TerminalCellFlag::Underline))
        appendSolid(out, x, top, width, thickness, color);
    if (flags & flag(TerminalCellFlag::DoubleUnderline)) {
        int first = m.underlineTop;
        int second = first + 2 * thickness;
        if (second + thickness > m.cellHeight) {
            second = m.cellHeight - thickness;
            first = std::max(0, second - 2 * thickness);
        }
        appendSolid(out, x, y + first, width, thickness, color);
        appendSolid(out, x, y + second, width, thickness, color);
    }
    if (flags & flag(TerminalCellFlag::CurlyUnderline)) {
        for (int k = 0; k < cells; ++k)
            appendGlyph(out, m_atlas.curly(), x + k * cellWidth, y, color);
    }
    if (flags & flag(TerminalCellFlag::DottedUnderline)) {
        const int dots = std::max(1, cellWidth / (2 * thickness));
        for (int k = 0; k < cells; ++k) {
            for (int i = 0; i < dots; ++i) {
                const int dx = int(std::lround((i + 0.5) * cellWidth / dots - thickness / 2.0));
                appendSolid(out, x + k * cellWidth + dx, top, thickness, thickness, color);
            }
        }
    }
    if (flags & flag(TerminalCellFlag::DashedUnderline)) {
        const int length = std::max(thickness, int(std::lround(cellWidth * 0.6)));
        for (int k = 0; k < cells; ++k)
            appendSolid(out, x + k * cellWidth + (cellWidth - length) / 2, top, length, thickness,
                        color);
    }
    if ((flags & flag(TerminalCellFlag::Link)) && !(flags & kAnyUnderline))
        appendSolid(out, x, top, width, thickness, color);
    if (flags & flag(TerminalCellFlag::Strikeout))
        appendSolid(out, x, y + m.strikeTop, width, thickness, cell.fg);
}

void RootNode::buildRow(int row, const RenderInput &input)
{
    const Metrics &m = m_builtMetrics;
    const int cellWidth = m.cellWidth;
    const int cellHeight = m.cellHeight;
    const int y = row * cellHeight;
    const GridCell *cells = m_grid.data() + std::size_t(row) * std::size_t(m_columns);
    const float dpr = float(m.dpr);
    const float padding = float(input.padding);

    // Backgrounds: runs of equal color; the default background is the rectangle below.
    m_scratchBackgrounds.clear();
    int runStart = -1;
    std::uint32_t runColor = 0;
    for (int column = 0; column <= m_columns; ++column) {
        const std::uint32_t color = column < m_columns ? cells[column].bg : 0;
        const bool draw = column < m_columns && color != m_frameBackground && (color >> 24) != 0;
        if (runStart >= 0 && (!draw || color != runColor)) {
            m_scratchBackgrounds.push_back(
                    Quad { float(runStart * cellWidth), float(y), float(column * cellWidth),
                           float(y + cellHeight), 0, 0, 0, 0, runColor, 0 });
            runStart = -1;
        }
        if (draw && runStart < 0) {
            runStart = column;
            runColor = color;
        }
    }
    QSGGeometryNode *backgroundNode = m_backgroundNodes[std::size_t(row)];
    QSGGeometry *backgroundGeometry = backgroundNode->geometry();
    const std::size_t backgrounds = std::min(m_scratchBackgrounds.size(), kMaxQuadsPerNode);
    backgroundGeometry->allocate(int(backgrounds * 4), int(backgrounds * 6));
    auto *vertices = backgroundGeometry->vertexDataAsColoredPoint2D();
    quint16 *indices = backgroundGeometry->indexDataAsUShort();
    for (std::size_t i = 0; i < backgrounds; ++i) {
        const Quad &quad = m_scratchBackgrounds[i];
        unsigned char c[4];
        premultiply(quad.color, c);
        const float x0 = padding + quad.x0 / dpr;
        const float x1 = padding + quad.x1 / dpr;
        const float y0 = padding + quad.y0 / dpr;
        const float y1 = padding + quad.y1 / dpr;
        vertices[4 * i + 0].set(x0, y0, c[0], c[1], c[2], c[3]);
        vertices[4 * i + 1].set(x1, y0, c[0], c[1], c[2], c[3]);
        vertices[4 * i + 2].set(x0, y1, c[0], c[1], c[2], c[3]);
        vertices[4 * i + 3].set(x1, y1, c[0], c[1], c[2], c[3]);
        const auto base = quint16(4 * i);
        const quint16 quadIndices[6] = { base, quint16(base + 1), quint16(base + 2),
                                         quint16(base + 1), quint16(base + 3), quint16(base + 2) };
        std::memcpy(indices + 6 * i, quadIndices, sizeof(quadIndices));
    }
    backgroundNode->markDirty(QSGNode::DirtyGeometry);

    // Glyphs first, then decorations on top of them.
    m_scratchGlyphs.clear();
    m_scratchDecorations.clear();
    const bool ligatures = m.font.ligatures;
    if (ligatures)
        findLigatures(cells);
    for (int column = 0; column < m_columns; ++column) {
        const GridCell &cell = cells[column];
        if (cell.flags & flag(TerminalCellFlag::WideSpacer))
            continue;
        if (cell.flags & flag(TerminalCellFlag::Hidden))
            continue;
        const bool wide = (cell.flags & flag(TerminalCellFlag::Wide)) != 0;
        const int span = wide && column + 1 < m_columns ? 2 : 1;
        const int x = column * cellWidth;
        const quint32 shaped = ligatures ? m_scratchLigatures[std::size_t(column)] : 0;
        if (shaped != 0) {
            const int style = ((cell.flags & flag(TerminalCellFlag::Bold)) ? 1 : 0)
                    | ((cell.flags & flag(TerminalCellFlag::Italic)) ? 2 : 0);
            appendGlyph(m_scratchGlyphs, m_atlas.glyphByIndex(shaped, style), x, y, cell.fg);
        } else if (cell.cluster != 0 || (cell.ch != U' ' && cell.ch != 0)) {
            appendGlyph(m_scratchGlyphs, glyphForCell(row, cell, span), x, y, cell.fg);
        }
        appendDecorations(m_scratchDecorations, cell, x, y, span * cellWidth);
    }
    m_scratchGlyphs.insert(m_scratchGlyphs.end(), m_scratchDecorations.begin(),
                           m_scratchDecorations.end());
    writeGlyphGeometry(m_glyphNodes[std::size_t(row)], m_scratchGlyphs, input);
}

void RootNode::buildOverlay(const RenderInput &input)
{
    m_scratchGlyphs.clear();
    const Metrics &m = m_builtMetrics;
    const CursorInfo &cursor = m_cursor;
    const bool inGrid = m_hasFrame && cursor.row >= 0 && cursor.row < m_lines
            && cursor.column >= 0 && cursor.column < m_columns;
    if (inGrid) {
        const int cellWidth = m.cellWidth;
        const int cellHeight = m.cellHeight;
        const int y = cursor.row * cellHeight;
        const std::size_t index =
                std::size_t(cursor.row) * std::size_t(m_columns) + std::size_t(cursor.column);
        const GridCell &under = m_grid[index];
        const int beam = std::max(1, int(std::lround(m.dpr * 1.5)));

        if (!input.preedit.isEmpty() && input.focused) {
            // Input method preedit: drawn over the grid from the cursor, underlined, with its
            // own caret.
            const std::uint32_t fg = (under.fg >> 24) != 0 ? under.fg : cursor.color;
            const std::uint32_t bg = (under.bg >> 24) != 0 ? under.bg : m_frameBackground;
            int column = cursor.column;
            int caret = -1;
            QTextBoundaryFinder finder(QTextBoundaryFinder::Grapheme, input.preedit);
            qsizetype start = 0;
            while (column < m_columns) {
                const qsizetype end = finder.toNextBoundary();
                if (end < 0 || end <= start)
                    break;
                const QString grapheme = input.preedit.mid(start, end - start);
                const int span = std::min(m_atlas.spanOf(grapheme), m_columns - column);
                if (caret < 0 && input.preeditCursor <= start)
                    caret = column;
                appendSolid(m_scratchGlyphs, column * cellWidth, y, span * cellWidth, cellHeight,
                            bg);
                appendGlyph(m_scratchGlyphs, m_atlas.cluster(grapheme, 0, span),
                            column * cellWidth, y, fg);
                column += span;
                start = end;
            }
            if (caret < 0)
                caret = column;
            appendSolid(m_scratchGlyphs, cursor.column * cellWidth, y + m.underlineTop,
                        (column - cursor.column) * cellWidth, m.lineThickness, fg);
            if (input.preeditCursor >= 0)
                appendSolid(m_scratchGlyphs, std::min(caret, m_columns) * cellWidth, y, beam,
                            cellHeight, cursor.color);
        } else if (cursor.shape != kCursorHidden && (!cursor.blinking || input.cursorBlinkOn)) {
            const int span = cursor.wide && cursor.column + 1 < m_columns ? 2 : 1;
            const int x = cursor.column * cellWidth;
            const int width = span * cellWidth;
            const int thickness = std::max(1, int(std::lround(m.dpr)));
            switch (static_cast<TerminalCursorShape>(cursor.shape)) {
            case TerminalCursorShape::Block:
                appendSolid(m_scratchGlyphs, x, y, width, cellHeight, cursor.color);
                if (!(under.flags & flag(TerminalCellFlag::Hidden))
                    && (under.cluster != 0 || (under.ch != U' ' && under.ch != 0)))
                    appendGlyph(m_scratchGlyphs, glyphForCell(cursor.row, under, span), x, y,
                                cursor.textColor);
                break;
            case TerminalCursorShape::HollowBlock:
                appendSolid(m_scratchGlyphs, x, y, width, thickness, cursor.color);
                appendSolid(m_scratchGlyphs, x, y + cellHeight - thickness, width, thickness,
                            cursor.color);
                appendSolid(m_scratchGlyphs, x, y + thickness, thickness,
                            cellHeight - 2 * thickness, cursor.color);
                appendSolid(m_scratchGlyphs, x + width - thickness, y + thickness, thickness,
                            cellHeight - 2 * thickness, cursor.color);
                break;
            case TerminalCursorShape::Beam:
                appendSolid(m_scratchGlyphs, x, y, beam, cellHeight, cursor.color);
                break;
            case TerminalCursorShape::Underline: {
                const int height = std::max(2 * m.lineThickness, beam);
                appendSolid(m_scratchGlyphs, x, y + cellHeight - height, width, height,
                            cursor.color);
                break;
            }
            default:
                break;
            }
        }
    }
    writeGlyphGeometry(m_overlay, m_scratchGlyphs, input);
}

void RootNode::writeGlyphGeometry(QSGGeometryNode *node, const std::vector<Quad> &quads,
                                  const RenderInput &input) const
{
    const Metrics &m = m_builtMetrics;
    const float dpr = float(m.dpr);
    const float padding = float(input.padding);
    const float atlasWidth = float(std::max(1, m_atlas.image().width()));
    const float atlasHeight = float(std::max(1, m_atlas.image().height()));
    const std::size_t count = std::min(quads.size(), kMaxQuadsPerNode);

    QSGGeometry *geometry = node->geometry();
    geometry->allocate(int(count * 4), int(count * 6));
    auto *vertices = static_cast<GlyphVertex *>(geometry->vertexData());
    quint16 *indices = geometry->indexDataAsUShort();
    for (std::size_t i = 0; i < count; ++i) {
        const Quad &quad = quads[i];
        unsigned char c[4];
        premultiply(quad.color, c);
        const float x0 = padding + quad.x0 / dpr;
        const float x1 = padding + quad.x1 / dpr;
        const float y0 = padding + quad.y0 / dpr;
        const float y1 = padding + quad.y1 / dpr;
        const float u0 = quad.u0 / atlasWidth;
        const float u1 = quad.u1 / atlasWidth;
        const float v0 = quad.v0 / atlasHeight;
        const float v1 = quad.v1 / atlasHeight;
        vertices[4 * i + 0] = GlyphVertex { x0, y0, u0, v0, c[0], c[1], c[2], c[3], quad.kind };
        vertices[4 * i + 1] = GlyphVertex { x1, y0, u1, v0, c[0], c[1], c[2], c[3], quad.kind };
        vertices[4 * i + 2] = GlyphVertex { x0, y1, u0, v1, c[0], c[1], c[2], c[3], quad.kind };
        vertices[4 * i + 3] = GlyphVertex { x1, y1, u1, v1, c[0], c[1], c[2], c[3], quad.kind };
        const auto base = quint16(4 * i);
        const quint16 quadIndices[6] = { base, quint16(base + 1), quint16(base + 2),
                                         quint16(base + 1), quint16(base + 3), quint16(base + 2) };
        std::memcpy(indices + 6 * i, quadIndices, sizeof(quadIndices));
    }
    node->markDirty(QSGNode::DirtyGeometry);
}

void RootNode::uploadAtlas(QQuickWindow *window, RenderStats &stats)
{
    if (m_texture && !m_atlas.isDirty())
        return;
    // A new texture for the whole atlas: fine once the visible glyphs are in (ADR 0013).
    QSGTexture *texture =
            window->createTextureFromImage(m_atlas.image(), QQuickWindow::TextureHasAlphaChannel);
    if (!texture)
        return;
    texture->setFiltering(QSGTexture::Linear);
    texture->setMipmapFiltering(QSGTexture::None);
    texture->setHorizontalWrapMode(QSGTexture::ClampToEdge);
    texture->setVerticalWrapMode(QSGTexture::ClampToEdge);
    delete m_texture;
    m_texture = texture;
    m_glyphMaterial.texture = texture;
    for (QSGGeometryNode *node : m_glyphNodes)
        node->markDirty(QSGNode::DirtyMaterial);
    if (m_overlay)
        m_overlay->markDirty(QSGNode::DirtyMaterial);
    m_atlas.markClean();
    ++stats.textureUploads;
}

} // namespace opensesh::terminal
