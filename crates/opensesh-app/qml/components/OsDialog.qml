// Modal dialog centered in its window over a `Theme.scrim` layer: title header with a close
// button, content, and a footer with a reject (ghost) and an accept (primary) button.
// T.Dialog provides Accessible.role Dialog and uses `title` as the accessible name.
//   acceptText: string   accept button label (default "OK"); empty hides the button
//   rejectText: string   reject button label (default "Cancel")
//   showReject: bool     show the reject button (default true)
//   dangerous: bool      destructive action: the accept button uses the "danger" variant
//   acceptEnabled: bool  enable the accept button (e.g. false while a form is invalid)
//   showClose: bool      show the header close button (default true)
//   maxWidth, maxHeight: real  the window size minus a margin; the dialog never grows past them
// The body goes in as children (like any Popup) and sizes the dialog through its implicit size
// (at least 400 px wide at scale 1). Wrap text in a fixed-width Column: a wrapped Text alone
// reports its unwrapped width. Escape, the close button and the reject button call `reject()`;
// the accept button calls `accept()`. Assign `footer` to replace the default buttons. Closing
// gives the focus back to where it was, with its focus ring (OsFocusReturn).
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.Dialog {
    id: control

    property string acceptText: qsTr("OK")
    property string rejectText: qsTr("Cancel")
    property bool showReject: true
    property bool dangerous: false
    property bool acceptEnabled: true
    property bool showClose: true

    readonly property real maxWidth: T.Overlay.overlay ? T.Overlay.overlay.width - 2 * Theme.spacingXl
                                                       : implicitWidth
    readonly property real maxHeight: T.Overlay.overlay ? T.Overlay.overlay.height - 2 * Theme.spacingXl
                                                        : implicitHeight
    readonly property bool headerShown: title.length > 0 || showClose
    readonly property OsFocusReturn focusReturn: OsFocusReturn {
        popup: control
    }

    anchors.centerIn: T.Overlay.overlay
    width: Math.max(0, Math.min(implicitWidth, maxWidth))
    height: Math.max(0, Math.min(implicitHeight, maxHeight))

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding,
                            implicitHeaderWidth,
                            implicitFooterWidth)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding
                             + (implicitHeaderHeight > 0 ? implicitHeaderHeight + spacing : 0)
                             + (implicitFooterHeight > 0 ? implicitFooterHeight + spacing : 0))

    modal: true
    focus: true
    closePolicy: T.Popup.CloseOnEscape
    spacing: 0
    leftPadding: Theme.spacingXl
    rightPadding: Theme.spacingXl
    // The header row already leaves room under the title.
    topPadding: headerShown ? Theme.spacingSm : Theme.spacingXl
    bottomPadding: Theme.spacingXl

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize

    onAboutToShow: focusReturn.save()
    onClosed: focusReturn.restore()

    enter: Transition {
        NumberAnimation {
            property: "opacity"
            from: 0
            to: 1
            duration: Theme.durationNormal
            easing.type: Easing.OutCubic
        }
        NumberAnimation {
            property: "scale"
            from: 0.96
            to: 1
            duration: Theme.durationNormal
            easing.type: Easing.OutCubic
        }
    }

    exit: Transition {
        NumberAnimation {
            property: "opacity"
            from: 1
            to: 0
            duration: Theme.durationNormal
            easing.type: Easing.InCubic
        }
        NumberAnimation {
            property: "scale"
            from: 1
            to: 0.96
            duration: Theme.durationNormal
            easing.type: Easing.InCubic
        }
    }

    background: Rectangle {
        implicitWidth: Theme.spacingXs * 100
        color: Theme.surface
        radius: Theme.radiusCard
        border.color: Theme.borderStrong
        border.width: Theme.borderWidth
    }

    header: Item {
        visible: control.headerShown
        implicitWidth: Theme.spacingXl + titleText.implicitWidth + Theme.spacingMd
                       + (control.showClose ? closeButton.implicitWidth : 0) + Theme.spacingLg
        implicitHeight: Theme.spacingLg + Theme.controlHeight

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
            onClicked: control.reject()

            OsTooltip {
                visible: closeButton.hovered
                text: qsTr("Close")
            }
        }
    }

    footer: Item {
        visible: control.acceptText.length > 0 || (control.showReject && control.rejectText.length > 0)
        implicitWidth: footerRow.implicitWidth + 2 * Theme.spacingXl
        implicitHeight: footerRow.implicitHeight + Theme.spacingXl

        Row {
            id: footerRow

            anchors.top: parent.top
            anchors.right: parent.right
            anchors.rightMargin: Theme.spacingXl
            spacing: Theme.spacingSm

            OsButton {
                id: rejectButton

                visible: control.showReject && text.length > 0
                variant: "ghost"
                text: control.rejectText
                onClicked: control.reject()
            }

            OsButton {
                id: acceptButton

                visible: text.length > 0
                enabled: control.acceptEnabled
                variant: control.dangerous ? "danger" : "primary"
                text: control.acceptText
                onClicked: control.accept()
            }
        }
    }

    // The dimmer's opacity is set to 1 on open and 0 on close through QQmlProperty, so this
    // Behavior fades it in step with the dialog.
    T.Overlay.modal: Rectangle {
        color: Theme.scrim

        Behavior on opacity {
            NumberAnimation {
                duration: Theme.durationNormal
            }
        }
    }

    T.Overlay.modeless: Rectangle {
        color: Theme.scrim

        Behavior on opacity {
            NumberAnimation {
                duration: Theme.durationNormal
            }
        }
    }
}
