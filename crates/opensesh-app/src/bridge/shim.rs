//! Bindings to the hand-written C++ helpers in `cpp/app_shim.h`, for the few `QGuiApplication`
//! features that cxx-qt-lib doesn't expose yet.

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;

        include!("opensesh-app/app_shim.h");

        /// Sets the default window icon. Returns `false` if the image can't be loaded.
        #[namespace = "opensesh"]
        fn set_window_icon(path: &QString) -> bool;

        /// `QGuiApplication::platformName`, e.g. `wayland`, `xcb`, `windows` or `offscreen`.
        #[namespace = "opensesh"]
        fn platform_name() -> QString;

        /// Routes every Qt log message (level, category, text) to `sink`.
        #[namespace = "opensesh"]
        fn install_qt_message_handler(sink: fn(level: i32, category: &QString, message: &QString));
    }
}

use cxx_qt_lib::QString;

/// `QtMsgType` values (qlogging.h).
const QT_DEBUG: i32 = 0;
const QT_WARNING: i32 = 1;
const QT_CRITICAL: i32 = 2;
const QT_FATAL: i32 = 3;
const QT_INFO: i32 = 4;

/// Sends Qt and QML messages to `tracing` (target `qt`), so they end up in the log file too.
pub fn install_qt_message_handler() {
    ffi::install_qt_message_handler(forward_qt_message);
}

fn forward_qt_message(level: i32, category: &QString, message: &QString) {
    let category = category.to_string();
    match level {
        QT_DEBUG => tracing::debug!(target: "qt", %category, "{message}"),
        QT_INFO => tracing::info!(target: "qt", %category, "{message}"),
        QT_WARNING => tracing::warn!(target: "qt", %category, "{message}"),
        QT_CRITICAL => tracing::error!(target: "qt", %category, "{message}"),
        QT_FATAL => {
            tracing::error!(target: "qt", %category, "{message}");
            // Qt aborts as soon as this returns, before the async log file is written.
            crate::crash::record_qt_fatal(&category, &message.to_string());
        }
        _ => tracing::warn!(target: "qt", %category, level, "{message}"),
    }
}
