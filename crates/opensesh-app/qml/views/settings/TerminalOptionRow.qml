// A row of Settings > Terminal for one profile option (PLAN §6.2): the label, the control
// (children go into the control slot and fill its width), and in a profile other than the
// default one, whether the value is its own or inherited. A button makes a value the profile
// sets inherit again (in the default profile: go back to OpenSesh's default).
//   page: Item       the SettingsTerminalPage (its `values`, `setKeys`, `isDefault`, `reset()`)
//   key: string      the option, e.g. "font_size"
//   value: var       read-only; the option's current value in the profile being edited
//   own: bool        read-only; this profile sets the value itself
//   note: string     optional muted text under the control (before the inherit note)
pragma ComponentBehavior: Bound

import QtQuick
import cc.caixa.opensesh

SettingsRow {
    id: row

    required property Item page
    required property string key
    property string note

    readonly property var value: page.values[key]
    readonly property bool own: page.setKeys.indexOf(key) >= 0
    readonly property bool showsInherit: !page.isDefault

    default property alias control: slot.data

    helpText: {
        const parts = [];
        if (note.length > 0)
            parts.push(note);
        if (showsInherit)
            parts.push(own ? qsTr("Set in this profile.") : qsTr("Inherited from the default profile."));
        return parts.join(" ");
    }

    Row {
        width: parent.width
        spacing: Theme.spacingSm

        Item {
            id: slot

            width: parent.width - (resetButton.visible ? resetButton.width + parent.spacing : 0)
            height: childrenRect.height
        }

        OsIconButton {
            id: resetButton

            anchors.verticalCenter: parent.verticalCenter
            visible: row.own && !row.page.readOnly
            iconName: "rotate-ccw"
            toolTip: row.showsInherit ? qsTr("Use the inherited value") : qsTr("Use OpenSesh's default")
            onClicked: row.page.reset(row.key)
        }
    }
}
