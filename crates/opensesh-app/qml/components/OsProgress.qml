// Progress bar, determinate (`value` between `from` and `to`) or `indeterminate`. The
// indeterminate bar sweeps an accent segment; with reduce motion (Theme.durationNormal is 0)
// it shows static accent stripes instead.
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.ProgressBar {
    id: control

    readonly property bool animated: indeterminate && visible && Theme.durationNormal > 0
    readonly property color fillColor: enabled ? Theme.accent : Theme.textDisabled

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    Accessible.role: Accessible.ProgressBar

    contentItem: Item {
        id: track

        implicitWidth: Theme.spacingXxl * 6
        implicitHeight: Theme.spacingXs
        clip: control.indeterminate

        // Determinate fill.
        Rectangle {
            x: control.mirrored ? parent.width - width : 0
            width: control.position * parent.width
            height: parent.height
            radius: height / 2
            color: control.fillColor
            visible: !control.indeterminate && control.position > 0

            Behavior on width {
                NumberAnimation {
                    duration: Theme.durationNormal
                    easing.type: Easing.OutCubic
                }
            }
        }

        // Indeterminate: sweeping segment.
        Rectangle {
            id: segment

            width: Math.max(parent.height, parent.width * 0.3)
            height: parent.height
            radius: height / 2
            color: control.fillColor
            visible: control.animated

            NumberAnimation on x {
                running: control.animated
                loops: Animation.Infinite
                from: control.mirrored ? track.width : -segment.width
                to: control.mirrored ? -segment.width : track.width
                duration: Theme.durationNormal * 8
                easing.type: Easing.InOutQuad
            }
        }

        // Indeterminate with reduce motion: static stripes.
        Row {
            visible: control.indeterminate && !control.animated
            spacing: Theme.spacingSm

            Repeater {
                model: control.indeterminate && !control.animated
                       ? Math.ceil(track.width / (2 * Theme.spacingSm)) : 0

                Rectangle {
                    width: Theme.spacingSm
                    height: track.height
                    radius: height / 2
                    color: control.fillColor
                }
            }
        }
    }

    background: Rectangle {
        implicitWidth: Theme.spacingXxl * 6
        implicitHeight: Theme.spacingXs
        y: (control.height - height) / 2
        height: Theme.spacingXs
        radius: height / 2
        color: Theme.border
    }
}
