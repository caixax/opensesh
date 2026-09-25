// Color chooser (see docs/design/components.md): a row of preset swatches, a preview chip and a
// `#RRGGBB` field.
//   value: color         the chosen opaque color (defaults to Theme.accent)
//   presets: var         swatch colors, as "#RRGGBB" strings
//   presetNames: var     accessible names of the swatches, index for index with `presets`
//                        (a missing name falls back to the hex code)
//   signal accepted(color value)  the user picked a swatch or confirmed a valid hex code
// Keyboard: Tab reaches the selected swatch, the arrow keys (and Home/End) move between
// swatches, Space or Enter picks one; in the field, Enter confirms and Escape reverts.
pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: picker

    property color value: Theme.accent
    property var presets: ["#E6B450", "#F29E4C", "#E07A5F", "#D9667B", "#A983D8", "#5B9BD5", "#4DB6AC", "#7CB342"] // lint-qml: allow (preset data)
    property var presetNames: [qsTr("Sesame"), qsTr("Amber"), qsTr("Terracotta"), qsTr("Rose"),
        qsTr("Lavender"), qsTr("Blue"), qsTr("Teal"), qsTr("Green")]

    readonly property real disabledOpacity: 0.4

    // Swatch that Tab lands on: the selected one, else the first.
    readonly property int tabIndex: Math.max(0, selectedIndex)
    readonly property int selectedIndex: {
        for (let i = 0; i < presets.length; ++i) {
            if (Qt.colorEqual(presets[i], value))
                return i;
        }
        return -1;
    }

    signal accepted(color value)

    function hexOf(c: color): string {
        // Opaque colors print as #rrggbb.
        return c.toString().toUpperCase();
    }

    // Six hex digits with an optional leading "#" (what the field accepts).
    function isHex(input: string): bool {
        return /^#?[0-9A-Fa-f]{6}$/.test(input);
    }

    // "e6b450" or "#e6b450" -> "#E6B450" (the input must already be six hex digits).
    function normalized(input: string): string {
        return (input.startsWith("#") ? input : "#" + input).toUpperCase();
    }

    function pick(hex: string) {
        const changed = !Qt.colorEqual(hex, value);
        value = hex;
        if (changed)
            accepted(value);
    }

    implicitWidth: Math.max(swatches.implicitWidth, fieldRow.implicitWidth)
    implicitHeight: column.implicitHeight

    Accessible.role: Accessible.Grouping
    Accessible.name: qsTr("Color")

    onValueChanged: {
        preview.color = value;
        if (!field.activeFocus)
            field.text = hexOf(value);
    }
    Component.onCompleted: {
        preview.color = value;
        field.text = hexOf(value);
    }

    Column {
        id: column

        width: parent.width
        spacing: Theme.spacingSm

        Row {
            id: swatches

            spacing: Theme.spacingSm

            Repeater {
                id: repeater

                model: picker.presets

                delegate: T.AbstractButton {
                    id: swatch

                    required property int index
                    required property string modelData

                    readonly property bool selected: picker.selectedIndex === index

                    function moveTo(target: int) {
                        const count = repeater.count;
                        if (count === 0)
                            return;
                        const item = repeater.itemAt((target + count) % count);
                        if (item)
                            item.forceActiveFocus(Qt.TabFocusReason);
                    }

                    implicitWidth: Theme.controlHeightSmall
                    implicitHeight: Theme.controlHeightSmall
                    padding: 0
                    hoverEnabled: true
                    // Swatch colors are data, not theme colors: dim them when disabled.
                    opacity: enabled ? 1 : picker.disabledOpacity
                    // Roving tab stop: only one swatch is in the tab order.
                    focusPolicy: index === picker.tabIndex ? Qt.StrongFocus : Qt.ClickFocus

                    Accessible.role: Accessible.RadioButton
                    Accessible.name: index < picker.presetNames.length && picker.presetNames[index]
                                     ? picker.presetNames[index] : modelData
                    Accessible.description: modelData
                    Accessible.checkable: true
                    Accessible.checked: selected

                    onClicked: picker.pick(modelData)

                    Keys.onLeftPressed: moveTo(index - 1)
                    Keys.onRightPressed: moveTo(index + 1)
                    Keys.onUpPressed: moveTo(index - 1)
                    Keys.onDownPressed: moveTo(index + 1)
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Home)
                            moveTo(0);
                        else if (event.key === Qt.Key_End)
                            moveTo(repeater.count - 1);
                        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                            picker.pick(modelData);
                        else
                            return;
                        event.accepted = true;
                    }

                    contentItem: Item {
                        OsIcon {
                            anchors.centerIn: parent
                            visible: swatch.selected
                            name: "check"
                            size: Theme.iconSizeSmall
                            color: Theme.textOn(chip.color)
                        }
                    }

                    background: Rectangle {
                        id: chip

                        radius: width / 2
                        color: swatch.modelData
                        border.width: Theme.borderWidth
                        border.color: swatch.hovered || swatch.down ? Theme.text : Theme.border

                        Behavior on border.color {
                            ColorAnimation {
                                duration: Theme.durationFast
                            }
                        }

                        OsFocusRing {
                            target: swatch
                            baseRadius: chip.radius
                        }
                    }
                }
            }
        }

        Row {
            id: fieldRow

            spacing: Theme.spacingSm

            Rectangle {
                id: preview

                // Shows what the field holds while it is valid, else the current value. It is
                // set from signal handlers: a binding on the field's text loops at start-up.
                width: Theme.controlHeight
                height: Theme.controlHeight
                radius: Theme.radiusControl
                opacity: picker.enabled ? 1 : picker.disabledOpacity
                border.width: Theme.borderWidth
                border.color: Theme.border
                Accessible.ignored: true
            }

            OsTextField {
                id: field

                width: Theme.controlHeight * 3.5
                font.family: Theme.monoFontFamily
                maximumLength: 7
                placeholderText: qsTr("#RRGGBB")
                error: !acceptableInput && !activeFocus
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                validator: RegularExpressionValidator {
                    regularExpression: /#?[0-9A-Fa-f]{6}/
                }

                Accessible.name: qsTr("Hex color code")
                Accessible.description: qsTr("Six hexadecimal digits, for example %1. Press Enter to apply.")
                                        .arg(picker.presets.length > 0 ? picker.presets[0] : "")

                Keys.onEscapePressed: event => {
                    if (text !== picker.hexOf(picker.value)) {
                        text = picker.hexOf(picker.value);
                        event.accepted = true;
                    } else {
                        event.accepted = false;
                    }
                }
                onTextChanged: preview.color = picker.isHex(text) ? picker.normalized(text) : picker.value
                onEditingFinished: {
                    if (!acceptableInput)
                        return;
                    const hex = picker.normalized(text);
                    text = hex;
                    picker.pick(hex);
                }
                onActiveFocusChanged: {
                    if (!activeFocus && acceptableInput)
                        text = picker.hexOf(picker.value);
                }
            }
        }
    }
}
