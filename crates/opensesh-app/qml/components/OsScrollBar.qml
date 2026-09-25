// Thin scroll bar (see docs/design/components.md). Attach it with
// `T.ScrollBar.vertical: OsScrollBar {}` (or `ScrollBar.vertical` with QtQuick.Controls).
// With the default `policy` (AsNeeded) it only shows while scrolling or hovered and fades out
// afterwards; the handle darkens on hover and while dragged.
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.ScrollBar {
    id: control

    readonly property real thickness: Theme.spacingXs * 1.5

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: Theme.spacingXs / 2
    hoverEnabled: true
    visible: policy !== T.ScrollBar.AlwaysOff
    minimumSize: orientation === Qt.Horizontal ? height / width : width / height

    contentItem: Rectangle {
        id: handle

        implicitWidth: control.interactive ? control.thickness : control.thickness / 2
        implicitHeight: control.interactive ? control.thickness : control.thickness / 2
        radius: Math.min(width, height) / 2
        color: control.pressed || control.hovered ? Theme.textMuted : Theme.border
        opacity: 0

        Behavior on color {
            ColorAnimation {
                duration: Theme.durationFast
            }
        }

        states: State {
            name: "active"
            when: control.policy === T.ScrollBar.AlwaysOn || (control.active && control.size < 1.0)

            PropertyChanges {
                handle.opacity: 1
            }
        }

        transitions: Transition {
            from: "active"

            SequentialAnimation {
                PauseAnimation {
                    duration: Theme.durationNormal * 4
                }
                NumberAnimation {
                    target: handle
                    property: "opacity"
                    to: 0
                    duration: Theme.durationNormal
                }
            }
        }
    }
}
