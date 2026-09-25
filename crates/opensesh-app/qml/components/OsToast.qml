// Transient notification card: an accent bar and icon in the kind's status color, the message,
// an optional action button and a dismiss button. OsToastHost stacks them for the `Toasts`
// singleton; it can also be placed inline.
//   kind: string         "info" (default) | "success" | "warning" | "danger"
//   text: string         message (wraps); also the accessible name
//   actionText: string   optional action button label; empty hides the button
//   showClose: bool      show the dismiss button (default true)
//   maxWidth: real       widest the card grows (420 logical px at scale 1)
//   hovered: bool        read-only; the pointer is over the card
//   focusInside: bool    read-only; one of its buttons has keyboard focus
// Signals: actionClicked(), closeClicked().
import QtQuick
import cc.caixa.opensesh

Rectangle {
    id: toast

    property string kind: "info"
    property string text
    property string actionText
    property bool showClose: true
    property real maxWidth: Theme.spacingXs * 105

    readonly property bool hovered: hoverHandler.hovered
    readonly property bool focusInside: actionButton.activeFocus || closeButton.activeFocus

    readonly property color kindColor: {
        switch (kind) {
        case "success":
            return Theme.success;
        case "warning":
            return Theme.warning;
        case "danger":
            return Theme.danger;
        default:
            return Theme.info;
        }
    }
    readonly property string kindIcon: {
        switch (kind) {
        case "success":
            return "circle-check";
        case "warning":
            return "triangle-alert";
        case "danger":
            return "circle-x";
        default:
            return "info";
        }
    }

    // Layout conditions; never the children's `visible`, which is false while the toast is hidden.
    readonly property bool hasAction: actionText.length > 0
    // Space taken by everything except the message.
    readonly property real chromeWidth: bar.anchors.leftMargin + bar.width + icon.anchors.leftMargin + icon.width
                                        + message.anchors.leftMargin + message.anchors.rightMargin
                                        + (hasAction ? actionButton.implicitWidth + actionButton.anchors.rightMargin : 0)
                                        + (showClose ? closeButton.implicitWidth + closeButton.anchors.rightMargin : 0)

    signal actionClicked
    signal closeClicked

    implicitWidth: Math.min(maxWidth, chromeWidth + Math.ceil(message.implicitWidth))
    implicitHeight: Math.max(Theme.controlHeightSmall + 2 * Theme.spacingSm, message.height + 2 * Theme.spacingMd)

    color: Theme.surface
    radius: Theme.radiusCard
    border.color: Theme.borderStrong
    border.width: Theme.borderWidth

    Accessible.role: Accessible.AlertMessage
    Accessible.name: text

    HoverHandler {
        id: hoverHandler
    }

    FontMetrics {
        id: metrics

        font: message.font
    }

    Rectangle {
        id: bar

        anchors.left: parent.left
        anchors.leftMargin: Theme.spacingSm
        anchors.top: parent.top
        anchors.topMargin: Theme.spacingSm
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Theme.spacingSm
        width: Math.max(Theme.borderWidth * 3, Math.round(Theme.spacingXs * 0.75))
        radius: width / 2
        color: toast.kindColor
    }

    OsIcon {
        id: icon

        anchors.left: bar.right
        anchors.leftMargin: Theme.spacingMd
        // Centered on the first line of the message.
        y: message.y + Math.round((metrics.height - height) / 2)
        name: toast.kindIcon
        color: toast.kindColor
        size: Theme.iconSize
    }

    OsText {
        id: message

        anchors.left: icon.right
        anchors.leftMargin: Theme.spacingSm
        anchors.right: toast.hasAction ? actionButton.left : toast.showClose ? closeButton.left : parent.right
        anchors.rightMargin: toast.hasAction || toast.showClose ? Theme.spacingSm : Theme.spacingMd
        y: Math.max(Theme.spacingMd, Math.round((toast.height - height) / 2))
        text: toast.text
        wrapMode: Text.Wrap
        maximumLineCount: 5
        elide: Text.ElideRight
        verticalAlignment: Text.AlignTop
        Accessible.ignored: true
    }

    OsButton {
        id: actionButton

        anchors.right: toast.showClose ? closeButton.left : parent.right
        anchors.rightMargin: toast.showClose ? Theme.spacingXs : Theme.spacingSm
        anchors.verticalCenter: parent.verticalCenter
        implicitHeight: Theme.controlHeightSmall
        leftPadding: Theme.spacingMd
        rightPadding: Theme.spacingMd
        visible: toast.hasAction
        variant: "secondary"
        text: toast.actionText
        onClicked: toast.actionClicked()
    }

    OsButton {
        id: closeButton

        anchors.right: parent.right
        anchors.rightMargin: Theme.spacingSm
        anchors.verticalCenter: parent.verticalCenter
        implicitWidth: Theme.controlHeightSmall
        implicitHeight: Theme.controlHeightSmall
        leftPadding: 0
        rightPadding: 0
        visible: toast.showClose
        variant: "ghost"
        iconName: "x"
        Accessible.name: qsTr("Dismiss")
        onClicked: toast.closeClicked()

        OsTooltip {
            visible: closeButton.hovered
            text: qsTr("Dismiss")
        }
    }
}
