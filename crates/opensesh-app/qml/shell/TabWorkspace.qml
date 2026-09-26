pragma ComponentBehavior: Bound

// Content of one terminal tab (PLAN Sprint 4): a binary tree of panes (TerminalPane), one
// terminal session each. The tree lives in `layout` (JSON, see the Layouts singleton); Rust
// computes the pane rectangles and the dividers, and the panes are laid out flat, keyed by pane
// id, so splitting, closing, swapping or resizing never recreates a terminal. Every tab stays
// alive while hidden, so its shells keep running.
//
// Dividers are dragged with the mouse (a double click puts them back in the middle); the
// keyboard resizes the focused pane (Alt+Shift+arrows). A maximized pane (Ctrl+Shift+Z) fills
// the tab while the others keep running underneath.
//
// Broadcast (MultiExec, Ctrl+Shift+B): what is typed in a receiving pane also goes to the other
// receiving panes (the Rust side encodes each key for each target's own modes). Every pane
// receives until it leaves (the chip in its corner or its menu). A paste into several panes asks
// once per broadcast; scrolling can follow along (`syncScroll`).
//
// `seed` (JSON) describes the tab when it opens: `{layout, focused, zoomed, panes: [{id, kind,
// profile, directory, fontZoom, highlightOn}], broadcast, broadcastExcluded, syncScroll,
// pasteConfirmed}`; empty for one new pane. A tab moved to another window takes its seed from
// `capture(true)` and attaches to the running sessions; a restored workspace brings new ids, so
// new shells start in the saved directories.
//   shell: Item            the AppShell (currentTabId, updateTab(), closeTabById(),
//                          focusInTabStrip(), shortcutText(), currentWorkspace)
//   tabId: int             the tab's id
//   seed: string           see above (model role)
//   startSession: bool     false: panes without a shell (screenshot runs)
//   edgeInset: real        room kept free at the right edge (a frameless window's resize grip)
//   layout: string         read-only outside; the split tree
//   focusedPane: int       read-only outside; the pane with the focus
//   zoomedPane: int        read-only outside; the maximized pane, 0 for none
//   focusedItem: TerminalPane  read-only; the focused pane's item
//   paneCount: int         read-only
//   broadcast: bool        read-only outside; broadcast is on in this tab
//   participants: var      read-only; ids of the receiving panes (empty while off)
//   syncScroll: bool       receiving panes scroll together
//   askingToPaste: bool    read-only; the paste confirmation is open
// Functions: focusPane(id), splitPane(id, axis) (0: the focused pane), closePane(id),
// moveFocus(direction), resizePane(direction), swapPane(direction), toggleZoom(id), equalize(),
// setRatio(pathKey, ratio), toggleBroadcast(), setPaneReceiving(id, on), capture(live),
// paneItem(id), focusTerminal().
import QtQuick
import cc.caixa.opensesh

