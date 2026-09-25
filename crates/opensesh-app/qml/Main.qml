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

    // True if a window at (left, top) this wide would show enough of its title bar on a screen
    // (a saved position can point at a monitor that is gone).
    function positionVisible(windowX, windowY, windowWidth) {
        for (const screen of Qt.application.screens) {
            const left = Math.max(windowX, screen.virtualX);
            const right = Math.min(windowX + windowWidth, screen.virtualX + screen.width);
            const top = Math.max(windowY, screen.virtualY);
            const bottom = Math.min(windowY + Theme.titleBarHeight, screen.virtualY + screen.height);
            if (right - left >= Theme.spacingXxl * 3 && bottom - top >= Theme.spacingLg)
                return true;
        }
        return false;
    }

    function restoreGeometry() {
        if (screenshotMode) {
            width = 1280;
            height = 800;
            return false;
        }
        width = Math.max(minimumWidth, UiState.windowWidth);
        height = Math.max(minimumHeight, UiState.windowHeight);
        if (positionSupported && UiState.hasPosition && positionVisible(UiState.windowX, UiState.windowY, width)) {
            x = UiState.windowX;
            y = UiState.windowY;
        }
        return UiState.maximized;
    }

    // Size and position only while windowed, so un-maximizing returns to them.
    function recordGeometry() {
        if (!persistState || !geometryReady)
            return;
        if (visibility === Window.Windowed) {
            UiState.windowWidth = width;
            UiState.windowHeight = height;
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
    flags: frameless ? (Qt.Window | Qt.FramelessWindowHint) : Qt.Window

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

        function onProblem(message) {
            Toasts.show(message, "danger");
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
        window: window
        steps: shell.smokeSteps()
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
        onFinished: Qt.exit(0)
    }
}
