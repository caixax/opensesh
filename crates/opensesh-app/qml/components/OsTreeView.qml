// Tree view: a ListView over the visible rows of a flattened tree. Rows are inserted and removed
// as nodes expand and collapse, so the view keeps its delegates and scroll position.
// Keyboard: Up/Down/Home/End/PageUp/PageDown move the current row; Right expands, or moves to
// the first child; Left collapses, or moves to the parent (mirrored in RTL); Enter activates.
// Mouse: click selects, double-click toggles and activates, the chevron toggles.
//   nodes: var            [{ id, text, iconName, children: [...] }]; texts already translated
//   currentId: string     id of the current (selected) row; "" for none
//   rowHeight: real       height of a row
//   indentation: real     extra indent per depth level (Theme.spacingLg)
//   signal activated(string id)       Enter or double-click
//   signal currentChanged(string id)  `currentId` changed
//   expand(id), collapse(id), toggle(id), isExpanded(id), expandAll(), collapseAll()
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

ListView {
    id: tree

    property var nodes: []
    property string currentId: ""
    property real rowHeight: Theme.controlHeightSmall + Theme.spacingXs
    property real indentation: Theme.spacingLg

    // id -> true for expanded nodes (replaced, never mutated, so bindings update).
    property var expandedState: ({})
    // id -> { node, depth, parentId } for every node, visible or not.
    property var nodeInfo: ({})
    // The last input was the keyboard: shows the focus ring on the current row. ListView has no
    // focus reason, so any focus that doesn't come from a click on a row counts as keyboard.
    property bool keyboardActive: false
    readonly property bool showFocus: activeFocus && keyboardActive
    property bool mouseFocusing: false
    property bool syncing: false

    signal activated(string id)
    signal currentChanged(string id)

    implicitWidth: Theme.spacingXxl * 8
    implicitHeight: Theme.spacingXxl * 8
    clip: true
    activeFocusOnTab: true
    keyNavigationEnabled: false
    boundsBehavior: Flickable.StopAtBounds
    currentIndex: -1
    highlightFollowsCurrentItem: false

    Accessible.role: Accessible.Tree

    model: ListModel {
        id: rows
    }

    function hasChildren(node) {
        return !!node && Array.isArray(node.children) && node.children.length > 0;
    }

    function isExpanded(id) {
        return expandedState[id] === true;
    }

    function setExpanded(id, value) {
        const state = Object.assign({}, expandedState);
        if (value)
            state[id] = true;
        else
            delete state[id];
        expandedState = state;
    }

    function indexOfId(id) {
        for (let i = 0; i < rows.count; ++i) {
            if (rows.get(i).nodeId === id)
                return i;
        }
        return -1;
    }

    function isDescendant(id, ancestorId) {
        let info = nodeInfo[id];
        while (info && info.parentId !== "") {
            if (info.parentId === ancestorId)
                return true;
            info = nodeInfo[info.parentId];
        }
        return false;
    }

    function visibleRows(list, depth, parentId, out) {
        if (!Array.isArray(list))
            return out;
        for (const node of list) {
            const id = String(node.id);
            out.push({
                nodeId: id,
                label: node.text !== undefined ? String(node.text) : "",
                iconName: node.iconName !== undefined ? String(node.iconName) : "",
                depth: depth,
                parentId: parentId,
                hasChildren: hasChildren(node)
            });
            if (isExpanded(id))
                visibleRows(node.children, depth + 1, id, out);
        }
        return out;
    }

    function rebuild() {
        const info = {};
        const walk = (list, depth, parentId) => {
            if (!Array.isArray(list))
                return;
            for (const node of list) {
                const id = String(node.id);
                info[id] = { node: node, depth: depth, parentId: parentId };
                walk(node.children, depth + 1, id);
            }
        };
        walk(nodes, 0, "");
        nodeInfo = info;

        syncing = true;
        rows.clear();
        for (const row of visibleRows(nodes, 0, "", []))
            rows.append(row);
        currentIndex = indexOfId(currentId);
        syncing = false;
    }

    function expand(id) {
        id = String(id);
        if (isExpanded(id))
            return;
        setExpanded(id, true);
        const info = nodeInfo[id];
        const index = indexOfId(id);
        // Unknown or hidden (an ancestor is collapsed): the state is used when it shows up.
        if (!info || index < 0)
            return;
        const children = visibleRows(info.node.children, info.depth + 1, id, []);
        syncing = true;
        for (let i = 0; i < children.length; ++i)
            rows.insert(index + 1 + i, children[i]);
        // The current node may have just become visible.
        currentIndex = indexOfId(currentId);
        syncing = false;
    }

    function collapse(id) {
        id = String(id);
        if (!isExpanded(id))
            return;
        const moveCurrent = isDescendant(currentId, id);
        setExpanded(id, false);
        const index = indexOfId(id);
        if (index < 0)
            return;
        const depth = rows.get(index).depth;
        let end = index + 1;
        while (end < rows.count && rows.get(end).depth > depth)
            ++end;
        syncing = true;
        if (end > index + 1)
            rows.remove(index + 1, end - index - 1);
        currentIndex = indexOfId(currentId);
        syncing = false;
        if (moveCurrent)
            currentId = id;
    }

    function toggle(id) {
        if (isExpanded(String(id)))
            collapse(id);
        else
            expand(id);
    }

    function expandAll() {
        const state = {};
        for (const id in nodeInfo) {
            if (hasChildren(nodeInfo[id].node))
                state[id] = true;
        }
        expandedState = state;
        rebuild();
    }

    function collapseAll() {
        if (currentId.length > 0 && nodeInfo[currentId]) {
            let root = currentId;
            while (nodeInfo[root] && nodeInfo[root].parentId !== "")
                root = nodeInfo[root].parentId;
            currentId = root;
        }
        expandedState = {};
        rebuild();
    }

    function moveTo(index) {
        if (rows.count === 0)
            return;
        currentIndex = Math.max(0, Math.min(rows.count - 1, index));
        positionViewAtIndex(currentIndex, ListView.Contain);
    }

    onNodesChanged: rebuild()
    Component.onCompleted: rebuild()

    onCurrentIndexChanged: {
        if (!syncing && currentIndex >= 0 && currentIndex < rows.count)
            currentId = rows.get(currentIndex).nodeId;
    }
    onCurrentIdChanged: {
        const index = indexOfId(currentId);
        if (index !== currentIndex) {
            syncing = true;
            currentIndex = index;
            syncing = false;
        }
        currentChanged(currentId);
    }
    onActiveFocusChanged: {
        if (activeFocus)
            keyboardActive = !mouseFocusing;
    }

    function focusFromMouse() {
        keyboardActive = false;
        mouseFocusing = true;
        forceActiveFocus(Qt.MouseFocusReason);
        mouseFocusing = false;
    }

    Keys.onPressed: event => {
        const row = currentIndex >= 0 && currentIndex < rows.count ? rows.get(currentIndex) : null;
        const forward = LayoutMirroring.enabled ? Qt.Key_Left : Qt.Key_Right;
        const backward = LayoutMirroring.enabled ? Qt.Key_Right : Qt.Key_Left;
        const page = Math.max(1, Math.floor(height / rowHeight) - 1);
        switch (event.key) {
        case Qt.Key_Up:
            moveTo(currentIndex < 0 ? 0 : currentIndex - 1);
            break;
        case Qt.Key_Down:
            moveTo(currentIndex + 1);
            break;
        case Qt.Key_Home:
            moveTo(0);
            break;
        case Qt.Key_End:
            moveTo(rows.count - 1);
            break;
        case Qt.Key_PageUp:
            moveTo(currentIndex - page);
            break;
        case Qt.Key_PageDown:
            moveTo(currentIndex + page);
            break;
        case forward:
            if (row && row.hasChildren) {
                if (isExpanded(row.nodeId))
                    moveTo(currentIndex + 1);
                else
                    expand(row.nodeId);
            }
            break;
        case backward:
            if (row) {
                if (row.hasChildren && isExpanded(row.nodeId))
                    collapse(row.nodeId);
                else if (row.parentId !== "")
                    moveTo(indexOfId(row.parentId));
            }
            break;
        case Qt.Key_Return:
        case Qt.Key_Enter:
            if (row)
                activated(row.nodeId);
            break;
        default:
            return;
        }
        keyboardActive = true;
        event.accepted = true;
    }

    delegate: T.ItemDelegate {
        id: row

        required property int index
        required property string nodeId
        required property string label
        required property string iconName
        required property int depth
        required property bool hasChildren

        readonly property bool expanded: tree.expandedState[nodeId] === true
        readonly property bool current: ListView.isCurrentItem
        readonly property color inkColor: enabled ? Theme.text : Theme.textDisabled
        readonly property real indent: Theme.spacingXs + depth * tree.indentation

        width: ListView.view.width
        height: tree.rowHeight
        leftPadding: mirrored ? Theme.spacingSm : indent
        rightPadding: mirrored ? indent : Theme.spacingSm
        topPadding: 0
        bottomPadding: 0
        spacing: Theme.spacingXs
        hoverEnabled: true
        focusPolicy: Qt.NoFocus

        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSize

        Accessible.role: Accessible.TreeItem
        Accessible.name: label
        Accessible.selected: current
        Accessible.description: hasChildren ? (expanded ? qsTr("Expanded") : qsTr("Collapsed")) : ""

        onClicked: {
            tree.currentId = nodeId;
            tree.focusFromMouse();
        }
        onDoubleClicked: {
            if (hasChildren)
                tree.toggle(nodeId);
            tree.activated(nodeId);
        }

        contentItem: Item {
            Row {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width
                spacing: row.spacing

                // Chevron column, also present on leaves so labels line up per depth.
                Item {
                    id: chevron

                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.iconSizeSmall + Theme.spacingXs
                    height: tree.rowHeight

                    OsIcon {
                        anchors.centerIn: parent
                        visible: row.hasChildren
                        name: row.expanded ? "chevron-down" : row.mirrored ? "chevron-left" : "chevron-right"
                        size: Theme.iconSizeSmall
                        color: row.enabled ? Theme.textMuted : Theme.textDisabled
                    }

                    MouseArea {
                        anchors.fill: parent
                        enabled: row.hasChildren
                        onClicked: {
                            tree.toggle(row.nodeId);
                            tree.focusFromMouse();
                        }
                    }
                }

                OsIcon {
                    id: nodeIcon

                    anchors.verticalCenter: parent.verticalCenter
                    visible: row.iconName.length > 0
                    name: row.iconName
                    size: Theme.iconSizeSmall
                    color: row.current && row.enabled ? Theme.accentFg : row.enabled ? Theme.textMuted : Theme.textDisabled
                }

                OsText {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.max(0, parent.width - chevron.width - parent.spacing
                                       - (nodeIcon.visible ? nodeIcon.width + parent.spacing : 0))
                    leftPadding: nodeIcon.visible ? Theme.spacingXs : 0
                    text: row.label
                    font: row.font
                    color: row.inkColor
                    horizontalAlignment: Text.AlignLeft
                }
            }
        }

        background: Rectangle {
            radius: Theme.radiusControl
            color: row.current ? Theme.selection : "transparent"

            Behavior on color {
                ColorAnimation {
                    duration: Theme.durationFast
                }
            }

            Rectangle {
                anchors.fill: parent
                radius: parent.radius
                visible: row.enabled
                color: row.down ? Theme.pressed : row.hovered ? Theme.hover : "transparent"

                Behavior on color {
                    ColorAnimation {
                        duration: Theme.durationFast
                    }
                }
            }

            OsFocusRing {
                anchors.margins: 0
                target: tree
                baseRadius: Theme.radiusControl - Theme.focusRingWidth - gap
                visible: tree.showFocus && row.current
            }
        }
    }

    T.ScrollBar.vertical: T.ScrollBar {
        id: scrollBar

        implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                                implicitContentWidth + leftPadding + rightPadding)
        implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                                 implicitContentHeight + topPadding + bottomPadding)
        padding: Theme.borderWidth * 2
        minimumSize: orientation === Qt.Horizontal ? height / width : width / height
        policy: T.ScrollBar.AsNeeded
        visible: policy !== T.ScrollBar.AlwaysOff && size < 1.0

        contentItem: Rectangle {
            implicitWidth: Theme.spacingXs + Theme.borderWidth * 2
            implicitHeight: implicitWidth
            radius: width / 2
            color: scrollBar.pressed ? Theme.textMuted : Theme.borderStrong
            opacity: scrollBar.active || scrollBar.hovered ? 1 : 0.5

            Behavior on opacity {
                NumberAnimation {
                    duration: Theme.durationFast
                }
            }
        }
    }
}
