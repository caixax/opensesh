// Thin C++ helpers for Qt features that cxx-qt-lib does not expose yet.
#pragma once

#include <cstdint>

#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtQml/QQmlApplicationEngine>

#include "rust/cxx.h"

namespace opensesh {

// Sets the default window icon from a file or resource path. Returns false (and leaves the icon
// unchanged) if the image can't be loaded, e.g. when the Qt SVG image plugin is missing.
bool set_window_icon(const QString& path);

// QGuiApplication::platformName, e.g. "wayland", "xcb", "windows" or "offscreen".
QString platform_name();

// Receives every Qt log message: level (QtMsgType value), category and text.
using QtMessageSink = rust::Fn<void(std::int32_t, const QString&, const QString&)>;

// Routes qDebug/qWarning/console.log/QML errors to `sink` instead of stderr/OutputDebugString.
// Call it once, before the QGuiApplication is created.
void install_qt_message_handler(QtMessageSink sink);

// Registers `image://icon/<name>?color=<#rrggbb|#aarrggbb>&size=<px>` on the engine. Icons are
// read from the module's qrc (qml/icons/<name>.svg), recolored by replacing `currentColor`,
// rendered with QSvgRenderer and cached. The engine owns the provider.
void install_icon_provider(QQmlApplicationEngine& engine);

// Registers every bundled font (qrc fonts/*.ttf|otf) with QFontDatabase and returns how many
// families were added.
std::int32_t register_bundled_fonts();

// Sets the application default font family (size stays the platform default).
void set_application_font_family(const QString& family);

// Asks every Qt Quick window created from now on for an alpha channel (see
// QQuickWindow::setDefaultAlphaBuffer), so a translucent terminal shows what is behind the window.
void enable_window_alpha();

// Turns off Qt Quick's automatic shader pipeline cache, which Qt writes to the per-user cache
// folder (QStandardPaths::CacheLocation): portable mode keeps everything next to the executable.
void disable_shader_disk_cache();

// Installed font families, optionally only fixed-pitch ones, sorted.
QStringList font_families(bool monospace_only);

// The system's alert sound: MessageBeep on Windows, the X11 bell on X11. Wayland has no
// standard one yet. Returns whether a sound was requested.
bool platform_beep();

// The keyboard modifiers held right now (Qt::KeyboardModifiers), read from the system: the tab
// switcher polls it to see Ctrl released, even outside the window.
std::int32_t keyboard_modifiers();

// Portable text of a key combination, e.g. "Ctrl+Shift+P" (QKeySequence::PortableText).
QString key_sequence_text(std::int32_t key, std::int32_t modifiers);

// Remembers the engine whose bindings are re-evaluated when the language changes.
void set_translation_engine(QQmlApplicationEngine& engine);

// Codes of the bundled translations (qrc i18n/opensesh_<code>.qm), e.g. ["pseudo"].
QStringList available_translations();

// Installs the translation for `code` ("system", "en", "es", "pseudo", ...) and retranslates
// the QML engine. "en" and a missing translation fall back to the English source strings.
// Returns whether a translation file was loaded.
bool apply_translation(const QString& code);

// Native name of a language code ("es" -> "español").
QString language_native_name(const QString& code);

} // namespace opensesh
