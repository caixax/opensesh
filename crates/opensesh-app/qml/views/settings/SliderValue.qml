// A slider with its value shown after it, for settings stored in a file: `committed(value)` is
// emitted once the user stops (mouse released, or a short pause after arrow keys), so a drag
// doesn't rewrite the file at every step.
//   from, to, stepSize: real   as OsSlider
//   value: real                the stored value (the slider follows it while not pressed)
//   text: string               the value as shown; build it from `sliderValue`
//   sliderValue: real          read-only; the slider's position
//   signal committed(real value)
// Set `Accessible.name` to the setting's label.
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

Row {
    id: control

    property real from: 0
    property real to: 1
    property real stepSize: 0.1
    property real value: 0
    property string text
    readonly property alias sliderValue: slider.value

    signal committed(real value)

    spacing: Theme.spacingMd

    OsSlider {
        id: slider

        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(Theme.spacingXxl * 3, Math.min(Theme.spacingXxl * 8, control.width - label.width - control.spacing))
        from: control.from
        to: control.to
        stepSize: control.stepSize
        snapMode: T.Slider.SnapAlways
        Accessible.name: control.Accessible.name
        Accessible.description: control.text

        onPressedChanged: {
            if (pressed)
                commitDelay.stop();
            else
                commitDelay.restart();
        }

        Binding on value {
            when: !slider.pressed && !commitDelay.running
            value: control.value
            restoreMode: Binding.RestoreNone
        }
    }

    OsText {
        id: label

        anchors.verticalCenter: parent.verticalCenter
        width: Math.ceil(metrics.advanceWidth) + Theme.spacingXs
        text: control.text
        horizontalAlignment: Text.AlignRight
        elide: Text.ElideNone
        font.features: ({
                "tnum": 1
            })

        TextMetrics {
            id: metrics

            font: label.font
            text: "8888 px" // lint-qml: allow (sizes the label, never shown)
        }
    }

    Timer {
        id: commitDelay

        interval: 250
        onTriggered: {
            if (Math.abs(slider.value - control.value) > 1e-9)
                control.committed(slider.value);
        }
    }
}
