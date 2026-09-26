// Ligature spike (Sprint 3, ADR 0015): how programming fonts form ligatures, and what shaping a
// terminal row costs.
//
// For each sample it shapes the text with QTextLayout (HarfBuzz, the font's default features)
// and compares every shaped glyph with the glyph the character gets on its own (the cmap
// lookup the renderer uses today). It prints the glyph count against the character count, the
// shaped advances against the cell width, and which characters get another glyph. Then it
// times shaping 10,000 code-like rows, cold and through a cache.
//
// Usage: ligatures <font file> [more font files...]
#include <QtCore/QElapsedTimer>
#include <QtCore/QHash>
#include <QtCore/QStringList>
#include <QtGui/QFontDatabase>
#include <QtGui/QFontMetricsF>
#include <QtGui/QGlyphRun>
#include <QtGui/QGuiApplication>
#include <QtGui/QRawFont>
#include <QtGui/QTextLayout>

#include <cmath>
#include <cstdio>

namespace {

struct Shaped
{
    QList<quint32> glyphs;
    QList<qsizetype> indexes;
    QList<QPointF> positions;
    bool singleRun = true;
};

Shaped shape(const QFont &font, const QString &text)
{
    Shaped out;
    QTextLayout layout(text, font);
    QTextOption option;
    option.setWrapMode(QTextOption::NoWrap);
    layout.setTextOption(option);
    layout.beginLayout();
    QTextLine line = layout.createLine();
    layout.endLayout();
    if (!line.isValid())
        return out;
    const QList<QGlyphRun> runs = layout.glyphRuns(0, text.size(),
                                                   QTextLayout::RetrieveGlyphIndexes
                                                           | QTextLayout::RetrieveGlyphPositions
                                                           | QTextLayout::RetrieveStringIndexes);
    out.singleRun = runs.size() == 1;
    for (const QGlyphRun &run : runs) {
        out.glyphs += run.glyphIndexes();
        out.indexes += run.stringIndexes();
        out.positions += run.positions();
    }
    return out;
}

} // namespace

int main(int argc, char **argv)
{
    qputenv("QT_QPA_PLATFORM", "offscreen");
    QGuiApplication app(argc, argv);
    const QStringList args = app.arguments().mid(1);
    if (args.isEmpty()) {
        std::fprintf(stderr, "usage: ligatures <font file>...\n");
        return 2;
    }
    const QStringList samples = {
        QStringLiteral("->"),       QStringLiteral("=>"),     QStringLiteral("!="),
        QStringLiteral("==="),      QStringLiteral("<=>"),    QStringLiteral("|>"),
        QStringLiteral("::"),       QStringLiteral("<!--"),   QStringLiteral("www"),
        QStringLiteral("0xFF"),     QStringLiteral("a -> b"), QStringLiteral("fi fl"),
        QStringLiteral("fn main() -> Result<(), Error> {"),
        QStringLiteral("if a != b && c >= d || e == f {"),
    };

    for (const QString &file : args) {
        const int id = QFontDatabase::addApplicationFont(file);
        if (id < 0) {
            std::printf("could not load %s\n", qPrintable(file));
            continue;
        }
        const QString family = QFontDatabase::applicationFontFamilies(id).value(0);
        QFont font(family);
        font.setPointSizeF(11.0);
        font.setKerning(false);
        const QRawFont raw = QRawFont::fromFont(font);
        const qreal cell = QFontMetricsF(font).horizontalAdvance(QLatin1Char('M'));
        std::printf("\n== %s (cell %.2f px)\n", qPrintable(family), cell);

        for (const QString &text : samples) {
            const Shaped shaped = shape(font, text);
            QStringList changed;
            bool oneToOne = shaped.glyphs.size() == text.size();
            for (qsizetype i = 0; i < shaped.glyphs.size() && i < shaped.indexes.size(); ++i) {
                const qsizetype at = shaped.indexes[i];
                if (at != i)
                    oneToOne = false;
                const QList<quint32> own = raw.glyphIndexesForString(text.mid(at, 1));
                if (!own.isEmpty() && own.first() != shaped.glyphs[i])
                    changed << QStringLiteral("%1").arg(text.at(at));
            }
            // Do the shaped glyphs keep the grid? (x of glyph i == i cells)
            bool onGrid = true;
            for (qsizetype i = 0; i < shaped.positions.size(); ++i) {
                if (std::abs(shaped.positions[i].x() - i * cell) > 0.5)
                    onGrid = false;
            }
            std::printf("  %-36s chars %2lld glyphs %2lld  1:1 %-3s grid %-3s changed [%s]\n",
                        qPrintable(QLatin1Char('"') + text + QLatin1Char('"')),
                        static_cast<long long>(text.size()),
                        static_cast<long long>(shaped.glyphs.size()), oneToOne ? "yes" : "NO",
                        onGrid ? "yes" : "NO", qPrintable(changed.join(QLatin1Char(' '))));
        }

        // Cost: 10,000 rows of 120 columns of code-like text, cold and through a cache.
        QStringList rows;
        for (int i = 0; i < 10000; ++i) {
            QString row = QStringLiteral("let value_%1 = compute(x -> y, a != b && c >= %2) => ok;")
                                  .arg(i)
                                  .arg(i * 7 % 997);
            while (row.size() < 120)
                row += QStringLiteral(" /* padding */");
            rows << row.left(120);
        }
        QElapsedTimer timer;
        timer.start();
        qsizetype glyphs = 0;
        for (const QString &row : std::as_const(rows))
            glyphs += shape(font, row).glyphs.size();
        const double coldMs = timer.nsecsElapsed() / 1e6;
        QHash<QString, Shaped> cache;
        timer.restart();
        for (int pass = 0; pass < 2; ++pass) {
            for (const QString &row : std::as_const(rows)) {
                auto found = cache.constFind(row);
                if (found == cache.constEnd())
                    cache.insert(row, shape(font, row));
            }
        }
        const double cachedMs = timer.nsecsElapsed() / 1e6;
        std::printf("  shaping 10000 rows x 120 columns: %.1f ms (%.1f us per row), %lld glyphs\n",
                    coldMs, coldMs * 1000.0 / rows.size(), static_cast<long long>(glyphs));
        std::printf("  the same twice through a cache: %.1f ms\n", cachedMs);
    }
    return 0;
}
