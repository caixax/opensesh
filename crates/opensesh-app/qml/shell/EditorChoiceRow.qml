pragma ComponentBehavior: Bound

// A labelled drop-down of the host or group editor, bound to one field of its draft (see
// EditorTextRow). With `inherit`, the first entry leaves the field unset and says what is
// inherited instead ("Inherit: On (from Production)").
//   editor: var         the dialog (see EditorTextRow)
//   path: string        the draft field
//   options: var        [{text, value}]; texts already translated
//   inherit: bool       offer "Inherit" first (default true)
//   inheritKey: string  the inherited key (default: `path`)
//   field: string       the validation field (default: `path`)
import QtQuick
import cc.caixa.opensesh

OsFormRow {
    id: row

    // The dialog (a Popup, so not an Item).
    required property var editor
    required property string path
    property var options: []
    property bool inherit: true
    property string inheritKey: path
    property string field: path
    readonly property string inheritLabel: {
        if (editor.revision < 0)
            return "";
        const info = editor.inheritedInfo(inheritKey);
        if (!info || info.value === null || info.value === undefined)
            return qsTr("Inherit");
        const option = options.find(entry => entry.value === info.value);
        const text = option ? option.text : String(info.value);
        return info.origin === "group" ? qsTr("Inherit: %1 (from %2)").arg(text).arg(info.groupName)
                                       : qsTr("Inherit: %1").arg(text);
    }
    readonly property var entries: inherit ? [{ text: inheritLabel, value: undefined }].concat(options) : options

    function show() {
        const value = editor.value(path);
        const index = entries.findIndex(entry => entry.value === value);
        combo.currentIndex = index >= 0 ? index : 0;
    }

    width: parent ? parent.width : implicitWidth
    errorText: editor.revision >= 0 ? editor.errorText(field) : ""

    OsComboBox {
        id: combo

        width: parent.width
        enabled: !row.editor.readOnly
        model: row.entries
        textRole: "text"
        Accessible.name: row.label

        onActivated: index => row.editor.setValue(row.path, row.entries[index].value)
    }

    // The inherited entry's text changes with the group; keep the same entry selected.
    onEntriesChanged: Qt.callLater(show)

    Connections {
        target: row.editor

        function onLoaded() {
            row.show();
        }
    }

    Component.onCompleted: show()
}
