// TerminalItemBase: the Qt Quick item that draws a terminal grid through the scene graph with its
// own glyph atlas (ADR 0013), and turns Qt input events into calls the Rust side implements.
//
// The Rust `TerminalItem` (src/bridge/terminal_view.rs) derives from it and implements the pure
// virtual functions below; QML only ever creates `TerminalItem`. This class owns the Qt side:
// font metrics and the grid size, the cursor blink timer, input plumbing and the clipboard. The
// render state (atlas, geometry, texture) lives in scene graph nodes (terminal_render.h).
//
// Qt 6.8 caveat: QML can't use QQuickItem members that carry a revision on `TerminalItem`
// (for example `activeFocusOnTab`, QtQuick 2.1): Qt 6.8.2 fails with "... is not available in
// cc.caixa.opensesh 255.255", while 6.10.3 accepts them. Set such properties from C++ (the item
// is focusable with Tab by default) or on a wrapping item.
#pragma once

#include <cstdint>
#include <memory>

#include <QtCore/QString>
#include <QtCore/QTimer>
#include <QtQml/qqmlregistration.h>
#include <QtQuick/QQuickItem>

#include "rust/cxx.h"

#include "opensesh-app/terminal_render.h"

class QElapsedTimer;

namespace opensesh {

// Shared structs defined by the cxx bridge (src/bridge/terminal_view.rs).
struct TerminalCell;
struct TerminalFrameInfo;
struct TerminalFrameRequest;
struct TerminalMouseEvent;
struct TerminalWheelEvent;

class TerminalItemBase : public QQuickItem
{
    Q_OBJECT
    QML_ANONYMOUS

    // Font of the grid. Bold and italic use the same family's bold and italic faces.
    Q_PROPERTY(QString fontFamily READ fontFamily WRITE setFontFamily NOTIFY fontFamilyChanged)
    Q_PROPERTY(qreal fontPointSize READ fontPointSize WRITE setFontPointSize NOTIFY
                       fontPointSizeChanged)
    // Space between the item's edge and the grid, in logical pixels. The frame's default
    // background fills it too.
    Q_PROPERTY(qreal padding READ padding WRITE setPadding NOTIFY paddingChanged)
    // No cursor blinking (bind it to Theme.reduceMotion).
    Q_PROPERTY(bool reduceMotion READ reduceMotion WRITE setReduceMotion NOTIFY reduceMotionChanged)
    // Grid size that fits the item, from its size, the padding and the cell size.
    Q_PROPERTY(int columns READ columns NOTIFY gridSizeChanged)
    Q_PROPERTY(int lines READ lines NOTIFY gridSizeChanged)
    // Cell size in logical pixels (a whole number of device pixels).
    Q_PROPERTY(qreal cellWidth READ cellWidth NOTIFY cellSizeChanged)
    Q_PROPERTY(qreal cellHeight READ cellHeight NOTIFY cellSizeChanged)

public:
    // TerminalMouseEvent::kind.
    enum MouseKind : int { MousePress = 0, MouseRelease = 1, MouseMove = 2 };

    explicit TerminalItemBase(QQuickItem *parent = nullptr);
    // cxx-qt generates `TerminalItem(QObject *parent) : TerminalItemBase(parent)`.
    explicit TerminalItemBase(QObject *parent);
    ~TerminalItemBase() override;

    QString fontFamily() const { return m_fontFamily; }
    void setFontFamily(const QString &family);
    qreal fontPointSize() const { return m_fontPointSize; }
    void setFontPointSize(qreal size);
    qreal padding() const { return m_padding; }
    void setPadding(qreal padding);
    bool reduceMotion() const { return m_reduceMotion; }
    void setReduceMotion(bool reduce);
    int columns() const { return m_columns; }
    int lines() const { return m_lines; }
    qreal cellWidth() const;
    qreal cellHeight() const;

    // Schedules a new frame (QQuickItem::update() is a protected slot, so QML can't call it).
    Q_INVOKABLE void requestFrame() { update(); }

    // Clipboard helpers for the Rust side (cxx-qt-lib has no QClipboard). `primarySelection`
    // uses the X11/Wayland primary selection, and does nothing where it doesn't exist.
    void setClipboardText(const QString &text, bool primarySelection);
    QString clipboardText(bool primarySelection) const;
    bool supportsPrimarySelection() const;
    // Pointer shape over the grid: a pointing hand over a link the user can open, else a text
    // cursor.
    void setLinkCursor(bool overLink);
    // Opens a URL with the desktop's handler (QDesktopServices). The Rust side decides which
    // URLs may be opened. Returns whether a handler took it.
    bool openUrl(const QString &url) const;

Q_SIGNALS:
    void fontFamilyChanged();
    void fontPointSizeChanged();
    void paddingChanged();
    void reduceMotionChanged();
    void gridSizeChanged(int columns, int lines);
    void cellSizeChanged();

protected:
    // ---- Implemented in Rust -------------------------------------------------------------------

