// Minimize, maximize/restore and close buttons for the custom title bar (decorations "custom").
// Flat, full title-bar height; close turns Theme.danger on hover. They are not Tab stops, like
// native caption buttons: the window manager's keyboard shortcuts cover them.
//   window: Window   the window they control
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

Row {
    id: buttons

    required property Window window

    readonly property bool maximized: window.visibility === Window.Maximized

    component WindowButton: T.Button {
        id: button

        property string iconName
        property bool danger: false

        readonly property bool dangerShown: danger && (hovered || down)

        implicitWidth: Math.round(Theme.titleBarHeight * 1.1)
        implicitHeight: Theme.titleBarHeight
        padding: 0
        focusPolicy: Qt.NoFocus
        hoverEnabled: true

        Accessible.role: Accessible.Button
        Accessible.name: text

        contentItem: Item {
            OsIcon {
                anchors.centerIn: parent
                name: button.iconName
                size: Theme.iconSizeSmall
                color: button.dangerShown ? Theme.textOn(Theme.danger) : Theme.text
            }
        }

        background: Rectangle {
            color: button.dangerShown ? Theme.danger : "transparent"

            Behavior on color {
                ColorAnimation {
                    duration: Theme.durationFast
                }
            }

            Rectangle {
                anchors.fill: parent
                color: button.down ? Theme.pressed : button.hovered && !button.danger ? Theme.hover : "transparent"
            }
        }

        OsTooltip {
            visible: button.hovered
            text: button.text
        }
    }

    WindowButton {
        text: qsTr("Minimize")
        iconName: "minus"
        onClicked: buttons.window.showMinimized()
    }

    WindowButton {
        text: buttons.maximized ? qsTr("Restore") : qsTr("Maximize")
        iconName: buttons.maximized ? "copy" : "square"
        onClicked: {
            if (buttons.maximized)
                buttons.window.showNormal();
            else
                buttons.window.showMaximized();
        }
    }

    WindowButton {
        text: qsTr("Close")
        iconName: "x"
        danger: true
        onClicked: buttons.window.close()
    }
}
