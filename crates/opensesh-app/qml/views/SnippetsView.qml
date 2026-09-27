pragma ComponentBehavior: Bound

// Snippets view (PLAN §5.4, Sprint 10): the snippets and macros, by folder or tag, with a
// search over names, folders, tags, descriptions and text. Each row runs its snippet (asking
// where, and the values of its variables), and has a menu to edit, duplicate or delete it.
// Keys in the list: Up/Down, Enter (run), F2 (edit), Delete, Ctrl+N (new).
// Functions: smokeSteps(smoke).
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: view

    readonly property Item shell: WindowRegistry.mainShell
    readonly property var all: JSON.parse(Snippets.list || "[]")
    readonly property var folders: JSON.parse(Snippets.folders || "[]")
    readonly property var tags: JSON.parse(Snippets.tags || "[]")
    // "" (all), "folder:<path>" or "tag:<name>".
    property string scope: ""
    property string query: ""
    readonly property var results: {
        const words = query.toLowerCase().split(/\s+/).filter(word => word.length > 0);
        return all.filter(snippet => {
            if (scope.startsWith("folder:")) {
                const folder = scope.slice(7);
                if (snippet.folder !== folder && !snippet.folder.startsWith(folder + "/"))
                    return false;
            } else if (scope.startsWith("tag:")) {
                const tag = scope.slice(4).toLowerCase();
                if (!snippet.tags.some(each => each.toLowerCase() === tag))
                    return false;
            }
            const text = [snippet.name, snippet.folder, snippet.tags.join(" "), snippet.description, snippet.text]
                .join(" ").toLowerCase();
            return words.every(word => text.indexOf(word) >= 0);
        });
    }

    function preview(snippet) {
        if (snippet.macro)
            return qsTr("%n step(s)", "", snippet.steps.length);
        const lines = snippet.text.split("\n").filter(line => line.length > 0);
        return lines.length > 1 ? qsTr("%1 (+%n more line(s))", "", lines.length - 1).arg(lines[0]) : (lines[0] ?? "");
    }

    function askDelete(id) {
        deleteDialog.snippetId = id;
        deleteDialog.open();
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.spacingLg
        spacing: Theme.spacingMd
        visible: view.all.length > 0

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingSm

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0

                OsText {
                    text: qsTr("Snippets")
                    size: "title"
                }
                OsText {
                    text: qsTr("%n snippet(s)", "", Snippets.count)
                    muted: true
                }
            }

            OsSearchField {
                Layout.preferredWidth: Theme.spacingXxl * 7
                placeholderText: qsTr("Search snippets")
                Accessible.name: qsTr("Search snippets")
                onTextChanged: view.query = text
            }

            OsButton {
                text: qsTr("New snippet")
                iconName: "plus"
                variant: "primary"
                onClicked: view.shell.editSnippet("")
            }
        }

        OsText {
            Layout.fillWidth: true
            visible: Snippets.readOnly
            text: qsTr("snippets.toml comes from a newer OpenSesh or can't be read: changes here are not saved.")
            color: Theme.warning
            wrapMode: Text.Wrap
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: Theme.spacingLg

            // Folders and tags.
            Flickable {
                Layout.preferredWidth: Theme.spacingXxl * 5
                Layout.fillHeight: true
                contentHeight: scopes.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: scopes

                    width: parent.width
                    spacing: 0

                    OsListRow {
                        width: parent.width
                        iconName: "scroll-text"
                        text: qsTr("All snippets")
                        trailingText: String(view.all.length)
                        selected: view.scope === ""
                        onClicked: view.scope = ""
                    }

                    OsText {
                        width: parent.width
                        visible: view.folders.length > 0
                        topPadding: Theme.spacingMd
                        leftPadding: Theme.controlPadding
                        text: qsTr("Folders")
                        muted: true
                        size: "small"
                    }

                    Repeater {
                        model: view.folders

                        delegate: OsListRow {
                            required property string modelData

                            width: parent ? parent.width : 0
                            leftPadding: Theme.controlPadding + Theme.spacingMd * (modelData.split("/").length - 1)
                            iconName: "folder"
                            text: modelData.split("/").pop()
                            selected: view.scope === "folder:" + modelData
                            onClicked: view.scope = "folder:" + modelData
                        }
                    }

                    OsText {
                        width: parent.width
                        visible: view.tags.length > 0
                        topPadding: Theme.spacingMd
                        leftPadding: Theme.controlPadding
                        text: qsTr("Tags")
                        muted: true
                        size: "small"
                    }

                    Repeater {
                        model: view.tags

                        delegate: OsListRow {
                            required property var modelData

                            width: parent ? parent.width : 0
                            iconName: "tag"
                            text: modelData.tag
                            trailingText: String(modelData.count)
                            selected: view.scope === "tag:" + modelData.tag
                            onClicked: view.scope = "tag:" + modelData.tag
                        }
                    }
                }
            }

            ListView {
                id: list

                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                spacing: Theme.spacingXs
                model: view.results
                activeFocusOnTab: true
                keyNavigationEnabled: true
                Accessible.role: Accessible.List
                Accessible.name: qsTr("Snippets")

                T.ScrollBar.vertical: OsScrollBar {}

                Keys.onPressed: event => {
                    const snippet = view.results[currentIndex];
                    if (event.key === Qt.Key_N && event.modifiers & Qt.ControlModifier) {
                        view.shell.editSnippet("");
                    } else if (!snippet) {
                        return;
                    } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                        view.shell.runSnippet(snippet.id, "ask");
                    } else if (event.key === Qt.Key_F2) {
                        view.shell.editSnippet(snippet.id);
                    } else if (event.key === Qt.Key_Delete) {
                        view.askDelete(snippet.id);
                    } else {
                        return;
                    }
                    event.accepted = true;
                }

                delegate: Rectangle {
                    id: row

                    required property var modelData
                    required property int index

                    width: ListView.view ? ListView.view.width : 0
                    implicitHeight: rowLayout.implicitHeight + 2 * Theme.spacingSm
                    radius: Theme.radiusControl
                    color: ListView.isCurrentItem && list.activeFocus ? Theme.selection : rowArea.containsMouse ? Theme.hover : Theme.surface
                    border.width: Theme.borderWidth
                    border.color: Theme.border

                    MouseArea {
                        id: rowArea

                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onClicked: mouse => {
                            list.currentIndex = row.index;
                            list.forceActiveFocus(Qt.MouseFocusReason);
                            if (mouse.button === Qt.RightButton)
                                rowMenu.popup();
                        }
                        onDoubleClicked: view.shell.editSnippet(row.modelData.id)
                    }

                    RowLayout {
                        id: rowLayout

                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.leftMargin: Theme.spacingMd
                        anchors.rightMargin: Theme.spacingSm
                        spacing: Theme.spacingMd

                        OsIcon {
                            name: row.modelData.macro ? "play" : "scroll-text"
                            size: Theme.iconSize
                            color: Theme.textMuted
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spacingXs / 2

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spacingSm

                                OsText {
                                    text: row.modelData.name
                                    font.weight: Font.DemiBold
                                    elide: Text.ElideRight
                                    Layout.maximumWidth: rowLayout.width / 2
                                }
                                OsText {
                                    visible: row.modelData.folder.length > 0
                                    text: row.modelData.folder
                                    muted: true
                                    size: "small"
                                }
                                Repeater {
                                    model: row.modelData.tags

                                    delegate: OsTag {
                                        required property string modelData

                                        text: modelData
                                    }
                                }
                                Item {
                                    Layout.fillWidth: true
                                }
                                OsText {
                                    visible: row.modelData.shortcut.length > 0
                                    text: row.modelData.shortcut
                                    muted: true
                                    size: "small"
                                }
                            }

                            OsText {
                                Layout.fillWidth: true
                                text: view.preview(row.modelData)
                                font.family: row.modelData.macro ? Theme.fontFamily : Theme.monoFontFamily
                                muted: true
                                size: "small"
                                elide: Text.ElideRight
                            }
                        }

                        OsButton {
                            text: qsTr("Run…")
                            iconName: "play"
                            onClicked: view.shell.runSnippet(row.modelData.id, "ask")
                        }

                        OsIconButton {
                            id: moreButton

                            iconName: "ellipsis"
                            toolTip: qsTr("More")
                            onClicked: {
                                list.currentIndex = row.index;
                                rowMenu.popup(moreButton, 0, moreButton.height);
                            }
                        }
                    }

                    OsContextMenu {
                        id: rowMenu

                        OsMenuItem {
                            text: qsTr("Run…")
                            iconName: "play"
                            shortcutText: qsTr("Enter")
                            onTriggered: view.shell.runSnippet(row.modelData.id, "ask")
                        }
                        OsMenuItem {
                            text: qsTr("Edit…")
                            iconName: "pencil"
                            shortcutText: qsTr("F2")
                            onTriggered: view.shell.editSnippet(row.modelData.id)
                        }
                        OsMenuItem {
                            text: qsTr("Duplicate")
                            iconName: "copy"
                            onTriggered: Snippets.duplicate(row.modelData.id)
                        }
                        OsMenuSeparator {}
                        OsMenuItem {
                            text: qsTr("Delete…")
                            iconName: "trash-2"
                            shortcutText: qsTr("Del")
                            onTriggered: view.askDelete(row.modelData.id)
                        }
                    }
                }
            }
        }
    }

    OsEmptyState {
        anchors.fill: parent
        visible: view.all.length === 0
        iconName: "scroll-text"
        title: qsTr("No snippets")
        description: qsTr("Save the commands you run often, with variables like {{host}}, and run them in one terminal or in many at once. Record a macro from a terminal's menu.")

        OsButton {
            text: qsTr("New snippet")
            iconName: "plus"
            variant: "primary"
            onClicked: view.shell.editSnippet("")
        }
    }

    OsDialog {
        id: deleteDialog

        property string snippetId: ""
        readonly property var snippet: view.all.find(entry => entry.id === snippetId) ?? null

        title: qsTr("Delete this snippet?")
        acceptText: qsTr("Delete")
        dangerous: true
        onAccepted: Snippets.remove(snippetId)

        OsText {
            width: Math.min(Theme.spacingXxl * 12, deleteDialog.maxWidth - deleteDialog.leftPadding - deleteDialog.rightPadding)
            text: deleteDialog.snippet ? deleteDialog.snippet.name : ""
            wrapMode: Text.Wrap
        }
    }
}
