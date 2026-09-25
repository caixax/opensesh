#include "opensesh-app/terminal_item.h"

// The bridge declares a #[qobject], so its .cxxqt.h must be included (not the .cxx.h): it defines
// TerminalCell and TerminalFrameInfo.
#include "opensesh-app/src/bridge/terminal_view.cxxqt.h"

#include <algorithm>
#include <atomic>
#include <cmath>

#include <QtCore/QElapsedTimer>
#include <QtCore/QLoggingCategory>
#include <QtCore/QUrl>
#include <QtGui/QClipboard>
#include <QtGui/QCursor>
#include <QtGui/QDesktopServices>
#include <QtGui/QGuiApplication>
#include <QtGui/QInputMethod>
#include <QtGui/QInputMethodEvent>
#include <QtGui/QKeyEvent>
#include <QtGui/QStyleHints>
#include <QtQuick/QQuickWindow>
#include <QtQuick/QSGRendererInterface>

// Debug messages are off unless QT_LOGGING_RULES="opensesh.terminal.debug=true".
Q_LOGGING_CATEGORY(lcTerminal, "opensesh.terminal", QtInfoMsg)

namespace opensesh {

namespace {

constexpr qreal kDefaultPointSize = 11.0;
constexpr qreal kMinPointSize = 4.0;
constexpr qreal kMaxPointSize = 200.0;
constexpr int kMaxGridSide = 0xFFFF;

const char *graphicsApiName(QSGRendererInterface::GraphicsApi api)
{
    switch (api) {
    case QSGRendererInterface::Software:
        return "software";
    case QSGRendererInterface::OpenGL:
        return "opengl";
    case QSGRendererInterface::Direct3D11:
        return "d3d11";
    case QSGRendererInterface::Direct3D12:
        return "d3d12";
    case QSGRendererInterface::Vulkan:
        return "vulkan";
    case QSGRendererInterface::Metal:
        return "metal";
    case QSGRendererInterface::Null:
        return "null";
    default:
        return "other";
    }
}

} // namespace

struct TerminalItemBase::FfiBuffers
{
    TerminalFrameInfo info {};
    ::rust::Vec<std::uint16_t> rows;
    ::rust::Vec<TerminalCell> cells;
    ::rust::Vec<std::uint32_t> clusters;
};

TerminalItemBase::TerminalItemBase(QQuickItem *parent)
    : QQuickItem(parent)
    , m_fontFamily(QStringLiteral("JetBrains Mono"))
    , m_fontPointSize(kDefaultPointSize)
    , m_ffi(std::make_unique<FfiBuffers>())
{
    setFlag(ItemHasContents, true);
    setFlag(ItemAcceptsInputMethod, true);
    setAcceptedMouseButtons(Qt::AllButtons);
    setAcceptHoverEvents(true);
    setActiveFocusOnTab(true);
    setCursor(Qt::IBeamCursor);

    connect(&m_blinkTimer, &QTimer::timeout, this, &TerminalItemBase::onBlinkTimeout);
    connect(this, &QQuickItem::visibleChanged, this, &TerminalItemBase::updateBlinkTimer);
    if (QStyleHints *hints = QGuiApplication::styleHints())
        connect(hints, &QStyleHints::cursorFlashTimeChanged, this,
                &TerminalItemBase::updateBlinkTimer);

    m_stats = qEnvironmentVariableIntValue("OPENSESH_TERMINAL_STATS") > 0;
    if (m_stats)
        m_statsClock = std::make_unique<QElapsedTimer>();
}

TerminalItemBase::TerminalItemBase(QObject *parent)
    : TerminalItemBase(qobject_cast<QQuickItem *>(parent))
{
    if (parent && !parentItem())
        setParent(parent);
}

TerminalItemBase::~TerminalItemBase() = default;

// ---- Properties -------------------------------------------------------------------------------

void TerminalItemBase::setFontFamily(const QString &family)
{
    if (family == m_fontFamily)
        return;
    m_fontFamily = family;
    Q_EMIT fontFamilyChanged();
    updateMetrics();
}

void TerminalItemBase::setFontPointSize(qreal size)
{
    size = std::clamp(size, kMinPointSize, kMaxPointSize);
    if (qFuzzyCompare(size, m_fontPointSize))
        return;
    m_fontPointSize = size;
    Q_EMIT fontPointSizeChanged();
    updateMetrics();
}

void TerminalItemBase::setPadding(qreal padding)
{
    padding = std::max<qreal>(0.0, padding);
    if (qFuzzyCompare(padding + 1.0, m_padding + 1.0))
        return;
    m_padding = padding;
    Q_EMIT paddingChanged();
    updateGridSize();
    update();
}

void TerminalItemBase::setReduceMotion(bool reduce)
{
    if (reduce == m_reduceMotion)
        return;
    m_reduceMotion = reduce;
    Q_EMIT reduceMotionChanged();
    updateBlinkTimer();
}

qreal TerminalItemBase::cellWidth() const
{
    return m_metrics.valid() ? m_metrics.cellWidth / m_metrics.dpr : 0.0;
}

qreal TerminalItemBase::cellHeight() const
{
    return m_metrics.valid() ? m_metrics.cellHeight / m_metrics.dpr : 0.0;
}

void TerminalItemBase::updateMetrics()
{
    QQuickWindow *win = window();
    qreal dpr = win ? win->effectiveDevicePixelRatio() : 0.0;
    if (dpr <= 0.0)
        dpr = qGuiApp ? qGuiApp->devicePixelRatio() : 1.0;
    const terminal::Metrics metrics = terminal::computeMetrics(m_fontFamily, m_fontPointSize, dpr);
    if (metrics == m_metrics)
        return;
    const qreal oldWidth = cellWidth();
    const qreal oldHeight = cellHeight();
    m_metrics = metrics;
    qCDebug(lcTerminal).nospace() << "cell " << metrics.cellWidth << "x" << metrics.cellHeight
                                  << " device px at dpr " << metrics.dpr << " for '"
                                  << metrics.family << "' " << metrics.pointSize << " pt";
    if (oldWidth != cellWidth() || oldHeight != cellHeight())
        Q_EMIT cellSizeChanged();
    updateGridSize();
    update();
}

void TerminalItemBase::updateGridSize()
{
    int columns = 0;
    int lines = 0;
    if (m_metrics.valid()) {
        const qreal width = std::max<qreal>(0.0, this->width() - 2 * m_padding) * m_metrics.dpr;
        const qreal height = std::max<qreal>(0.0, this->height() - 2 * m_padding) * m_metrics.dpr;
        columns = std::min(kMaxGridSide, int(std::floor(width / m_metrics.cellWidth + 1e-6)));
        lines = std::min(kMaxGridSide, int(std::floor(height / m_metrics.cellHeight + 1e-6)));
    }
    const bool gridChanged = columns != m_columns || lines != m_lines;
    const bool cellChanged =
            m_metrics.cellWidth != m_sentCellWidth || m_metrics.cellHeight != m_sentCellHeight;
    if (!gridChanged && !cellChanged)
        return;
    m_columns = columns;
    m_lines = lines;
    m_sentCellWidth = m_metrics.cellWidth;
    m_sentCellHeight = m_metrics.cellHeight;
    if (gridChanged) {
        qCDebug(lcTerminal).nospace() << "grid " << columns << "x" << lines;
        Q_EMIT gridSizeChanged(columns, lines);
    }
    handleGridSize(columns, lines, m_metrics.cellWidth, m_metrics.cellHeight);
    update();
}

QPoint TerminalItemBase::cellAt(const QPointF &position) const
{
    const qreal width = cellWidth();
    const qreal height = cellHeight();
    if (width <= 0.0 || height <= 0.0 || m_columns <= 0 || m_lines <= 0)
        return QPoint(0, 0);
    const int column = int(std::floor((position.x() - m_padding) / width));
    const int line = int(std::floor((position.y() - m_padding) / height));
    return QPoint(std::clamp(column, 0, m_columns - 1), std::clamp(line, 0, m_lines - 1));
}

QRectF TerminalItemBase::cursorRectangle() const
{
    if (!m_imeCursorRect.isEmpty())
        return m_imeCursorRect;
    return QRectF(m_padding, m_padding, cellWidth(), cellHeight());
}

// ---- Clipboard --------------------------------------------------------------------------------

void TerminalItemBase::setClipboardText(const QString &text, bool primarySelection)
{
    QClipboard *clipboard = QGuiApplication::clipboard();
    if (!clipboard)
        return;
    if (!primarySelection)
        clipboard->setText(text, QClipboard::Clipboard);
    else if (clipboard->supportsSelection())
        clipboard->setText(text, QClipboard::Selection);
}

QString TerminalItemBase::clipboardText(bool primarySelection) const
{
    QClipboard *clipboard = QGuiApplication::clipboard();
    if (!clipboard)
        return QString();
    if (!primarySelection)
        return clipboard->text(QClipboard::Clipboard);
    return clipboard->supportsSelection() ? clipboard->text(QClipboard::Selection) : QString();
}

bool TerminalItemBase::supportsPrimarySelection() const
{
    QClipboard *clipboard = QGuiApplication::clipboard();
    return clipboard && clipboard->supportsSelection();
}

void TerminalItemBase::setLinkCursor(bool overLink)
{
    setCursor(overLink ? Qt::PointingHandCursor : Qt::IBeamCursor);
}

bool TerminalItemBase::openUrl(const QString &url) const
{
    const QUrl parsed(url, QUrl::StrictMode);
    if (!parsed.isValid())
        return false;
    return QDesktopServices::openUrl(parsed);
}

// ---- Scene graph ------------------------------------------------------------------------------

QSGNode *TerminalItemBase::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *)
{
    QQuickWindow *win = window();
    if (!win) {
        delete oldNode;
        return nullptr;
    }
    QElapsedTimer timer;
    if (m_stats)
        timer.start();

    auto *root = static_cast<terminal::RootNode *>(oldNode);
    if (!root)
        root = new terminal::RootNode(win);
    const QSGRendererInterface::GraphicsApi api = win->rendererInterface()->graphicsApi();
    const bool software = !QSGRendererInterface::isApiRhiBased(api);
    if (software) {
        static std::atomic<bool> logged { false };
        if (!logged.exchange(true))
            qCInfo(lcTerminal) << "the" << graphicsApiName(api)
                               << "scene graph backend draws only the terminal background "
                                  "(ADR 0013)";
    }

    // A snapshot of the terminal, from Rust.
    const int columns = m_columns;
    const int lines = m_lines;
    if (columns > 0 && lines > 0) {
        const bool full = root->needsFullFrame(columns, lines);
        FfiBuffers &ffi = *m_ffi;
        const TerminalFrameRequest request { std::uint16_t(columns), std::uint16_t(lines), full };
        if (fillFrame(request, ffi.info, ffi.rows, ffi.cells, ffi.clusters)) {
            root->applyFrame(ffi.info, ffi.rows, ffi.cells, ffi.clusters, columns, lines, full);
            m_statsRows += int(ffi.rows.size());
        }
    }

    // State the GUI thread needs between frames (the GUI thread is blocked right now).
    const terminal::CursorInfo &cursor = root->cursor();
    m_cursorBlinks = root->hasFrame() && cursor.blinking;
    QRectF cursorRect;
    if (root->hasFrame() && m_metrics.valid()) {
        const qreal width = cellWidth();
        const qreal height = cellHeight();
        cursorRect = QRectF(m_padding + cursor.column * width, m_padding + cursor.row * height,
                            width * (cursor.wide ? 2 : 1), height);
    }
    if (cursorRect != m_imeCursorRect) {
        m_imeCursorRect = cursorRect;
        QMetaObject::invokeMethod(
                this,
                [this] {
                    if (hasActiveFocus())
                        if (QInputMethod *inputMethod = QGuiApplication::inputMethod())
                            inputMethod->update(Qt::ImCursorRectangle);
                },
                Qt::QueuedConnection);
    }

    terminal::RenderInput input;
    input.metrics = m_metrics;
    input.size = QSizeF(width(), height());
    input.padding = m_padding;
    input.software = software;
    input.cursorBlinkOn = m_blinkOn;
    input.focused = hasActiveFocus();
    input.preedit = m_preedit;
    input.preeditCursor = m_preeditCursor;
    root->sync(win, input, m_renderStats);

    if (m_stats) {
        const qint64 elapsed = timer.nsecsElapsed();
        m_statsSyncNs += elapsed;
        m_statsSyncMaxNs = std::max(m_statsSyncMaxNs, elapsed);
        ++m_statsSyncs;
        if (!m_statsConnection) {
            // Emitted on the render thread with the threaded render loop, like this function.
            m_statsConnection = connect(win, &QQuickWindow::frameSwapped, this,
                                        &TerminalItemBase::onFrameSwapped, Qt::DirectConnection);
            m_statsClock->start();
            qCInfo(lcTerminal).nospace()
                    << "terminal stats on: backend " << graphicsApiName(api) << ", dpr "
                    << m_metrics.dpr << ", cell " << m_metrics.cellWidth << "x"
                    << m_metrics.cellHeight << " device px";
        }
    }
    return root;
}

