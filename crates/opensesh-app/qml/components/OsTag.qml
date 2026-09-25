// Small pill label, e.g. a host tag or a filter chip.
//   text: string          label (elided beyond maxTextWidth)
//   iconName: string      optional leading icon
//   removable: bool       trailing "x" button (a Tab stop) that emits removeClicked
//   variant: string       "neutral" (default) | "accent"
//   maxTextWidth: real
//   signal removeClicked
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

Rectangle {
    id: control

    property string text
    property string iconName: ""
    property bool removable: false
    property string variant: "neutral"
    property real maxTextWidth: Theme.spacingXxl * 6

    signal removeClicked

    readonly property bool accent: variant === "accent" && enabled
    readonly property color inkColor: !enabled ? Theme.textDisabled : accent ? Theme.accentText : Theme.text

    implicitWidth: row.implicitWidth
    implicitHeight: Theme.controlHeightSmall - Theme.spacingXs
    radius: height / 2
    color: accent ? Theme.accent : Theme.surface2
    border.width: accent ? 0 : Theme.borderWidth
    border.color: Theme.border

    Accessible.role: Accessible.StaticText
    Accessible.name: text

    Behavior on color {
        ColorAnimation {
            duration: Theme.durationFast
        }
    }

    Row {
        id: row

        anchors.verticalCenter: parent.verticalCenter
        leftPadding: Theme.spacingSm
        rightPadding: control.removable ? Theme.spacingXs / 2 : Theme.spacingSm
        spacing: Theme.spacingXs

        OsIcon {
            anchors.verticalCenter: parent.verticalCenter
            visible: control.iconName.length > 0
            name: control.iconName
            size: Theme.iconSizeSmall - Theme.borderWidth * 2
            color: control.inkColor
        }

        OsText {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, control.maxTextWidth)
            text: control.text
            size: "small"
            font.weight: Font.Medium
            color: control.inkColor
        }

        T.AbstractButton {
            id: removeButton

            anchors.verticalCenter: parent.verticalCenter
            width: control.height - Theme.spacingXs
            height: width
            visible: control.removable
            enabled: control.enabled
            focusPolicy: Qt.TabFocus
            hoverEnabled: true

            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Remove %1").arg(control.text)

            onClicked: control.removeClicked()

            background: Rectangle {
                radius: width / 2
                color: removeButton.down ? Theme.pressed : removeButton.hovered ? Theme.hover : "transparent"

                Behavior on color {
                    ColorAnimation {
                        duration: Theme.durationFast
                    }
                }

                OsFocusRing {
                    target: removeButton
                    baseRadius: width / 2
                }
            }

            contentItem: Item {
                OsIcon {
                    anchors.centerIn: parent
                    name: "x"
                    size: Math.round(removeButton.width * 0.7)
                    color: control.inkColor
                }
            }
        }
    }
}
