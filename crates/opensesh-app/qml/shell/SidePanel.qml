// Collapsible side panel (PLAN §5.3): contextual tools for the current session in three tabs:
// SFTP (the files of the focused terminal, SessionFiles), Info (its host, SessionInfo) and
// Snippets (SessionSnippets).
//   currentIndex: int       selected tab
//   terminalPane: Item      the current tab's focused TerminalPane, or null
//   signal closeRequested   the header close button was clicked
import QtQuick
import QtQuick.Layouts
import cc.caixa.opensesh

Rectangle {
    id: panel

    property int currentIndex: 0
    property Item terminalPane: null
    readonly property alias files: sessionFiles
    readonly property alias info: sessionInfo

    signal closeRequested

    color: Theme.surface

    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Side panel")

    Item {
        id: header

        width: parent.width
        height: Theme.titleBarHeight

        OsTabBar {
            id: tabs

            anchors.left: parent.left
            anchors.leftMargin: Theme.spacingSm
            anchors.right: closeButton.left
            anchors.rightMargin: Theme.spacingSm
            anchors.verticalCenter: parent.verticalCenter
            currentIndex: panel.currentIndex
            Accessible.name: qsTr("Side panel tabs")

            onCurrentIndexChanged: panel.currentIndex = currentIndex

            OsTabButton {
                text: qsTr("SFTP")
                iconName: "folder-sync"
                closable: false
            }
            OsTabButton {
                text: qsTr("Info")
                iconName: "info"
                closable: false
            }
            OsTabButton {
                text: qsTr("Snippets")
                iconName: "scroll-text"
                closable: false
            }
        }

        OsIconButton {
            id: closeButton

            anchors.right: parent.right
            anchors.rightMargin: Theme.spacingSm
            anchors.verticalCenter: parent.verticalCenter
            implicitWidth: Theme.controlHeightSmall
            implicitHeight: Theme.controlHeightSmall
            iconName: "x"
            toolTip: qsTr("Close side panel")
            onClicked: panel.closeRequested()
        }
    }

    Rectangle {
        anchors.top: header.bottom
        width: parent.width
        height: Theme.borderWidth
        color: Theme.border
    }

    StackLayout {
        anchors.top: header.bottom
        anchors.topMargin: Theme.borderWidth
        anchors.bottom: parent.bottom
        width: parent.width
        currentIndex: panel.currentIndex

        SessionFiles {
            id: sessionFiles

            terminalPane: panel.visible && panel.currentIndex === 0 ? panel.terminalPane : null
        }

        SessionInfo {
            id: sessionInfo

            terminalPane: panel.visible && panel.currentIndex === 1 ? panel.terminalPane : null
        }

        SessionSnippets {}
    }
}
