pragma ComponentBehavior: Bound

// What a file is (Sprint 8): its path, kind, size, modification time, permissions, owner and group
// ids, and where a link points.
// Functions: show(entry) (an SftpBrowser.entryJson object).
import QtQuick
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property var entry: ({})
    readonly property var rows: [
        { label: qsTr("Path"), value: entry.path ?? "" },
        { label: qsTr("Kind"), value: kindText(entry) },
        { label: qsTr("Size"), value: entry.dirLike ? "" : qsTr("%1 (%2 bytes)").arg(FileFormat.size(entry.size ?? 0)).arg((entry.size ?? 0).toLocaleString(Qt.locale(), "f", 0)) },
        { label: qsTr("Modified"), value: FileFormat.time(entry.modified) },
        { label: qsTr("Permissions"), value: entry.permissionsText ? qsTr("%1 (%2)").arg(entry.permissionsText).arg((entry.mode ?? 0).toString(8).padStart(4, "0")) : "" },
        { label: qsTr("Owner and group"), value: entry.owner ?? "" },
        { label: qsTr("Points to"), value: entry.linkTarget ?? "" }
    ].filter(row => row.value && row.value.length > 0)

    function kindText(item) {
        switch (item.kind) {
        case "dir":
            return qsTr("Folder");
        case "symlink":
            return item.targetKind === "dir" ? qsTr("Link to a folder") : item.targetKind === "file" ? qsTr("Link to a file") : qsTr("Broken link");
        case "file":
            return qsTr("File");
        default:
            return qsTr("Special file");
        }
    }

    function show(item) {
        entry = item;
        open();
    }

    title: entry.name ?? ""
    acceptText: qsTr("Close")
    showReject: false

    Column {
        width: Math.min(Theme.spacingXxl * 14, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingSm

        Repeater {
            model: dialog.rows

            delegate: OsFormRow {
                id: propertyRow

                required property var modelData

                width: parent.width
                label: modelData.label

                TextEdit {
                    width: parent.width
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextEdit.WrapAnywhere
                    textFormat: TextEdit.PlainText
                    text: propertyRow.modelData.value
                    color: Theme.text
                    selectionColor: Theme.selection
                    selectedTextColor: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize
                    Accessible.role: Accessible.StaticText
                    Accessible.name: text
                }
            }
        }
    }
}