void TerminalItemBase::onFrameSwapped()
{
    ++m_statsFrames;
    const qint64 elapsed = m_statsClock ? m_statsClock->elapsed() : 0;
    if (elapsed < 2000)
        return;
    const double seconds = elapsed / 1000.0;
    const terminal::RenderStats &render = m_renderStats;
    qCInfo(lcTerminal).nospace()
            << "terminal stats: " << m_columns << "x" << m_lines << " cells, "
            << m_statsFrames / seconds << " frames/s, " << m_statsSyncs / seconds
            << " syncs/s, sync avg "
            << (m_statsSyncs ? m_statsSyncNs / 1e6 / m_statsSyncs : 0.0) << " ms, max "
            << m_statsSyncMaxNs / 1e6 << " ms, rows from Rust " << m_statsRows << ", rows built "
            << render.rowsBuilt << ", glyphs rasterized " << render.glyphsRasterized << " ("
            << render.rasterNs / 1e6 << " ms), texture uploads " << render.textureUploads
            << ", atlas " << render.atlasSize << " px, atlas resets " << render.atlasResets;
    m_statsClock->restart();
    m_statsFrames = 0;
    m_statsSyncs = 0;
    m_statsSyncNs = 0;
    m_statsSyncMaxNs = 0;
    m_statsRows = 0;
    const int atlasSize = m_renderStats.atlasSize;
    m_renderStats = terminal::RenderStats();
    m_renderStats.atlasSize = atlasSize;
}

