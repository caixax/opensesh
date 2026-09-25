// Vertical navigation rail: a column of OsRailItem entries on `surface2`, with optional footer
// entries pinned to the bottom (e.g. settings). The rail is one Tab stop (roving focus): Up/Down
// move between entries (header and footer together), Home/End jump to the ends, Enter/Return or
// Space activate. Activating an entry sets `currentId` and emits `activated(id)`.
//   model: var           [{ id, text, iconName, enabled? }]; `text` must already be translated
//   footerModel: var     same shape, pinned to the bottom
//   currentId: string    id of the selected entry
//   showLabels: bool     shows the texts next to the icons (wider rail)
//   signal activated(string id)
import QtQuick
import cc.caixa.opensesh

FocusScope {
    id: rail

    property var model: []
    property var footerModel: []
    property string currentId: ""
    property bool showLabels: false

    // Entry that is in the Tab chain; follows the current entry until the arrows move it.
    property int focusIndex: -1

    signal activated(string id)

    readonly property int count: mainRepeater.count + footerRepeater.count
    readonly property int tabIndex: {
        if (focusIndex >= 0 && focusIndex < count)
            return focusIndex;
        const current = indexOfId(currentId);
        return current >= 0 ? current : 0;
    }

    implicitWidth: showLabels ? Theme.railWidthLabels : Theme.railWidth
    implicitHeight: mainColumn.implicitHeight + footerColumn.implicitHeight

    Accessible.role: Accessible.PageTabList
    Accessible.name: qsTr("Navigation")

    function itemAt(index) {
        if (index < 0)
            return null;
        if (index < mainRepeater.count)
            return mainRepeater.itemAt(index);
        return footerRepeater.itemAt(index - mainRepeater.count);
    }

    function entryAt(index) {
        const item = itemAt(index);
        return item ? item.modelData : null;
    }

    function indexOfId(id) {
        for (let i = 0; i < count; ++i) {
            const entry = entryAt(i);
            if (entry && entry.id === id)
                return i;
        }
        return -1;
    }

    function activate(index) {
        const entry = entryAt(index);
        if (!entry)
            return;
        const item = itemAt(index);
        if (item && !item.enabled)
            return;
        // Keep the focus on the activated entry when the rail has it (e.g. a click while
        // another entry is focused).
        if (item && !item.activeFocus && rail.activeFocus)
            item.forceActiveFocus(Qt.MouseFocusReason);
        focusIndex = index;
        currentId = entry.id;
        activated(entry.id);
    }

    // Focuses the first enabled entry from `index` in the direction of `step` (+1 or -1).
    function moveFocus(index, step) {
        for (let i = index; i >= 0 && i < count; i += step) {
            const item = itemAt(i);
            if (item && item.enabled) {
                item.forceActiveFocus(Qt.TabFocusReason);
                focusIndex = i;
                return;
            }
        }
    }

    onCurrentIdChanged: focusIndex = -1

    Keys.onUpPressed: event => {
        rail.moveFocus(rail.tabIndex - 1, -1);
        event.accepted = true;
    }
    Keys.onDownPressed: event => {
        rail.moveFocus(rail.tabIndex + 1, 1);
        event.accepted = true;
    }
    Keys.onPressed: event => {
        switch (event.key) {
        case Qt.Key_Home:
            rail.moveFocus(0, 1);
            break;
        case Qt.Key_End:
            rail.moveFocus(rail.count - 1, -1);
            break;
        case Qt.Key_Return:
        case Qt.Key_Enter:
            rail.activate(rail.tabIndex);
            break;
        default:
            return;
        }
        event.accepted = true;
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.surface2
    }

    // Edge line between the rail and the content.
    Rectangle {
        x: rail.LayoutMirroring.enabled ? 0 : parent.width - width
        width: Theme.borderWidth
        height: parent.height
        color: Theme.border
    }

    Column {
        id: mainColumn

        width: parent.width
        topPadding: Theme.spacingSm
        bottomPadding: Theme.spacingSm
        spacing: Theme.spacingXs

        Repeater {
            id: mainRepeater

            model: rail.model

            OsRailItem {
                required property var modelData
                required property int index

                readonly property int railIndex: index

                width: rail.width
                text: modelData.text !== undefined ? modelData.text : ""
                iconName: modelData.iconName !== undefined ? modelData.iconName : ""
                showLabel: rail.showLabels
                enabled: modelData.enabled !== false
                checked: modelData.id === rail.currentId
                focusPolicy: railIndex === rail.tabIndex || activeFocus ? Qt.TabFocus : Qt.NoFocus

                onClicked: rail.activate(railIndex)
            }
        }
    }

    Column {
        id: footerColumn

        anchors.bottom: parent.bottom
        width: parent.width
        topPadding: Theme.spacingSm
        bottomPadding: Theme.spacingSm
        spacing: Theme.spacingXs

        Repeater {
            id: footerRepeater

            model: rail.footerModel

            OsRailItem {
                required property var modelData
                required property int index

                readonly property int railIndex: index + mainRepeater.count

                width: rail.width
                text: modelData.text !== undefined ? modelData.text : ""
                iconName: modelData.iconName !== undefined ? modelData.iconName : ""
                showLabel: rail.showLabels
                enabled: modelData.enabled !== false
                checked: modelData.id === rail.currentId
                focusPolicy: railIndex === rail.tabIndex || activeFocus ? Qt.TabFocus : Qt.NoFocus

                onClicked: rail.activate(railIndex)
            }
        }
    }
}
