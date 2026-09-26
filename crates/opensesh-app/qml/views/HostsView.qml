pragma ComponentBehavior: Bound

// Hosts view (PLAN §5.3, Sprint 5). Left, the lists: all hosts, Favorites, Recent, the hosts
// linked from ~/.ssh/config, hosts in no group, then the group tree with counts. Right, a search
// bar (fuzzy on name, address, user, tags and group), protocol and tag filters, the order, the
// cards/list toggle and "+ Host", then the hosts as cards or rows: icon, name, user@host:port,
// tags, favorite, and a dot while a session to the host is open.
//
// Selection: click, Ctrl+click toggles, Shift+click extends; the arrows move (Shift extends),
// Space toggles, Ctrl+A selects all. Enter connects, F2 edits, Delete deletes, the Menu key or a
// right click opens the host menu (connect, connect in a split, duplicate, edit, copy the ssh
// command, favorite, move to a group, delete). Dragging hosts onto a group moves them there;
// dragging a group onto another nests it.
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: view

    readonly property Item shell: WindowRegistry.mainShell
    property string scope: "all"
    property string protocolFilter: ""
    property string tagFilter: ""
    property string sort: "name"
    property bool cards: true
    property var results: []
    // id -> true
    property var selected: ({})
    property string anchorId: ""
    // How long the last search took (ms), for the smoke test.
    property real searchMs: 0
    readonly property var groups: JSON.parse(Hosts.groups || "[]")
    readonly property var tags: JSON.parse(Hosts.tags || "[]")
    readonly property int selectedCount: Object.keys(selected).length
    readonly property Item hostView: cards ? grid : list
    readonly property var scopeEntries: {
        const entries = [
            { kind: "scope", id: "all", text: qsTr("All hosts"), iconName: "server", count: Hosts.count },
            { kind: "scope", id: "favorites", text: qsTr("Favorites"), iconName: "star", count: Hosts.favoriteCount },
            { kind: "scope", id: "recent", text: qsTr("Recent"), iconName: "history", count: Hosts.recentCount }
        ];
        if (Hosts.linkedCount > 0)
            entries.push({ kind: "scope", id: "linked", text: qsTr("~/.ssh/config"), iconName: "file-text", count: Hosts.linkedCount });
        entries.push({ kind: "header", id: "", text: qsTr("Groups"), iconName: "", count: 0 });
        if (Hosts.ungroupedCount > 0 && view.groups.length > 0)
            entries.push({ kind: "scope", id: "ungrouped", text: qsTr("No group"), iconName: "folder", count: Hosts.ungroupedCount });
        for (const group of view.groups) {
            entries.push({
                kind: "group",
                id: group.id,
                text: group.name,
                iconName: "folder",
                count: group.total,
                depth: group.depth,
                color: group.color
            });
        }
        return entries;
    }

    function refresh() {
        const started = Date.now();
        results = JSON.parse(Hosts.search(search.text, scope, protocolFilter, tagFilter, sort) || "[]");
        // The views keep their delegates: only rows whose host changed are rewritten.
        const revision = Hosts.revision;
        for (let i = 0; i < results.length; ++i) {
            const host = results[i];
            if (i < resultModel.count) {
                const row = resultModel.get(i);
                if (row.key !== host.id || row.rev !== revision)
                    resultModel.set(i, { key: host.id, rev: revision, json: JSON.stringify(host) });
            } else {
                resultModel.append({ key: host.id, rev: revision, json: JSON.stringify(host) });
            }
        }
        if (resultModel.count > results.length)
            resultModel.remove(results.length, resultModel.count - results.length);
        searchMs = Date.now() - started;
        // Keep the selection on hosts that are still listed.
        const listed = {};
        for (const host of results)
            listed[host.id] = true;
        const kept = {};
        for (const id of Object.keys(selected)) {
            if (listed[id])
                kept[id] = true;
        }
        selected = kept;
    }

    function indexOf(id) {
        return results.findIndex(host => host.id === id);
    }

    function selectedIds() {
        return results.filter(host => selected[host.id]).map(host => host.id);
    }

    function selectOnly(id) {
        const next = {};
        if (id.length > 0)
            next[id] = true;
        selected = next;
        anchorId = id;
    }

    function toggle(id) {
        const next = Object.assign({}, selected);
        if (next[id])
            delete next[id];
        else
            next[id] = true;
        selected = next;
        anchorId = id;
    }

    function extendTo(id) {
        const from = indexOf(anchorId.length > 0 ? anchorId : id);
        const to = indexOf(id);
        if (from < 0 || to < 0) {
            selectOnly(id);
            return;
        }
        const next = {};
        for (let i = Math.min(from, to); i <= Math.max(from, to); ++i)
            next[results[i].id] = true;
        selected = next;
    }

    function selectAll() {
        const next = {};
        for (const host of results)
            next[host.id] = true;
        selected = next;
    }

    // A click with `modifiers` on the host at `index`.
    function clicked(index, modifiers) {
        const id = results[index].id;
        hostView.currentIndex = index;
        if (modifiers & Qt.ControlModifier)
            toggle(id);
        else if (modifiers & Qt.ShiftModifier)
            extendTo(id);
        else
            selectOnly(id);
        hostView.forceActiveFocus(Qt.MouseFocusReason);
    }

    function moveCurrent(index, modifiers) {
        if (results.length === 0)
            return;
        index = Math.max(0, Math.min(results.length - 1, index));
        hostView.currentIndex = index;
        hostView.positionViewAtIndex(index, GridView.Contain);
        if (modifiers & Qt.ShiftModifier)
            extendTo(results[index].id);
        else if (!(modifiers & Qt.ControlModifier))
            selectOnly(results[index].id);
    }

    // The host a keyboard or menu command applies to: the selection, else the current one.
    function targets() {
        const ids = selectedIds();
        if (ids.length > 0)
            return ids;
        const current = results[hostView.currentIndex];
        return current ? [current.id] : [];
    }

    function connect(ids, where) {
        if (!shell)
            return;
        for (const id of ids.slice(0, 10))
            shell.connectHost(id, where);
    }

    function openMenu(index, item, x, y) {
        if (index >= 0 && !selected[results[index].id])
            selectOnly(results[index].id);
        hostMenu.ids = targets();
        hostMenu.popup(item, x, y);
    }

    function iconFor(host) {
        if (host.icon && host.icon !== "auto")
            return host.icon;
        switch (host.protocol) {
        case "rdp":
        case "vnc":
            return "monitor";
        case "serial":
            return "usb";
        case "local":
            return "square-terminal";
        case "docker":
            return "os-docker";
        case "kube":
            return "os-kubernetes";
        case "sftp":
            return "folder-sync";
        case "telnet":
            return "network";
        default:
            return "server";
        }
    }

    function protocolLabel(protocol) {
        switch (protocol) {
        case "ssh":
            return qsTr("SSH");
        case "sftp":
            return qsTr("SFTP");
        case "telnet":
            return qsTr("Telnet");
        case "serial":
            return qsTr("Serial");
        case "mosh":
            return qsTr("Mosh");
        case "rdp":
            return qsTr("RDP");
        case "vnc":
            return qsTr("VNC");
        case "local":
            return qsTr("Local");
        case "docker":
            return qsTr("Docker");
        case "kube":
            return qsTr("Kubernetes");
        default:
            return protocol;
        }
    }

    // The smoke test: the fixture in every mode, search timing, selection, the editor.
    function smokeSteps(smoke) {
        return [
            () => Hosts.loadFixture(1000),
            () => {
                refresh();
                const all = searchMs;
                search.text = "web eu"; // lint-qml: allow (a test query)
                if (results.length === 0)
                    smoke.fail("the fuzzy search found nothing in the fixture");
                console.info("smoke test: 1000 hosts: listing all took", all, "ms, searching took", searchMs, "ms");
            },
            () => {
                search.text = "";
                cards = false;
                scope = "favorites";
            },
            () => {
                if (results.some(host => !host.favorite))
                    smoke.fail("Favorites lists a host that isn't one");
                scope = "G00";
                cards = true;
                moveCurrent(0, 0);
                moveCurrent(2, Qt.ShiftModifier);
                if (selectedCount !== 3)
                    smoke.fail("Shift did not extend the selection to three hosts");
                selectAll();
            },
            () => {
                scope = "all";
                openMenu(0, grid, 0, 0);
            },
            () => hostMenu.close(),
            () => shell.editHost(results[0].id),
            () => shell.closeHostDialogs(),
            () => shell.editGroup("G01"),
            () => shell.closeHostDialogs(),
            () => shell.showSshImport(""),
            () => shell.closeHostDialogs()
        ];
    }

    // The listed hosts, one row per host (its summary as JSON), updated in place.
    ListModel {
        id: resultModel
    }

    onScopeChanged: refresh()
    onProtocolFilterChanged: refresh()
    onTagFilterChanged: refresh()
    onSortChanged: refresh()
    Component.onCompleted: refresh()

    Connections {
        target: Hosts

        function onChanged() {
            view.refresh();
        }
    }

    // The whole list is empty.
    OsEmptyState {
        anchors.fill: parent
        visible: Hosts.count === 0
        iconName: "server"
        title: qsTr("No hosts yet")
        description: qsTr("Save the servers you connect to and open them with one click, or bring in the ones in ~/.ssh/config.")

        OsButton {
            text: qsTr("New host")
            iconName: "plus"
            variant: "primary"
            onClicked: view.shell.newHost("")
        }

        OsButton {
            text: qsTr("Quick connect")
            iconName: "plug-zap"
            onClicked: ActionRegistry.trigger("app.quickConnect")
        }

        OsButton {
            text: qsTr("Local terminal")
            iconName: "square-terminal"
            onClicked: ActionRegistry.trigger("tab.newLocal")
        }

        OsButton {
            text: qsTr("Import")
            iconName: "import"
            onClicked: view.shell.showSshImport("")
        }
    }

    RowLayout {
        anchors.fill: parent
        visible: Hosts.count > 0
        spacing: 0

        // The lists and the group tree.
        Rectangle {
            Layout.preferredWidth: Theme.spacingXxl * 6
            Layout.fillHeight: true
            color: Theme.surface

            ListView {
                id: scopeList

                anchors.fill: parent
                anchors.margins: Theme.spacingSm
                model: view.scopeEntries
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                keyNavigationEnabled: true
                activeFocusOnTab: true
                currentIndex: view.scopeEntries.findIndex(entry => entry.kind !== "header" && entry.id === view.scope)
                Accessible.role: Accessible.List
                Accessible.name: qsTr("Host lists and groups")

                Keys.onReturnPressed: {
                    const entry = view.scopeEntries[currentIndex];
                    if (entry && entry.kind !== "header")
                        view.scope = entry.id;
                }
                Keys.onUpPressed: event => {
                    let index = currentIndex - 1;
                    while (index >= 0 && view.scopeEntries[index].kind === "header")
                        index -= 1;
                    if (index >= 0)
                        view.scope = view.scopeEntries[index].id;
                    event.accepted = true;
                }
                Keys.onDownPressed: event => {
                    let index = currentIndex + 1;
                    while (index < count && view.scopeEntries[index].kind === "header")
                        index += 1;
                    if (index < count)
                        view.scope = view.scopeEntries[index].id;
                    event.accepted = true;
                }

                delegate: Item {
                    id: scopeRow

                    required property var modelData
                    required property int index
                    readonly property bool header: modelData.kind === "header"
                    readonly property bool current: !header && modelData.id === view.scope

                    width: ListView.view.width
                    height: header ? Theme.rowHeight + Theme.spacingSm : Theme.rowHeight

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: Theme.spacingSm
                        visible: scopeRow.header

                        OsText {
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignBottom
                            text: scopeRow.modelData.text
                            size: "small"
                            muted: true
                            font.weight: Font.DemiBold
                        }

                        OsIconButton {
                            Layout.alignment: Qt.AlignBottom
                            implicitWidth: Theme.controlHeightSmall
                            implicitHeight: Theme.controlHeightSmall
                            iconName: "plus"
                            toolTip: qsTr("New group")
                            onClicked: view.shell.newGroup("")
                        }
                    }

                    OsListRow {
                        id: scopeItem

                        anchors.fill: parent
                        visible: !scopeRow.header
                        leftPadding: Theme.controlPadding + (scopeRow.modelData.depth ?? 0) * Theme.spacingLg
                        text: scopeRow.modelData.text
                        iconName: scopeRow.modelData.iconName
                        trailingText: scopeRow.modelData.count > 0 ? String(scopeRow.modelData.count) : ""
                        selected: scopeRow.current
                        highlighted: drop.containsDrag
                        focusPolicy: Qt.NoFocus

                        onClicked: view.scope = scopeRow.modelData.id

                        Rectangle {
                            visible: TabColors.color(scopeRow.modelData.color) !== "transparent"
                            implicitWidth: Theme.spacingSm
                            implicitHeight: Theme.spacingSm
                            radius: width / 2
                            color: TabColors.color(scopeRow.modelData.color)
                        }

                        TapHandler {
                            acceptedButtons: Qt.RightButton
                            enabled: scopeRow.modelData.kind === "group"
                            onTapped: (eventPoint, button) => {
                                groupMenu.groupId = scopeRow.modelData.id;
                                groupMenu.popup(scopeItem, eventPoint.position.x, eventPoint.position.y);
                            }
                        }

                        // A group dragged onto another group moves inside it.
                        DragHandler {
                            id: groupDrag

                            target: null
                            enabled: scopeRow.modelData.kind === "group" && !Hosts.readOnly
                            onActiveChanged: {
                                if (active) {
                                    dragProxy.kind = "group";
                                    dragProxy.ids = [scopeRow.modelData.id];
                                    dragProxy.label = scopeRow.modelData.text;
                                } else {
                                    dragProxy.finish();
                                }
                            }
                            onCentroidChanged: {
                                if (active)
                                    dragProxy.follow(centroid.scenePosition);
                            }
                        }
                    }

                    DropArea {
                        id: drop

                        anchors.fill: parent
                        enabled: !scopeRow.header && ["group", "all", "ungrouped"].indexOf(scopeRow.modelData.kind === "group" ? "group" : scopeRow.modelData.id) >= 0
                        keys: ["opensesh-hosts", "opensesh-group"]

                        onDropped: drop => {
                            const group = scopeRow.modelData.kind === "group" ? scopeRow.modelData.id : "";
                            if (dragProxy.kind === "hosts")
                                Hosts.moveHosts(JSON.stringify(dragProxy.ids), group);
                            else if (dragProxy.ids[0] !== group && !Hosts.moveGroup(dragProxy.ids[0], group))
                                Toasts.show(qsTr("A group can't go inside itself or one of its subgroups."), "warning");
                            drop.accept();
                        }
                    }
                }
            }

            Rectangle {
                anchors.right: parent.right
                width: Theme.borderWidth
                height: parent.height
                color: Theme.border
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: Theme.spacingSm

            // Search, filters, order, view and "+ Host".
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: Theme.spacingMd
                Layout.bottomMargin: 0
                spacing: Theme.spacingSm

                OsSearchField {
                    id: search

                    Layout.fillWidth: true
                    Layout.minimumWidth: Theme.spacingXxl * 4
                    placeholderText: qsTr("Search by name, address, user, tag or group")
                    Accessible.name: qsTr("Search hosts")
                    onTextChanged: view.refresh()
                    Keys.onDownPressed: event => {
                        view.hostView.forceActiveFocus(Qt.TabFocusReason);
                        view.moveCurrent(0, 0);
                        event.accepted = true;
                    }
                }

                OsComboBox {
                    Layout.preferredWidth: Theme.spacingXxl * 4
                    model: [{ text: qsTr("All protocols"), value: "" }].concat(["ssh", "sftp", "telnet", "serial", "mosh", "rdp", "vnc", "local",
                        "docker", "kube"].map(protocol => ({ text: view.protocolLabel(protocol), value: protocol })))
                    textRole: "text"
                    valueRole: "value"
                    Accessible.name: qsTr("Protocol")
                    onActivated: view.protocolFilter = currentValue
                }

                OsComboBox {
                    Layout.preferredWidth: Theme.spacingXxl * 4
                    model: [{ text: qsTr("All tags"), value: "" }].concat(view.tags.map(tag => ({ text: tag.tag, value: tag.tag })))
                    textRole: "text"
                    valueRole: "value"
                    Accessible.name: qsTr("Tag")
                    onActivated: view.tagFilter = currentValue
                }

                OsComboBox {
                    Layout.preferredWidth: Theme.spacingXxl * 4
                    model: [{ text: qsTr("By name"), value: "name" }, { text: qsTr("By address"), value: "address" },
                        { text: qsTr("Recently used"), value: "recent" }, { text: qsTr("By group"), value: "group" }]
                    textRole: "text"
                    valueRole: "value"
                    Accessible.name: qsTr("Order")
                    onActivated: view.sort = currentValue
                }

                OsIconButton {
                    iconName: "layout-grid"
                    checked: view.cards
                    toolTip: qsTr("Cards")
                    Accessible.checkable: true
                    Accessible.checked: checked
                    onClicked: view.cards = true
                }

                OsIconButton {
                    iconName: "list"
                    checked: !view.cards
                    toolTip: qsTr("List")
                    Accessible.checkable: true
                    Accessible.checked: checked
                    onClicked: view.cards = false
                }

                OsButton {
                    text: qsTr("Host")
                    iconName: "plus"
                    variant: "primary"
                    Accessible.name: qsTr("New host")
                    onClicked: view.shell.newHost(view.groups.some(group => group.id === view.scope) ? view.scope : "")
                }

                OsIconButton {
                    id: moreButton

                    iconName: "ellipsis"
                    toolTip: qsTr("More")
                    onClicked: moreMenu.popup(moreButton, 0, moreButton.height)
                }
            }

            SettingsNotice {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.spacingMd
                Layout.rightMargin: Theme.spacingMd
                visible: Hosts.readOnly
                kind: "danger"
                title: qsTr("hosts.toml is read-only")
                lines: [qsTr("It could not be read, or a newer OpenSesh wrote it. Changes are not saved until it is fixed.")]
                    .concat(JSON.parse(Hosts.problems || "[]").slice(0, 3))
            }

            // The hosts.
            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true

                GridView {
                    id: grid

                    readonly property int columns: Math.max(1, Math.floor(width / (Theme.spacingXxl * 7)))

                    anchors.fill: parent
                    anchors.margins: Theme.spacingMd
                    visible: view.cards
                    clip: true
                    model: view.cards ? resultModel : null
                    cellWidth: Math.floor(width / columns)
                    cellHeight: Theme.spacingXxl * 3
                    // A new search reuses the cards instead of building them again.
                    reuseItems: true
                    boundsBehavior: Flickable.StopAtBounds
                    keyNavigationEnabled: false
                    activeFocusOnTab: view.cards
                    currentIndex: 0
                    highlightMoveDuration: 0
                    Accessible.role: Accessible.List
                    Accessible.name: qsTr("Hosts")

                    Keys.onPressed: event => view.keyPressed(event, grid.columns)

                    delegate: HostCard {
                        view: view
                        width: GridView.view.cellWidth
                        height: GridView.view.cellHeight
                    }

                    T.ScrollBar.vertical: OsScrollBar {}
                }

                ListView {
                    id: list

                    anchors.fill: parent
                    anchors.margins: Theme.spacingSm
                    visible: !view.cards
                    clip: true
                    model: view.cards ? null : resultModel
                    reuseItems: true
                    boundsBehavior: Flickable.StopAtBounds
                    keyNavigationEnabled: false
                    activeFocusOnTab: !view.cards
                    currentIndex: 0
                    highlightMoveDuration: 0
                    Accessible.role: Accessible.List
                    Accessible.name: qsTr("Hosts")

                    Keys.onPressed: event => view.keyPressed(event, 1)

                    delegate: HostRow {
                        view: view
                        width: ListView.view.width
                    }

                    T.ScrollBar.vertical: OsScrollBar {}
                }

                OsEmptyState {
                    anchors.fill: parent
                    visible: view.results.length === 0
                    iconName: "search"
                    title: qsTr("No hosts here")
                    description: search.text.length > 0 || view.protocolFilter.length > 0 || view.tagFilter.length > 0
                                 ? qsTr("Nothing matches the search and filters.") : qsTr("This list is empty.")
                }
            }

            OsText {
                Layout.leftMargin: Theme.spacingMd
                Layout.bottomMargin: Theme.spacingSm
                size: "small"
                muted: true
                text: view.selectedCount > 1 ? qsTr("%n host(s), %1 selected", "", view.results.length).arg(view.selectedCount)
                                             : qsTr("%n host(s)", "", view.results.length)
            }
        }
    }

    function keyPressed(event, columns) {
        const current = hostView.currentIndex;
        switch (event.key) {
        case Qt.Key_Left:
            moveCurrent(current - 1, event.modifiers);
            break;
        case Qt.Key_Right:
            moveCurrent(current + 1, event.modifiers);
            break;
        case Qt.Key_Up:
            if (current < columns) {
                search.forceActiveFocus(Qt.TabFocusReason);
                break;
            }
            moveCurrent(current - columns, event.modifiers);
            break;
        case Qt.Key_Down:
            moveCurrent(current + columns, event.modifiers);
            break;
        case Qt.Key_Home:
            moveCurrent(0, event.modifiers);
            break;
        case Qt.Key_End:
            moveCurrent(results.length - 1, event.modifiers);
            break;
        case Qt.Key_Space:
            if (results[current])
                toggle(results[current].id);
            break;
        case Qt.Key_Return:
        case Qt.Key_Enter:
            connect(targets(), event.modifiers & Qt.ShiftModifier ? "right" : "tab");
            break;
        case Qt.Key_F2:
            if (results[current] && shell)
                shell.editHost(results[current].id);
            break;
        case Qt.Key_Delete:
            deleteDialog.ask(targets());
            break;
        case Qt.Key_Menu:
            openMenu(current, hostView.currentItem ?? hostView, 0, 0);
            break;
        case Qt.Key_A:
            if (!(event.modifiers & Qt.ControlModifier))
                return;
            selectAll();
            break;
        default:
            return;
        }
        event.accepted = true;
    }

    // Follows the pointer while hosts or a group are dragged; the drop areas see it.
    Rectangle {
        id: dragProxy

        property string kind: "hosts"
        property var ids: []
        property string label: ""
        property bool dragging: false

        function follow(scenePoint) {
            const point = view.mapFromItem(null, scenePoint.x, scenePoint.y);
            x = point.x + Theme.spacingSm;
            y = point.y + Theme.spacingSm;
            dragging = true;
        }

        function finish() {
            if (dragging)
                Drag.drop();
            dragging = false;
        }

        visible: dragging
        z: 100
        width: proxyText.implicitWidth + 2 * Theme.spacingMd
        height: Theme.controlHeightSmall
        radius: Theme.radiusControl
        color: Theme.surface
        border.width: Theme.borderWidth
        border.color: Theme.accent
        Drag.active: dragging
        Drag.keys: kind === "hosts" ? ["opensesh-hosts"] : ["opensesh-group"]
        Drag.hotSpot.x: 0
        Drag.hotSpot.y: 0

        OsText {
            id: proxyText

            anchors.centerIn: parent
            text: dragProxy.kind === "hosts" ? qsTr("%n host(s)", "", dragProxy.ids.length) : dragProxy.label
            size: "small"
        }
    }

    OsContextMenu {
        id: hostMenu

        property var ids: []
        readonly property var first: ids.length > 0 ? view.results.find(host => host.id === ids[0]) ?? null : null
        readonly property bool single: ids.length === 1

        OsMenuItem {
            text: hostMenu.single ? qsTr("Connect") : qsTr("Connect to %n host(s)", "", hostMenu.ids.length)
            iconName: "plug-zap"
            onTriggered: view.connect(hostMenu.ids, "tab")
        }

        OsContextMenu {
            title: qsTr("Connect in a split")

            OsMenuItem {
                text: qsTr("To the right")
                iconName: "columns-2"
                onTriggered: view.connect(hostMenu.ids.slice(0, 1), "right")
            }

            OsMenuItem {
                text: qsTr("Below")
                iconName: "rows-2"
                onTriggered: view.connect(hostMenu.ids.slice(0, 1), "down")
            }
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Edit…")
            iconName: "pencil"
            enabled: hostMenu.single
            onTriggered: view.shell.editHost(hostMenu.ids[0])
        }

        OsMenuItem {
            text: qsTr("Duplicate")
            iconName: "copy"
            enabled: hostMenu.single
            onTriggered: {
                const id = Hosts.duplicateHost(hostMenu.ids[0]);
                if (id.length > 0)
                    view.selectOnly(id);
            }
        }

        OsMenuItem {
            text: qsTr("Copy the ssh command")
            iconName: "terminal"
            enabled: hostMenu.single && hostMenu.first !== null && hostMenu.first.protocol === "ssh"
            onTriggered: {
                const command = Hosts.sshCommand(hostMenu.ids[0]);
                if (command.length > 0) {
                    Platform.copyText(command);
                    Toasts.show(qsTr("Copied: %1").arg(command), "info");
                }
            }
        }

        OsMenuItem {
            readonly property bool favorite: hostMenu.first !== null && hostMenu.first.favorite
            text: favorite ? qsTr("Remove from Favorites") : qsTr("Add to Favorites")
            iconName: "star"
            enabled: hostMenu.first !== null && !hostMenu.first.linked
            onTriggered: Hosts.setFavorite(JSON.stringify(hostMenu.ids), !favorite)
        }

        OsContextMenu {
            id: moveMenu

            title: qsTr("Move to group")
            enabled: hostMenu.first !== null && !hostMenu.first.linked

            OsMenuItem {
                text: qsTr("No group")
                onTriggered: Hosts.moveHosts(JSON.stringify(hostMenu.ids), "")
            }

            Instantiator {
                model: view.groups

                delegate: OsMenuItem {
                    required property var modelData

                    text: modelData.path
                    onTriggered: Hosts.moveHosts(JSON.stringify(hostMenu.ids), modelData.id)
                }

                onObjectAdded: (index, object) => moveMenu.insertItem(index + 1, object)
                onObjectRemoved: (index, object) => moveMenu.removeItem(object)
            }
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: hostMenu.single ? qsTr("Delete") : qsTr("Delete %n host(s)", "", hostMenu.ids.length)
            iconName: "trash-2"
            enabled: hostMenu.first !== null && !hostMenu.first.linked
            onTriggered: deleteDialog.ask(hostMenu.ids)
        }
    }

    OsContextMenu {
        id: moreMenu

        OsMenuItem {
            text: qsTr("New group…")
            iconName: "folder"
            onTriggered: view.shell.newGroup("")
        }

        OsMenuItem {
            text: qsTr("Import ~/.ssh/config…")
            iconName: "import"
            onTriggered: view.shell.showSshImport("")
        }

        OsMenuItem {
            text: qsTr("Quick connect…")
            iconName: "plug-zap"
            onTriggered: ActionRegistry.trigger("app.quickConnect")
        }

        Instantiator {
            model: JSON.parse(Hosts.sources || "[]")

            delegate: OsMenuItem {
                required property var modelData

                text: qsTr("Stop following %1").arg(modelData.path)
                onTriggered: Hosts.unlinkSource(modelData.path)
            }

            onObjectAdded: (index, object) => moreMenu.addItem(object)
            onObjectRemoved: (index, object) => moreMenu.removeItem(object)
        }
    }

    OsContextMenu {
        id: groupMenu

        property string groupId

        OsMenuItem {
            text: qsTr("New host here…")
            iconName: "plus"
            onTriggered: view.shell.newHost(groupMenu.groupId)
        }

        OsMenuItem {
            text: qsTr("New subgroup…")
            iconName: "folder"
            onTriggered: view.shell.newGroup(groupMenu.groupId)
        }

        OsMenuItem {
            text: qsTr("Edit…")
            iconName: "pencil"
            onTriggered: view.shell.editGroup(groupMenu.groupId)
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Delete group")
            iconName: "trash-2"
            onTriggered: groupDeleteDialog.ask(groupMenu.groupId)
        }
    }

    OsDialog {
        id: deleteDialog

        property var ids: []

        function ask(ids) {
            const own = ids.filter(id => {
                const host = view.results.find(entry => entry.id === id);
                return host && !host.linked;
            });
            if (own.length === 0)
                return;
            deleteDialog.ids = own;
            open();
        }

        title: qsTr("Delete %n host(s)?", "", ids.length)
        acceptText: qsTr("Delete")
        dangerous: true

        onAccepted: Hosts.deleteHosts(JSON.stringify(ids))

        OsText {
            width: Math.min(Theme.spacingXxl * 12, deleteDialog.maxWidth - deleteDialog.leftPadding - deleteDialog.rightPadding)
            text: qsTr("They are removed from hosts.toml (a backup of the file is kept). Open sessions stay open.")
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignLeft
        }
    }

    OsDialog {
        id: groupDeleteDialog

        property string groupId

        function ask(id) {
            groupId = id;
            open();
        }

        title: qsTr("Delete this group?")
        acceptText: qsTr("Delete")
        dangerous: true

        onAccepted: Hosts.deleteGroup(groupId)

        OsText {
            width: Math.min(Theme.spacingXxl * 12, groupDeleteDialog.maxWidth - groupDeleteDialog.leftPadding - groupDeleteDialog.rightPadding)
            text: qsTr("Its hosts and subgroups move to the group it is in. No host is deleted.")
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignLeft
        }
    }

    // Shared by cards and rows: selection, the context menu, double click, dragging.
    component HostInput: Item {
        id: input

        required property Item view
        required property int index
        required property var modelData

        anchors.fill: parent

        TapHandler {
            acceptedButtons: Qt.LeftButton
            onTapped: (eventPoint, button) => {
                if (tapCount === 2)
                    input.view.connect([input.modelData.id], "tab");
                else
                    input.view.clicked(input.index, point.modifiers);
            }
        }

        TapHandler {
            acceptedButtons: Qt.RightButton
            onTapped: (eventPoint, button) => input.view.openMenu(input.index, input, eventPoint.position.x, eventPoint.position.y)
        }

        DragHandler {
            target: null
            enabled: !input.modelData.linked && !Hosts.readOnly
            onActiveChanged: {
                const proxy = input.view.dragProxyItem();
                if (active) {
                    if (!input.view.selected[input.modelData.id])
                        input.view.selectOnly(input.modelData.id);
                    proxy.kind = "hosts";
                    proxy.ids = input.view.selectedIds();
                } else {
                    proxy.finish();
                }
            }
            onCentroidChanged: {
                if (active)
                    input.view.dragProxyItem().follow(centroid.scenePosition);
            }
        }
    }

    function dragProxyItem() {
        return dragProxy;
    }

    component HostCard: Item {
        id: card

        required property Item view
        required property int index
        required property string json
        readonly property var modelData: JSON.parse(json)
        readonly property bool selected: view.selected[modelData.id] === true
        readonly property bool current: GridView.isCurrentItem && GridView.view.activeFocus
        readonly property color mark: TabColors.color(modelData.color)
        readonly property int open: WindowRegistry.openHosts[modelData.id] ?? 0

        Accessible.role: Accessible.ListItem
        Accessible.name: modelData.name
        Accessible.description: modelData.target
        Accessible.selected: selected

        Rectangle {
            anchors.fill: parent
            anchors.margins: Theme.spacingXs
            radius: Theme.radiusCard
            color: card.selected ? Theme.selection : Theme.surface
            border.width: card.current ? Theme.focusRingWidth : Theme.borderWidth
            border.color: card.current ? Theme.focusRing : card.selected ? Theme.accent : Theme.border

            Rectangle {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.margins: Theme.spacingSm
                width: Theme.borderWidth * 3
                radius: width / 2
                color: card.mark
                visible: card.mark.a > 0
            }

            RowLayout {
                anchors.fill: parent
                anchors.margins: Theme.spacingMd
                anchors.leftMargin: Theme.spacingLg
                spacing: Theme.spacingMd

                Rectangle {
                    Layout.alignment: Qt.AlignTop
                    implicitWidth: Theme.controlHeight
                    implicitHeight: Theme.controlHeight
                    radius: Theme.radiusControl
                    color: Theme.surface2

                    OsIcon {
                        anchors.centerIn: parent
                        name: card.view.iconFor(card.modelData)
                        size: Theme.iconSize
                        color: Theme.textMuted
                    }

                    Rectangle {
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        width: Theme.spacingSm + Theme.borderWidth * 2
                        height: width
                        radius: width / 2
                        color: Theme.success
                        border.width: Theme.borderWidth
                        border.color: Theme.surface
                        visible: card.open > 0
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignTop
                    spacing: Theme.spacingXs

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spacingXs

                        OsText {
                            Layout.fillWidth: true
                            text: card.modelData.name
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                        }

                        OsIcon {
                            visible: card.modelData.favorite
                            name: "star"
                            size: Theme.iconSizeSmall
                            color: Theme.accentFg
                        }
                    }

                    OsText {
                        Layout.fillWidth: true
                        text: card.modelData.target.length > 0 ? card.modelData.target : card.view.protocolLabel(card.modelData.protocol)
                        size: "small"
                        muted: true
                        font.family: Theme.monoFontFamily
                        elide: Text.ElideMiddle
                    }

                    Row {
                        Layout.fillWidth: true
                        spacing: Theme.spacingXs
                        clip: true

                        OsTag {
                            visible: card.modelData.protocol !== "ssh"
                            text: card.modelData.sprint > 0 ? qsTr("%1 · Sprint %2").arg(card.view.protocolLabel(card.modelData.protocol)).arg(card.modelData.sprint)
                                                            : card.view.protocolLabel(card.modelData.protocol)
                        }

                        OsTag {
                            visible: card.modelData.linked
                            text: qsTr("ssh_config")
                        }

                        // Fixed slots: a new search only changes their texts.
                        OsTag {
                            visible: card.modelData.tags.length > 0
                            text: card.modelData.tags[0] ?? ""
                        }

                        OsTag {
                            visible: card.modelData.tags.length > 1
                            text: card.modelData.tags[1] ?? ""
                        }

                        OsTag {
                            visible: card.modelData.tags.length > 2
                            text: card.modelData.tags[2] ?? ""
                        }
                    }
                }
            }
        }

        HostInput {
            view: card.view
            index: card.index
            modelData: card.modelData
        }
    }

    component HostRow: Item {
        id: row

        required property Item view
        required property int index
        required property string json
        readonly property var modelData: JSON.parse(json)
        readonly property bool selected: view.selected[modelData.id] === true
        readonly property bool current: ListView.isCurrentItem && ListView.view.activeFocus
        readonly property int open: WindowRegistry.openHosts[modelData.id] ?? 0

        height: Theme.rowHeight + Theme.spacingSm
        Accessible.role: Accessible.ListItem
        Accessible.name: modelData.name
        Accessible.description: modelData.target
        Accessible.selected: selected

        Rectangle {
            anchors.fill: parent
            radius: Theme.radiusControl
            color: row.selected ? Theme.selection : "transparent"
            border.width: row.current ? Theme.focusRingWidth : 0
            border.color: Theme.focusRing
        }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Theme.spacingMd
            anchors.rightMargin: Theme.spacingMd
            spacing: Theme.spacingMd

            Rectangle {
                implicitWidth: Theme.borderWidth * 3
                implicitHeight: Theme.iconSize
                radius: width / 2
                color: TabColors.color(row.modelData.color)
            }

            OsIcon {
                name: row.view.iconFor(row.modelData)
                size: Theme.iconSize
                color: Theme.textMuted
            }

            OsText {
                Layout.preferredWidth: Theme.spacingXxl * 5
                text: row.modelData.name
                font.weight: Font.Medium
                elide: Text.ElideRight
            }

            OsText {
                Layout.fillWidth: true
                text: row.modelData.target.length > 0 ? row.modelData.target : row.view.protocolLabel(row.modelData.protocol)
                muted: true
                font.family: Theme.monoFontFamily
                elide: Text.ElideMiddle
            }

            OsText {
                Layout.preferredWidth: Theme.spacingXxl * 4
                text: row.modelData.groupPath
                size: "small"
                muted: true
                elide: Text.ElideLeft
            }

            Row {
                spacing: Theme.spacingXs

                OsTag {
                    visible: row.modelData.tags.length > 0
                    text: row.modelData.tags[0] ?? ""
                }

                OsTag {
                    visible: row.modelData.tags.length > 1
                    text: row.modelData.tags[1] ?? ""
                }
            }

            OsIcon {
                name: "star"
                size: Theme.iconSizeSmall
                color: Theme.accentFg
                opacity: row.modelData.favorite ? 1 : 0
            }

            Rectangle {
                implicitWidth: Theme.spacingSm
                implicitHeight: Theme.spacingSm
                radius: width / 2
                color: row.open > 0 ? Theme.success : "transparent"
            }
        }

        HostInput {
            view: row.view
            index: row.index
            modelData: row.modelData
        }
    }
}