void TerminalItemBase::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry)
{
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    if (newGeometry.size() != oldGeometry.size()) {
        updateGridSize();
        update();
    }
}

void TerminalItemBase::itemChange(ItemChange change, const ItemChangeData &value)
{
    QQuickItem::itemChange(change, value);
    if (change == ItemSceneChange || change == ItemDevicePixelRatioHasChanged)
        updateMetrics();
    if (change == ItemSceneChange) {
        disconnect(m_statsConnection);
        m_statsConnection = QMetaObject::Connection();
        updateBlinkTimer();
    }
    // A hidden item gets no frames, so what changed meanwhile is drawn when it shows again.
    if (change == ItemVisibleHasChanged && value.boolValue)
        update();
}

// ---- Cursor blinking --------------------------------------------------------------------------

void TerminalItemBase::updateBlinkTimer()
{
    const QStyleHints *hints = QGuiApplication::styleHints();
    // The platform's caret flash time is a full on/off period; 0 or less means no blinking.
    const int period = hints ? hints->cursorFlashTime() : 1060;
    const bool run = !m_reduceMotion && period > 0 && hasActiveFocus() && isVisible() && window();
    if (run) {
        const int half = std::max(100, period / 2);
        if (!m_blinkTimer.isActive() || m_blinkTimer.interval() != half)
            m_blinkTimer.start(half);
    } else {
        m_blinkTimer.stop();
        if (!m_blinkOn) {
            m_blinkOn = true;
            update();
        }
    }
}

