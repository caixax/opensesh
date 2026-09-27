// Asks for a name (Sprint 8): a new folder, a new file, a new name, or a new symbolic link and
// where it points. Names can't be empty, `.`, `..` or contain a slash.
// Functions: show(kind, name) (`kind`: "folder", "file", "rename" or "link").
// Signals: chosen(kind, name, target).
import QtQuick
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property string kind: "folder"
    readonly property string name: nameField.text.trim()
    readonly property bool valid: name.length > 0 && name !== "." && name !== ".." && name.indexOf("/") < 0
                                  && (kind !== "link" || targetField.text.trim().length > 0)

    signal chosen(string kind, string name, string target)

    function show(what, current) {
        kind = what;
        nameField.text = current ?? "";
        targetField.text = "";
        open();
        nameField.forceActiveFocus();
        // Renaming selects the name without its extension, as file managers do.
        const dot = nameField.text.lastIndexOf(".");
        if (what === "rename" && dot > 0)
            nameField.select(0, dot);
        else
            nameField.selectAll();
    }

    title: {
        switch (kind) {
        case "file":
            return qsTr("New file");
        case "rename":
            return qsTr("Rename");
        case "link":
            return qsTr("New symbolic link");
        default:
            return qsTr("New folder");
        }
    }
    acceptText: kind === "rename" ? qsTr("Rename") : qsTr("Create")
    acceptEnabled: valid

    onAccepted: dialog.chosen(dialog.kind, dialog.name, targetField.text.trim())

    Column {
        width: Math.min(Theme.spacingXxl * 12, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsFormRow {
            width: parent.width
            label: qsTr("Name")

            OsTextField {
                id: nameField

                width: parent.width
                Accessible.name: qsTr("Name")
                onAccepted: {
                    if (dialog.valid)
                        dialog.accept();
                }
            }
        }

        OsFormRow {
            width: parent.width
            visible: dialog.kind === "link"
            label: qsTr("Points to")
            helpText: qsTr("A path on the same side, absolute or relative to the link's folder.")

            OsTextField {
                id: targetField

                width: parent.width
                Accessible.name: qsTr("Points to")
                onAccepted: {
                    if (dialog.valid)
                        dialog.accept();
                }
            }
        }
    }
}
