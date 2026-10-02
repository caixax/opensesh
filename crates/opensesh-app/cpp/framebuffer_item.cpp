#include "opensesh-app/framebuffer_item.h"

#include <algorithm>
#include <cstring>

#include <QtGui/QClipboard>
#include <QtGui/QCursor>
#include <QtGui/QGuiApplication>
#include <QtGui/QKeyEvent>
#include <QtGui/QPixmap>
#include <QtQuick/QQuickWindow>
#include <QtQuick/QSGSimpleRectNode>
#include <QtQuick/QSGSimpleTextureNode>
#include <QtQuick/QSGTexture>

namespace opensesh {

namespace {

constexpr int kTile = 256;

// A tile's node owns its texture.
class TileNode : public QSGSimpleTextureNode
{
public:
    ~TileNode() override { delete texture(); }

    void replaceTexture(QSGTexture *next)
    {
        QSGTexture *previous = texture();
        setTexture(next);
        delete previous;
    }
};

// Root: the background, then one child per tile.
class RootNode : public QSGSimpleRectNode
{
};

} // namespace

FramebufferItemBase::FramebufferItemBase(QQuickItem *parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents, true);
    setAcceptedMouseButtons(Qt::AllButtons);
    setAcceptHoverEvents(true);
    setActiveFocusOnTab(true);
    if (QClipboard *clipboard = QGuiApplication::clipboard()) {
        connect(clipboard, &QClipboard::dataChanged, this, [this] {
            if (hasActiveFocus())
                emit localClipboardChanged();
        });
    }
}

FramebufferItemBase::FramebufferItemBase(QObject *parent)
    : FramebufferItemBase(qobject_cast<QQuickItem *>(parent))
{
    if (parent && !parentItem())
        setParent(parent);
}

FramebufferItemBase::~FramebufferItemBase() = default;

// ---- Properties -------------------------------------------------------------------------------

void FramebufferItemBase::setScaleMode(const QString &mode)
{
    const QString next = mode == QLatin1String("actual") || mode == QLatin1String("dynamic")
            ? mode
            : QStringLiteral("fit");
    if (next == m_scaleMode)
        return;
    m_scaleMode = next;
    emit scaleModeChanged();
    updateWantedSize();
    update();
}

void FramebufferItemBase::setBackgroundColor(const QColor &color)
{
    if (color == m_background)
        return;
    m_background = color;
    emit backgroundColorChanged();
    update();
}

qreal FramebufferItemBase::devicePixelRatio() const
{
    const QQuickWindow *win = window();
    return win ? win->effectiveDevicePixelRatio() : 1.0;
}

QRectF FramebufferItemBase::desktopRect() const
{
    const int width = m_shownWidth;
    const int height = m_shownHeight;
    if (width <= 0 || height <= 0)
        return {};
    // One desktop pixel per device pixel, unless it is scaled to fit.
    qreal scale = 1.0 / devicePixelRatio();
    if (m_scaleMode == QLatin1String("fit")) {
        scale = std::min(this->width() / width, this->height() / height);
        if (scale <= 0)
            return {};
    }
    const qreal drawnWidth = width * scale;
    const qreal drawnHeight = height * scale;
    // Centered when smaller than the item; from the top left when bigger ("actual").
    const qreal x = std::max(0.0, (this->width() - drawnWidth) / 2.0);
    const qreal y = std::max(0.0, (this->height() - drawnHeight) / 2.0);
    return QRectF(x, y, drawnWidth, drawnHeight);
}

bool FramebufferItemBase::toDesktop(const QPointF &position, int *x, int *y) const
{
    const QRectF rect = desktopRect();
    if (rect.isEmpty())
        return false;
    const qreal fx = (position.x() - rect.x()) * m_shownWidth / rect.width();
    const qreal fy = (position.y() - rect.y()) * m_shownHeight / rect.height();
    *x = std::clamp(int(fx), 0, m_shownWidth - 1);
    *y = std::clamp(int(fy), 0, m_shownHeight - 1);
    return true;
}

void FramebufferItemBase::updateWantedSize()
{
    const qreal ratio = devicePixelRatio();
    const int width = std::max(0, int(this->width() * ratio));
    const int height = std::max(0, int(this->height() * ratio));
    if (width == m_wantedWidth && height == m_wantedHeight)
        return;
    m_wantedWidth = width;
    m_wantedHeight = height;
    emit wantedSizeChanged();
    handleWantedSize(width, height);
}

void FramebufferItemBase::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry)
{
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    updateWantedSize();
    update();
}

