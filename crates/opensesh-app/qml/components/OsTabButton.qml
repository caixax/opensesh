// Title-bar session tab, used inside OsTabBar. The selected tab (`checked`) sits on `surface`
// with an accent underline; the others are flat with a hover overlay. Middle-click closes.
//   iconName: string       optional leading icon
//   closable: bool         shows a small close button on hover and on the current tab (default true)
//   activity: bool         accent dot for unseen activity (hidden while the close button shows)
//   maxTitleWidth: real    the title elides beyond this width
//   signal closeRequested  close button clicked or middle-click on the tab
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.TabButton {
    id: control

    property string iconName: ""
    property bool closable: true
    property bool activity: false
    property real maxTitleWidth: Theme.spacingXxl * 7

    signal closeRequested

    readonly property bool showClose: closable && enabled && (hovered || checked)
    readonly property color inkColor: !enabled ? Theme.textDisabled : checked ? Theme.text : Theme.textMuted
    readonly property real trailingSize: Theme.controlHeightSmall - Theme.spacingSm

    // Natural width: without an explicit width, TabBar stretches every tab to fill the bar.
    width: implicitWidth
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    leftPadding: Theme.controlPadding
    rightPadding: closable || activity ? Theme.spacingSm : Theme.controlPadding
    topPadding: 0
    bottomPadding: 0
    spacing: Theme.spacingSm
    hoverEnabled: true
    // Roving focus: only the current tab is a Tab stop; arrow keys move between tabs (OsTabBar).
    focusPolicy: checked || activeFocus ? Qt.TabFocus : Qt.NoFocus

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize
    font.weight: Font.Normal

    Accessible.role: Accessible.PageTab
    Accessible.name: text
    Accessible.selected: checked
    Accessible.description: activity ? qsTr("New activity") : ""

    TapHandler {
        acceptedButtons: Qt.MiddleButton
        enabled: control.closable && control.enabled
        onTapped: control.closeRequested()
    }

    contentItem: Item {
        implicitWidth: row.implicitWidth
        implicitHeight: row.implicitHeight

        Row {
            id: row

            anchors.verticalCenter: parent.verticalCenter
            spacing: control.spacing

            OsIcon {
                anchors.verticalCenter: parent.verticalCenter
                visible: control.iconName.length > 0
                name: control.iconName
                color: control.checked && control.enabled ? Theme.accentFg : control.inkColor
                size: Theme.iconSizeSmall
            }

            OsText {
                id: title

                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, control.maxTitleWidth)
                text: control.text
                font: control.font
                color: control.inkColor

                Behavior on color {
                    ColorAnimation {
                        duration: Theme.durationFast
                    }
                }
            }

            // Trailing slot: reserved whenever the tab can close or show activity, so the width
            // doesn't jump on hover.
            Item {
                anchors.verticalCenter: parent.verticalCenter
                visible: control.closable || control.activity
                implicitWidth: control.trailingSize
                implicitHeight: control.trailingSize

                Rectangle {
                    anchors.centerIn: parent
                    width: Theme.spacingSm
                    height: width
                    radius: width / 2
                    color: Theme.accent
                    visible: control.activity && !control.showClose
                }

                T.AbstractButton {
                    id: closeButton

                    anchors.fill: parent
                    visible: control.showClose
                    focusPolicy: Qt.NoFocus
                    hoverEnabled: true

                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("Close %1").arg(control.text)

                    onClicked: control.closeRequested()

                    background: Rectangle {
                        radius: Theme.radiusSmall
                        color: closeButton.down ? Theme.pressed : closeButton.hovered ? Theme.hover : "transparent"

                        Behavior on color {
                            ColorAnimation {
                                duration: Theme.durationFast
                            }
                        }
                    }

                    contentItem: Item {
                        OsIcon {
                            anchors.centerIn: parent
                            name: "x"
                            size: Math.round(control.trailingSize * 0.7)
                            color: closeButton.hovered ? Theme.text : Theme.textMuted
                        }
                    }
                }
            }
        }
    }

    background: Rectangle {
        implicitHeight: Theme.titleBarHeight - Theme.spacingSm
        radius: Theme.radiusControl
        color: control.checked ? Theme.surface : "transparent"
        border.width: control.checked ? Theme.borderWidth : 0
        border.color: Theme.border

        Behavior on color {
            ColorAnimation {
                duration: Theme.durationFast
            }
        }

        // Hover and press feedback on inactive tabs.
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            visible: control.enabled && !control.checked
            color: control.down ? Theme.pressed : control.hovered ? Theme.hover : "transparent"

            Behavior on color {
                ColorAnimation {
                    duration: Theme.durationFast
                }
            }
        }

        // Selected indicator.
        Rectangle {
            anchors.bottom: parent.bottom
            anchors.horizontalCenter: parent.horizontalCenter
            width: parent.width - 2 * Theme.radiusControl
            height: Theme.borderWidth * 2
            radius: height / 2
            color: control.enabled ? Theme.accent : Theme.textDisabled
            visible: control.checked
        }

        // Inset ring: the tab strip clips its tabs (it scrolls).
        OsFocusRing {
            anchors.margins: 0
            target: control
            baseRadius: Theme.radiusControl - Theme.focusRingWidth - gap
        }
    }
}