Item {
    id: workspace

    required property Item shell
    required property int tabId
    required property string seed
    required property bool startSession
    property real edgeInset: 0

    property string layout: ""
    property int focusedPane: 0
    property int zoomedPane: 0
    property Item focusedItem: null
    property bool broadcast: false
    property var broadcastExcluded: []
    property bool syncScroll: false
    property bool pasteConfirmed: false

    readonly property var paneIds: layout.length > 0 ? JSON.parse(Layouts.panes(layout)) : []
    readonly property int paneCount: paneIds.length
    readonly property var geometry: JSON.parse(Layouts.geometry(layout))
    readonly property var participants: broadcast ? paneIds.filter(id => broadcastExcluded.indexOf(id) < 0) : []
    readonly property bool current: shell.currentTabId !== 0 && shell.currentTabId === tabId
    // Space between panes; the dividers sit in it.
    readonly property real gap: Theme.spacingXs
    readonly property string title: focusedItem ? focusedItem.terminal.title : ""
    // The keyboard resize step, as a share of the tab.
    readonly property real resizeStep: 0.05

    // The paste waiting for confirmation.
    property int pastePane: 0
    property bool pasteFromSelection: false
    readonly property bool askingToPaste: pasteDialog.visible

    function focusTerminal() {
        if (focusedItem)
            focusedItem.focusTerminal();
    }

    function paneItem(id) {
        for (let i = 0; i < panes.count; ++i) {
            const item = panes.itemAt(i);
            if (item && item.paneId === id)
                return item;
        }
        return null;
    }

    function rowOf(id) {
        for (let i = 0; i < paneModel.count; ++i) {
            if (paneModel.get(i).paneId === id)
                return i;
        }
        return -1;
    }

    // Reads `seed`; a missing or broken one gives a single new pane.
    function load() {
        let data = null;
        try {
            data = seed.length > 0 ? JSON.parse(seed) : null;
        } catch (error) {
            console.warn("TabWorkspace: a tab with a broken seed opens a new terminal:", error);
        }
        if (!data || !data.layout || !Array.isArray(data.panes) || data.panes.length === 0) {
            const id = TerminalSessions.allocateId();
            data = {
                layout: { pane: id },
                focused: id,
                panes: [{ id: id, profile: AppSettings.terminalProfile, directory: "" }]
            };
        }
        for (const pane of data.panes) {
            paneModel.append({
                paneId: pane.id,
                profile: pane.profile && pane.profile.length > 0 ? pane.profile : AppSettings.terminalProfile,
                directory: pane.directory || "",
                startZoom: pane.fontZoom || 0,
                startHighlight: pane.highlightOn !== false
            });
        }
        layout = JSON.stringify(data.layout);
        focusedPane = paneIds.indexOf(data.focused) >= 0 ? data.focused : (paneIds[0] ?? 0);
        zoomedPane = paneIds.indexOf(data.zoomed) >= 0 ? data.zoomed : 0;
        broadcast = data.broadcast === true;
        broadcastExcluded = Array.isArray(data.broadcastExcluded) ? data.broadcastExcluded : [];
        syncScroll = data.syncScroll === true;
        pasteConfirmed = data.pasteConfirmed === true;
        if (broadcast)
            shell.updateTab(tabId, "broadcasting", true);
        syncDividers();
        updateFocusedItem();
    }

    // The tab as a workspace entry (see Workspaces); `live` adds the broadcast state, for a tab
    // that moves to another window with its sessions.
    function capture(live) {
        const list = [];
        for (let i = 0; i < paneModel.count; ++i) {
            const row = paneModel.get(i);
            const item = paneItem(row.paneId);
            list.push({
                id: row.paneId,
                kind: "local",
                profile: row.profile,
                directory: item ? item.currentDirectory() : row.directory,
                fontZoom: item ? item.fontZoom : row.startZoom,
                highlightOn: item ? item.highlightOn : row.startHighlight
            });
        }
        const out = {
            focused: focusedPane,
            zoomed: zoomedPane,
            layout: JSON.parse(layout),
            panes: list
        };
        if (live) {
            out.broadcast = broadcast;
            out.broadcastExcluded = broadcastExcluded;
            out.syncScroll = syncScroll;
            out.pasteConfirmed = pasteConfirmed;
        }
        return out;
    }

    function updateFocusedItem() {
        const item = paneItem(focusedPane);
        if (focusedItem !== item)
            focusedItem = item;
    }

    // A pane got the keyboard focus (a click, or focusPane()).
    function setFocusedPane(id) {
        if (focusedPane !== id)
            focusedPane = id;
        updateFocusedItem();
    }

    function focusPane(id) {
        if (paneIds.indexOf(id) < 0)
            return;
        if (zoomedPane !== 0 && zoomedPane !== id)
            zoomedPane = 0;
        setFocusedPane(id);
        if (current && focusedItem)
            focusedItem.focusTerminal();
    }

    // axis: "horizontal" puts the new pane on the right, "vertical" below. It starts with the
    // profile, directory and zoom of the pane it splits. Returns the new pane's id, 0 on failure.
    function splitPane(id, axis) {
        const target = id > 0 ? id : focusedPane;
        const source = paneItem(target);
        const newId = TerminalSessions.allocateId();
        const next = Layouts.split(layout, target, axis, newId, true);
        if (next.length === 0)
            return 0;
        paneModel.append({
            paneId: newId,
            profile: source ? source.profile : AppSettings.terminalProfile,
            directory: source ? source.currentDirectory() : "",
            startZoom: source ? source.fontZoom : 0,
            startHighlight: source ? source.highlightOn : true
        });
        zoomedPane = 0;
        layout = next;
        focusPane(newId);
        return newId;
    }

    // Closes a pane and ends its session; the last pane closes the tab.
    function closePane(id) {
        if (paneIds.indexOf(id) < 0)
            return;
        if (paneCount <= 1) {
            shell.closeTabById(tabId);
            return;
        }
        const result = Layouts.close(layout, id);
        if (result.length === 0)
            return;
        const next = JSON.parse(result);
        const window = workspace.Window.window;
        const item = paneItem(id);
        const hadFocus = item !== null && window !== null && shell.isInside(window.activeFocusItem, item);
        // The shell ends in the background; the pane's item only lets go of it.
        TerminalSessions.close(id);
        if (zoomedPane === id)
            zoomedPane = 0;
        broadcastExcluded = broadcastExcluded.filter(pane => pane !== id);
        layout = JSON.stringify(next.layout);
        const row = rowOf(id);
        if (row >= 0)
            paneModel.remove(row);
        if (focusedPane === id) {
            setFocusedPane(next.focus);
            if (hadFocus || current)
                Qt.callLater(workspace.focusTerminal);
        } else {
            updateFocusedItem();
        }
    }

    function moveFocus(direction) {
        const id = Layouts.neighbor(layout, focusedPane, direction);
        if (id > 0)
            focusPane(id);
    }

    // Grows the focused pane toward `direction`; at the tab's edge, moves the opposite divider.
    function resizePane(direction) {
        const opposite = { left: "right", right: "left", up: "down", down: "up" };
        let next = Layouts.resize(layout, focusedPane, direction, resizeStep);
        if (next.length === 0)
            next = Layouts.resize(layout, focusedPane, opposite[direction], -resizeStep);
        if (next.length > 0)
            layout = next;
    }

    // Exchanges the focused pane with its neighbor in `direction`; the focus stays on it.
    function swapPane(direction) {
        const other = Layouts.neighbor(layout, focusedPane, direction);
        if (other <= 0)
            return;
        const next = Layouts.swap(layout, focusedPane, other);
        if (next.length > 0)
            layout = next;
    }

    function toggleZoom(id) {
        const target = id > 0 ? id : focusedPane;
        if (paneCount < 2)
            return;
        zoomedPane = zoomedPane === target ? 0 : target;
        focusPane(target);
    }

    function equalize() {
        layout = Layouts.equalize(layout);
    }

    // pathKey: a divider's path as JSON (from the geometry).
    function setRatio(pathKey, ratio) {
        const next = Layouts.setRatio(layout, pathKey, ratio);
        if (next.length > 0 && next !== layout)
            layout = next;
    }

    function toggleBroadcast() {
        broadcast = !broadcast;
        if (!broadcast) {
            // The next broadcast starts with every pane and asks again before pasting.
            broadcastExcluded = [];
            pasteConfirmed = false;
            syncScroll = false;
        }
        shell.updateTab(tabId, "broadcasting", broadcast);
    }

    function setPaneReceiving(id, on) {
        const list = broadcastExcluded.filter(pane => pane !== id);
        if (!on)
            list.push(id);
        broadcastExcluded = list;
    }

    function setPaneProfile(id, profile) {
        const row = rowOf(id);
        if (row >= 0)
            paneModel.setProperty(row, "profile", profile);
    }

    function paneActivity(id) {
        if (!current)
            shell.updateTab(tabId, "newOutput", true);
    }

    function paneBell(id) {
        shell.updateTab(tabId, "bellRang", true);
    }

    // A paste into several panes waits for this confirmation, once per broadcast.
    function confirmPaste(id, selection) {
        pastePane = id;
        pasteFromSelection = selection;
        pasteDialog.open();
    }

    // Closes the paste confirmation; `paste` true pastes as if confirmed.
    function answerPaste(paste) {
        if (paste)
            pasteDialog.accept();
        else
            pasteDialog.reject();
    }

    // Pixel box of a pane (the dependencies are arguments, so bindings follow them).
    function boxOf(id, geometry, zoomed, width, height) {
        if (zoomed !== 0)
            return zoomed === id ? Qt.rect(0, 0, width, height) : Qt.rect(0, 0, 0, 0);
        const eps = 1e-6;
        const half = gap / 2;
        for (const r of geometry.panes) {
            if (r.pane !== id)
                continue;
            const left = Math.round(r.x * width + (r.x > eps ? half : 0));
            const top = Math.round(r.y * height + (r.y > eps ? half : 0));
            const right = Math.round((r.x + r.width) * width - (r.x + r.width < 1 - eps ? half : 0));
            const bottom = Math.round((r.y + r.height) * height - (r.y + r.height < 1 - eps ? half : 0));
            return Qt.rect(left, top, Math.max(0, right - left), Math.max(0, bottom - top));
        }
        return Qt.rect(0, 0, 0, 0);
    }

    // Dividers stay the same items while only their ratios change, so a drag isn't interrupted.
    function syncDividers() {
        const list = zoomedPane !== 0 ? [] : geometry.dividers;
        let same = list.length === dividerModel.count;
        for (let i = 0; same && i < list.length; ++i)
            same = dividerModel.get(i).pathKey === JSON.stringify(list[i].path);
        if (!same)
            dividerModel.clear();
        for (let i = 0; i < list.length; ++i) {
            const d = list[i];
            const row = {
                pathKey: JSON.stringify(d.path),
                axis: d.axis,
                ax: d.area.x,
                ay: d.area.y,
                aw: d.area.width,
                ah: d.area.height,
                ratio: d.ratio
            };
            if (same)
                dividerModel.set(i, row);
            else
                dividerModel.append(row);
        }
    }

    function becameCurrent() {
        shell.updateTab(tabId, "newOutput", false);
        shell.updateTab(tabId, "bellRang", false);
        shell.currentWorkspace = workspace;
        // Keyboard users moving along the tab strip keep their place there.
        if (!shell.focusInTabStrip())
            Qt.callLater(() => {
                if (workspace.current)
                    workspace.focusTerminal();
            });
    }

    visible: current

    onCurrentChanged: {
        if (current)
            becameCurrent();
        else if (shell.currentWorkspace === workspace)
            shell.currentWorkspace = null;
    }
    onGeometryChanged: syncDividers()
    onZoomedPaneChanged: syncDividers()
    onTitleChanged: shell.updateTab(tabId, "title", title)
    Component.onCompleted: {
        load();
        if (current)
            becameCurrent();
    }
    Component.onDestruction: {
        if (shell.currentWorkspace === workspace)
            shell.currentWorkspace = null;
    }

    ListModel {
        id: paneModel
    }

    ListModel {
        id: dividerModel
    }

    Repeater {
        id: panes

        model: paneModel

        TerminalPane {
            id: paneDelegate

            required property real startZoom
            required property bool startHighlight
            readonly property rect box: workspace.boxOf(paneId, workspace.geometry, workspace.zoomedPane,
                                                         workspace.width, workspace.height)

            workspace: workspace
            startSession: workspace.startSession
            x: box.x
            y: box.y
            width: box.width
            height: box.height
            visible: box.width > 0 && box.height > 0
            edgeInset: box.x + box.width >= workspace.width - 1 ? workspace.edgeInset : 0
            fontZoom: startZoom
            highlightOn: startHighlight
        }

        onItemAdded: workspace.updateFocusedItem()
        onItemRemoved: workspace.updateFocusedItem()
    }

    Repeater {
        model: dividerModel

        Item {
            id: divider

            required property string pathKey
            required property string axis
            required property real ax
            required property real ay
            required property real aw
            required property real ah
            required property real ratio
            // A "horizontal" split puts its panes side by side: the divider is a vertical line.
            readonly property bool sideBySide: axis === "horizontal"
            readonly property real grab: workspace.gap + Theme.spacingXs
            readonly property real position: sideBySide ? (ax + aw * ratio) * workspace.width
                                                        : (ay + ah * ratio) * workspace.height
            readonly property bool active: dragArea.pressed || dragArea.containsMouse

            x: sideBySide ? Math.round(position - grab / 2) : Math.round(ax * workspace.width)
            y: sideBySide ? Math.round(ay * workspace.height) : Math.round(position - grab / 2)
            width: sideBySide ? grab : Math.round(aw * workspace.width)
            height: sideBySide ? Math.round(ah * workspace.height) : grab
            z: 5

            Accessible.role: Accessible.Separator
            Accessible.name: qsTr("Pane divider")

            // The gap between the panes, then the line in its middle.
            Rectangle {
                anchors.centerIn: parent
                width: divider.sideBySide ? workspace.gap : parent.width
                height: divider.sideBySide ? parent.height : workspace.gap
                color: Theme.bg
            }

            Rectangle {
                anchors.centerIn: parent
                width: divider.sideBySide ? (divider.active ? Theme.borderWidth * 2 : Theme.borderWidth) : parent.width
                height: divider.sideBySide ? parent.height : (divider.active ? Theme.borderWidth * 2 : Theme.borderWidth)
                color: divider.active ? Theme.accent : Theme.border
            }

            MouseArea {
                id: dragArea

                anchors.fill: parent
                hoverEnabled: true
                cursorShape: divider.sideBySide ? Qt.SplitHCursor : Qt.SplitVCursor

                onPositionChanged: mouse => {
                    if (!pressed || divider.aw <= 0 || divider.ah <= 0)
                        return;
                    const point = mapToItem(workspace, mouse.x, mouse.y);
                    const value = divider.sideBySide ? (point.x / workspace.width - divider.ax) / divider.aw
                                                     : (point.y / workspace.height - divider.ay) / divider.ah;
                    workspace.setRatio(divider.pathKey, value);
                }
                onDoubleClicked: workspace.setRatio(divider.pathKey, 0.5)
            }
        }
    }

    OsDialog {
        id: pasteDialog

        title: qsTr("Paste into %n panes?", "", workspace.participants.length)
        acceptText: qsTr("Paste")

        onAccepted: {
            workspace.pasteConfirmed = true;
            const item = workspace.paneItem(workspace.pastePane);
            if (!item)
                return;
            if (workspace.pasteFromSelection)
                item.terminal.pasteSelection();
            else
                item.terminal.paste();
        }
        onClosed: {
            const item = workspace.paneItem(workspace.pastePane);
            if (item && workspace.current)
                item.focusTerminal();
        }

        Column {
            width: Math.min(Theme.spacingXxl * 12, pasteDialog.maxWidth - pasteDialog.leftPadding - pasteDialog.rightPadding)

            OsText {
                width: parent.width
                text: qsTr("Broadcast is on, so the text goes to every receiving pane of this tab. You won't be asked again until broadcast is turned off.")
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                horizontalAlignment: Text.AlignLeft
            }
        }
    }
}
