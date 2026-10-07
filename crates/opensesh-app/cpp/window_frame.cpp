#include "opensesh-app/window_frame.h"

#include <QtGui/QGuiApplication>

#ifdef Q_OS_WIN
#include <QtCore/QAbstractNativeEventFilter>
#include <QtCore/QByteArray>
#include <QtCore/QTimer>
#include <QtGui/QWindow>

#include <qt_windows.h>
#include <dwmapi.h>

#pragma comment(lib, "dwmapi.lib")

namespace {

// Qt gives a window with FramelessWindowHint the style WS_POPUP, without a sizing frame or a
// caption. Windows then treats it as a popup: no Aero Snap when it is dragged to an edge or the
// top of the screen, no Win+arrows, no minimize or maximize animation, and DWM draws no shadow.
// Adding both styles back makes it an ordinary window to the system. Nothing of the frame shows:
// Qt answers WM_NCCALCSIZE for frameless windows with a client area as large as the window.
constexpr LONG_PTR frame_styles = WS_THICKFRAME | WS_CAPTION;

// The top-level frameless window with this handle, unless it is full screen (Qt gives full
// screen windows a style of their own, which must stay as it is).
QWindow *frameless_window(HWND hwnd)
{
    const auto windows = QGuiApplication::topLevelWindows();
    for (QWindow *window : windows) {
        if (window->handle() == nullptr || reinterpret_cast<HWND>(window->winId()) != hwnd)
            continue;
        const bool ours = window->type() == Qt::Window
                && window->flags().testFlag(Qt::FramelessWindowHint)
                && !window->windowStates().testFlag(Qt::WindowFullScreen);
        return ours ? window : nullptr;
    }
    return nullptr;
}

// DWM draws the shadow (and on Windows 11 the rounded corners and the border) of a window whose
// frame was removed only when the frame reaches into the client area. One pixel is enough, and
// it is under the window's own opaque content.
void extend_frame(HWND hwnd)
{
    const MARGINS margins{1, 1, 1, 1};
    DwmExtendFrameIntoClientArea(hwnd, &margins);
}

void adopt(HWND hwnd)
{
    const LONG_PTR style = GetWindowLongPtrW(hwnd, GWL_STYLE);
    if ((style & frame_styles) != frame_styles) {
        SetWindowLongPtrW(hwnd, GWL_STYLE, style | frame_styles);
        SetWindowPos(hwnd, nullptr, 0, 0, 0, 0,
                     SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER
                             | SWP_NOACTIVATE);
    }
    extend_frame(hwnd);
}

class WindowFrameFilter : public QAbstractNativeEventFilter
{
public:
    bool nativeEventFilter(const QByteArray &eventType, void *message, qintptr *result) override
    {
        if (eventType != "windows_generic_MSG")
            return false;
        auto *msg = static_cast<MSG *>(message);
        switch (msg->message) {
        case WM_SHOWWINDOW:
            // Qt created the window with its own style: add the frame before it first shows.
            if (msg->wParam && frameless_window(msg->hwnd))
                adopt(msg->hwnd);
            return false;
        case WM_STYLECHANGING:
            // Qt sets the style again when the window's flags or full screen state change.
            if (msg->wParam == static_cast<WPARAM>(GWL_STYLE) && frameless_window(msg->hwnd)) {
                reinterpret_cast<STYLESTRUCT *>(msg->lParam)->styleNew |= frame_styles;
                // Qt resets the frame's extension right after it changes the style.
                const HWND hwnd = msg->hwnd;
                QTimer::singleShot(0, qGuiApp, [hwnd] {
                    if (IsWindow(hwnd))
                        extend_frame(hwnd);
                });
            }
            return false;
        case WM_NCCALCSIZE:
            // Windows makes a maximized window larger than its monitor by its sizing frame, all
            // around: the client area is the monitor's work area, or the title bar and the edges
            // would be off the screen.
            if (msg->wParam && IsZoomed(msg->hwnd) && frameless_window(msg->hwnd)) {
                MONITORINFO info{};
                info.cbSize = sizeof(info);
                if (GetMonitorInfoW(MonitorFromWindow(msg->hwnd, MONITOR_DEFAULTTONEAREST), &info)) {
                    reinterpret_cast<NCCALCSIZE_PARAMS *>(msg->lParam)->rgrc[0] = info.rcWork;
                    *result = 0;
                    return true;
                }
            }
            return false;
        default:
            return false;
        }
    }
};

} // namespace
#endif

namespace opensesh {

void install_window_frame_filter()
{
#ifdef Q_OS_WIN
    if (qGuiApp && QGuiApplication::platformName() == QLatin1String("windows"))
        qGuiApp->installNativeEventFilter(new WindowFrameFilter);
#endif
}

} // namespace opensesh
