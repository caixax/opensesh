pragma ComponentBehavior: Bound

// The quick snippet picker (Sprint 10, Ctrl+Shift+Space): a search over the snippets' names,
// folders, tags and text; Enter runs the chosen one in the focused pane (in the broadcast panes
// while the tab broadcasts), asking for its variables first. Escape closes.
// Functions: show().
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

T.Popup {
    id: picker

    readonly property Item shell: WindowRegistry.mainShell
    readonly property var all: JSON.parse(Snippets.list || "[]")
    property string query: ""
    readonly property var results: {
        const words = query.toLowerCase().split(/\s+/).filter(word => word.length > 0);
        return all.filter(snippet => {
            const text = [snippet.name, snippet.folder, snippet.tags.join(" "), snippet.description, snippet.text]
                .join(" ").toLowerCase();
            return words.every(word => text.indexOf(word) >= 0);
        });
    }

    function show() {
        query = "";
        search.text = "";
        list.currentIndex = 0;
        open();
        search.forceActiveFocus();
    }

    function runCurrent() {
        const snippet = results[list.currentIndex];
        close();
        if (snippet && shell)
            shell.runSnippet(snippet.id, "auto");
    }

    parent: T.Overlay.overlay
    x: parent ? Math.round((parent.width - width) / 2) : 0
    y: Theme.spacingXxl * 2
    width: parent ? Math.min(Theme.spacingXxl * 14, parent.width - 2 * Theme.spacingLg) : 0
    height: Math.min(implicitHeight, parent ? parent.height - y - Theme.spacingLg : implicitHeight)
    implicitHeight: content.implicitHeight + topPadding + bottomPadding
    padding: Theme.spacingSm
    modal: true
    focus: true

    T.Overlay.modal: Rectangle {
        color: Theme.scrim
    }

    background: Rectangle {
        radius: Theme.radiusCard
        color: Theme.surface
        border.width: Theme.borderWidth
        border.color: Theme.border
    }

    contentItem: ColumnLayout {
        id: content

        spacing: Theme.spacingSm

        OsSearchField {
            id: search

            Layout.fillWidth: true
            placeholderText: qsTr("Run a snippet…")
            Accessible.name: qsTr("Search snippets")
            onTextChanged: {
                picker.query = text;
                list.currentIndex = 0;
            }
            Keys.onDownPressed: list.incrementCurrentIndex()
            Keys.onUpPressed: list.decrementCurrentIndex()
            Keys.onReturnPressed: picker.runCurrent()
            Keys.onEnterPressed: picker.runCurrent()
        }

        ListView {
            id: list

            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(contentHeight, Theme.rowHeight * 9)
            clip: true
            model: picker.results
            highlightMoveDuration: 0
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Snippets")

            T.ScrollBar.vertical: OsScrollBar {}

            delegate: OsListRow {
                required property var modelData
                required property int index

                width: ListView.view.width
                iconName: modelData.macro ? "play" : "scroll-text"
                text: modelData.name
                subtitle: modelData.macro ? qsTr("%n step(s)", "", modelData.steps.length) : modelData.text.split("\n")[0]
                trailingText: modelData.folder
                highlighted: ListView.isCurrentItem
                onClicked: {
                    list.currentIndex = index;
                    picker.runCurrent();
                }
            }
        }

        OsText {
            Layout.fillWidth: true
            Layout.margins: Theme.spacingSm
            visible: picker.results.length === 0
            text: picker.all.length === 0 ? qsTr("No snippets yet: make one in the Snippets view.") : qsTr("No snippet matches.")
            muted: true
            wrapMode: Text.Wrap
        }
    }
}
