// Horizontal slider (see docs/design/components.md). Uses the Qt Slider API (`from`, `to`,
// `value`, `stepSize`, `snapMode`, `live`, `moved()`); arrow keys move it by `stepSize`.
// Set `Accessible.name` to the label of the value it controls.
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.Slider {
    id: control

    readonly property color fillColor: enabled ? Theme.accent : Theme.textDisabled

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitHandleWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(Theme.controlHeight,
                             implicitBackgroundHeight + topInset + bottomInset,
                             implicitHandleHeight + topPadding + bottomPadding)

    padding: Theme.spacingXs
    focusPolicy: Qt.StrongFocus
    hoverEnabled: true
    orientation: Qt.Horizontal

    handle: Rectangle {
        x: control.leftPadding + control.visualPosition * (control.availableWidth - width)
        y: control.topPadding + (control.availableHeight - height) / 2
        implicitWidth: Theme.iconSize
        implicitHeight: Theme.iconSize
        radius: width / 2
        color: control.fillColor
        border.width: Theme.borderWidth * 2
        border.color: Theme.surface
        scale: control.pressed ? 0.9 : control.hovered ? 1.1 : 1

        Behavior on scale {
            NumberAnimation {
                duration: Theme.durationFast
            }
        }

        OsFocusRing {
            target: control
            baseRadius: parent.radius
        }
    }

    background: Rectangle {
        x: control.leftPadding
        y: control.topPadding + (control.availableHeight - height) / 2
        implicitWidth: Theme.controlHeight * 5
        implicitHeight: Theme.spacingXs
        width: control.availableWidth
        height: implicitHeight
        radius: height / 2
        color: Theme.surface2
        border.width: Theme.borderWidth
        border.color: Theme.border

        // Filled part, from the start up to the middle of the handle.
        Rectangle {
            readonly property real handleCenter: control.handle
                                                 ? control.handle.x - control.leftPadding + control.handle.width / 2
                                                 : control.visualPosition * parent.width

            x: control.mirrored ? handleCenter : 0
            width: control.mirrored ? parent.width - handleCenter : handleCenter
            height: parent.height
            radius: parent.radius
            color: control.fillColor
        }
    }
}
