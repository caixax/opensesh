// A labelled text field of the host or group editor, bound to one field of its draft (a dotted
// `path` such as `user` or `ssh.keepalive_secs`). Empty means "not set here": the placeholder
// then shows the inherited value and where it comes from.
//   editor: var         the dialog: value(path), setValue(path, value), inheritedText(key),
//                       errorText(field), readOnly, revision (grows with each change), loaded()
//   path: string        the draft field
//   inheritKey: string  the inherited key shown in the placeholder (default: `path`; empty for
//                       none)
//   field: string       the validation field whose problem shows under it (default: `path`)
//   type: string        "text", "int" or "list" (comma-separated; `none` is an empty list)
//   placeholder: string placeholder when nothing is inherited
// Functions: focusField().
import QtQuick
import cc.caixa.opensesh

OsFormRow {
    id: row

    // The dialog (a Popup, so not an Item).
    required property var editor
    required property string path
    property string inheritKey: path
    property string field: path
    property string type: "text"
    property string placeholder: ""
    readonly property string inheritedText: editor.revision >= 0 && inheritKey.length > 0 ? editor.inheritedText(inheritKey) : ""

    function parse(text) {
        const trimmed = text.trim();
        if (trimmed.length === 0)
            return undefined;
        if (type === "int") {
            const number = Number(trimmed);
            return Number.isInteger(number) ? number : trimmed;
        }
        if (type === "list") {
            if (trimmed.toLowerCase() === "none")
                return [];
            return trimmed.split(",").map(item => item.trim()).filter(item => item.length > 0);
        }
        return text;
    }

    function focusField() {
        input.forceActiveFocus(Qt.OtherFocusReason);
    }

    function show() {
        const value = editor.value(path);
        if (value === undefined || value === null)
            input.text = "";
        else if (Array.isArray(value))
            input.text = value.length === 0 ? "none" : value.join(", "); // lint-qml: allow (the keyword typed for no jump hosts)
        else
            input.text = String(value);
    }

    width: parent ? parent.width : implicitWidth
    errorText: editor.revision >= 0 ? editor.errorText(field) : ""

    OsTextField {
        id: input

        width: parent.width
        readOnly: row.editor.readOnly
        error: row.errorText.length > 0
        placeholderText: row.inheritedText.length > 0 ? row.inheritedText : row.placeholder
        Accessible.name: row.label
        Accessible.description: row.inheritedText

        onTextEdited: row.editor.setValue(row.path, row.parse(text))
    }

    Connections {
        target: row.editor

        function onLoaded() {
            row.show();
        }
    }

    Component.onCompleted: show()
}
