// Status bar (PLAN §5.3). Left: the session status: the current terminal's working directory
// when the shell reports it (OSC 7), else its title (a live monitor arrives in Sprint 11).
// Right: the notifications button with the unread count, a theme quick switch
// (System -> Dark -> Light) and the version.
//   terminal: TerminalItem   the terminal of the current tab, or null
import QtQuick
import cc.caixa.opensesh

Rectangle {
    id: bar

    property TerminalItem terminal: null
    readonly property string sessionText: {
        if (!terminal)
            return qsTr("No active session");
        if (terminal.workingDirectory.length > 0)
            return terminal.workingDirectory;
        return terminal.title.length > 0 ? terminal.title : qsTr("Local terminal");
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
            color: !bar.terminal ? Theme.textDisabled : bar.terminal.running ? Theme.success : Theme.danger
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
    }

    Row {
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacingMd
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.spacingXs

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