void FramebufferItemBase::itemChange(ItemChange change, const ItemChangeData &value)
{
    QQuickItem::itemChange(change, value);
    if (change == ItemDevicePixelRatioHasChanged || change == ItemSceneChange)
        updateWantedSize();
}

// ---- Frames -----------------------------------------------------------------------------------

void FramebufferItemBase::resizeFramebuffer(int width, int height)
{
    width = std::max(0, width);
    height = std::max(0, height);
    m_width = width;
    m_height = height;
    m_tiles.clear();
    m_tileColumns = (width + kTile - 1) / kTile;
    const int rows = (height + kTile - 1) / kTile;
    for (int row = 0; row < rows; ++row) {
        for (int column = 0; column < m_tileColumns; ++column) {
            Tile tile;
            tile.x = column * kTile;
            tile.y = row * kTile;
            // Opaque (the helper sends the fourth byte as 255): drawn without blending.
            tile.image = QImage(std::min(kTile, width - tile.x), std::min(kTile, height - tile.y),
                                QImage::Format_RGBX8888);
            tile.image.fill(Qt::black);
            m_tiles.push_back(std::move(tile));
        }
    }
    m_rebuildNodes = true;
    // The GUI thread's copy of the size, for input and the properties.
    QMetaObject::invokeMethod(
            this,
            [this, width, height] {
                if (width == m_shownWidth && height == m_shownHeight)
                    return;
                m_shownWidth = width;
                m_shownHeight = height;
                emit desktopSizeChanged();
                // Laid out at the new size on the next frame.
                update();
            },
            Qt::QueuedConnection);
}

void FramebufferItemBase::writePixels(int x, int y, int width, int height,
                                      ::rust::Slice<const std::uint8_t> frame, int frameStride)
{
    const int right = std::min(x + width, m_width);
    const int bottom = std::min(y + height, m_height);
    x = std::max(0, x);
    y = std::max(0, y);
    if (right <= x || bottom <= y || frameStride < m_width * 4
        || frame.size() < std::size_t(frameStride) * std::size_t(m_height))
        return;
    for (Tile &tile : m_tiles) {
        const int left = std::max(x, tile.x);
        const int top = std::max(y, tile.y);
        const int tileRight = std::min(right, tile.x + tile.image.width());
        const int tileBottom = std::min(bottom, tile.y + tile.image.height());
        if (tileRight <= left || tileBottom <= top)
            continue;
        const std::size_t bytes = std::size_t(tileRight - left) * 4;
        for (int row = top; row < tileBottom; ++row) {
            const std::uint8_t *from =
                    frame.data() + std::size_t(row) * std::size_t(frameStride) + std::size_t(left) * 4;
            uchar *to = tile.image.scanLine(row - tile.y) + (left - tile.x) * 4;
            std::memcpy(to, from, bytes);
        }
        tile.dirty = true;
    }
}

QSGNode *FramebufferItemBase::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *)
{
    QQuickWindow *win = window();
    if (!win) {
        delete oldNode;
        return nullptr;
    }
    auto *root = static_cast<RootNode *>(oldNode);
    if (!root)
        root = new RootNode;
    root->setRect(boundingRect());
    root->setColor(m_background);

    // What changed, from Rust.
    fillFramebuffer();

    if (m_rebuildNodes) {
        while (QSGNode *child = root->firstChild()) {
            root->removeChildNode(child);
            delete child;
        }
        // Each node gets its texture before it joins the tree: the software renderer reads it
        // as soon as the node is added.
        for (Tile &tile : m_tiles) {
            auto *node = new TileNode;
            node->replaceTexture(win->createTextureFromImage(tile.image));
            tile.dirty = false;
            root->appendChildNode(node);
        }
        m_rebuildNodes = false;
    }
    // Laid out with the GUI thread's size: the new one shows once the queued update arrived.
    const QRectF rect = (m_shownWidth == m_width && m_shownHeight == m_height) ? desktopRect()
                                                                            : QRectF();
    const qreal scale = rect.isEmpty() ? 0.0 : rect.width() / m_width;
    const bool pixelExact = qFuzzyCompare(scale * devicePixelRatio(), 1.0);
    QSGNode *child = root->firstChild();
    for (Tile &tile : m_tiles) {
        if (!child)
            break;
        auto *node = static_cast<TileNode *>(child);
        if (tile.dirty || !node->texture()) {
            node->replaceTexture(win->createTextureFromImage(tile.image));
            tile.dirty = false;
        }
        node->setFiltering(pixelExact ? QSGTexture::Nearest : QSGTexture::Linear);
        node->setRect(QRectF(rect.x() + tile.x * scale, rect.y() + tile.y * scale,
                             tile.image.width() * scale, tile.image.height() * scale));
        child = child->nextSibling();
    }
    return root;
}

