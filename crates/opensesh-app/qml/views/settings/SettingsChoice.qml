// One-of-N choice for a setting. It shows a segmented row of OsButtons (the selected one filled
// with the accent) when that fits in `width`, and an OsComboBox otherwise, so narrow windows
// still work. The segments are a single Tab stop (the selected one); Left/Right, Up/Down and
// Home/End pick another value, like a group of radio buttons.
//   values: var      allowed values in UI order, e.g. AppSettings.choices("theme")
//   labels: var      { value: translated label }; a value without a label is shown as is
//   icons: var       optional { value: icon name } for the segments
//   value: string    the current value
//   segmented: bool  read-only; true while the segmented row is shown
//   signal picked(string value)   the user chose a different value
// Set `Accessible.name` to the setting's label.
pragma ComponentBehavior: Bound

import QtQuick
import cc.caixa.opensesh

Item {
    id: choice

    property var values: []
    property var labels: ({})
    property var icons: ({})
    property string value

    readonly property var options: values.map(v => ({
                value: v,
                text: choice.labels[v] !== undefined ? choice.labels[v] : v,
                iconName: choice.icons[v] !== undefined ? choice.icons[v] : ""
            }))
    readonly property int currentIndex: values.indexOf(value)
    // Segment that Tab lands on: the selected one, else the first.
    readonly property int tabIndex: Math.max(0, currentIndex)
    readonly property bool segmented: track.implicitWidth <= width

    signal picked(string value)

    function pick(index: int) {
        if (index < 0 || index >= values.length)
            return;
        if (values[index] !== value)
            picked(values[index]);
    }

    // Moves the keyboard focus to the segment at `index` (wrapping around) and picks it.
    function pickSegment(index: int) {
        const count = segments.count;
        if (count === 0)
            return;
        const target = (index + count) % count;
        const item = segments.itemAt(target);
        if (item)
            item.forceActiveFocus(Qt.TabFocusReason);
        pick(target);
    }

    implicitWidth: track.implicitWidth
    implicitHeight: Theme.controlHeight

    Rectangle {
        id: track

        readonly property real inset: Theme.borderWidth * 2

        visible: choice.segmented
        width: implicitWidth
        height: choice.height
        implicitWidth: segmentRow.implicitWidth + 2 * inset
        radius: Theme.radiusControl + inset
        color: Theme.surface2
        border.width: Theme.borderWidth
        border.color: Theme.border

        Accessible.role: Accessible.Grouping
        Accessible.name: choice.Accessible.name

        Row {
            id: segmentRow

            x: track.inset
            y: track.inset
            height: track.height - 2 * track.inset
            spacing: Theme.borderWidth

            Repeater {
                id: segments

                model: choice.options

                delegate: OsButton {
                    id: segment

                    required property var modelData
                    required property int index

                    readonly property bool selected: index === choice.currentIndex
                    readonly property int direction: choice.LayoutMirroring.enabled ? -1 : 1

                    height: segmentRow.height
                    text: modelData.text
                    iconName: modelData.iconName
                    variant: selected ? "primary" : "ghost"
                    // Roving tab stop: only one segment is in the Tab chain (the focused one keeps
                    // its policy: Qt refuses to take Tab focus away from the active focus item).
                    focusPolicy: index === choice.tabIndex || activeFocus ? Qt.StrongFocus : Qt.ClickFocus

                    Accessible.role: Accessible.RadioButton
                    Accessible.checkable: true
                    Accessible.checked: selected

                    onClicked: choice.pick(index)

                    Keys.onLeftPressed: choice.pickSegment(index - direction)
                    Keys.onRightPressed: choice.pickSegment(index + direction)
                    Keys.onUpPressed: choice.pickSegment(index - 1)
                    Keys.onDownPressed: choice.pickSegment(index + 1)
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Home)
                            choice.pickSegment(0);
                        else if (event.key === Qt.Key_End)
                            choice.pickSegment(segments.count - 1);
                        else
                            return;
                        event.accepted = true;
                    }
                }
            }
        }
    }

    OsComboBox {
        id: combo

        visible: !choice.segmented
        width: choice.width
        textRole: "text"
        valueRole: "value"
        model: choice.options
        // `model` is read so the index is set again after the model is rebuilt (retranslation).
        currentIndex: model && count > 0 ? choice.currentIndex : -1

        Accessible.name: choice.Accessible.name

        onActivated: index => choice.pick(index)
    }
}
