// Push button (reference implementation for the library, see docs/design/components.md).
//   variant: string   "primary" | "secondary" (default) | "ghost" | "danger"
//   iconName: string  optional leading icon
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.Button {
    id: control

    property string variant: "secondary"
    property string iconName: ""

    readonly property color fillColor: {
        switch (variant) {
        case "primary":
            return Theme.accent;
        case "danger":
            return Theme.danger;
        case "ghost":
            return "transparent";
        default:
            return Theme.surface2;
        }
    }
    readonly property color inkColor: {
        if (!enabled)
            return Theme.textDisabled;
        switch (variant) {
        case "primary":
            return Theme.accentText;
        case "danger":
            return Theme.textOn(Theme.danger);
        default:
            return Theme.text;
        }
    }

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    leftPadding: Theme.controlPadding
    rightPadding: Theme.controlPadding
    topPadding: 0
    bottomPadding: 0
    spacing: Theme.spacingSm
    focusPolicy: Qt.StrongFocus

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize
    font.weight: Font.Medium

    Accessible.role: Accessible.Button
    Accessible.name: text

    contentItem: Item {
        implicitWidth: row.implicitWidth
        implicitHeight: row.implicitHeight

        Row {
            id: row

            anchors.centerIn: parent
            spacing: control.spacing

            OsIcon {
                anchors.verticalCenter: parent.verticalCenter
                visible: control.iconName.length > 0
                name: control.iconName
                color: control.inkColor
                size: Theme.iconSizeSmall
            }

            OsText {
                anchors.verticalCenter: parent.verticalCenter
                visible: control.text.length > 0
                text: control.text
                font: control.font
                color: control.inkColor
            }
        }
    }

    background: Rectangle {
        implicitWidth: Theme.controlHeight * 2
        implicitHeight: Theme.controlHeight
        radius: Theme.radiusControl
        color: control.enabled || control.variant === "ghost" ? control.fillColor : Theme.surface2
        border.width: control.variant === "secondary" ? Theme.borderWidth : 0
        border.color: Theme.border

        Behavior on color {
            ColorAnimation {
                duration: Theme.durationFast
            }
        }

        // Hover and press feedback, drawn over the fill.
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

        OsFocusRing {
            target: control
            baseRadius: Theme.radiusControl
        }
    }
}