void TerminalItemBase::restartBlink()
{
    if (!m_blinkOn) {
        m_blinkOn = true;
        update();
    }
    if (m_blinkTimer.isActive())
        m_blinkTimer.start();
}

void TerminalItemBase::onBlinkTimeout()
{
    if (m_cursorBlinks) {
        m_blinkOn = !m_blinkOn;
        update();
    } else if (!m_blinkOn) {
        m_blinkOn = true;
        update();
    }
}

// ---- Input ------------------------------------------------------------------------------------

bool TerminalItemBase::event(QEvent *event)
{
    if (event->type() == QEvent::ShortcutOverride) {
        auto *key = static_cast<QKeyEvent *>(event);
        if (handleShortcutOverride(key->key(), int(key->modifiers().toInt()))) {
            event->accept();
            return true;
        }
    }
    return QQuickItem::event(event);
}

void TerminalItemBase::keyPressEvent(QKeyEvent *event)
{
    const Qt::KeyboardModifiers modifiers = event->modifiers();
    // Holding Ctrl over a link highlights it (Ctrl+click opens it).
    if (event->key() == Qt::Key_Control && !event->isAutoRepeat())
        resendHover(modifiers);
    const bool accepted =
            handleKey(event->key(), int(modifiers.toInt()), event->text(),
                      modifiers.testFlag(Qt::KeypadModifier), event->isAutoRepeat());
    if (accepted) {
        event->accept();
        restartBlink();
    } else {
        event->ignore();
    }
}

