//! Bindings to the hand-written C++ helpers in `cpp/app_shim.h`, for the Qt features that
//! cxx-qt-lib doesn't expose yet.

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        /// Qt string list type from cxx-qt-lib.
        type QStringList = cxx_qt_lib::QStringList;

        include!("cxx-qt-lib/qqmlapplicationengine.h");
        /// QML engine type from cxx-qt-lib.
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;

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

        /// Registers the `image://icon/...` provider on the engine.
        #[namespace = "opensesh"]
        fn install_icon_provider(engine: Pin<&mut QQmlApplicationEngine>);

        /// Registers the bundled fonts; returns how many files were loaded.
        #[namespace = "opensesh"]
        fn register_bundled_fonts() -> i32;

        /// Sets the application default font family.
        #[namespace = "opensesh"]
        fn set_application_font_family(family: &QString);

        /// Stops Qt Quick from writing its shader pipeline cache to the per-user cache folder.
        #[namespace = "opensesh"]
        fn disable_shader_disk_cache();

        /// Installed font families, optionally only fixed-pitch ones.
        #[namespace = "opensesh"]
        fn font_families(monospace_only: bool) -> QStringList;

        /// Asks Qt Quick windows for an alpha channel (before they are created).
        #[namespace = "opensesh"]
        fn enable_window_alpha();

        /// The system's alert sound; returns whether one was requested (not on Wayland).
        #[namespace = "opensesh"]
        fn platform_beep() -> bool;

        /// The keyboard modifiers held right now (`Qt::KeyboardModifiers` bits).
        #[namespace = "opensesh"]
        fn keyboard_modifiers() -> i32;

        /// Puts text on the clipboard.
        #[namespace = "opensesh"]
        fn clipboard_set_text(text: &QString);

        /// Portable text of a key combination (e.g. `Ctrl+Shift+P`).
        #[namespace = "opensesh"]
        fn key_sequence_text(key: i32, modifiers: i32) -> QString;

        /// Engine to retranslate when the language changes.
        #[namespace = "opensesh"]
        fn set_translation_engine(engine: Pin<&mut QQmlApplicationEngine>);

        /// Codes of the bundled translations.
        #[namespace = "opensesh"]
        fn available_translations() -> QStringList;

        /// Installs a translation and retranslates the UI.
        #[namespace = "opensesh"]
        fn apply_translation(code: &QString) -> bool;

        /// Native name of a language code.
        #[namespace = "opensesh"]
        fn language_native_name(code: &QString) -> QString;
    }
}

use std::sync::atomic::{AtomicUsize, Ordering};

use cxx_qt_lib::QString;

/// `QtMsgType` values (qlogging.h).
const QT_DEBUG: i32 = 0;
const QT_WARNING: i32 = 1;
const QT_CRITICAL: i32 = 2;
const QT_FATAL: i32 = 3;
const QT_INFO: i32 = 4;

/// Warnings (and worse) about our own QML seen so far; the smoke test fails if any.
static QML_WARNINGS: AtomicUsize = AtomicUsize::new(0);

/// Sends Qt and QML messages to `tracing` (target `qt`), so they end up in the log file too.
pub fn install_qt_message_handler() {
    ffi::install_qt_message_handler(forward_qt_message);
}

/// Number of warnings about OpenSesh's own QML or icons since startup.
pub fn qml_warning_count() -> usize {
    QML_WARNINGS.load(Ordering::SeqCst)
}

/// Qt Quick warnings that our QML causes but that Qt logs without a QML location, in the
/// `default` category (layout and polish loops, for example).
const QT_QUICK_PREFIXES: &[&str] = &[
    "Qt Quick Layouts:",
    "QQuickItem",
    "QQuickWindow",
    "possible QQuickItem::polish() loop",
];

/// Whether a Qt message is about our own QML module or assets (not environment noise such as
/// missing system fonts on the offscreen platform).
fn is_about_our_ui(category: &str, message: &str) -> bool {
    category == "qml"
        || category == "js"
        || category.starts_with("qt.qml")
        || category.starts_with("qt.quick")
        || message.contains("qrc:/qt/qml/cc/caixa/opensesh/")
        || message.starts_with("OsIcon:")
        || QT_QUICK_PREFIXES
            .iter()
            .any(|prefix| message.starts_with(prefix))
}

fn forward_qt_message(level: i32, category: &QString, message: &QString) {
    let category = category.to_string();
    if level != QT_DEBUG && level != QT_INFO && is_about_our_ui(&category, &message.to_string()) {
        QML_WARNINGS.fetch_add(1, Ordering::SeqCst);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_ui_messages_count_as_qml_warnings() {
        assert!(is_about_our_ui("qml", "anything"));
        assert!(is_about_our_ui(
            "default",
            "qrc:/qt/qml/cc/caixa/opensesh/qml/Main.qml:10: TypeError"
        ));
        assert!(is_about_our_ui(
            "default",
            "OsIcon: unknown icon \"srever\""
        ));
        assert!(!is_about_our_ui(
            "default",
            "QFontDatabase: Cannot find font directory"
        ));
    }

    #[test]
    fn qt_quick_warnings_without_a_location_count_too() {
        assert!(is_about_our_ui(
            "default",
            "Qt Quick Layouts: Detected recursive rearrange. Aborting after two iterations."
        ));
        assert!(is_about_our_ui(
            "default",
            "possible QQuickItem::polish() loop"
        ));
        assert!(is_about_our_ui("qt.quick.dirty", "anything"));
    }
}