QString FramebufferItemBase::pixelAt(int x, int y) const
{
    for (const Tile &tile : m_tiles) {
        if (x >= tile.x && y >= tile.y && x < tile.x + tile.image.width()
            && y < tile.y + tile.image.height())
            return QColor::fromRgba(tile.image.pixel(x - tile.x, y - tile.y)).name(QColor::HexRgb);
    }
    return {};
}

// ---- The pointer and the clipboard ------------------------------------------------------------

void FramebufferItemBase::setRemotePointer(::rust::Slice<const std::uint8_t> rgba, int width,
                                           int height, int hotX, int hotY)
{
    if (width <= 0 || height <= 0 || rgba.size() < std::size_t(width) * std::size_t(height) * 4) {
        setCursor(Qt::ArrowCursor);
        return;
    }
    QImage image(width, height, QImage::Format_RGBA8888);
    for (int row = 0; row < height; ++row)
        std::memcpy(image.scanLine(row), rgba.data() + std::size_t(row) * std::size_t(width) * 4,
                    std::size_t(width) * 4);
    setCursor(QCursor(QPixmap::fromImage(image), hotX, hotY));
}

void FramebufferItemBase::setPointerHidden(bool hidden)
{
    setCursor(hidden ? Qt::BlankCursor : Qt::ArrowCursor);
}

void FramebufferItemBase::setClipboardText(const QString &text)
{
    if (QClipboard *clipboard = QGuiApplication::clipboard())
        clipboard->setText(text);
}

QString FramebufferItemBase::clipboardText() const
{
    const QClipboard *clipboard = QGuiApplication::clipboard();
    return clipboard ? clipboard->text() : QString();
}

// ---- Input ------------------------------------------------------------------------------------

bool FramebufferItemBase::isEscape(const QKeyEvent *event)
{
    const Qt::KeyboardModifiers modifiers = event->modifiers();
    return event->key() == Qt::Key_Home && modifiers.testFlag(Qt::ControlModifier)
            && modifiers.testFlag(Qt::AltModifier);
}

bool FramebufferItemBase::event(QEvent *event)
{
    // While focused, every key goes to the desktop, shortcuts included.
    if (event->type() == QEvent::ShortcutOverride) {
        auto *key = static_cast<QKeyEvent *>(event);
        if (!isEscape(key)) {
            event->accept();
            return true;
        }
    }
    return QQuickItem::event(event);
}

void FramebufferItemBase::keyPressEvent(QKeyEvent *event)
{
    if (isEscape(event)) {
        emit escapeRequested();
        event->accept();
        return;
    }
    handleKey(event->nativeScanCode(), event->key(), event->text(), true, event->isAutoRepeat());
    event->accept();
}

void FramebufferItemBase::keyReleaseEvent(QKeyEvent *event)
{
    handleKey(event->nativeScanCode(), event->key(), event->text(), false, event->isAutoRepeat());
    event->accept();
}

void FramebufferItemBase::sendPointer(int kind, int button, const QPointF &position)
{
    int x = 0;
    int y = 0;
    if (toDesktop(position, &x, &y))
        handlePointer(kind, button, x, y);
}

void FramebufferItemBase::mousePressEvent(QMouseEvent *event)
{
    forceActiveFocus(Qt::MouseFocusReason);
    sendPointer(PointerPress, int(event->button()), event->position());
    event->accept();
}

void FramebufferItemBase::mouseMoveEvent(QMouseEvent *event)
{
    sendPointer(PointerMove, int(Qt::NoButton), event->position());
    event->accept();
}

void FramebufferItemBase::mouseReleaseEvent(QMouseEvent *event)
{
    sendPointer(PointerRelease, int(event->button()), event->position());
    event->accept();
}

void FramebufferItemBase::mouseDoubleClickEvent(QMouseEvent *event)
{
    // Qt 6 delivers the second press before this event: the desktop counts its own clicks.
    event->accept();
}

void FramebufferItemBase::wheelEvent(QWheelEvent *event)
{
    handleWheel(event->angleDelta().x(), event->angleDelta().y());
    event->accept();
}

void FramebufferItemBase::hoverMoveEvent(QHoverEvent *event)
{
    sendPointer(PointerMove, int(Qt::NoButton), event->position());
    event->accept();
}

void FramebufferItemBase::focusInEvent(QFocusEvent *event)
{
    QQuickItem::focusInEvent(event);
    handleFocusChange(true);
    emit localClipboardChanged();
}

void FramebufferItemBase::focusOutEvent(QFocusEvent *event)
{
    QQuickItem::focusOutEvent(event);
    handleFocusChange(false);
}

} // namespace opensesh
