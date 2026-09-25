// Thin C++ helpers for QGuiApplication features that cxx-qt-lib does not expose yet.
#pragma once

#include <cstdint>

#include <QtCore/QString>

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

} // namespace opensesh
