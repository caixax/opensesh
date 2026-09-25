// Navigation rail entry, used inside OsRail. The selected entry (`checked`) shows an accent bar
// on the rail edge and an accent icon. Without a label, the text is shown in a tooltip on hover
// and on keyboard focus.
//   iconName: string   icon (required for a useful rail)
//   showLabel: bool    shows the text next to the icon (the rail is Theme.railWidthLabels wide)
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.AbstractButton {
    id: control

    property string iconName: ""
    property bool showLabel: false

    readonly property color inkColor: !enabled ? Theme.textDisabled
                                    : checked ? Theme.text
                                    : hovered ? Theme.text
                                    : Theme.textMuted

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    leftInset: Theme.spacingSm
    rightInset: Theme.spacingSm
    leftPadding: showLabel ? Theme.spacingSm + Theme.spacingMd : Theme.spacingSm
    rightPadding: showLabel ? Theme.spacingSm + Theme.spacingMd : Theme.spacingSm
    topPadding: 0
    bottomPadding: 0
    spacing: Theme.spacingMd
    hoverEnabled: true
    focusPolicy: Qt.NoFocus

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize
    font.weight: checked ? Font.DemiBold : Font.Normal

    Accessible.role: Accessible.PageTab
    Accessible.name: text
    Accessible.selected: checked

    contentItem: Item {
        implicitWidth: control.showLabel ? icon.width + row.spacing + label.implicitWidth : Theme.iconSize
        implicitHeight: Theme.iconSize

        Row {
            id: row

            anchors.verticalCenter: parent.verticalCenter
            x: control.showLabel ? 0 : (parent.width - width) / 2
            width: control.showLabel ? parent.width : implicitWidth
            spacing: control.spacing

            OsIcon {
                id: icon

                anchors.verticalCenter: parent.verticalCenter
                name: control.iconName
                size: Theme.iconSize
                color: control.checked && control.enabled ? Theme.accentFg : control.inkColor
            }

            OsText {
                id: label

                anchors.verticalCenter: parent.verticalCenter
                visible: control.showLabel
                width: Math.max(0, row.width - icon.width - row.spacing)
                text: control.text
                font: control.font
                color: control.inkColor
                horizontalAlignment: Text.AlignLeft
            }
        }
    }

    background: Rectangle {
        implicitWidth: Theme.railWidth - 2 * Theme.spacingSm
        implicitHeight: Theme.rowHeight
        radius: Theme.radiusControl
        color: control.checked ? Theme.selection : "transparent"

        Behavior on color {
            ColorAnimation {
                duration: Theme.durationFast
            }
        }

        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            visible: control.enabled
            color: control.down ? Theme.pressed : control.hovered ? Theme.hover : "transparent"

            Behavior on color {
                ColorAnimation {
                    duration: Theme.durationFast
                }
            }
        }

        // Selected indicator on the rail edge.
        Rectangle {
            x: control.mirrored ? parent.width + control.rightInset - width : -control.leftInset
            anchors.verticalCenter: parent.verticalCenter
            width: Theme.borderWidth * 3
            height: control.checked ? parent.height - 2 * Theme.spacingSm : 0
            radius: width / 2
            color: control.enabled ? Theme.accent : Theme.textDisabled
            visible: height > 0

            Behavior on height {
                NumberAnimation {
                    duration: Theme.durationFast
                    easing.type: Easing.OutCubic
                }
            }
        }

        OsFocusRing {
            target: control
            baseRadius: Theme.radiusControl
        }
    }

    // Label tooltip for the icon-only rail (inline: OsTooltip is a separate component).
    T.ToolTip {
        id: tip

        parent: control
        visible: !control.showLabel && control.text.length > 0 && (control.hovered || control.visualFocus)
        delay: control.visualFocus ? 0 : 500
        text: control.text
        x: control.mirrored ? control.leftInset - implicitWidth - Theme.spacingSm
                            : control.width - control.rightInset + Theme.spacingSm
        y: (control.height - implicitHeight) / 2
        margins: Theme.spacingSm
        leftPadding: Theme.spacingSm
        rightPadding: Theme.spacingSm
        topPadding: Theme.spacingXs
        bottomPadding: Theme.spacingXs
        implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                                implicitContentWidth + leftPadding + rightPadding)
        implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                                 implicitContentHeight + topPadding + bottomPadding)
        closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutsideParent

        contentItem: OsText {
            text: tip.text
            size: "small"
        }

        background: Rectangle {
            color: Theme.surface2
            radius: Theme.radiusSmall
            border.width: Theme.borderWidth
            border.color: Theme.borderStrong
        }
    }
}
