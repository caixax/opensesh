// A secondary window (PLAN Sprint 4), opened when a tab moves out of a window: the tab strip,
// the tabs' panes and the status bar, without the rail, Home and the side panel (their views
// open in the main window). It closes when its last tab closes or moves away; closing it ends
// the sessions of its tabs, which "Reopen closed tab" can start again. Its geometry isn't
// remembered, and on Wayland the compositor places it. Created by WindowRegistry.openWindow().
//   entries: var     the tabs to show at start (workspace entries, see AppShell.adoptTab)
//   shell: AppShell  read-only
import QtQuick
import cc.caixa.opensesh

Window {
    id: window

    property var entries: []
    readonly property alias shell: shell
    readonly property string decorations: Platform.effectiveDecorations(AppSettings.windowDecorations)
    readonly property bool frameless: decorations === "custom" || decorations === "none"
    readonly property bool windowed: visibility === Window.Windowed
    property bool closingDown: false

    minimumWidth: 480
    minimumHeight: 320
    title: shell.currentWorkspace && shell.currentWorkspace.title.length > 0
           ? qsTr("%1 - OpenSesh").arg(shell.currentWorkspace.title) : qsTr("OpenSesh")
    color: shell.translucentTerminal ? "transparent" : Theme.bg // lint-qml: allow (no paint, not a design color)
    // The same flags as the main window (see Main.qml).
    flags: frameless ? (Qt.Window | Qt.FramelessWindowHint | Qt.WindowSystemMenuHint
                        | Qt.WindowMinMaxButtonsHint | Qt.WindowCloseButtonHint) : Qt.Window

    onActiveChanged: {
        if (active)
            WindowRegistry.activated(shell);
    }
    onClosing: {
        if (closingDown)
            return;
        closingDown = true;
        shell.closeAllTabs();
        // Not from inside the signal: the window is still in use.
        Qt.callLater(window.destroy);
    }

    Component.onCompleted: {
        for (const entry of entries)
            shell.adoptTab(entry);
        entries = [];
        show();
        raise();
        requestActivate();
    }

    Rectangle {
        anchors.fill: parent
        color: shell.translucentTerminal ? "transparent" : Theme.bg // lint-qml: allow (no paint, not a design color)

        AppShell {
            id: shell

            anchors.fill: parent
            window: window
            detached: true
            persistState: false
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
}
