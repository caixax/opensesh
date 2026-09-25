// Main window (PLAN §5.3). The content is the AppShell; this file owns what belongs to the
// window itself: decorations, geometry restored from and saved to UiState, the live language
// switch, settings toasts, the smoke test and the screenshot runs.
import QtQuick
import cc.caixa.opensesh

Window {
    id: window

    readonly property bool screenshotMode: AppInfo.screenshotDir.length > 0
    // Smoke tests and screenshot runs read the user's settings but never write their files.
    readonly property bool persistState: !AppInfo.smokeTest && !screenshotMode
    readonly property string decorations: Platform.effectiveDecorations(AppSettings.windowDecorations)
    readonly property bool frameless: decorations === "custom" || decorations === "none"
    readonly property bool windowed: visibility === Window.Windowed
    // Wayland doesn't let clients read or set the window position.
    readonly property bool positionSupported: Qt.platform.pluginName !== "wayland"

    property bool geometryReady: false
    property string appliedLanguage: ""
    // Room left for a native title bar and frame, which QML can't measure.
    readonly property int nativeFrameAllowance: 48
    // The size restoreGeometry() shrank the window to so it fits its screen. It isn't saved over
    // the remembered size until the user resizes the window.
    property size fittedSize: Qt.size(0, 0)

    // The screen that would show enough of the title bar of a window at (left, top) this wide,
    // or null (a saved position can point at a monitor that is gone).
    function screenShowing(windowX, windowY, windowWidth) {
        for (const screen of Qt.application.screens) {
            const left = Math.max(windowX, screen.virtualX);
            const right = Math.min(windowX + windowWidth, screen.virtualX + screen.width);
            const top = Math.max(windowY, screen.virtualY);
            const bottom = Math.min(windowY + Theme.titleBarHeight, screen.virtualY + screen.height);
            if (right - left >= Theme.spacingXxl * 3 && bottom - top >= Theme.spacingLg)
                return screen;
        }
        return null;
    }

    // The area of a screen that the taskbar or panels leave free. QML only gives the free area of
    // the whole desktop (desktopAvailableWidth/Height), so with several screens this is the size
    // of the screen.
    function freeArea(screen) {
        return Qt.size(Math.min(screen.width, screen.desktopAvailableWidth),
                       Math.min(screen.height, screen.desktopAvailableHeight));
    }

    function restoreGeometry() {
        if (screenshotMode) {
            width = 1280;
            height = 800;
            return false;
        }
        const savedWidth = Math.max(minimumWidth, UiState.windowWidth);
        const savedHeight = Math.max(minimumHeight, UiState.windowHeight);
        const savedScreen = positionSupported && UiState.hasPosition
                ? screenShowing(UiState.windowX, UiState.windowY, savedWidth) : null;
        // The saved size (or the default one) can be too large for the screen the window opens
        // on: another monitor, resolution or scale factor. Without a saved position that is the
        // window's own screen.
        const free = freeArea(savedScreen ?? Screen);
        const frame = frameless ? 0 : nativeFrameAllowance;
        if (savedWidth + frame <= free.width && savedHeight + frame <= free.height) {
            width = savedWidth;
            height = savedHeight;
            if (savedScreen) {
                x = UiState.windowX;
                y = UiState.windowY;
            }
            return UiState.maximized;
        }
        // Too large: shrink it below 8/9 of the free area and drop the saved position. Qt centers
        // a window that small in the free area of its screen, whichever side the taskbar is on.
        width = Math.max(minimumWidth, Math.min(savedWidth, Math.floor(free.width * 8 / 9) - 1));
        height = Math.max(minimumHeight, Math.min(savedHeight, Math.floor(free.height * 8 / 9) - 1));
        fittedSize = Qt.size(width, height);
        if (savedScreen)
            screen = savedScreen;
        return UiState.maximized;
    }

    // Size and position only while windowed, so un-maximizing returns to them.
    function recordGeometry() {
        if (!persistState || !geometryReady)
            return;
        if (visibility === Window.Windowed) {
            if (width !== fittedSize.width || height !== fittedSize.height) {
                UiState.windowWidth = width;
                UiState.windowHeight = height;
                fittedSize = Qt.size(0, 0);
            }
            if (positionSupported) {
                UiState.windowX = x;
                UiState.windowY = y;
                UiState.hasPosition = true;
            }
        }
        if (visibility === Window.Windowed || visibility === Window.Maximized)
            UiState.maximized = visibility === Window.Maximized;
        UiState.save();
    }

    function scheduleGeometrySave() {
        if (geometryReady && persistState)
            geometryTimer.restart();
    }

    minimumWidth: 640
    minimumHeight: 420
    title: qsTr("OpenSesh")
    color: Theme.bg
    // Frameless windows keep the system menu and the minimize, maximize and close functions. On
    // Windows these add no caption, and without them the taskbar button, Win+Down / Win+Up and
    // Alt+Space do nothing; X11 and Wayland only look at FramelessWindowHint.
    flags: frameless ? (Qt.Window | Qt.FramelessWindowHint | Qt.WindowSystemMenuHint
                        | Qt.WindowMinMaxButtonsHint | Qt.WindowCloseButtonHint) : Qt.Window

    onWidthChanged: scheduleGeometrySave()
    onHeightChanged: scheduleGeometrySave()
    onXChanged: scheduleGeometrySave()
    onYChanged: scheduleGeometrySave()
    onVisibilityChanged: scheduleGeometrySave()
    onClosing: {
        geometryTimer.stop();
        recordGeometry();
    }

    Component.onCompleted: {
        appliedLanguage = AppSettings.language;
        const maximized = restoreGeometry();
        if (maximized)
            showMaximized();
        else
            show();
        geometryReady = true;
        if (screenshotMode)
            screenshots.start();
    }

    // Update checks (only when the user enabled them, never in test runs): 10 s after start,
    // then once a day.
    Timer {
        id: updateTimer

        interval: 10 * 1000
        repeat: true
        running: AppSettings.checkForUpdates && window.persistState
        onTriggered: {
            interval = 24 * 60 * 60 * 1000;
            if (Updater.state !== "available")
                Updater.check();
        }
    }

    Connections {
        target: Updater

        function onUpdateAvailable() {
            Toasts.show(qsTr("OpenSesh %1 is available.").arg(Updater.latestVersion), "info",
                        Updater.canInstall ? qsTr("Update") : qsTr("Download"), "app.update");
        }

        function onQuitForUpdate() {
            window.close();
        }
    }

    // The window manager sends a burst of changes while the user moves or resizes.
    Timer {
        id: geometryTimer

        interval: 300
        onTriggered: window.recordGeometry()
    }

    Connections {
        target: AppSettings

        function onSettingsChanged() {
            // The language at startup was applied before QML loaded.
            if (AppSettings.language !== window.appliedLanguage) {
                window.appliedLanguage = AppSettings.language;
                Platform.applyLanguage(AppSettings.language);
            }
        }

        function onReloadedFromDisk() {
            Toasts.show(qsTr("Settings reloaded from config.toml"), "info");
        }

        // kind names the problem; detail is technical text (a path, an OS or parser error), which
        // the log has too and only a failed save shows.
        function onProblem(kind, detail) {
            switch (kind) {
            case "load-failed":
                Toasts.show(qsTr("config.toml could not be read, so OpenSesh uses the default settings. The file is left as it is until you fix it."), "danger");
                break;
            case "load-newer":
                Toasts.show(qsTr("config.toml was written by a newer version of OpenSesh. Your changes apply but are not saved."), "warning");
                break;
            case "load-warnings":
                Toasts.show(qsTr("Some settings in config.toml were not valid and use their default values. See Settings > General."), "warning");
                break;
            case "reload-failed":
                Toasts.show(qsTr("Your edit of config.toml could not be applied, so the current settings stay. Fix the file to apply it."), "danger");
                break;
            case "save-blocked-newer":
                Toasts.show(qsTr("Changes are not saved, because config.toml was written by a newer version of OpenSesh."), "danger");
                break;
            case "save-blocked-unreadable":
                Toasts.show(qsTr("Changes are not saved until config.toml is fixed."), "danger");
                break;
            case "save-failed":
                Toasts.show(qsTr("Could not save the settings: %1").arg(detail || ""), "danger");
                break;
            default:
                // A kind this file doesn't know yet (fails the smoke test).
                console.warn("Main: unknown settings problem", kind);
                Toasts.show(qsTr("There is a problem with config.toml. The log has the details."), "danger");
            }
        }
    }

    ThemeBinder {
        id: themeBinder
    }

    // grabToImage() doesn't capture Window.color, so the content paints its own background.
    // The screenshots grab window.contentItem, which also holds the popup overlay.
    Rectangle {
        id: root

        anchors.fill: parent
        color: Theme.bg

        AppShell {
            id: shell

            anchors.fill: parent
            window: window
            persistState: window.persistState
        }

        // Frameless windows get no system border; draw a hairline while windowed.
        Rectangle {
            anchors.fill: parent
            visible: window.frameless && window.windowed
            color: "transparent"
            border.width: Theme.borderWidth
            border.color: Theme.borderStrong
            z: 9000
        }

        WindowResizeHandles {
            visible: window.frameless && window.windowed
            enabled: visible
            window: window
        }
    }

    SmokeTest {
        id: smoke

        window: window
        steps: shell.smokeSteps(smoke)
    }

    // Two series: the shell on the Hosts view, then Settings > Appearance.
    ScreenshotRunner {
        id: screenshots

        target: window.contentItem
        binder: themeBinder
        prefix: "main"
        prepare: () => shell.prepareScreenshot()
        onFinished: settingsScreenshots.start()
    }

    ScreenshotRunner {
        id: settingsScreenshots

        target: window.contentItem
        binder: themeBinder
        prefix: "settings"
        prepare: () => shell.prepareSettingsScreenshot()
        // Exit code 7: a capture failed (see the warnings in the log).
        onFinished: Qt.exit(screenshots.failures + settingsScreenshots.failures > 0 ? 7 : 0)
    }
}
