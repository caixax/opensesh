#include "opensesh-app/app_shim.h"

#include <optional>

#include <QtCore/QMessageLogContext>
#include <QtCore/QSize>
#include <QtCore/QtLogging>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QPixmap>

namespace opensesh {

namespace {

std::optional<QtMessageSink> g_message_sink;

void forward_message(QtMsgType type, const QMessageLogContext& context, const QString& message)
{
    if (!g_message_sink) {
        return;
    }
    const QString category = QString::fromUtf8(context.category ? context.category : "");
    (*g_message_sink)(static_cast<std::int32_t>(type), category, message);
}

} // namespace

bool set_window_icon(const QString& path)
{
    const QIcon icon(path);
    // QIcon loads lazily; render once to make sure the file exists and can be decoded.
    if (icon.isNull() || icon.pixmap(QSize(64, 64)).isNull()) {
        return false;
    }
    QGuiApplication::setWindowIcon(icon);
    return true;
}

QString platform_name()
{
    return QGuiApplication::platformName();
}

void install_qt_message_handler(QtMessageSink sink)
{
    g_message_sink = sink;
    qInstallMessageHandler(forward_message);
}

} // namespace opensesh