void TerminalItemBase::keyReleaseEvent(QKeyEvent *event)
{
    // Releases are not reported: the legacy xterm encoding has no release events. Releasing
    // Ctrl over a link removes its highlight.
    if (event->key() == Qt::Key_Control)
        resendHover(event->modifiers());
    event->ignore();
}

void TerminalItemBase::resendHover(Qt::KeyboardModifiers modifiers)
{
    if (!m_hovering)
        return;
    const QPoint cell = cellAt(m_hoverPosition);
    handleHover(m_hoverPosition.x(), m_hoverPosition.y(), cell.x(), cell.y(),
                int(modifiers.toInt()));
}

int TerminalItemBase::clickCountFor(const QMouseEvent *event)
{
    const QStyleHints *hints = QGuiApplication::styleHints();
    const qint64 now = qint64(event->timestamp());
    const int interval = hints ? hints->mouseDoubleClickInterval() : 400;
    const int distance = hints ? hints->mouseDoubleClickDistance() : 5;
    const bool again = event->button() == Qt::LeftButton && m_clickCount > 0
            && now - m_lastPressTime <= interval
            && (event->position() - m_lastPressPosition).manhattanLength() <= distance;
    m_clickCount = again ? m_clickCount % 3 + 1 : 1;
    m_lastPressTime = now;
    m_lastPressPosition = event->position();
    return m_clickCount;
}

void TerminalItemBase::sendMouse(int kind, const QMouseEvent *event, int clickCount)
{
    const QPoint cell = cellAt(event->position());
    const TerminalMouseEvent mouse {
        kind,
        kind == MouseMove ? int(Qt::NoButton) : int(event->button()),
        int(event->buttons().toInt()),
        int(event->modifiers().toInt()),
        event->position().x(),
        event->position().y(),
        cell.x(),
        cell.y(),
        clickCount,
    };
    handleMouse(mouse);
}

