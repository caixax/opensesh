pragma ComponentBehavior: Bound

// Saved workspaces (PLAN Sprint 4): save the tabs of every window (their split layouts, the
// profile and directory of each pane, tab names and colors) under a name, and open, rename or
// delete the saved ones (`workspaces/<id>.toml` in the config folder, see Workspaces). Opening
// adds the workspace's tabs to this window, with new shells in the saved directories; its other
// windows open as new windows. Saving under an existing name replaces that workspace.
// Functions: show(mode) ("save" focuses the name field, "open" the list).
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    readonly property var saved: JSON.parse(Workspaces.workspaces || "[]")
    readonly property real bodyWidth: Math.min(Theme.spacingXxl * 14, maxWidth - leftPadding - rightPadding)

    function show(mode) {
        nameField.text = "";
        open();
        if (mode === "save" || list.count === 0)
            nameField.forceActiveFocus(Qt.OtherFocusReason);
        else
            list.forceActiveFocus(Qt.OtherFocusReason);
    }

    function existingId(name) {
        const key = name.trim().toLowerCase();
        const match = saved.find(entry => entry.name.toLowerCase() === key);
        return match ? match.id : "";
    }

    function save() {
        const name = nameField.text.trim();
        if (name.length === 0)
            return;
        const workspace = WindowRegistry.capture();
        if (workspace.windows.length === 0) {
            Toasts.show(qsTr("There are no terminal tabs to save."), "warning");
            return;
        }
        const id = Workspaces.save(existingId(name), name, JSON.stringify(workspace));
        if (id.length > 0)
            Toasts.show(qsTr("Workspace \"%1\" saved.").arg(name), "success");
        else
            Toasts.show(qsTr("The workspace could not be saved. The log has the details."), "danger");
        nameField.text = "";
    }

    function openWorkspace(id) {
        const text = Workspaces.open(id);
        if (text.length === 0) {
            Toasts.show(qsTr("The workspace could not be opened. The log has the details."), "danger");
            return;
        }
        close();
        WindowRegistry.openWorkspace(JSON.parse(text), WindowRegistry.activeShell);
    }

    title: qsTr("Workspaces")
    acceptText: ""
    rejectText: qsTr("Close")

    Column {
        width: dialog.bodyWidth
        spacing: Theme.spacingMd

        OsText {
            width: parent.width
            text: qsTr("Save the tabs of every window, with their split panes, as a workspace, and open it again later.")
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignLeft
        }

        RowLayout {
            width: parent.width
            spacing: Theme.spacingSm

            OsTextField {
                id: nameField

                Layout.fillWidth: true
                placeholderText: qsTr("Workspace name")
                Accessible.name: qsTr("Name of the workspace to save")
                onAccepted: dialog.save()
            }

            OsButton {
                text: dialog.existingId(nameField.text).length > 0 ? qsTr("Replace") : qsTr("Save")
                iconName: "save"
                variant: "primary"
                enabled: nameField.text.trim().length > 0
                onClicked: dialog.save()
            }
        }

        OsText {
            width: parent.width
            visible: list.count === 0
            text: qsTr("No saved workspaces yet.")
            muted: true
        }

        ListView {
            id: list

            width: parent.width
            height: Math.min(contentHeight, Theme.rowHeight * 7)
            visible: count > 0
            clip: true
            model: dialog.saved
            boundsBehavior: Flickable.StopAtBounds
            keyNavigationEnabled: true
            highlightMoveDuration: 0
            activeFocusOnTab: true

            Accessible.role: Accessible.List
            Accessible.name: qsTr("Saved workspaces")

            Keys.onReturnPressed: {
                if (currentItem)
                    dialog.openWorkspace(currentItem.modelData.id);
            }
            Keys.onEnterPressed: {
                if (currentItem)
                    dialog.openWorkspace(currentItem.modelData.id);
            }
            Keys.onDeletePressed: {
                if (currentItem)
                    deleteDialog.ask(currentItem.modelData);
            }

            delegate: OsListRow {
                id: row

                required property var modelData
                required property int index

                width: ListView.view.width
                text: modelData.name
                subtitle: qsTr("%1, %2").arg(qsTr("%n tab(s)", "", modelData.tabs)).arg(qsTr("%n pane(s)", "", modelData.panes))
                iconName: "layout-grid"
                highlighted: ListView.isCurrentItem && list.activeFocus
                focusPolicy: Qt.NoFocus

                onClicked: dialog.openWorkspace(modelData.id)

                OsIconButton {
                    implicitWidth: Theme.controlHeightSmall
                    implicitHeight: Theme.controlHeightSmall
                    iconName: "pencil"
                    toolTip: qsTr("Rename")
                    onClicked: renameDialog.ask(row.modelData)
                }

                OsIconButton {
                    implicitWidth: Theme.controlHeightSmall
                    implicitHeight: Theme.controlHeightSmall
                    iconName: "trash-2"
                    toolTip: qsTr("Delete")
                    onClicked: deleteDialog.ask(row.modelData)
                }
            }

            T.ScrollBar.vertical: OsScrollBar {}
        }

        OsText {
            width: parent.width
            visible: list.count > 0
            text: qsTr("Opening adds the tabs to this window and starts new shells in the saved folders.")
            size: "small"
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignLeft
        }
    }

    OsDialog {
        id: renameDialog

        property string workspaceId

        function ask(entry) {
            workspaceId = entry.id;
            renameField.text = entry.name;
            open();
            renameField.forceActiveFocus(Qt.OtherFocusReason);
            renameField.selectAll();
        }

        title: qsTr("Rename workspace")
        acceptText: qsTr("Rename")
        acceptEnabled: renameField.text.trim().length > 0

        onAccepted: Workspaces.rename(workspaceId, renameField.text.trim())

        OsTextField {
            id: renameField

            width: Math.min(Theme.spacingXxl * 12, renameDialog.maxWidth - renameDialog.leftPadding - renameDialog.rightPadding)
            placeholderText: qsTr("Workspace name")
            onAccepted: {
                if (renameDialog.acceptEnabled)
                    renameDialog.accept();
            }
        }
    }

    OsDialog {
        id: deleteDialog

        property string workspaceId
        property string workspaceName

        function ask(entry) {
            workspaceId = entry.id;
            workspaceName = entry.name;
            open();
        }

        title: qsTr("Delete this workspace?")
        acceptText: qsTr("Delete")
        dangerous: true

        onAccepted: Workspaces.remove(workspaceId)

        OsText {
            width: Math.min(Theme.spacingXxl * 12, deleteDialog.maxWidth - deleteDialog.leftPadding - deleteDialog.rightPadding)
            text: qsTr("%1 is removed from the workspaces folder. Open tabs are not affected.").arg(deleteDialog.workspaceName)
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignLeft
        }
    }
}
