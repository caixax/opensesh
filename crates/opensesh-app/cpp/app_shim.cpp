#include "opensesh-app/app_shim.h"

#include <algorithm>
#include <optional>

#include <QtCore/QCache>
#include <QtCore/QCoreApplication>
#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QHash>
#include <QtCore/QLocale>
#include <QtCore/QMessageLogContext>
#include <QtCore/QMutex>
#include <QtCore/QMutexLocker>
#include <QtCore/QPointer>
#include <QtCore/QSize>
#include <QtCore/QTranslator>
#include <QtCore/QUrlQuery>
#include <QtCore/QtLogging>
#include <QtGui/QColor>
#include <QtGui/QFont>
#include <QtGui/QFontDatabase>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QImage>
#include <QtGui/QKeySequence>
#include <QtGui/QPainter>
#include <QtGui/QPixmap>
#include <QtQuick/QQuickImageProvider>
#include <QtQuick/QQuickWindow>
#include <QtSvg/QSvgRenderer>

#if defined(Q_OS_WIN)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#elif defined(Q_OS_LINUX) && QT_CONFIG(xcb)
#include <dlfcn.h>
#endif

namespace opensesh {

namespace {

// Resource folders of the QML module (see build.rs).
const QString kModulePrefix = QStringLiteral(":/qt/qml/cc/caixa/opensesh/");

std::optional<QtMessageSink> g_message_sink;

void forward_message(QtMsgType type, const QMessageLogContext& context, const QString& message)
{
    if (!g_message_sink) {
        return;
    }
    const QString category = QString::fromUtf8(context.category ? context.category : "");
    (*g_message_sink)(static_cast<std::int32_t>(type), category, message);
}

// image://icon/<name>?color=<color>&size=<logical px>
//
// With `sourceSize` set on the Image, Qt passes `requestedSize` already multiplied by the
// device pixel ratio, so the icon is rasterized at physical size and stays crisp. Requests may
// arrive on a loader thread, hence the mutex and a renderer per call.
class IconImageProvider final : public QQuickImageProvider {
public:
    IconImageProvider()
        : QQuickImageProvider(QQuickImageProvider::Image)
        , m_images(512)
    {
    }

    QImage requestImage(const QString& id, QSize* size, const QSize& requestedSize) override
    {
        const qsizetype queryStart = id.indexOf(u'?');
        const QString name = id.left(queryStart);
        const QUrlQuery query(queryStart < 0 ? QString() : id.mid(queryStart + 1));

        QColor color(query.queryItemValue(QStringLiteral("color"), QUrl::FullyDecoded));
        if (!color.isValid()) {
            color = QColor(0, 0, 0);
        }
        int pixels = 0;
        if (requestedSize.isValid() && requestedSize.width() > 0) {
            pixels = requestedSize.width();
        } else {
            bool ok = false;
            pixels = query.queryItemValue(QStringLiteral("size")).toInt(&ok);
            if (!ok || pixels <= 0) {
                pixels = 16;
            }
        }
        pixels = std::clamp(pixels, 1, 2048);

        const QString key
            = QStringLiteral("%1|%2|%3").arg(name, color.name(QColor::HexArgb)).arg(pixels);
        QMutexLocker lock(&m_mutex);
        if (const QImage* cached = m_images.object(key)) {
            if (size) {
                *size = cached->size();
            }
            return *cached;
        }

        const QByteArray svg = loadSvg(name);
        QImage image(pixels, pixels, QImage::Format_ARGB32_Premultiplied);
        image.fill(Qt::transparent);
        if (!svg.isEmpty()) {
            QByteArray recolored = svg;
            recolored.replace("currentColor", color.name(QColor::HexRgb).toLatin1());
            QSvgRenderer renderer(recolored);
            if (renderer.isValid()) {
                QPainter painter(&image);
                painter.setRenderHint(QPainter::Antialiasing);
                painter.setOpacity(color.alphaF());
                renderer.render(&painter, QRectF(0, 0, pixels, pixels));
            } else {
                qWarning("OsIcon: icon \"%s\" is not a valid SVG", qUtf8Printable(name));
            }
        }
        if (size) {
            *size = image.size();
        }
        m_images.insert(key, new QImage(image));
        return image;
    }

private:
    QByteArray loadSvg(const QString& name)
    {
        if (const auto it = m_svgs.constFind(name); it != m_svgs.constEnd()) {
            return *it;
        }
        QByteArray bytes;
        QFile file(kModulePrefix + QStringLiteral("qml/icons/") + name + QStringLiteral(".svg"));
        if (file.open(QIODevice::ReadOnly)) {
            bytes = file.readAll();
        } else {
            qWarning("OsIcon: unknown icon \"%s\" (add it to assets/icons/icons.toml)",
                qUtf8Printable(name));
        }
        m_svgs.insert(name, bytes);
        return bytes;
    }

