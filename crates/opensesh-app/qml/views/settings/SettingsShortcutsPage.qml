// Settings > Shortcuts (PLAN §6.4): every action with its keyboard shortcut, editable. Click a
// shortcut (or press Enter on it) and press the new combination; Backspace clears it. Clashes
// between actions are marked, and so are combinations terminal programs need (Ctrl+letter,
// bare keys), which the app should not take. Changes go to keybindings.toml.
pragma ComponentBehavior: Bound

import QtQuick
import cc.caixa.opensesh

SettingsPage {
    id: page

    property string filter: ""

    // Actions with their state, grouped by category in registry order; recomputed when a
    // shortcut or the action list changes.
    readonly property var rows: {
        const revision = Keybindings.revision;
        const actions = ActionRegistry.actions.filter(action => action.actionId.indexOf("debug.") !== 0);
        const counts = {};
        for (const action of actions) {
            const key = action.shortcut.toLowerCase();
            if (key.length > 0)
                counts[key] = (counts[key] || 0) + 1;
        }
        const query = filter.trim().toLowerCase();
        const out = [];
        for (const action of actions) {
            if (query.length > 0 && action.text.toLowerCase().indexOf(query) < 0 && action.shortcut.toLowerCase().indexOf(query) < 0)
                continue;
            out.push({
                action: action,
                custom: revision >= 0 && Keybindings.isCustom(action.actionId),
                conflict: action.shortcut.length > 0 && counts[action.shortcut.toLowerCase()] > 1,
                terminalKey: action.shortcut.length > 0 && Keybindings.takesTerminalKey(action.shortcut)
            });
        }
        return out;
    }

    readonly property int conflictCount: rows.filter(row => row.conflict).length

    title: qsTr("Shortcuts")
    description: qsTr("Every command and its keyboard shortcut. App shortcuts use Shift or Alt so terminal programs keep keys like Ctrl+A, Ctrl+R or Ctrl+K.")

    SettingsGroup {
        width: parent.width

        Row {
            width: parent.width
            spacing: Theme.spacingSm

            OsSearchField {
                width: parent.width - restoreButton.width - parent.spacing
                placeholderText: qsTr("Find a command or a shortcut")
                onTextChanged: page.filter = text
            }

            OsButton {
                id: restoreButton

                text: qsTr("Restore all defaults")
                iconName: "rotate-ccw"
                enabled: !Keybindings.readOnly
                onClicked: Keybindings.resetAll()
            }
        }

        OsText {
            width: parent.width
            visible: page.conflictCount > 0
            text: qsTr("Some shortcuts are used by more than one command: only one of them works.")
            color: Theme.warning
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignLeft
            Accessible.role: Accessible.AlertMessage
            Accessible.name: text
        }

        OsText {
            width: parent.width
            visible: Keybindings.readOnly
            text: qsTr("%1 comes from a newer OpenSesh or can't be read, so shortcuts can't be changed here.").arg(Keybindings.path)
            color: Theme.warning
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignLeft
        }

        Column {
            width: parent.width
            spacing: Theme.spacingXs

            Repeater {
                model: page.rows

                delegate: Column {
                    id: row

                    required property var modelData
                    required property int index

                    readonly property bool firstOfCategory: index === 0 || page.rows[index - 1].action.category !== modelData.action.category

                    width: parent.width
                    spacing: Theme.spacingXs

                    OsText {
                        visible: row.firstOfCategory
                        topPadding: row.index === 0 ? 0 : Theme.spacingMd
                        text: row.modelData.action.category
                        size: "small"
                        muted: true
                        font.weight: Font.DemiBold
                        Accessible.role: Accessible.Heading
                    }

                    Row {
                        width: parent.width
                        spacing: Theme.spacingSm

                        Column {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - capture.width - resetButton.width - 2 * parent.spacing
                            spacing: 0

                            OsText {
                                width: parent.width
                                text: row.modelData.action.text
                                elide: Text.ElideRight
                            }

                            OsText {
                                width: parent.width
                                visible: row.modelData.conflict || row.modelData.terminalKey
                                text: row.modelData.conflict ? qsTr("Also used by another command")
                                                             : qsTr("Terminal programs use this key too")
                                size: "small"
                                color: row.modelData.conflict ? Theme.danger : Theme.warning
                                elide: Text.ElideRight
                            }
                        }

                        OsKeybindCapture {
                            id: capture

                            anchors.verticalCenter: parent.verticalCenter
                            width: Theme.spacingXxl * 7
                            enabled: !Keybindings.readOnly
                            sequence: row.modelData.action.shortcut
                            Accessible.name: qsTr("Shortcut for %1").arg(row.modelData.action.text)
                            onSequenceEdited: sequence => Keybindings.setShortcut(row.modelData.action.actionId, sequence, row.modelData.action.defaultShortcut)
                        }

                        OsIconButton {
                            id: resetButton

                            anchors.verticalCenter: parent.verticalCenter
                            opacity: row.modelData.custom ? 1 : 0
                            enabled: row.modelData.custom && !Keybindings.readOnly
                            iconName: "rotate-ccw"
                            toolTip: row.modelData.action.defaultShortcut.length > 0
                                     ? qsTr("Back to the default (%1)").arg(row.modelData.action.defaultShortcut)
                                     : qsTr("Back to the default (none)")
                            onClicked: Keybindings.reset(row.modelData.action.actionId)
                        }
                    }
                }
            }
        }
    }
}
