pragma ComponentBehavior: Bound

// The side panel's snippets (Sprint 10): the snippets, searchable, to run in the current
// terminal with a click (in the broadcast panes while the tab broadcasts). The Snippets view
// makes and edits them.
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: panel

    readonly property Item shell: WindowRegistry.mainShell
    readonly property var all: JSON.parse(Snippets.list || "[]")
    property string query: ""
    readonly property var results: {
        const words = query.toLowerCase().split(/\s+/).filter(word => word.length > 0);
        return all.filter(snippet => {
            const text = [snippet.name, snippet.folder, snippet.tags.join(" "), snippet.text].join(" ").toLowerCase();
            return words.every(word => text.indexOf(word) >= 0);
        });
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.spacingSm
        spacing: Theme.spacingSm
        visible: panel.all.length > 0

        OsSearchField {
            Layout.fillWidth: true
            placeholderText: qsTr("Search snippets")
            Accessible.name: qsTr("Search snippets")
            onTextChanged: panel.query = text
        }

        ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: panel.results
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Snippets")

            T.ScrollBar.vertical: OsScrollBar {}

            delegate: OsListRow {
                required property var modelData

                width: ListView.view.width
                iconName: modelData.macro ? "play" : "scroll-text"
                text: modelData.name
                subtitle: modelData.macro ? qsTr("%n step(s)", "", modelData.steps.length) : modelData.text.split("\n")[0]
                onClicked: panel.shell.runSnippet(modelData.id, "auto")
            }
        }

        OsButton {
            Layout.fillWidth: true
            text: qsTr("Manage snippets")
            iconName: "scroll-text"
            onClicked: panel.shell.showView("snippets")
        }
    }

    OsEmptyState {
        anchors.fill: parent
        visible: panel.all.length === 0
        iconName: "scroll-text"
        title: qsTr("No snippets")
        description: qsTr("Save the commands you run often, with variables, and run them here with a click.")

        OsButton {
            text: qsTr("New snippet")
            iconName: "plus"
            variant: "primary"
            onClicked: panel.shell.editSnippet("")
        }
    }
}
