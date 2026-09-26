pragma ComponentBehavior: Bound

// Command palette (PLAN §5.5, §6.4): a modal popup near the top of the window with a search field
// and the fuzzy-filtered list of ActionRegistry actions (icon, title, muted category and shortcut).
// Up/Down/PageUp/PageDown move the selection, Enter runs it, Escape clears the query and then
// closes; a click runs an entry. Disabled actions are not listed. With an empty query, the actions
// run most recently come first. The popup closes before the action runs, so the focus is back
// where it was when the action starts (and an action that opens another popup keeps it).
//   topOffset: real            distance from the top of the window (default Theme.spacingXxl * 2)
//   preferredWidth: real       width (560 logical px at scale 1), capped to the window
//   maxVisibleRows: int        rows shown before the list scrolls (default 9)
//   shortcutText: var          function(action) -> string shown for the action's shortcut;
//                              defaults to the portable text (`action.shortcut`)
//   extraResults: var          optional function(query) -> [{action: {text, category, iconName,
//                              shortcut, enabled, actionId}, run: function}], listed after the
//                              actions while there is a query (e.g. "Connect to <host>")
//   query: string              read-only; the current search text
//   resultCount: int           read-only; entries currently listed
// Functions: openWith(text) opens with a prefilled query; setQuery(text); runCurrent().
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.Popup {
    id: control

    property real topOffset: Theme.spacingXxl * 2
    property real preferredWidth: Theme.spacingXs * 140
    property int maxVisibleRows: 9
    property var shortcutText: action => action.shortcut

    readonly property string query: field.text
    readonly property int resultCount: list.count

    // Action ids, most recently run first (this session only).
    property var recent: []
    property string pendingActionId: ""
    property var pendingRun: null
    property var extraResults: null
    readonly property OsFocusReturn focusReturn: OsFocusReturn {
        popup: control
    }

    readonly property var results: {
        const found = ActionRegistry.search(field.text).filter(entry => entry.action.enabled);
        if (field.text.length > 0 && typeof extraResults === "function")
            return found.concat(extraResults(field.text));
        if (field.text.length > 0 || recent.length === 0)
            return found;
        const rank = id => {
            const at = recent.indexOf(id);
            return at < 0 ? recent.length : at;
        };
        // Array.prototype.sort is stable, so the rest keeps the search order (alphabetical).
        return found.sort((a, b) => rank(a.action.actionId) - rank(b.action.actionId));
    }

    function openWith(text) {
        open();
        setQuery(text);
    }

    function setQuery(text) {
        field.text = text;
        field.cursorPosition = field.length;
    }

    function run(index) {
        const entry = results[index];
        if (!entry || !entry.action.enabled)
            return;
        pendingActionId = entry.action.actionId;
        pendingRun = entry.run ?? null;
        close();
    }

    function runCurrent() {
        run(list.currentIndex);
    }

    parent: T.Overlay.overlay
    x: parent ? Math.round((parent.width - width) / 2) : 0
    y: parent ? Math.round(Math.min(topOffset, Math.max(0, parent.height - height) / 2)) : 0
    width: parent ? Math.max(0, Math.min(preferredWidth, parent.width - 2 * Theme.spacingXl)) : preferredWidth

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    modal: true
    focus: true
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside
    padding: Theme.spacingSm

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize

    onAboutToShow: {
        focusReturn.save();
        pendingActionId = "";
        pendingRun = null;
        field.clear();
        list.currentIndex = 0;
    }
    onOpened: field.forceActiveFocus(Qt.PopupFocusReason)
    onClosed: {
        focusReturn.restore();
        const actionId = pendingActionId;
        const run = pendingRun;
        pendingActionId = "";
        pendingRun = null;
        if (typeof run === "function")
            run();
        else if (actionId.length > 0)
            ActionRegistry.trigger(actionId);
    }

    Connections {
        target: ActionRegistry

        function onRan(actionId) {
            control.recent = [actionId].concat(control.recent.filter(id => id !== actionId)).slice(0, 8);
        }
    }

    // Opening fades in; closing is instant so the chosen action runs at once.
    enter: Transition {
        NumberAnimation {
            property: "opacity"
            from: 0
            to: 1
            duration: Theme.durationFast
        }
    }

    contentItem: Column {
        spacing: Theme.spacingSm

        Accessible.role: Accessible.Dialog
        Accessible.name: qsTr("Command palette")

        OsSearchField {
            id: field

            width: parent.width
            placeholderText: qsTr("Type a command")
            Accessible.name: qsTr("Search commands")

            onTextChanged: list.currentIndex = 0
            onAccepted: control.runCurrent()

            Keys.onUpPressed: event => {
                list.decrementCurrentIndex();
                event.accepted = true;
            }
            Keys.onDownPressed: event => {
                list.incrementCurrentIndex();
                event.accepted = true;
            }
            Keys.onPressed: event => {
                const page = Math.max(1, control.maxVisibleRows - 1);
                if (event.key === Qt.Key_PageUp)
                    list.currentIndex = Math.max(0, list.currentIndex - page);
                else if (event.key === Qt.Key_PageDown)
                    list.currentIndex = Math.min(list.count - 1, list.currentIndex + page);
                else
                    return;
                event.accepted = true;
            }
        }

        ListView {
            id: list

            readonly property real rowHeight: Theme.rowHeight
            // Room left under the popup's top edge in the window.
            readonly property real availableHeight: control.parent
                                                    ? control.parent.height - control.y - field.height
                                                      - 2 * Theme.spacingXl
                                                    : contentHeight

            width: parent.width
            height: Math.max(0, Math.min(contentHeight, control.maxVisibleRows * rowHeight, availableHeight))
            visible: count > 0
            clip: true
            model: control.results
            currentIndex: 0
            keyNavigationWraps: true
            boundsBehavior: Flickable.StopAtBounds
            highlightMoveDuration: 0

            Accessible.role: Accessible.List
            Accessible.name: qsTr("Commands")

            onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)

            delegate: OsListRow {
                id: row

                required property var modelData
                required property int index

                readonly property string keys: modelData.action.shortcut.length > 0
                                               ? control.shortcutText(modelData.action) : ""

                width: ListView.view.width
                text: modelData.action.text
                iconName: modelData.action.iconName.length > 0 ? modelData.action.iconName : "command"
                trailingText: modelData.action.category
                highlighted: ListView.isCurrentItem
                // The search field keeps the keyboard focus.
                focusPolicy: Qt.NoFocus
                Accessible.description: keys.length > 0 ? qsTr("%1, shortcut %2").arg(modelData.action.category).arg(keys)
                                                        : modelData.action.category

                onClicked: control.run(index)

                Rectangle {
                    visible: row.keys.length > 0
                    implicitWidth: keysText.implicitWidth + 2 * Theme.spacingSm
                    implicitHeight: keysText.implicitHeight + Theme.spacingXs
                    radius: Theme.radiusSmall
                    color: Theme.surface2
                    border.width: Theme.borderWidth
                    border.color: Theme.border

                    OsText {
                        id: keysText

                        anchors.centerIn: parent
                        text: row.keys
                        size: "small"
                        muted: true
                        Accessible.ignored: true
                    }
                }
            }

            T.ScrollBar.vertical: OsScrollBar {}
        }

        OsText {
            width: parent.width
            height: Theme.rowHeight
            visible: list.count === 0
            text: qsTr("No matching commands")
            muted: true
            horizontalAlignment: Text.AlignHCenter
        }
    }

    background: Rectangle {
        implicitWidth: Theme.spacingXs * 140
        color: Theme.surface
        radius: Theme.radiusCard
        border.color: Theme.borderStrong
        border.width: Theme.borderWidth
    }

    T.Overlay.modal: Rectangle {
        color: Theme.scrim

        Behavior on opacity {
            NumberAnimation {
                duration: Theme.durationFast
            }
        }
    }

    T.Overlay.modeless: Rectangle {
        color: Theme.scrim
    }
}