void TerminalItemBase::mousePressEvent(QMouseEvent *event)
{
    forceActiveFocus(Qt::MouseFocusReason);
    sendMouse(MousePress, event, clickCountFor(event));
    event->accept();
}

void TerminalItemBase::mouseMoveEvent(QMouseEvent *event)
{
    sendMouse(MouseMove, event, 0);
    event->accept();
}

void TerminalItemBase::mouseReleaseEvent(QMouseEvent *event)
{
    sendMouse(MouseRelease, event, 0);
    event->accept();
}

void TerminalItemBase::mouseDoubleClickEvent(QMouseEvent *event)
{
    // Qt 6 delivers the second press before this event; clicks are counted on presses.
    event->accept();
}

void TerminalItemBase::wheelEvent(QWheelEvent *event)
{
    const QPoint cell = cellAt(event->position());
    const TerminalWheelEvent wheel {
        event->position().x(),
        event->position().y(),
        cell.x(),
        cell.y(),
        double(event->angleDelta().x()),
        double(event->angleDelta().y()),
        double(event->pixelDelta().x()),
        double(event->pixelDelta().y()),
        int(event->modifiers().toInt()),
    };
    handleWheel(wheel);
    event->accept();
}

void TerminalItemBase::hoverMoveEvent(QHoverEvent *event)
{
    // Qt Quick delivers the last hover position again after frames, with the application's
    // modifiers, which lag behind a Ctrl press or release (Windows): only moves count here, and
    // modifier changes come from the key events (resendHover).
    if (m_hovering && event->position() == m_hoverPosition) {
        event->accept();
        return;
    }
    m_hovering = true;
    m_hoverPosition = event->position();
    const QPoint cell = cellAt(event->position());
    handleHover(event->position().x(), event->position().y(), cell.x(), cell.y(),
                int(event->modifiers().toInt()));
    event->accept();
}

void TerminalItemBase::hoverLeaveEvent(QHoverEvent *event)
{
    m_hovering = false;
    handleHover(-1.0, -1.0, -1, -1, int(event->modifiers().toInt()));
    event->accept();
}

void TerminalItemBase::focusInEvent(QFocusEvent *event)
{
    QQuickItem::focusInEvent(event);
    handleFocusChange(true);
    updateBlinkTimer();
    restartBlink();
    update();
}

void TerminalItemBase::focusOutEvent(QFocusEvent *event)
{
    QQuickItem::focusOutEvent(event);
    m_preedit.clear();
    handleFocusChange(false);
    updateBlinkTimer();
    update();
}

void TerminalItemBase::inputMethodEvent(QInputMethodEvent *event)
{
    if (!event->commitString().isEmpty())
        handleImeCommit(event->commitString());
    m_preedit = event->preeditString();
    m_preeditCursor = int(m_preedit.size());
    for (const QInputMethodEvent::Attribute &attribute : event->attributes()) {
        // A zero length hides the input method's caret.
        if (attribute.type == QInputMethodEvent::Cursor)
            m_preeditCursor = attribute.length == 0 ? -1 : attribute.start;
    }
    restartBlink();
    event->accept();
    update();
}

QVariant TerminalItemBase::inputMethodQuery(Qt::InputMethodQuery query) const
{
    switch (query) {
    case Qt::ImEnabled:
        return true;
    case Qt::ImCursorRectangle:
    case Qt::ImAnchorRectangle:
        return cursorRectangle();
    case Qt::ImHints:
        return int(Qt::ImhNoPredictiveText | Qt::ImhNoAutoUppercase | Qt::ImhMultiLine);
    case Qt::ImFont:
        return terminal::terminalFont(m_fontFamily, m_fontPointSize, 0);
    case Qt::ImSurroundingText:
    case Qt::ImCurrentSelection:
        return QString();
    case Qt::ImCursorPosition:
    case Qt::ImAnchorPosition:
        return 0;
    default:
        return QQuickItem::inputMethodQuery(query);
    }
}

} // namespace opensesh
