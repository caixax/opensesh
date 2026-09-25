// On/off switch (see docs/design/components.md). `text` is an optional label drawn after the
// track; it is also the accessible name. Screen readers see a check box (there is no switch
// role in Qt 6.8).
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.Switch {
    id: control

    readonly property real trackHeight: Math.round(Theme.iconSize * 1.1)
    readonly property real trackWidth: Math.round(trackHeight * 1.8)
    readonly property real handleInset: Math.max(2, Math.round(Theme.spacingXs * 0.75))

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

    Accessible.role: Accessible.CheckBox
    Accessible.name: text
    Accessible.checkable: true
    Accessible.checked: checked

    indicator: Rectangle {
        id: track

        implicitWidth: control.trackWidth
        implicitHeight: control.trackHeight
        x: control.text ? (control.mirrored ? control.width - width - control.rightPadding : control.leftPadding)
                        : control.leftPadding + (control.availableWidth - width) / 2
        y: control.topPadding + (control.availableHeight - height) / 2
        radius: height / 2
        color: !control.checked ? "transparent" : control.enabled ? Theme.accent : Theme.textDisabled
        border.width: Theme.borderWidth
        border.color: control.checked ? color
                                      : control.enabled ? Theme.borderStrong : Theme.textDisabled

        Behavior on color {
            ColorAnimation {
                duration: Theme.durationFast
            }
        }

        // Hover and press feedback, drawn over the track.
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            visible: control.enabled
            color: control.down ? Theme.pressed : control.hovered ? Theme.hover : "transparent"
        }

        Rectangle {
            id: handle

            readonly property real travel: track.width - width - 2 * control.handleInset

            width: track.height - 2 * control.handleInset
            height: width
            radius: width / 2
            x: control.handleInset + Math.max(0, Math.min(1, control.visualPosition)) * travel
            y: control.handleInset
            color: !control.enabled ? (control.checked ? Theme.surface : Theme.textDisabled)
                                    : control.checked ? Theme.accentText : Theme.textMuted

            Behavior on x {
                enabled: !control.down
                NumberAnimation {
                    duration: Theme.durationFast
                    easing.type: Easing.OutCubic
                }
            }
            Behavior on color {
                ColorAnimation {
                    duration: Theme.durationFast
                }
            }
        }

        OsFocusRing {
            target: control
            baseRadius: track.radius
        }
    }

    // The paddings reserve the track's room (and the gap only when there is a label, so a switch
    // without text is exactly as wide as its track).
    contentItem: OsText {
        readonly property real trackSpace: control.indicator
                                           ? control.indicator.width + (control.text.length > 0 ? control.spacing : 0)
                                           : 0

        leftPadding: control.mirrored ? 0 : trackSpace
        rightPadding: control.mirrored ? trackSpace : 0
        text: control.text
        font: control.font
        Accessible.ignored: true
    }
}
