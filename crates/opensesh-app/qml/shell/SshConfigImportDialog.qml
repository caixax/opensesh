pragma ComponentBehavior: Bound

// Import ~/.ssh/config (PLAN Sprint 5, ADR 0020): shows the hosts the file names (following its
// Include lines) and what was skipped, then either links the file (its hosts show read-only in
// their own list and follow the file as it changes) or copies them into a new group.
// Functions: show(path) ("" for ~/.ssh/config).
import QtQuick
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property var preview: ({ hosts: [], warnings: [] })
    property string mode: "link"

    function show(path) {
        pathField.text = path && path.length > 0 ? path : Hosts.defaultSshConfig();
        reload();
        mode = preview.linked ? "copy" : "link";
        open();
    }

    function reload() {
        preview = JSON.parse(Hosts.previewSshConfig(pathField.text) || "{}");
    }

    title: qsTr("Import ~/.ssh/config")
    acceptText: mode === "link" ? qsTr("Link") : qsTr("Import copies")
    acceptEnabled: (preview.hosts ?? []).length > 0 && !(mode === "link" && preview.linked)

    onAccepted: {
        const result = JSON.parse(Hosts.importSshConfig(pathField.text, mode, groupField.text) || "{}");
        if (result.added === undefined) {
            Toasts.show(qsTr("The hosts could not be imported."), "danger");
        } else if (mode === "link") {
            Toasts.show(qsTr("%n host(s) linked from %1.", "", result.added).arg(preview.source), "success");
        } else {
            Toasts.show(result.skipped > 0 ? qsTr("%n host(s) imported; %1 already saved were skipped.", "", result.added).arg(result.skipped)
                                           : qsTr("%n host(s) imported.", "", result.added), "success");
        }
    }

    ColumnLayout {
        implicitWidth: Math.min(Theme.spacingXxl * 16, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        width: implicitWidth
        spacing: Theme.spacingMd

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingSm

            OsTextField {
                id: pathField

                Layout.fillWidth: true
                Accessible.name: qsTr("File")
                onEditingFinished: dialog.reload()
            }

            OsButton {
                text: qsTr("Choose…")
                iconName: "folder-open"
                onClicked: fileDialog.open()
            }
        }

        OsText {
            Layout.fillWidth: true
            text: (dialog.preview.hosts ?? []).length > 0 ? qsTr("%n host(s) found.", "", dialog.preview.hosts.length)
                                                           : qsTr("No hosts found in this file.")
            muted: true
        }

        ListView {
            id: hostList

            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(contentHeight, Theme.rowHeight * 6)
            visible: count > 0
            clip: true
            model: dialog.preview.hosts ?? []
            boundsBehavior: Flickable.StopAtBounds
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Hosts in the file")

            delegate: OsListRow {
                id: importRow

                required property var modelData

                width: ListView.view.width
                text: modelData.alias
                subtitle: modelData.jump.length > 0 ? qsTr("%1 through %2").arg(modelData.target).arg(modelData.jump.join(", "))
                                                    : modelData.target
                iconName: "server"
                focusPolicy: Qt.NoFocus

                OsTag {
                    visible: importRow.modelData.saved
                    text: qsTr("Already saved")
                }
            }

            T.ScrollBar.vertical: OsScrollBar {}
        }

        SettingsNotice {
            Layout.fillWidth: true
            visible: (dialog.preview.warnings ?? []).length > 0
            kind: "info"
            title: qsTr("Skipped")
            lines: (dialog.preview.warnings ?? []).slice(0, 6).concat((dialog.preview.warnings ?? []).length > 6
                                                                      ? [qsTr("…and %n more.", "", dialog.preview.warnings.length - 6)] : [])
        }

        OsCheckBox {
            text: qsTr("Link the file: its hosts show read-only and follow it as it changes")
            checked: dialog.mode === "link"
            enabled: !dialog.preview.linked
            onToggled: dialog.mode = checked ? "link" : "copy"
        }

        OsText {
            Layout.fillWidth: true
            visible: dialog.preview.linked === true
            text: qsTr("This file is already linked.")
            muted: true
            size: "small"
        }

        OsFormRow {
            Layout.fillWidth: true
            visible: dialog.mode === "copy"
            label: qsTr("New group")

            OsTextField {
                id: groupField

                width: parent.width
                placeholderText: qsTr("~/.ssh/config")
                Accessible.name: qsTr("Name of the group for the copies")
            }
        }
    }

    FileDialog {
        id: fileDialog

        title: qsTr("Choose an OpenSSH config file")
        onAccepted: {
            pathField.text = Platform.localPath(selectedFile);
            dialog.reload();
        }
    }
}
