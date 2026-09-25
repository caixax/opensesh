// Collapsible side panel (PLAN §5.3): contextual tools for the current session in three tabs,
// SFTP, Info and Snippets. Placeholders until those features land (Sprints 8, 11 and 10).
//   currentIndex: int       selected tab
//   signal closeRequested   the header close button was clicked
import QtQuick
import QtQuick.Layouts
import cc.caixa.opensesh

Rectangle {
    id: panel

    property int currentIndex: 0

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

        OsEmptyState {
            iconName: "folder-sync"
            title: qsTr("SFTP")
            description: qsTr("The file browser for the current session arrives in Sprint 8.")
        }

        OsEmptyState {
            iconName: "info"
            title: qsTr("Host info")
            description: qsTr("Host details and the live system monitor arrive in Sprint 11.")
        }

        OsEmptyState {
            iconName: "scroll-text"
            title: qsTr("Snippets")
            description: qsTr("Quick snippets for the current session arrive in Sprint 10.")
        }
    }
}
