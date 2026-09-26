// Drop-down of terminal themes (the `themes` JSON of TerminalProfiles): built-in themes by name,
// the user's own marked as such.
//   themes: var         the parsed theme list
//   currentId: string   the selected theme's id
//   signal picked(string id)
// Set `Accessible.name` to the setting's label.
import QtQuick
import cc.caixa.opensesh

OsComboBox {
    id: box

    property var themes: []
    property string currentId

    signal picked(string id)

    textRole: "label"
    valueRole: "id"
    model: themes.map(theme => ({
                id: theme.id,
                label: theme.builtin ? theme.name : qsTr("%1 (yours)").arg(theme.name)
            }))
    currentIndex: {
        for (let i = 0; i < themes.length; ++i) {
            if (themes[i].id === currentId)
                return i;
        }
        return -1;
    }
    displayText: currentIndex >= 0 ? currentText : qsTr("Missing theme (%1)").arg(currentId)

    onActivated: index => {
        if (index >= 0 && index < themes.length)
            box.picked(themes[index].id);
    }
}
