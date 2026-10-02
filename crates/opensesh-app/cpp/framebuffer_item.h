// FramebufferItemBase: a remote desktop (RDP; VNC in Sprint 14) as a Qt Quick item (ADR 0034).
//
// The desktop is kept in tiles of 256x256 pixels, each its own texture: only the tiles that the
// changed rectangles touch are uploaded again. It is drawn scaled to fit the item ("fit"), one
// desktop pixel per device pixel ("actual"), or at the item's own size, which the desktop then
// follows ("dynamic").
//
// The Rust `RdpItem` (src/bridge/rdp_view.rs) derives from it and implements the pure virtual
// functions: it hands over what changed in `fillFramebuffer` (render thread, GUI thread blocked)
// by calling `resizeFramebuffer` and `writePixels`, and gets the keyboard (with the native scan
// code), the mouse and the wheel in desktop pixels. While the item has the focus every key goes
// to the desktop, shortcuts included, except the escape combination (Ctrl+Alt+Home), which emits
// `escapeRequested()` so the app can take the keyboard back.
#pragma once

#include <cstdint>
#include <vector>

#include <QtCore/QString>
#include <QtGui/QColor>
#include <QtGui/QImage>
#include <QtQml/qqmlregistration.h>
#include <QtQuick/QQuickItem>

#include "rust/cxx.h"

namespace opensesh {

class FramebufferItemBase : public QQuickItem
{
    Q_OBJECT
    QML_ANONYMOUS

    // "fit", "actual" or "dynamic" (see above).
    Q_PROPERTY(QString scaleMode READ scaleMode WRITE setScaleMode NOTIFY scaleModeChanged)
    // Around the desktop when it doesn't fill the item.
    Q_PROPERTY(QColor backgroundColor READ backgroundColor WRITE setBackgroundColor NOTIFY
                       backgroundColorChanged)
    // The desktop's size in pixels (0 before the first image).
    Q_PROPERTY(int desktopWidth READ desktopWidth NOTIFY desktopSizeChanged)
    Q_PROPERTY(int desktopHeight READ desktopHeight NOTIFY desktopSizeChanged)
    // The item's size in device pixels: the desktop's size in "dynamic" mode.
    Q_PROPERTY(int wantedWidth READ wantedWidth NOTIFY wantedSizeChanged)
    Q_PROPERTY(int wantedHeight READ wantedHeight NOTIFY wantedSizeChanged)

public:
    // handlePointer kinds.
    enum PointerKind : int { PointerPress = 0, PointerRelease = 1, PointerMove = 2 };

    explicit FramebufferItemBase(QQuickItem *parent = nullptr);
    // cxx-qt generates `RdpItem(QObject *parent) : FramebufferItemBase(parent)`.
    explicit FramebufferItemBase(QObject *parent);
    ~FramebufferItemBase() override;

    QString scaleMode() const { return m_scaleMode; }
    void setScaleMode(const QString &mode);
    QColor backgroundColor() const { return m_background; }
    void setBackgroundColor(const QColor &color);
    int desktopWidth() const { return m_shownWidth; }
    int desktopHeight() const { return m_shownHeight; }
    int wantedWidth() const { return m_wantedWidth; }
    int wantedHeight() const { return m_wantedHeight; }

    // For fillFramebuffer only (render thread, GUI thread blocked): a new size (black), and
    // pixels of the frame (RGBA, `frameStride` bytes a row) for a rectangle of it.
    void resizeFramebuffer(int width, int height);
    void writePixels(int x, int y, int width, int height,
                     ::rust::Slice<const std::uint8_t> frame, int frameStride);

    // The pointer over the desktop: a picture (RGBA, not premultiplied) with its hot spot,
    // hidden, or the system's arrow.
    void setRemotePointer(::rust::Slice<const std::uint8_t> rgba, int width, int height,
                          int hotX, int hotY);
    void setPointerHidden(bool hidden);

    // Schedules a frame (QQuickItem::update() is a protected slot).
    Q_INVOKABLE void requestFrame() { update(); }
    // The clipboard, for the Rust side (cxx-qt-lib has no QClipboard).
    void setClipboardText(const QString &text);
    QString clipboardText() const;
    // A pixel of the desktop as "#rrggbb" (empty outside it): for the smoke test.
    Q_INVOKABLE QString pixelAt(int x, int y) const;

Q_SIGNALS:
    void scaleModeChanged();
    void backgroundColorChanged();
    void desktopSizeChanged();
    void wantedSizeChanged();
    // The escape combination: the app takes the keyboard back.
    void escapeRequested();
    // This computer's clipboard changed while the item has the focus, or it got the focus.
    void localClipboardChanged();

protected:
    // ---- Implemented in Rust -------------------------------------------------------------------

    // On the render thread while the GUI thread is blocked: hand over what changed.
    virtual void fillFramebuffer() = 0;
    // A key: its native scan code (0 when the platform has none), Qt key and text.
    virtual void handleKey(std::uint32_t nativeScanCode, int key, const QString &text,
                           bool pressed, bool autoRepeat) = 0;
    // A press, release (with Qt's button) or move, in desktop pixels.
    virtual void handlePointer(int kind, int button, int x, int y) = 0;
    // The wheel, in eighths of a degree (Qt's angle delta: 120 a notch).
    virtual void handleWheel(int angleX, int angleY) = 0;
    // The focus came or went.
    virtual void handleFocusChange(bool focused) = 0;
    // The item's size in device pixels changed (the desktop's in "dynamic" mode).
    virtual void handleWantedSize(int width, int height) = 0;

    // ---- QQuickItem ----------------------------------------------------------------------------

    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) override;
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;
    void itemChange(ItemChange change, const ItemChangeData &value) override;
    bool event(QEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
    void keyReleaseEvent(QKeyEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void mouseDoubleClickEvent(QMouseEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;
    void hoverMoveEvent(QHoverEvent *event) override;
    void focusInEvent(QFocusEvent *event) override;
    void focusOutEvent(QFocusEvent *event) override;

private:
    struct Tile
    {
        int x = 0;
        int y = 0;
        QImage image;
        bool dirty = true;
    };

    // Where the desktop is drawn, in item coordinates, and its scale (item units a pixel).
    QRectF desktopRect() const;
    qreal devicePixelRatio() const;
    bool toDesktop(const QPointF &position, int *x, int *y) const;
    void sendPointer(int kind, int button, const QPointF &position);
    void updateWantedSize();
    static bool isEscape(const QKeyEvent *event);

    QString m_scaleMode = QStringLiteral("fit");
    QColor m_background = Qt::black;
    // The frame as the render thread knows it.
    int m_width = 0;
    int m_height = 0;
    std::vector<Tile> m_tiles;
    int m_tileColumns = 0;
    bool m_rebuildNodes = false;
    // The size the GUI thread shows (updated from the render thread through a queued call).
    int m_shownWidth = 0;
    int m_shownHeight = 0;
    int m_wantedWidth = 0;
    int m_wantedHeight = 0;
};

} // namespace opensesh
