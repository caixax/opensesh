// Check box (see docs/design/components.md), including the partially-checked state: set
// `tristate: true` (user cycles through it) or assign `checkState: Qt.PartiallyChecked` (for a
// "select all" box that reflects its children). `text` is the label and the accessible name.
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.CheckBox {
    id: control

    readonly property bool marked: checkState !== Qt.Unchecked
    readonly property color boxFill: !marked ? "transparent" : enabled ? Theme.accent : Theme.textDisabled

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    topPadding: Theme.spacingXs
    bottomPadding: Theme.spacingXs
    leftPadding: 0
    rightPadding: 0
    spacing: Theme.spacingSm
    focusPolicy: Qt.StrongFocus
    hoverEnabled: true

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize

    Accessible.name: text

    indicator: Rectangle {
        id: box

        implicitWidth: Theme.iconSize
        implicitHeight: Theme.iconSize
        x: control.text ? (control.mirrored ? control.width - width - control.rightPadding : control.leftPadding)
                        : control.leftPadding + (control.availableWidth - width) / 2
        y: control.topPadding + (control.availableHeight - height) / 2
        radius: Theme.radiusSmall
        color: control.boxFill
        border.width: Theme.borderWidth
        border.color: control.marked ? control.boxFill
                                     : control.enabled ? Theme.borderStrong : Theme.textDisabled

        Behavior on color {
            ColorAnimation {
                duration: Theme.durationFast
            }
        }

        // Hover and press feedback, drawn over the box.
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            visible: control.enabled
            color: control.down ? Theme.pressed : control.hovered ? Theme.hover : "transparent"
        }

        OsIcon {
            anchors.centerIn: parent
            visible: control.marked
            name: control.checkState === Qt.PartiallyChecked ? "minus" : "check"
            size: Math.round(box.width * 0.8)
            color: control.enabled ? Theme.accentText : Theme.textOn(Theme.textDisabled)
        }

        OsFocusRing {
            target: control
            baseRadius: Theme.radiusSmall
        }
    }

    contentItem: OsText {
        leftPadding: control.indicator && !control.mirrored ? control.indicator.width + control.spacing : 0
        rightPadding: control.indicator && control.mirrored ? control.indicator.width + control.spacing : 0
        text: control.text
        font: control.font
        Accessible.ignored: true
    }
}
