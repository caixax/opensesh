// Visual editor of a terminal theme (PLAN §6.3): every color of the theme as a swatch; the picked
// swatch is edited with a color picker, and the sample shows the result at once. Saving a
// built-in theme creates a copy (built-in themes never change).
// Functions: openTheme(id).
pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property string themeId
    property bool builtin: false
    property var colors: null
    // The slot being edited: a key of `colors`, or "normal:3" / "bright:3".
    property string slot: "background"

    readonly property var slotNames: ({
            foreground: qsTr("Text"),
            background: qsTr("Background"),
            cursor: qsTr("Cursor"),
            cursorText: qsTr("Text under the cursor"),
            selectionBackground: qsTr("Selection"),
            selectionForeground: qsTr("Selected text"),
            matchBackground: qsTr("Search match"),
            matchForeground: qsTr("Search match text"),
            focusedMatchBackground: qsTr("Current match"),
            focusedMatchForeground: qsTr("Current match text")
        })
    readonly property var ansiNames: [qsTr("Black"), qsTr("Red"), qsTr("Green"), qsTr("Yellow"), qsTr("Blue"), qsTr("Magenta"), qsTr("Cyan"), qsTr("White")]
    readonly property var namedSlots: ["background", "foreground", "cursor", "cursorText", "selectionBackground", "selectionForeground", "matchBackground", "matchForeground", "focusedMatchBackground", "focusedMatchForeground"]

    function openTheme(id) {
        const themes = JSON.parse(TerminalProfiles.themes || "[]");
        const found = themes.find(theme => theme.id === id);
        if (!found)
            return;
        themeId = found.id;
        builtin = found.builtin;
        colors = JSON.parse(JSON.stringify(found.colors));
        nameField.text = found.builtin ? qsTr("%1 (edited)").arg(found.name) : found.name;
        slot = "background";
        open();
    }

    function slotLabel(key) {
        const parts = key.split(":");
        if (parts.length === 2)
            return parts[0] === "normal" ? ansiNames[Number(parts[1])] : qsTr("Bright %1").arg(ansiNames[Number(parts[1])].toLowerCase());
        return slotNames[key] !== undefined ? slotNames[key] : key;
    }

    function colorOf(key) {
        if (!colors)
            return "";
        const parts = key.split(":");
        if (parts.length === 2)
            return colors[parts[0]][Number(parts[1])];
        const value = colors[key];
        // An empty selected-text color keeps each character's own color.
        return value && value.length > 0 ? value : (key === "selectionForeground" ? colors.foreground : "");
    }

    function setColor(key, value) {
        const next = JSON.parse(JSON.stringify(colors));
        const parts = key.split(":");
        if (parts.length === 2)
            next[parts[0]][Number(parts[1])] = value;
        else
            next[key] = value;
        colors = next;
    }

    title: builtin ? qsTr("Edit a copy of this theme") : qsTr("Edit theme")
    acceptText: qsTr("Save")
    acceptEnabled: nameField.text.trim().length > 0

    onAccepted: {
        const id = TerminalProfiles.saveTheme(builtin ? "" : themeId, nameField.text.trim(), JSON.stringify(colors));
        if (id.length === 0)
            Toasts.show(qsTr("Could not save the theme."), "warning");
        else
            Toasts.show(qsTr("Theme saved."), "success");
    }

    Column {
        width: Math.min(Theme.spacingXxl * 22, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsFormRow {
            width: parent.width
            label: qsTr("Name")
            labelWidth: Theme.spacingXxl * 3

            OsTextField {
                id: nameField

                width: parent.width
                Accessible.name: qsTr("Theme name")
            }
        }

        ThemeSample {
            width: parent.width
            colors: dialog.colors
        }

        Flickable {
            id: flick

            width: parent.width
            height: Math.min(slotColumn.implicitHeight, Math.max(Theme.spacingXxl * 5, dialog.maxHeight - Theme.spacingXxl * 13))
            contentHeight: slotColumn.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            T.ScrollBar.vertical: OsScrollBar {}

            Column {
                id: slotColumn

                width: flick.width - Theme.spacingMd
                spacing: Theme.spacingSm

                OsText {
                    text: qsTr("Pick a color to change it.")
                    muted: true
                    size: "small"
                }

                Flow {
                    width: parent.width
                    spacing: Theme.spacingXs

                    Repeater {
                        model: dialog.namedSlots.concat([0, 1, 2, 3, 4, 5, 6, 7].map(i => "normal:" + i)).concat([0, 1, 2, 3, 4, 5, 6, 7].map(i => "bright:" + i))

                        delegate: T.AbstractButton {
                            id: swatch

                            required property string modelData

                            readonly property bool current: dialog.slot === modelData

                            width: Theme.spacingXxl * 4
                            height: Theme.controlHeight
                            focusPolicy: Qt.StrongFocus
                            Accessible.role: Accessible.RadioButton
                            Accessible.name: qsTr("%1: %2").arg(dialog.slotLabel(modelData)).arg(dialog.colorOf(modelData))
                            Accessible.checkable: true
                            Accessible.checked: current

                            onClicked: dialog.slot = modelData

                            background: Rectangle {
                                radius: Theme.radiusControl
                                color: swatch.current ? Theme.selection : swatch.hovered ? Theme.hover : Theme.surface2
                                border.width: swatch.current || swatch.visualFocus ? Theme.focusRingWidth : Theme.borderWidth
                                border.color: swatch.current || swatch.visualFocus ? Theme.accent : Theme.border
                            }

                            contentItem: Row {
                                spacing: Theme.spacingXs
                                leftPadding: Theme.spacingXs

                                Rectangle {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: Theme.iconSize
                                    height: Theme.iconSize
                                    radius: Theme.radiusSmall
                                    color: dialog.colorOf(swatch.modelData).length > 0 ? dialog.colorOf(swatch.modelData) : Theme.surface2
                                    border.width: Theme.borderWidth
                                    border.color: Theme.borderStrong
                                }

                                OsText {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: swatch.width - Theme.iconSize - 3 * Theme.spacingXs
                                    text: dialog.slotLabel(swatch.modelData)
                                    size: "small"
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }
                }
            }
        }

        OsColorPicker {
            id: picker

            width: parent.width
            presets: dialog.colors ? dialog.colors.normal.concat(dialog.colors.bright) : []
            Accessible.name: dialog.slotLabel(dialog.slot)
            onAccepted: picked => dialog.setColor(dialog.slot, picker.hexOf(picked))

            Binding on value {
                when: dialog.colors !== null
                value: dialog.colorOf(dialog.slot)
                restoreMode: Binding.RestoreNone
            }
        }
    }
}
