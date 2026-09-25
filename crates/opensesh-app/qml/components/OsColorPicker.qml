// Color chooser (see docs/design/components.md): a row of preset swatches, a preview chip and a
// `#RRGGBB` field.
//   value: color            the chosen opaque color (defaults to Theme.accent). A pick sets it,
//                           which breaks a plain binding: keep it tied to its source with a
//                           Binding element, as Settings > Appearance does
//   presets: var            swatch colors, as "#RRGGBB" strings (defaults to Theme.accentPresets)
//   presetNames: var        accessible names of the swatches, index for index with `presets`
//                           (a missing name falls back to the hex code)
//   showDefault: bool       adds a first swatch for the default color (default false)
//   defaultColor: color     that swatch's color (defaults to Theme.defaultAccent, which follows
//                           the light or dark mode)
//   defaultName: string     its accessible name (defaults to qsTr("Sesame (default)"))
//   defaultSelected: bool   the default is the current choice: its swatch shows as selected and
//                           the preview and the field show `defaultColor`. The picker never sets
//                           it; bind it to the setting, e.g. `AppSettings.accent === "default"`
//   shownColor: color       read-only; `defaultColor` while the default is selected, else `value`
//   signal accepted(color value)  the user picked a preset swatch or confirmed an edited hex code
//   signal defaultPicked()        the user picked the default swatch (store "default", not a
//                                 color, so the choice keeps following the mode)
// Keyboard: Tab reaches the selected swatch, the arrow keys (and Home/End) move between
// swatches, Space or Enter picks one; in the field, Enter confirms and Escape reverts (a second
// Escape reaches an enclosing popup).
pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: picker

    property color value: Theme.accent
    property var presets: Theme.accentPresets
    // Index for index with Theme.accentPresets.
    property var presetNames: [qsTr("Amber"), qsTr("Terracotta"), qsTr("Rose"), qsTr("Lavender"),
        qsTr("Blue"), qsTr("Teal"), qsTr("Green")]
    property bool showDefault: false
    property color defaultColor: Theme.defaultAccent
    property string defaultName: qsTr("Sesame (default)")
    property bool defaultSelected: false

    readonly property color shownColor: showDefault && defaultSelected ? defaultColor : value
    readonly property real disabledOpacity: 0.4

    // Swatches before the presets (the default one).
    readonly property int presetOffset: showDefault ? 1 : 0
    // Swatch that Tab lands on: the selected one, else the first.
    readonly property int tabIndex: Math.max(0, selectedIndex)
    readonly property int selectedIndex: {
        if (showDefault && defaultSelected)
            return 0;
        for (let i = 0; i < presets.length; ++i) {
            if (Qt.colorEqual(presets[i], value))
                return i + presetOffset;
        }
        return -1;
    }

    signal accepted(color value)
    signal defaultPicked

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

    // Always reports an explicit pick, even of the color `value` already holds: `value` may be
    // stale when the owner changed the color elsewhere.
    function pick(hex: string) {
        value = hex;
        accepted(value);
    }

    implicitWidth: Math.max(swatches.implicitWidth, fieldRow.implicitWidth)
    implicitHeight: column.implicitHeight

    Accessible.role: Accessible.Grouping
    Accessible.name: qsTr("Color")

    onShownColorChanged: {
        preview.color = shownColor;
        if (!field.activeFocus)
            field.text = hexOf(shownColor);
    }
    Component.onCompleted: {
        preview.color = shownColor;
        field.text = hexOf(shownColor);
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

                // A count, not the colors: the default swatch's color changes with the mode, and
                // a new model would recreate the swatches under the keyboard focus.
                model: picker.presetOffset + picker.presets.length

                delegate: T.AbstractButton {
                    id: swatch

                    required property int index

                    readonly property bool isDefault: index < picker.presetOffset
                    readonly property int presetIndex: index - picker.presetOffset
                    readonly property string hex: isDefault ? picker.hexOf(picker.defaultColor)
                                                            : String(picker.presets[presetIndex]).toUpperCase()
                    readonly property bool selected: picker.selectedIndex === index

                    function moveTo(target: int) {
                        const count = repeater.count;
                        if (count === 0)
                            return;
                        const item = repeater.itemAt((target + count) % count);
                        if (item)
                            item.forceActiveFocus(Qt.TabFocusReason);
                    }

                    function choose() {
                        if (isDefault)
                            picker.defaultPicked();
                        else
                            picker.pick(hex);
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
                    Accessible.name: isDefault ? picker.defaultName
                                               : presetIndex < picker.presetNames.length && picker.presetNames[presetIndex]
                                                 ? picker.presetNames[presetIndex] : hex
                    Accessible.description: hex
                    Accessible.checkable: true
                    Accessible.checked: selected

                    onClicked: choose()

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
                            choose();
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
                        color: swatch.hex
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

                // Shows what the field holds while it is valid, else the shown color. It is set
                // from signal handlers: a binding on the field's text loops at start-up.
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

                // Take Escape before an enclosing popup does when there is an edit to revert:
                // the popup would close, and losing the focus would then apply the edit.
                Keys.onShortcutOverride: event => {
                    if (event.key === Qt.Key_Escape && text !== picker.hexOf(picker.shownColor))
                        event.accepted = true;
                }
                Keys.onEscapePressed: event => {
                    if (text !== picker.hexOf(picker.shownColor)) {
                        text = picker.hexOf(picker.shownColor);
                        event.accepted = true;
                    } else {
                        event.accepted = false;
                    }
                }
                onTextChanged: preview.color = picker.isHex(text) ? picker.normalized(text) : picker.shownColor
                // Also runs when the field loses the focus: only an edit is a pick, so tabbing
                // through the field never turns the default into a fixed color.
                onEditingFinished: {
                    if (!acceptableInput)
                        return;
                    const hex = picker.normalized(text);
                    text = hex;
                    if (!Qt.colorEqual(hex, picker.shownColor))
                        picker.pick(hex);
                }
                onActiveFocusChanged: {
                    if (!activeFocus && acceptableInput)
                        text = picker.hexOf(picker.shownColor);
                }
            }
        }
    }
}
