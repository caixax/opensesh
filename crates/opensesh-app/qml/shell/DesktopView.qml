pragma ComponentBehavior: Bound

// A remote desktop pane's content (Sprint 13, ADR 0034): the RdpItem, a slim bar above it (what
// it connects to and its state, Ctrl+Alt+Del, full screen and the pane's menu) and the
// SshOverlay for the server's certificate, the password and disconnections.
//
// While the desktop has the keyboard every key goes to it, the app's shortcuts included, except
// the "Give the keyboard back" action's (Ctrl+Alt+Home unless changed in Settings > Shortcuts),
// which moves the focus to the bar's menu button. A click on the desktop gives it the keyboard
// again.
//
// The desktop follows the pane's size ("dynamic": it asks the server for the new size once the
// pane stops changing), is scaled to fit it ("fit") or is shown one pixel per pixel ("actual"),
// as the host says; the menu changes it for the pane.
//   pane: Item        the TerminalPane (paneId, host, target, label, startSession, edgeInset,
//                     workspace, shell, closePane())
//   desktop: RdpItem  read-only
// Functions: focusDesktop(), releaseKeyboard().
import QtQuick
import QtQuick.Layouts
import cc.caixa.opensesh

Item {
    id: view

    required property Item pane
    readonly property alias desktop: rdp
    readonly property var hostData: pane.host.length > 0 ? JSON.parse(Hosts.hostJson(pane.host) || "{}") : ({})
    // RDP desktops follow the pane by default; VNC servers mostly keep their size, so they fit.
    property string scaleMode: view.pane.kind === "vnc" ? ((hostData.vnc ?? {}).scaling ?? "fit")
                                                        : ((hostData.rdp ?? {}).scaling ?? "dynamic")
    // The workspace goes first when a tab closes.
    readonly property bool zoomed: view.pane.workspace !== null && view.pane.workspace.zoomedPane === view.pane.paneId
    readonly property int paneCount: view.pane.workspace !== null ? view.pane.workspace.paneCount : 1
    readonly property var connection: overlay.connection
    // A VNC host set to view only: nothing typed or clicked reaches it.
    readonly property bool viewOnly: pane.kind === "vnc" && (hostData.vnc ?? {}).read_only === true

    function focusDesktop() {
        if (overlay.asking)
            overlay.focusPrompt();
        else
            rdp.forceActiveFocus(Qt.OtherFocusReason);
    }

    // The app takes the keyboard back.
    function releaseKeyboard() {
        moreButton.forceActiveFocus(Qt.ShortcutFocusReason);
        Toasts.show(qsTr("The keyboard is back with OpenSesh. Click the desktop to give it the keyboard again."), "info");
    }

    function setScaleMode(mode) {
        scaleMode = mode;
        if (mode === "dynamic")
            resizeTimer.restart();
    }

    function stateText() {
        switch (overlay.phase) {
        case "connecting":
            return qsTr("Connecting…");
        case "authenticating":
            return qsTr("Signing in…");
        case "connected":
            return rdp.desktopWidth > 0 ? qsTr("%1 × %2").arg(rdp.desktopWidth).arg(rdp.desktopHeight) : qsTr("Connected");
        case "disconnected":
            return qsTr("Disconnected");
        default:
            return "";
        }
    }

    Rectangle {
        id: bar

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: barRow.implicitHeight + 2 * Theme.spacingXs
        color: Theme.surface
        z: 4

        Accessible.role: Accessible.ToolBar
        Accessible.name: qsTr("Remote desktop")

        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: Theme.borderWidth
            color: Theme.border
        }

        RowLayout {
            id: barRow

            anchors.fill: parent
            anchors.leftMargin: Theme.spacingSm
            anchors.rightMargin: Theme.spacingXs + view.pane.edgeInset
            spacing: Theme.spacingSm

            OsIcon {
                name: "monitor"
                size: Theme.iconSizeSmall
                color: overlay.phase === "connected" ? Theme.accent : Theme.textMuted
            }

            OsText {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: view.pane.label
                size: "small"
            }

            OsTag {
                visible: view.viewOnly
                text: qsTr("View only")
            }

            // VNC without VeNCrypt: said as plainly as for telnet.
            Row {
                id: unencrypted

                visible: !rdp.encrypted && rdp.running
                spacing: Theme.spacingXs
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("Not encrypted: what you type and see crosses the network as it is.")

                OsIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "triangle-alert"
                    size: Theme.iconSizeSmall
                    color: Theme.warning
                }

                OsText {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Not encrypted")
                    size: "small"
                    color: Theme.warning
                }

                HoverHandler {
                    id: unencryptedHover
                }

                OsTooltip {
                    visible: unencryptedHover.hovered
                    text: qsTr("This VNC server didn't offer TLS: what you type and see crosses the network as it is. Use VeNCrypt on the server, or reach it through a jump host.")
                }
            }

            OsText {
                text: view.stateText()
                size: "small"
                muted: true
                font.features: { "tnum": 1 }
            }

            OsIconButton {
                iconName: "keyboard"
                toolTip: qsTr("Send Ctrl+Alt+Del")
                enabled: rdp.running && !view.viewOnly
                focusPolicy: Qt.NoFocus
                onClicked: {
                    rdp.sendCtrlAltDel();
                    view.focusDesktop();
                }
            }

            OsIconButton {
                iconName: view.pane.Window.window && view.pane.Window.window.visibility === Window.FullScreen ? "minimize-2" : "maximize-2"
                toolTip: qsTr("Full screen")
                focusPolicy: Qt.NoFocus
                onClicked: {
                    view.pane.shell.toggleFullScreen();
                    view.focusDesktop();
                }
            }

            OsIconButton {
                id: moreButton

                iconName: "ellipsis"
                toolTip: qsTr("Remote desktop menu")
                onClicked: menu.popup(moreButton, 0, moreButton.height)
            }
        }
    }

    RdpItem {
        id: rdp

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: bar.bottom
        anchors.bottom: parent.bottom
        paneId: view.pane.paneId
        host: view.pane.host
        target: view.pane.target
        keyboardLocale: Qt.inputMethod.locale.name
        scaleMode: view.scaleMode
        backgroundColor: Theme.bg
        releaseShortcut: view.pane.shell.shortcutText("desktop.releaseKeyboard")
        Accessible.role: Accessible.Graphic
        Accessible.name: qsTr("Remote desktop of %1").arg(view.pane.label)

        onActiveFocusChanged: {
            if (activeFocus)
                view.pane.workspace.setFocusedPane(view.pane.paneId);
        }
        onEscapeRequested: view.releaseKeyboard()
        onLocalClipboardChanged: offerClipboard()
        onWantedWidthChanged: resizeTimer.restart()
        onWantedHeightChanged: resizeTimer.restart()
        onRunningChanged: {
            if (running)
                resizeTimer.restart();
        }
        Component.onCompleted: {
            if (view.pane.startSession)
                start();
        }
    }

    // The desktop asks for the pane's size once it stops changing (a divider dragged, the
    // window resized).
    Timer {
        id: resizeTimer

        interval: 400
        onTriggered: rdp.resizeDesktop()
    }

    SshOverlay {
        id: overlay

        anchors.fill: rdp
        terminal: rdp
        shell: view.pane.shell
        label: view.pane.label
        edgeInset: view.pane.edgeInset
        onAnswered: view.focusDesktop()
        onCloseRequested: view.pane.closePane()
    }

    OsContextMenu {
        id: menu

        OsMenuItem {
            text: qsTr("Send Ctrl+Alt+Del")
            iconName: "keyboard"
            enabled: rdp.running
            onTriggered: rdp.sendCtrlAltDel()
        }

        OsMenuItem {
            text: qsTr("Give the keyboard back to OpenSesh")
            shortcutText: view.pane.shell.shortcutText("desktop.releaseKeyboard")
            enabled: rdp.activeFocus
            onTriggered: view.releaseKeyboard()
        }

        OsContextMenu {
            title: qsTr("Scaling")

            OsMenuItem {
                text: qsTr("Follow the pane's size")
                checkable: true
                checked: view.scaleMode === "dynamic"
                onTriggered: view.setScaleMode("dynamic")
            }

            OsMenuItem {
                text: qsTr("Fit in the pane")
                checkable: true
                checked: view.scaleMode === "fit"
                onTriggered: view.setScaleMode("fit")
            }

            OsMenuItem {
                text: qsTr("Actual size")
                checkable: true
                checked: view.scaleMode === "actual"
                onTriggered: view.setScaleMode("actual")
            }
        }

        OsMenuItem {
            text: qsTr("Full screen")
            iconName: "maximize-2"
            shortcutText: view.pane.shell.shortcutText("app.fullscreen")
            onTriggered: view.pane.shell.toggleFullScreen()
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Reconnect")
            iconName: "refresh-cw"
            enabled: overlay.phase === "disconnected" && !overlay.asking
            onTriggered: rdp.reconnect()
        }

        OsMenuItem {
            text: qsTr("Disconnect")
            iconName: "unplug"
            enabled: overlay.phase !== "disconnected" && overlay.phase !== ""
            onTriggered: rdp.disconnect()
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Split right")
            iconName: "columns-2"
            shortcutText: view.pane.shell.shortcutText("pane.splitRight")
            onTriggered: view.pane.workspace.splitPane(view.pane.paneId, "horizontal")
        }

        OsMenuItem {
            text: qsTr("Split down")
            iconName: "rows-2"
            shortcutText: view.pane.shell.shortcutText("pane.splitDown")
            onTriggered: view.pane.workspace.splitPane(view.pane.paneId, "vertical")
        }

        OsMenuItem {
            text: view.zoomed ? qsTr("Restore pane size") : qsTr("Maximize pane")
            iconName: view.zoomed ? "minimize-2" : "maximize-2"
            shortcutText: view.pane.shell.shortcutText("pane.zoom")
            enabled: view.paneCount > 1
            onTriggered: view.pane.workspace.toggleZoom(view.pane.paneId)
        }

        OsMenuItem {
            text: qsTr("Close pane")
            iconName: "x"
            shortcutText: view.pane.shell.shortcutText("pane.close")
            onTriggered: view.pane.closePane()
        }
    }
}