    QMutex m_mutex;
    QHash<QString, QByteArray> m_svgs;
    QCache<QString, QImage> m_images;
};

QPointer<QQmlApplicationEngine> g_translation_engine;
QTranslator* g_translator = nullptr;

QString translation_path(const QString& code)
{
    return kModulePrefix + QStringLiteral("i18n/opensesh_") + code + QStringLiteral(".qm");
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

void install_icon_provider(QQmlApplicationEngine& engine)
{
    engine.addImageProvider(QStringLiteral("icon"), new IconImageProvider);
}

std::int32_t register_bundled_fonts()
{
    const QDir dir(kModulePrefix + QStringLiteral("fonts"));
    std::int32_t added = 0;
    const QStringList files = dir.entryList({ QStringLiteral("*.ttf"), QStringLiteral("*.otf") },
        QDir::Files, QDir::Name);
    for (const QString& file : files) {
        const int id = QFontDatabase::addApplicationFont(dir.filePath(file));
        if (id < 0) {
            qWarning("Could not load the bundled font %s", qUtf8Printable(file));
        } else {
            ++added;
        }
    }
    return added;
}

void disable_shader_disk_cache()
{
    QCoreApplication::setAttribute(Qt::AA_DisableShaderDiskCache, true);
}

void set_application_font_family(const QString& family)
{
    QFont font = QGuiApplication::font();
    font.setFamilies({ family });
    QGuiApplication::setFont(font);
}

void enable_window_alpha()
{
    QQuickWindow::setDefaultAlphaBuffer(true);
}

std::int32_t keyboard_modifiers()
{
    return std::int32_t(QGuiApplication::queryKeyboardModifiers().toInt());
}

bool platform_beep()
{
#if defined(Q_OS_WIN)
    return MessageBeep(MB_OK) != 0;
#elif defined(Q_OS_LINUX) && QT_CONFIG(xcb)
    auto *x11 = qGuiApp ? qGuiApp->nativeInterface<QNativeInterface::QX11Application>() : nullptr;
    if (!x11 || !x11->connection())
        return false;
    // libxcb is loaded by Qt's xcb plugin already; resolving the two calls at run time keeps it
    // out of the link (and out of the Wayland-only case).
    static void *library = dlopen("libxcb.so.1", RTLD_LAZY | RTLD_LOCAL);
    if (!library)
        return false;
    struct Cookie
    {
        unsigned int sequence;
    };
    using Bell = Cookie (*)(xcb_connection_t *, std::int8_t);
    using Flush = int (*)(xcb_connection_t *);
    static const auto bell = reinterpret_cast<Bell>(dlsym(library, "xcb_bell"));
    static const auto flush = reinterpret_cast<Flush>(dlsym(library, "xcb_flush"));
    if (!bell || !flush)
        return false;
    bell(x11->connection(), 0);
    flush(x11->connection());
    return true;
#else
    return false;
#endif
}

QStringList font_families(bool monospace_only)
{
    QStringList result;
    const QStringList families = QFontDatabase::families();
    for (const QString& family : families) {
        if (family.startsWith(u'.') || QFontDatabase::isPrivateFamily(family)) {
            continue;
        }
        if (monospace_only && !QFontDatabase::isFixedPitch(family)) {
            continue;
        }
        result.append(family);
    }
    result.removeDuplicates();
    std::sort(result.begin(), result.end(),
        [](const QString& a, const QString& b) { return a.compare(b, Qt::CaseInsensitive) < 0; });
    return result;
}

QString key_sequence_text(std::int32_t key, std::int32_t modifiers)
{
    // Keypad state is not part of a shortcut.
    const auto mods = Qt::KeyboardModifiers(modifiers) & ~Qt::KeypadModifier;
    const QKeySequence sequence(QKeyCombination(mods, Qt::Key(key)));
    // Portable text ("Ctrl+Shift+P") is stable, untranslated and safe to store in
    // keybindings.toml; NativeText follows Qt's own translations and the OS locale.
    return sequence.toString(QKeySequence::PortableText);
}

void set_translation_engine(QQmlApplicationEngine& engine)
{
    g_translation_engine = &engine;
}

QStringList available_translations()
{
    QStringList codes;
    const QDir dir(kModulePrefix + QStringLiteral("i18n"));
    const QStringList files
        = dir.entryList({ QStringLiteral("opensesh_*.qm") }, QDir::Files, QDir::Name);
    for (QString file : files) {
        file.remove(0, QStringLiteral("opensesh_").size());
        file.chop(QStringLiteral(".qm").size());
        codes.append(file);
    }
    return codes;
}

bool apply_translation(const QString& code)
{
    QStringList candidates;
    if (code == QStringLiteral("system")) {
        for (QString language : QLocale::system().uiLanguages()) {
            language.replace(u'-', u'_');
            candidates.append(language);
            candidates.append(language.section(u'_', 0, 0));
        }
    } else if (code != QStringLiteral("en")) {
        candidates.append(code);
    }

    if (g_translator) {
        QCoreApplication::removeTranslator(g_translator);
        delete g_translator;
        g_translator = nullptr;
    }
    bool loaded = false;
    for (const QString& candidate : std::as_const(candidates)) {
        if (candidate.startsWith(QStringLiteral("en"))) {
            break; // English is the source language.
        }
        auto* translator = new QTranslator(QCoreApplication::instance());
        if (translator->load(translation_path(candidate))) {
            QCoreApplication::installTranslator(translator);
            g_translator = translator;
            loaded = true;
            break;
        }
        delete translator;
    }
    if (g_translation_engine) {
        g_translation_engine->retranslate();
    }
    return loaded;
}

QString language_native_name(const QString& code)
{
    if (code == QStringLiteral("pseudo")) {
        return QStringLiteral("Pseudo-locale");
    }
    if (code == QStringLiteral("en")) {
        // QLocale("en") is US English ("American English"); the UI source strings are neutral.
        return QStringLiteral("English");
    }
    const QLocale locale(code);
    const QString name = locale.nativeLanguageName();
    if (name.isEmpty()) {
        return code;
    }
    return name.left(1).toUpper() + name.mid(1);
}

} // namespace opensesh
