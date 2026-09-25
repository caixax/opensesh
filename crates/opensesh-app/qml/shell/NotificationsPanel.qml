pragma ComponentBehavior: Bound

// Notification history (the `Toasts` singleton): a side sheet listing every toast of this
// session, newest first, with its action button and a "Clear all" button. Opening it marks
// everything as read, and so does any toast that arrives while it is open. Like every OsDrawer,
// closing it gives the focus back to where it was.
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDrawer {
    id: drawer

    title: qsTr("Notifications")
    edge: Qt.RightEdge

    onOpened: Toasts.markAllRead()

    Connections {
        target: Toasts
        enabled: drawer.opened

        function onShown() {
            Toasts.markAllRead();
        }
    }

    Item {
        anchors.fill: parent

        Item {
            id: toolbar

            width: parent.width
            height: clearButton.height

            OsText {
                anchors.left: parent.left
                anchors.right: clearButton.left
                anchors.rightMargin: Theme.spacingSm
                anchors.verticalCenter: parent.verticalCenter
                // Not "%n notification(s)": English has no translation file to pick the plural.
                //: Number of notifications in the history, which only covers this session
                text: qsTr("This session: %1").arg(Toasts.history.length)
                muted: true
            }

            OsButton {
                id: clearButton

                anchors.right: parent.right
                implicitHeight: Theme.controlHeightSmall
                variant: "ghost"
                iconName: "trash-2"
                text: qsTr("Clear all")
                enabled: Toasts.history.length > 0
                onClicked: Toasts.clearHistory()
            }
        }

        ListView {
            id: list

            anchors.top: toolbar.bottom
            anchors.topMargin: Theme.spacingMd
            anchors.bottom: parent.bottom
            width: parent.width
            clip: true
            spacing: Theme.spacingMd
            boundsBehavior: Flickable.StopAtBounds
            model: Toasts.history

            Accessible.role: Accessible.List
            Accessible.name: drawer.title

            delegate: Column {
                id: entry

                required property var modelData

                width: ListView.view.width
                spacing: Theme.spacingXs

                OsText {
                    width: parent.width
                    text: Qt.formatTime(entry.modelData.time, Qt.locale(), Locale.ShortFormat)
                    size: "small"
                    muted: true
                }

                OsToast {
                    width: parent.width
                    maxWidth: parent.width
                    kind: entry.modelData.kind
                    text: entry.modelData.text
                    actionText: entry.modelData.actionText
                    showClose: false
                    onActionClicked: {
                        if (entry.modelData.actionId.length > 0)
                            ActionRegistry.trigger(entry.modelData.actionId);
                    }
                }
            }

            T.ScrollBar.vertical: OsScrollBar {}
        }

        OsEmptyState {
            anchors.fill: list
            visible: list.count === 0
            iconName: "bell"
            title: qsTr("No notifications")
            description: qsTr("Messages from OpenSesh appear here.")
        }
    }
}
