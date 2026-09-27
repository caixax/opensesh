// Status bar (PLAN §5.3). Left: the session status: the current terminal's working directory
// when the shell reports it (OSC 7), else the state of an SSH connection, else its title (a live
// monitor arrives in Sprint 11), and while the tab broadcasts, how many panes receive the input
// (click to stop).
// Right: with a master password, the vault's lock (click to lock or unlock), the notifications
// button with the unread count, a theme quick switch (System -> Dark -> Light) and the version.
//   terminal: TerminalItem   the focused terminal of the current tab, or null
//   label: string            what that terminal connects to (a host's name), if anything
//   workspace: TabWorkspace  the current tab, or null
import QtQuick
import cc.caixa.opensesh

Rectangle {
    id: bar

    property TerminalItem terminal: null
    property string label: ""
    property Item workspace: null
    // The built-in SSH client's state of the terminal ("" for other sessions).
    readonly property string sshState: terminal && terminal.connection.length > 0 ? JSON.parse(terminal.connection).state ?? "" : ""
    readonly property int receiving: workspace && workspace.broadcast ? workspace.participants.length : 0
    readonly property string sessionText: {
        if (!terminal)
            return qsTr("No active session");
        if (terminal.workingDirectory.length > 0)
            return terminal.workingDirectory;
        switch (sshState) {
        case "connecting":
            return qsTr("Connecting to %1…").arg(label);
        case "authenticating":
            return qsTr("Authenticating on %1…").arg(label);
        case "disconnected":
            return qsTr("Disconnected from %1").arg(label);
        case "connected":
            return terminal.title.length > 0 ? terminal.title : qsTr("Connected to %1").arg(label);
        default:
            break;
        }
        if (terminal.title.length > 0)
            return terminal.title;
        return label.length > 0 ? label : qsTr("Local terminal");
    }

    readonly property real buttonSize: Theme.statusBarHeight - Theme.spacingXs
    readonly property var themeNames: ({
            system: qsTr("System"),
            dark: qsTr("Dark"),
            light: qsTr("Light")
        })
    readonly property string nextTheme: AppSettings.theme === "system" ? "dark"
                                      : AppSettings.theme === "dark" ? "light" : "system"

    implicitHeight: Theme.statusBarHeight
    color: Theme.bg

    Accessible.role: Accessible.StatusBar
    Accessible.name: qsTr("Status bar")

    Rectangle {
        width: parent.width
        height: Theme.borderWidth
        color: Theme.border
    }

    Row {
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacingMd
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.spacingSm

        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: Theme.spacingSm
            height: width
            radius: width / 2
            color: !bar.terminal ? Theme.textDisabled
                 : bar.sshState === "connecting" || bar.sshState === "authenticating" ? Theme.warning
                 : bar.sshState === "disconnected" || !bar.terminal.running ? Theme.danger : Theme.success
        }

        OsText {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, bar.width / 2)
            text: bar.sessionText
            size: "small"
            muted: true
            elide: Text.ElideMiddle
            Accessible.role: Accessible.StaticText
            Accessible.name: bar.terminal && bar.terminal.workingDirectory.length > 0
                             ? qsTr("Working directory: %1").arg(bar.terminal.workingDirectory) : text
        }

        OsButton {
            id: broadcastButton

            anchors.verticalCenter: parent.verticalCenter
            visible: bar.workspace !== null && bar.workspace.broadcast
            implicitHeight: bar.buttonSize
            leftPadding: Theme.spacingSm
            rightPadding: Theme.spacingSm
            variant: "danger"
            iconName: "radio-tower"
            text: bar.receiving > 1 ? qsTr("Broadcasting to %n panes", "", bar.receiving) : qsTr("Broadcast on, no other pane receives")
            Accessible.description: qsTr("Click to stop broadcasting")
            onClicked: bar.workspace.toggleBroadcast()

            OsTooltip {
                visible: broadcastButton.hovered
                text: qsTr("Stop broadcasting (%1)").arg(ActionRegistry.find("pane.broadcast") ? ActionRegistry.find("pane.broadcast").shortcut : "")
            }
        }
    }

    Row {
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacingMd
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.spacingXs

        // Transfers running: how many and how fast; a click shows the queue (the SFTP view).
        OsButton {
            id: transfersButton

            anchors.verticalCenter: parent.verticalCenter
            visible: Transfers.active > 0
            implicitHeight: bar.buttonSize
            leftPadding: Theme.spacingSm
            rightPadding: Theme.spacingSm
            variant: "ghost"
            iconName: "arrow-left-right"
            text: Transfers.speed > 0 ? qsTr("%1 · %2").arg(Transfers.active).arg(FileFormat.speed(Transfers.speed)) : String(Transfers.active)
            Accessible.name: qsTr("%n transfer(s) running", "", Transfers.active)
            onClicked: WindowRegistry.mainShell.showView("sftp")

            OsTooltip {
                visible: transfersButton.hovered
                text: qsTr("%n transfer(s) running. Click to see them.", "", Transfers.active)
            }
        }

        // Tunnels running; a click shows them.
        OsButton {
            id: tunnelsButton

            anchors.verticalCenter: parent.verticalCenter
            visible: Tunnels.running > 0
            implicitHeight: bar.buttonSize
            leftPadding: Theme.spacingSm
            rightPadding: Theme.spacingSm
            variant: "ghost"
            iconName: "waypoints"
            text: String(Tunnels.running)
            Accessible.name: qsTr("%n tunnel(s) running", "", Tunnels.running)
            onClicked: WindowRegistry.mainShell.showView("tunnels")

            OsTooltip {
                visible: tunnelsButton.hovered
                text: qsTr("%n tunnel(s) running. Click to see them.", "", Tunnels.running)
            }
        }

        OsIconButton {
            id: vaultButton

            readonly property bool locked: Keychain.vaultStatus === "locked"

            anchors.verticalCenter: parent.verticalCenter
            // Only a master password can lock; the keyring (or a remembered key) opens it anyway.
            visible: Keychain.protection === "password" && !Keychain.remembered
                     && (locked || Keychain.vaultStatus === "unlocked")
            implicitWidth: bar.buttonSize
            implicitHeight: bar.buttonSize
            iconName: locked ? "lock" : "lock-open"
            toolTip: locked ? qsTr("The vault is locked: click to unlock") : qsTr("The vault is unlocked: click to lock")
            onClicked: ActionRegistry.trigger(locked ? "vault.unlock" : "vault.lock")
        }

        OsIconButton {
            id: bellButton

            anchors.verticalCenter: parent.verticalCenter
            implicitWidth: bar.buttonSize
            implicitHeight: bar.buttonSize
            iconName: "bell"
            toolTip: Toasts.unread > 0 ? qsTr("Notifications (%n unread)", "", Toasts.unread) : qsTr("Notifications")
            onClicked: ActionRegistry.trigger("app.notifications")
        }

        // Unread count next to the bell: the bar is too low to overlap it on the icon.
        OsBadge {
            anchors.verticalCenter: parent.verticalCenter
            count: Toasts.unread
            Accessible.ignored: true
        }

        OsIconButton {
            anchors.verticalCenter: parent.verticalCenter
            implicitWidth: bar.buttonSize
            implicitHeight: bar.buttonSize
            iconName: AppSettings.theme === "dark" ? "moon" : AppSettings.theme === "light" ? "sun" : "sun-moon"
            toolTip: qsTr("Theme: %1. Click for %2.").arg(bar.themeNames[AppSettings.theme] || AppSettings.theme)
                                                     .arg(bar.themeNames[bar.nextTheme])
            onClicked: ActionRegistry.trigger("appearance.toggleTheme")
        }

        OsText {
            anchors.verticalCenter: parent.verticalCenter
            leftPadding: Theme.spacingXs
            text: qsTr("v%1").arg(AppInfo.version)
            size: "small"
            muted: true
        }
    }
}
