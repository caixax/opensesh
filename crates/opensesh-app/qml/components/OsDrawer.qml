// Side sheet that slides in from the left or right edge of the window over a `Theme.scrim`
// layer, with a title header and a close button like OsDialog.
//   edge: Qt.Edge          Qt.RightEdge (default) or Qt.LeftEdge
//   title: string          header text; also the accessible name of the sheet
//   showClose: bool        show the header close button (default true)
//   preferredWidth: real   sheet width (360 logical px at scale 1), capped to the window
// Children go into the body under the header, inset by Theme.spacingXl; size them against the
// body (`width: parent.width`, `anchors.fill: parent`). Escape, a click on the scrim and the
// close button close it. Swiping from the window edge is disabled (`dragMargin: 0`).
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.Drawer {
    id: control

    property string title
    property bool showClose: true
    property real preferredWidth: Theme.spacingXs * 90
    default property alias body: bodyItem.data

    readonly property bool headerShown: title.length > 0 || showClose

    parent: T.Overlay.overlay
    edge: Qt.RightEdge
    width: parent ? Math.max(0, Math.min(preferredWidth, parent.width - Theme.spacingXxl)) : preferredWidth
    height: parent ? parent.height : implicitHeight

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    modal: true
    focus: true
    dragMargin: 0
    padding: 0
    // Keep the content off the 1 px border on the inner side.
    leftPadding: edge === Qt.RightEdge ? Theme.borderWidth : 0
    rightPadding: edge === Qt.LeftEdge ? Theme.borderWidth : 0

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize

    // T.Drawer animates its `position` with these; the scrim follows the position.
    enter: Transition {
        NumberAnimation {
            duration: Theme.durationNormal
            easing.type: Easing.OutCubic
        }
    }

    exit: Transition {
        NumberAnimation {
            duration: Theme.durationNormal
            easing.type: Easing.InCubic
        }
    }

    background: Rectangle {
        color: Theme.surface

        // Border on the side that faces the window content.
        Rectangle {
            readonly property bool horizontal: control.edge === Qt.LeftEdge || control.edge === Qt.RightEdge

            x: control.edge === Qt.LeftEdge ? parent.width - width : 0
            y: control.edge === Qt.TopEdge ? parent.height - height : 0
            width: horizontal ? Theme.borderWidth : parent.width
            height: horizontal ? parent.height : Theme.borderWidth
            color: Theme.border
        }
    }

    contentItem: Item {
        implicitWidth: Math.max(header.implicitWidth, bodyItem.implicitWidth + 2 * Theme.spacingXl)
        implicitHeight: header.height + bodyItem.implicitHeight + 2 * Theme.spacingXl

        Accessible.role: Accessible.Pane
        Accessible.name: control.title

        Item {
            id: header

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            visible: control.headerShown
            implicitWidth: Theme.spacingXl + titleText.implicitWidth + Theme.spacingMd
                           + (control.showClose ? closeButton.implicitWidth : 0) + Theme.spacingLg
            height: control.headerShown ? Theme.spacingLg + Theme.controlHeight : 0

            OsText {
                id: titleText

                anchors.left: parent.left
                anchors.leftMargin: Theme.spacingXl
                anchors.right: control.showClose ? closeButton.left : parent.right
                anchors.rightMargin: control.showClose ? Theme.spacingMd : Theme.spacingXl
                anchors.verticalCenter: closeButton.verticalCenter
                text: control.title
                size: "large"
                Accessible.role: Accessible.Heading
            }

            OsButton {
                id: closeButton

                anchors.right: parent.right
                anchors.rightMargin: Theme.spacingLg
                anchors.top: parent.top
                anchors.topMargin: Theme.spacingLg + (Theme.controlHeight - height) / 2
                implicitWidth: Theme.controlHeightSmall
                implicitHeight: Theme.controlHeightSmall
                leftPadding: 0
                rightPadding: 0
                visible: control.showClose
                variant: "ghost"
                iconName: "x"
                Accessible.name: qsTr("Close")
                onClicked: control.close()

                OsTooltip {
                    visible: closeButton.hovered
                    text: qsTr("Close")
                }
            }
        }

        Item {
            id: bodyItem

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: header.bottom
            anchors.bottom: parent.bottom
            anchors.margins: Theme.spacingXl
            anchors.topMargin: control.headerShown ? Theme.spacingSm : Theme.spacingXl
            // From the children's implicit sizes, not childrenRect: a child that fills the body
            // (`anchors.fill: parent`) would feed the body's size back into it (binding loop).
            implicitWidth: {
                let widest = 0;
                for (let i = 0; i < children.length; ++i)
                    widest = Math.max(widest, children[i].implicitWidth);
                return widest;
            }
            implicitHeight: {
                let tallest = 0;
                for (let i = 0; i < children.length; ++i)
                    tallest = Math.max(tallest, children[i].implicitHeight);
                return tallest;
            }
        }
    }

    T.Overlay.modal: Rectangle {
        color: Theme.scrim
    }

    T.Overlay.modeless: Rectangle {
        color: Theme.scrim
    }
}