    // Called from updatePaintNode, on the render thread while the GUI thread is blocked: only
    // touch Rust state, never Qt objects. Fills `info`, `rows` (viewport row indices), `cells`
    // (rows.size() x info.columns, row after row) and `clusters` (combining characters: a cell's
    // `cluster` is 0, or 1 + the offset of an entry `[count, codepoint...]`), after clearing them.
    // `request.full` asks for every row (first frame, new grid size, scene graph rebuilt).
    // Returns false when nothing changed since the previous frame; the vectors are then ignored.
    virtual bool fillFrame(const TerminalFrameRequest &request, TerminalFrameInfo &info,
                           ::rust::Vec<std::uint16_t> &rows, ::rust::Vec<TerminalCell> &cells,
                           ::rust::Vec<std::uint32_t> &clusters) = 0;
    // A key press for the program. Returns whether it was consumed; unconsumed keys go on to the
    // parent items (e.g. Tab moves the focus).
    virtual bool handleKey(int key, int modifiers, const QString &text, bool keypad,
                           bool autoRepeat) = 0;
    // Whether the terminal takes a key that is also a window shortcut (ADR 0011).
    virtual bool handleShortcutOverride(int key, int modifiers) = 0;
    // Mouse press, release or move, with the cell under the pointer (clamped to the grid) and,
    // for presses, the click count (1 to 3).
    virtual void handleMouse(const TerminalMouseEvent &event) = 0;
    // Wheel or touchpad scroll: Qt's angle delta (1/8 degree, 120 per notch) and pixel delta.
    virtual void handleWheel(const TerminalWheelEvent &event) = 0;
    // Pointer moved without a button pressed (link hover), or the modifiers changed while the
    // pointer is over the item. `column` and `line` are -1 when the pointer left the item.
    virtual void handleHover(double x, double y, int column, int line, int modifiers) = 0;
    // The grid size or the cell size (device pixels) changed; 0 x 0 while there is no window.
    virtual void handleGridSize(int columns, int lines, int cellWidth, int cellHeight) = 0;
    virtual void handleFocusChange(bool focused) = 0;
    // Text committed by an input method (the preedit is drawn here, in C++).
    virtual void handleImeCommit(const QString &text) = 0;

    // ---- QQuickItem ----------------------------------------------------------------------------

    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *data) override;
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
    void hoverLeaveEvent(QHoverEvent *event) override;
    void focusInEvent(QFocusEvent *event) override;
    void focusOutEvent(QFocusEvent *event) override;
    void inputMethodEvent(QInputMethodEvent *event) override;
    QVariant inputMethodQuery(Qt::InputMethodQuery query) const override;

private:
    struct FfiBuffers;

    void updateMetrics();
    void updateGridSize();
    // Cell under a position in item coordinates, clamped to the grid.
    QPoint cellAt(const QPointF &position) const;
    QRectF cursorRectangle() const;
    void updateBlinkTimer();
    void restartBlink();
    void onBlinkTimeout();
    void onFrameSwapped();
    int clickCountFor(const QMouseEvent *event);
    void sendMouse(int kind, const QMouseEvent *event, int clickCount);
    // Re-sends the last hover position with new modifiers (Ctrl pressed or released over a link).
    void resendHover(Qt::KeyboardModifiers modifiers);

    QString m_fontFamily;
    qreal m_fontPointSize;
    qreal m_padding = 0.0;
    bool m_reduceMotion = false;
    int m_columns = 0;
    int m_lines = 0;
    // Cell size (device pixels) last passed to handleGridSize.
    int m_sentCellWidth = 0;
    int m_sentCellHeight = 0;
    terminal::Metrics m_metrics;

    // Last hover position, while the pointer is over the item.
    bool m_hovering = false;
    QPointF m_hoverPosition;

    // Cursor blinking (GUI thread). `m_blinkOn` is read during updatePaintNode.
    QTimer m_blinkTimer;
    bool m_blinkOn = true;
    // Written during updatePaintNode (GUI thread blocked), read on the GUI thread.
    bool m_cursorBlinks = false;
    QRectF m_imeCursorRect;

    // Input method preedit, drawn at the cursor.
    QString m_preedit;
    int m_preeditCursor = 0;

    // Click counting for double and triple clicks.
    qint64 m_lastPressTime = 0;
    QPointF m_lastPressPosition;
    int m_clickCount = 0;

    // Only touched in updatePaintNode.
    std::unique_ptr<FfiBuffers> m_ffi;

    // Frame statistics (OPENSESH_TERMINAL_STATS=1), render thread.
    bool m_stats = false;
    QMetaObject::Connection m_statsConnection;
    std::unique_ptr<QElapsedTimer> m_statsClock;
    qint64 m_statsSyncNs = 0;
    qint64 m_statsSyncMaxNs = 0;
    int m_statsSyncs = 0;
    int m_statsFrames = 0;
    int m_statsRows = 0;
    terminal::RenderStats m_renderStats;
};

} // namespace opensesh
