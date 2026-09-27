pragma ComponentBehavior: Bound

// One pane of files (Sprint 8): this computer's or a server's, in a virtualized list (10,000
// entries stay smooth).
//
// The toolbar goes up, home and refreshes, shows the folder as breadcrumbs (click the path, or
// Ctrl+L, to type one) and makes folders. Columns sort by a click on their header. Rows select
// with a click, Ctrl (toggle) and Shift (range), and the keyboard: arrows, Home/End, Ctrl+A,
// Enter (open), Backspace (up), Space (quick look), F2 (rename), Delete, F5 and F6 (copy or move
// to the other pane), F7 (new folder), Ctrl+C / Ctrl+X / Ctrl+V, Ctrl+H (hidden files), Ctrl+R
// (refresh). Opening a file of a server edits it (Transfers.edit); a local file opens with its
// application.
//
// Files dropped from the file manager are uploaded into the current folder; files dragged to the
// other pane are transferred there; files of this computer can be dragged out to other apps.
// A server's pane that connects by itself shows its questions (host key, password) over the list.
//   mode: string            "local" or "remote"
//   hostId, target: string  a saved host, or quick-connect text, for a connection of its own
//   terminalSession: int    a terminal's session id: its connection is used (no second login)
//   connectionSerial: int   that terminal's TerminalItem.connectionSerial
//   startPath: string       where to start (empty: home)
//   title: string           what the pane shows (a host's name, "This computer")
//   compact: bool           name and size only (the side panel)
//   peer: Item              the other pane of a dual view (F5/F6, "copy to the other side")
//   browser: SftpBrowser    read-only
//   selectedPaths(): the selection's full paths; closeDialogs(): closes its menus and dialogs
// Signals: activated() (the pane got the keyboard), chooseSource() (an idle remote pane's button).
import QtQuick
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

FocusScope {
    id: pane

    property string mode: "local"
    property string hostId: ""
    property string target: ""
    property int terminalSession: 0
    property int connectionSerial: 0
    property string startPath: ""
    property string title: ""
    property bool compact: false
    property Item peer: null
    readonly property alias browser: browser
    readonly property Item shell: WindowRegistry.mainShell
    readonly property bool ready: browser.status === "ready"
    // Selected names (name -> true); reassigned on each change so bindings follow.
    property var selection: ({})
    readonly property int selectionCount: Object.keys(selection).length
    property int anchorRow: -1
    property bool editingPath: false

    signal activated
    signal chooseSource

    function errorText(code, detail) {
        switch (code) {
        case "not-found":
            return qsTr("It isn't there any more.");
        case "permission":
            return qsTr("Permission denied.");
        case "exists":
            return qsTr("Something with that name is already there.");
        case "disconnected":
            return qsTr("The connection was lost.");
        case "timeout":
            return qsTr("The server didn't answer in time.");
        case "unsupported":
            return qsTr("The server can't do that.");
        case "no-session":
            return qsTr("The terminal isn't connected.");
        case "no-sftp":
            return qsTr("This server has no SFTP. Files can't be browsed on it yet.");
        case "cancelled":
            return qsTr("Cancelled.");
        case "auth":
            return qsTr("Authentication failed.");
        case "host-key":
            return qsTr("The server's key wasn't accepted.");
        case "network":
            return qsTr("The server can't be reached.");
        default:
            return detail.length > 0 ? detail : qsTr("Something went wrong.");
        }
    }

    function focusList() {
        list.forceActiveFocus(Qt.OtherFocusReason);
    }

    // Selection.
    function setSelection(names) {
        const next = {};
        for (const name of names)
            next[name] = true;
        selection = next;
    }

    function clearSelection() {
        selection = {};
        anchorRow = -1;
    }

    function selectRow(row, modifiers) {
        const name = browser.nameAt(row);
        if (name.length === 0)
            return;
        if (modifiers & Qt.ShiftModifier && anchorRow >= 0) {
            setSelection(browser.namesBetween(anchorRow, row));
        } else if (modifiers & Qt.ControlModifier) {
            const next = Object.assign({}, selection);
            if (next[name])
                delete next[name];
            else
                next[name] = true;
            selection = next;
            anchorRow = row;
        } else {
            setSelection([name]);
            anchorRow = row;
        }
        list.currentIndex = row;
    }

    function selectAll() {
        setSelection(browser.allNames());
    }

    // The selection, or the current row when nothing is selected.
    function selectedNames() {
        const names = Object.keys(selection);
        if (names.length > 0)
            return names;
        const current = browser.nameAt(list.currentIndex);
        return current.length > 0 ? [current] : [];
    }

    function selectedPaths() {
        return browser.pathsOf(selectedNames());
    }

    // Keeps the names that are still listed (after a refresh).
    function pruneSelection() {
        const kept = Object.keys(selection).filter(name => browser.rowOf(name) >= 0);
        if (kept.length !== selectionCount)
            setSelection(kept);
    }

    // Opening.
    function openRow(row) {
        if (row < 0)
            return;
        if (browser.openRow(row)) {
            clearSelection();
            return;
        }
        const path = browser.pathAt(row);
        if (browser.remote) {
            if (Transfers.edit(browser.paneId, path) === 0)
                Toasts.show(qsTr("The file can't be opened: the connection is gone."), "danger");
        } else {
            Qt.openUrlExternally(FileFormat.fileUrl(path));
        }
    }

    function goUp() {
        clearSelection();
        browser.up();
    }

    function navigate(path) {
        clearSelection();
        browser.navigate(path);
    }

    // Transfers.
    function copyToPeer(move) {
        if (!peer || !peer.ready)
            return;
        const paths = selectedPaths();
        if (paths.length === 0)
            return;
        if (Transfers.copy(browser.paneId, paths, peer.browser.paneId, peer.browser.path, move) === 0)
            Toasts.show(qsTr("The other side isn't connected."), "danger");
    }

    function copyToClipboard(cut) {
        const paths = selectedPaths();
        if (paths.length > 0)
            FileClipboard.set(browser.paneId, paths, browser.path, cut);
    }

    function paste() {
        if (FileClipboard.empty || !ready)
            return;
        const paths = FileClipboard.paths;
        if (FileClipboard.pane === browser.paneId) {
            if (FileClipboard.folder === browser.path)
                return;
            if (FileClipboard.cut) {
                // A move inside this pane's files: renames, one by one.
                for (const path of paths)
                    browser.moveInto(path, browser.path);
                FileClipboard.clear();
                return;
            }
        }
        if (Transfers.copy(FileClipboard.pane, paths, browser.paneId, browser.path, FileClipboard.cut) === 0) {
            Toasts.show(qsTr("The files can't be pasted: their side isn't connected any more."), "danger");
            return;
        }
        if (FileClipboard.cut)
            FileClipboard.clear();
    }

    function askDelete() {
        const paths = selectedPaths();
        if (paths.length === 0)
            return;
        if (AppSettings.sftpConfirmDelete) {
            deleteDialog.paths = paths;
            deleteDialog.open();
        } else {
            browser.remove(paths);
        }
    }

    function rename() {
        const names = selectedNames();
        if (names.length === 1)
            nameDialog.show("rename", names[0]);
    }

    function permissions() {
        const names = selectedNames();
        if (names.length === 0)
            return;
        const entry = JSON.parse(browser.entryJson(browser.rowOf(names[0])) || "{}");
        permissionsDialog.show(browser.pathsOf(names), entry.mode ?? 0o644);
    }

    function properties() {
        const names = selectedNames();
        if (names.length === 1)
            propertiesDialog.show(JSON.parse(browser.entryJson(browser.rowOf(names[0])) || "{}"));
    }

    function preview() {
        const names = selectedNames();
        if (names.length !== 1)
            return;
        const row = browser.rowOf(names[0]);
        const entry = JSON.parse(browser.entryJson(row) || "{}");
        if (!entry.dirLike)
            previewDialog.show(browser, row);
    }

    function copyPaths() {
        const paths = selectedPaths();
        if (paths.length > 0) {
            Platform.copyText(paths.join("\n"));
            Toasts.show(qsTr("Copied %n path(s).", "", paths.length), "success");
        }
    }

    function closeDialogs() {
        for (const popup of [fileMenu, backgroundMenu, nameDialog, permissionsDialog, propertiesDialog, previewDialog, deleteDialog])
            popup.close();
    }

    function editPath() {
        pathField.text = browser.path;
        editingPath = true;
        pathField.forceActiveFocus(Qt.ShortcutFocusReason);
        pathField.selectAll();
    }

    onActiveFocusChanged: {
        if (activeFocus)
            activated();
    }
    Component.onCompleted: browser.start()

    // A finished transfer may have changed this folder.
    Connections {
        target: Transfers

        function onFinished() {
            if (pane.ready)
                browser.refresh();
        }
    }

    SftpBrowser {
        id: browser

        mode: pane.mode
        hostId: pane.hostId
        target: pane.target
        terminalSession: pane.terminalSession
        connectionSerial: pane.connectionSerial
        startPath: pane.startPath

        onListingChanged: pane.pruneSelection()
        onDone: (token, code, detail) => {
            if (code.length > 0 && code !== "disconnected")
                Toasts.show(pane.errorText(code, detail), "danger");
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Toolbar: up, home, refresh, the folder, new folder, more.
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.spacingXs
            spacing: Theme.spacingXs

            OsIconButton {
                iconName: "arrow-up"
                toolTip: qsTr("Up (Backspace)")
                enabled: pane.ready
                onClicked: pane.goUp()
            }

            OsIconButton {
                iconName: "house"
                toolTip: qsTr("Home")
                enabled: pane.ready
                onClicked: pane.navigate("~")
            }

            OsIconButton {
                iconName: "refresh-cw"
                toolTip: qsTr("Refresh (Ctrl+R)")
                enabled: pane.ready
                onClicked: browser.refresh()
            }

            // Breadcrumbs, or a field to type a path.
            Item {
                Layout.fillWidth: true
                implicitHeight: Theme.controlHeight

                OsTextField {
                    id: pathField

                    anchors.fill: parent
                    visible: pane.editingPath
                    Accessible.name: qsTr("Folder")
                    onAccepted: {
                        pane.editingPath = false;
                        pane.navigate(text);
                        pane.focusList();
                    }
                    onActiveFocusChanged: {
                        if (!activeFocus)
                            pane.editingPath = false;
                    }
                    Keys.onEscapePressed: {
                        pane.editingPath = false;
                        pane.focusList();
                    }
                }

                T.AbstractButton {
                    id: crumbBar

                    anchors.fill: parent
                    visible: !pane.editingPath
                    enabled: pane.ready
                    hoverEnabled: true
                    focusPolicy: Qt.NoFocus
                    Accessible.name: qsTr("Folder: %1").arg(browser.path)
                    onClicked: pane.editPath()

                    background: Rectangle {
                        radius: Theme.radiusControl
                        color: crumbBar.hovered ? Theme.hover : "transparent"
                    }

                    contentItem: Flickable {
                        id: crumbFlick

                        clip: true
                        contentWidth: crumbRow.implicitWidth
                        contentHeight: height
                        interactive: false
                        // The deepest folder stays in view.
                        contentX: Math.max(0, contentWidth - width)

                        Row {
                            id: crumbRow

                            height: crumbFlick.height
                            spacing: 0

                            Repeater {
                                model: JSON.parse(browser.crumbs || "[]")

                                delegate: Row {
                                    id: crumb

                                    required property var modelData
                                    required property int index

                                    height: crumbRow.height

                                    OsIcon {
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: crumb.index > 0
                                        name: "chevron-right"
                                        size: Theme.iconSizeSmall
                                        color: Theme.textMuted
                                    }

                                    T.AbstractButton {
                                        id: crumbButton

                                        anchors.verticalCenter: parent.verticalCenter
                                        implicitWidth: implicitContentWidth + leftPadding + rightPadding
                                        height: parent.height - Theme.spacingXs
                                        leftPadding: Theme.spacingXs
                                        rightPadding: Theme.spacingXs
                                        hoverEnabled: true
                                        focusPolicy: Qt.NoFocus
                                        Accessible.name: crumbLabel.text
                                        onClicked: pane.navigate(crumb.modelData.path)

                                        background: Rectangle {
                                            radius: Theme.radiusControl
                                            color: crumbButton.hovered ? Theme.hover : "transparent"
                                        }

                                        contentItem: OsText {
                                            id: crumbLabel

                                            text: crumb.modelData.label.length > 0 ? crumb.modelData.label : qsTr("This computer")
                                            size: "small"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            OsIconButton {
                iconName: "folder-plus"
                toolTip: qsTr("New folder (F7)")
                enabled: pane.ready
                onClicked: nameDialog.show("folder", "")
            }

            OsIconButton {
                id: moreButton

                iconName: "ellipsis"
                toolTip: qsTr("More")
                onClicked: backgroundMenu.popup(moreButton, 0, moreButton.height)
            }
        }

        // A thin bar while listing or working.
        Item {
            Layout.fillWidth: true
            implicitHeight: Theme.spacingXs / 2

            OsProgress {
                anchors.fill: parent
                visible: browser.busy || browser.status === "connecting"
                indeterminate: true
            }
        }

        // Column headers: click to sort, again to reverse.
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: Theme.controlHeightSmall
            color: Theme.surface
            visible: pane.ready

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Theme.spacingSm
                anchors.rightMargin: Theme.spacingSm + scrollBar.width
                spacing: Theme.spacingSm

                SortHeader {
                    Layout.fillWidth: true
                    key: "name"
                    text: qsTr("Name")
                }
                SortHeader {
                    Layout.preferredWidth: Theme.spacingXxl * 2.5
                    key: "size"
                    text: qsTr("Size")
                    alignRight: true
                }
                SortHeader {
                    Layout.preferredWidth: Theme.spacingXxl * 4
                    visible: !pane.compact
                    key: "modified"
                    text: qsTr("Modified")
                }
                SortHeader {
                    Layout.preferredWidth: Theme.spacingXxl * 3
                    visible: !pane.compact
                    key: "permissions"
                    text: qsTr("Permissions")
                }
                SortHeader {
                    Layout.preferredWidth: Theme.spacingXxl * 2.5
                    visible: !pane.compact
                    key: "owner"
                    text: qsTr("Owner")
                }
            }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ListView {
                id: list

                anchors.fill: parent
                visible: pane.ready
                model: browser
                clip: true
                reuseItems: true
                currentIndex: -1
                keyNavigationEnabled: false
                boundsBehavior: Flickable.StopAtBounds
                focus: true
                activeFocusOnTab: true
                highlightFollowsCurrentItem: false
                Accessible.role: Accessible.List
                Accessible.name: pane.title.length > 0 ? qsTr("Files of %1").arg(pane.title) : qsTr("Files")

                function move(step, modifiers) {
                    const row = Math.max(0, Math.min(count - 1, (currentIndex < 0 ? -1 : currentIndex) + step));
                    if (count === 0)
                        return;
                    if (modifiers & Qt.ShiftModifier) {
                        if (pane.anchorRow < 0)
                            pane.anchorRow = Math.max(0, currentIndex);
                        pane.setSelection(browser.namesBetween(pane.anchorRow, row));
                    } else if (!(modifiers & Qt.ControlModifier)) {
                        pane.setSelection([browser.nameAt(row)]);
                        pane.anchorRow = row;
                    }
                    currentIndex = row;
                    positionViewAtIndex(row, ListView.Contain);
                }

                Keys.onPressed: event => {
                    const ctrl = (event.modifiers & Qt.ControlModifier) !== 0;
                    switch (event.key) {
                    case Qt.Key_Down:
                        move(1, event.modifiers);
                        break;
                    case Qt.Key_Up:
                        move(-1, event.modifiers);
                        break;
                    case Qt.Key_PageDown:
                        move(Math.max(1, Math.floor(height / Theme.controlHeightSmall) - 1), event.modifiers);
                        break;
                    case Qt.Key_PageUp:
                        move(-Math.max(1, Math.floor(height / Theme.controlHeightSmall) - 1), event.modifiers);
                        break;
                    case Qt.Key_Home:
                        move(-count, event.modifiers);
                        break;
                    case Qt.Key_End:
                        move(count, event.modifiers);
                        break;
                    case Qt.Key_Return:
                    case Qt.Key_Enter:
                        pane.openRow(currentIndex);
                        break;
                    case Qt.Key_Backspace:
                        pane.goUp();
                        break;
                    case Qt.Key_Space:
                        pane.preview();
                        break;
                    case Qt.Key_F2:
                        pane.rename();
                        break;
                    case Qt.Key_Delete:
                        pane.askDelete();
                        break;
                    case Qt.Key_F5:
                        pane.copyToPeer(false);
                        break;
                    case Qt.Key_F6:
                        pane.copyToPeer(true);
                        break;
                    case Qt.Key_F7:
                        nameDialog.show("folder", "");
                        break;
                    case Qt.Key_A:
                        if (!ctrl)
                            return;
                        pane.selectAll();
                        break;
                    case Qt.Key_C:
                        if (!ctrl)
                            return;
                        pane.copyToClipboard(false);
                        break;
                    case Qt.Key_X:
                        if (!ctrl)
                            return;
                        pane.copyToClipboard(true);
                        break;
                    case Qt.Key_V:
                        if (!ctrl)
                            return;
                        pane.paste();
                        break;
                    case Qt.Key_H:
                        if (!ctrl)
                            return;
                        browser.showHidden = !browser.showHidden;
                        break;
                    case Qt.Key_R:
                        if (!ctrl)
                            return;
                        browser.refresh();
                        break;
                    case Qt.Key_L:
                        if (!ctrl)
                            return;
                        pane.editPath();
                        break;
                    default:
                        return;
                    }
                    event.accepted = true;
                }

                delegate: FileRow {
                    pane: pane
                }

                // A click on the empty space below the rows clears the selection.
                TapHandler {
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                    onTapped: (point, button) => {
                        list.forceActiveFocus(Qt.MouseFocusReason);
                        if (list.indexAt(point.position.x, point.position.y + list.contentY) >= 0)
                            return;
                        pane.clearSelection();
                        if (button === Qt.RightButton)
                            backgroundMenu.popup(list, point.position.x, point.position.y);
                    }
                }

                T.ScrollBar.vertical: OsScrollBar {
                    id: scrollBar
                }
            }

            // Files dropped from the file manager, or dragged from another pane.
            DropArea {
                anchors.fill: parent
                enabled: pane.ready
                keys: ["text/uri-list", "application/x-opensesh-files"]

                onDropped: drop => {
                    if (drop.formats.indexOf("application/x-opensesh-files") >= 0) {
                        const data = JSON.parse(drop.getDataAsString("application/x-opensesh-files") || "{}");
                        if (data.pane === browser.paneId && data.folder === browser.path)
                            return;
                        if (data.pane === browser.paneId) {
                            for (const path of data.paths)
                                browser.moveInto(path, browser.path);
                        } else {
                            Transfers.copy(data.pane, data.paths, browser.paneId, browser.path, false);
                        }
                        drop.accept(Qt.CopyAction);
                        return;
                    }
                    if (drop.hasUrls) {
                        const paths = drop.urls.map(url => Platform.localPath(url)).filter(path => path.length > 0);
                        if (paths.length > 0) {
                            Transfers.copyLocal(paths, browser.paneId, browser.path);
                            drop.accept(Qt.CopyAction);
                        }
                    }
                }

                Rectangle {
                    anchors.fill: parent
                    visible: parent.containsDrag
                    color: "transparent"
                    border.width: Theme.borderWidth * 2
                    border.color: Theme.accent
                    radius: Theme.radiusControl
                }
            }

            // Empty folder.
            OsText {
                anchors.centerIn: parent
                visible: pane.ready && list.count === 0 && !browser.busy
                text: browser.hiddenCount > 0 && !browser.showHidden ? qsTr("Only hidden files here (Ctrl+H shows them).") : qsTr("This folder is empty.")
                muted: true
            }

            // Connecting.
            Column {
                anchors.centerIn: parent
                visible: browser.status === "connecting"
                spacing: Theme.spacingSm

                OsProgress {
                    anchors.horizontalCenter: parent.horizontalCenter
                    indeterminate: true
                }

                OsText {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: pane.title.length > 0 ? qsTr("Connecting to %1…").arg(pane.title) : qsTr("Connecting…")
                    muted: true
                }
            }

            // No source yet, an error, or the connection lost.
            OsEmptyState {
                anchors.fill: parent
                visible: browser.status === "idle" || browser.status === "error" || browser.status === "disconnected"
                iconName: browser.status === "idle" ? "folder-sync" : "unplug"
                title: browser.status === "idle" ? qsTr("Choose where to connect")
                     : browser.status === "disconnected" ? qsTr("Disconnected") : qsTr("Can't show these files")
                description: browser.status === "idle" ? qsTr("Pick a saved host, or type user@host.") : pane.errorText(browser.error, browser.errorDetail)

                OsButton {
                    visible: browser.status === "idle"
                    variant: "primary"
                    text: qsTr("Choose a host…")
                    iconName: "server"
                    onClicked: pane.chooseSource()
                }

                OsButton {
                    visible: browser.status !== "idle" && !(browser.terminalSession > 0 && browser.connectionSerial <= 0)
                    variant: "primary"
                    text: qsTr("Reconnect")
                    iconName: "refresh-cw"
                    onClicked: browser.reconnect()
                }
            }

            // The questions of the pane's own connection.
            SshOverlay {
                anchors.fill: parent
                visible: pane.mode === "remote" && pane.terminalSession <= 0
                terminal: browser
                shell: pane.shell ?? pane
                label: pane.title
                banner: false
                onAnswered: pane.focusList()
            }
        }

        // What is here and what is selected.
        OsText {
            Layout.fillWidth: true
            Layout.margins: Theme.spacingXs
            Layout.leftMargin: Theme.spacingSm
            visible: pane.ready
            size: "small"
            muted: true
            elide: Text.ElideRight
            text: {
                let text = qsTr("%n item(s)", "", browser.count);
                if (!browser.showHidden && browser.hiddenCount > 0)
                    text = qsTr("%1, %2 hidden").arg(text).arg(browser.hiddenCount);
                if (pane.selectionCount > 0)
                    text = qsTr("%1 · %2 selected").arg(text).arg(pane.selectionCount);
                return text;
            }
        }
    }

    // A column header that sorts.
    component SortHeader: T.AbstractButton {
        id: header

        required property string key
        property bool alignRight: false
        readonly property bool current: browser.sortKey === key

        implicitHeight: parent ? parent.height : Theme.controlHeightSmall
        hoverEnabled: true
        focusPolicy: Qt.NoFocus
        Accessible.name: current ? (browser.sortAscending ? qsTr("%1, sorted ascending").arg(text) : qsTr("%1, sorted descending").arg(text)) : text
        onClicked: {
            if (current)
                browser.sortAscending = !browser.sortAscending;
            else {
                browser.sortKey = key;
                browser.sortAscending = true;
            }
        }

        contentItem: Row {
            layoutDirection: header.alignRight ? Qt.RightToLeft : Qt.LeftToRight
            spacing: Theme.spacingXs

            OsText {
                anchors.verticalCenter: parent.verticalCenter
                text: header.text
                size: "small"
                muted: !header.current
                font.weight: header.current ? Font.DemiBold : Font.Normal
            }

            OsIcon {
                anchors.verticalCenter: parent.verticalCenter
                visible: header.current
                name: browser.sortAscending ? "chevron-up" : "chevron-down"
                size: Theme.iconSizeSmall
                color: Theme.textMuted
            }
        }

        background: Rectangle {
            color: header.hovered ? Theme.hover : "transparent"
            radius: Theme.radiusControl
        }
    }

    // One file of the list.
    component FileRow: Rectangle {
        id: row

        required property Item pane
        required property int index
        required property string name
        required property string kind
        required property var size
        required property var modified
        required property string permissions
        required property string owner
        required property string linkTarget
        required property bool dirLike
        required property bool hidden
        readonly property bool selected: row.pane.selection[name] === true
        readonly property bool current: ListView.isCurrentItem

        width: ListView.view ? ListView.view.width : 0
        height: Theme.controlHeightSmall
        color: selected ? Theme.selection : mouse.containsMouse ? Theme.hover : "transparent"
        opacity: hidden ? 0.7 : 1
        border.width: current && row.ListView.view.activeFocus ? Theme.borderWidth : 0
        border.color: Theme.accent

        Drag.active: dragHandler.active
        Drag.dragType: Drag.Automatic
        Drag.supportedActions: Qt.CopyAction
        Drag.mimeData: {
            const paths = row.pane.selectedPaths();
            const data = {
                "application/x-opensesh-files": JSON.stringify({ pane: browser.paneId, folder: browser.path, paths: paths })
            };
            // This computer's files go to other apps too.
            if (!browser.remote)
                data["text/uri-list"] = paths.map(path => FileFormat.fileUrl(path)).join("\r\n");
            return data;
        }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Theme.spacingSm
            anchors.rightMargin: Theme.spacingSm + scrollBar.width
            spacing: Theme.spacingSm

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacingSm

                OsIcon {
                    name: row.dirLike ? "folder" : row.kind === "symlink" ? "link"
                        : /\.(png|jpe?g|gif|bmp|webp|svg|ico)$/i.test(row.name) ? "image"
                        : /\.(txt|md|log|conf|cfg|ini|json|toml|ya?ml|xml|sh|py|rs|js|c|h|cpp|go|java|html|css)$/i.test(row.name) ? "file-text"
                        : "file"
                    size: Theme.iconSizeSmall
                    color: row.dirLike ? Theme.accent : Theme.textMuted
                }

                OsText {
                    Layout.fillWidth: true
                    text: row.kind === "symlink" && row.linkTarget.length > 0 ? qsTr("%1 → %2").arg(row.name).arg(row.linkTarget) : row.name
                    elide: Text.ElideMiddle
                }
            }

            OsText {
                Layout.preferredWidth: Theme.spacingXxl * 2.5
                horizontalAlignment: Text.AlignRight
                text: row.dirLike ? "" : FileFormat.size(row.size)
                size: "small"
                muted: true
            }

            OsText {
                Layout.preferredWidth: Theme.spacingXxl * 4
                visible: !row.pane.compact
                text: FileFormat.time(row.modified)
                size: "small"
                muted: true
            }

            Text {
                Layout.preferredWidth: Theme.spacingXxl * 3
                visible: !row.pane.compact
                text: row.permissions
                color: Theme.textMuted
                font.family: Theme.monoFontFamily
                font.pixelSize: Theme.fontSizeSmall
                elide: Text.ElideRight
            }

            OsText {
                Layout.preferredWidth: Theme.spacingXxl * 2.5
                visible: !row.pane.compact
                text: row.owner
                size: "small"
                muted: true
            }
        }

        MouseArea {
            id: mouse

            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton

            onPressed: mouseEvent => {
                row.ListView.view.forceActiveFocus(Qt.MouseFocusReason);
                if (mouseEvent.button === Qt.RightButton) {
                    if (!row.selected)
                        row.pane.selectRow(row.index, 0);
                    fileMenu.popup(row, mouseEvent.x, mouseEvent.y);
                } else if (!(row.selected && mouseEvent.modifiers === Qt.NoModifier)) {
                    row.pane.selectRow(row.index, mouseEvent.modifiers);
                }
            }
            onClicked: mouseEvent => {
                // A click on a selected row, without a drag, makes it the only one.
                if (mouseEvent.button === Qt.LeftButton && mouseEvent.modifiers === Qt.NoModifier && row.pane.selectionCount > 1)
                    row.pane.selectRow(row.index, 0);
            }
            onDoubleClicked: mouseEvent => {
                if (mouseEvent.button === Qt.LeftButton)
                    row.pane.openRow(row.index);
            }
        }

        DragHandler {
            id: dragHandler

            target: null
            acceptedButtons: Qt.LeftButton
            // Only whole selections are dragged; a drag starts on a selected row.
            enabled: row.selected
        }
    }

    OsContextMenu {
        id: fileMenu

        readonly property bool single: pane.selectedNames().length === 1

        OsMenuItem {
            text: qsTr("Open")
            iconName: "folder-open"
            enabled: fileMenu.single
            onTriggered: pane.openRow(list.currentIndex)
        }
        OsMenuItem {
            text: qsTr("Quick look")
            iconName: "eye"
            shortcutText: qsTr("Space")
            enabled: fileMenu.single
            onTriggered: pane.preview()
        }
        OsMenuItem {
            visible: pane.peer !== null
            height: visible ? implicitHeight : 0
            text: qsTr("Copy to the other side")
            iconName: "arrow-left-right"
            shortcutText: qsTr("F5")
            enabled: pane.peer !== null && pane.peer.ready
            onTriggered: pane.copyToPeer(false)
        }
        OsMenuItem {
            visible: pane.peer !== null
            height: visible ? implicitHeight : 0
            text: qsTr("Move to the other side")
            shortcutText: qsTr("F6")
            enabled: pane.peer !== null && pane.peer.ready
            onTriggered: pane.copyToPeer(true)
        }
        OsMenuItem {
            visible: browser.remote
            height: visible ? implicitHeight : 0
            text: qsTr("Download to…")
            iconName: "download"
            onTriggered: downloadDialog.open()
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Copy")
            iconName: "copy"
            shortcutText: qsTr("Ctrl+C")
            onTriggered: pane.copyToClipboard(false)
        }
        OsMenuItem {
            text: qsTr("Cut")
            iconName: "scissors"
            shortcutText: qsTr("Ctrl+X")
            onTriggered: pane.copyToClipboard(true)
        }
        OsMenuItem {
            text: qsTr("Copy the path")
            onTriggered: pane.copyPaths()
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Rename…")
            iconName: "pencil"
            shortcutText: qsTr("F2")
            enabled: fileMenu.single
            onTriggered: pane.rename()
        }
        OsMenuItem {
            text: qsTr("Permissions…")
            iconName: "lock"
            onTriggered: pane.permissions()
        }
        OsMenuItem {
            text: qsTr("Properties…")
            iconName: "info"
            enabled: fileMenu.single
            onTriggered: pane.properties()
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Delete…")
            iconName: "trash-2"
            shortcutText: qsTr("Del")
            onTriggered: pane.askDelete()
        }
    }

    OsContextMenu {
        id: backgroundMenu

        OsMenuItem {
            text: qsTr("Paste")
            iconName: "clipboard-paste"
            shortcutText: qsTr("Ctrl+V")
            enabled: pane.ready && !FileClipboard.empty
            onTriggered: pane.paste()
        }
        OsMenuItem {
            visible: browser.remote
            height: visible ? implicitHeight : 0
            text: qsTr("Upload files…")
            iconName: "upload"
            enabled: pane.ready
            onTriggered: uploadDialog.open()
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("New folder…")
            iconName: "folder-plus"
            shortcutText: qsTr("F7")
            enabled: pane.ready
            onTriggered: nameDialog.show("folder", "")
        }
        OsMenuItem {
            text: qsTr("New file…")
            iconName: "file-plus"
            enabled: pane.ready
            onTriggered: nameDialog.show("file", "")
        }
        OsMenuItem {
            text: qsTr("New symbolic link…")
            iconName: "link"
            enabled: pane.ready
            onTriggered: nameDialog.show("link", "")
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Show hidden files")
            checkable: true
            checked: browser.showHidden
            shortcutText: qsTr("Ctrl+H")
            onTriggered: browser.showHidden = !browser.showHidden
        }
        OsMenuItem {
            text: qsTr("Refresh")
            iconName: "refresh-cw"
            shortcutText: qsTr("Ctrl+R")
            enabled: pane.ready
            onTriggered: browser.refresh()
        }
        OsMenuItem {
            visible: !browser.remote
            height: visible ? implicitHeight : 0
            text: qsTr("Open in the file manager")
            iconName: "external-link"
            enabled: pane.ready && browser.path.length > 0
            onTriggered: Qt.openUrlExternally(FileFormat.fileUrl(browser.path))
        }
    }

    FileNameDialog {
        id: nameDialog

        onChosen: (kind, name, target) => {
            switch (kind) {
            case "file":
                browser.createFile(name);
                break;
            case "rename": {
                const row = browser.rowOf(pane.selectedNames()[0] ?? "");
                if (row >= 0)
                    browser.rename(row, name);
                break;
            }
            case "link":
                browser.symlink(name, target);
                break;
            default:
                browser.mkdir(name);
            }
            pane.focusList();
        }
    }

    PermissionsDialog {
        id: permissionsDialog

        onApply: (paths, mode) => browser.chmod(paths, mode)
    }

    FilePropertiesDialog {
        id: propertiesDialog
    }

    FilePreviewDialog {
        id: previewDialog
    }

    OsDialog {
        id: deleteDialog

        property var paths: []

        title: paths.length > 1 ? qsTr("Delete %n items?", "", paths.length) : qsTr("Delete this item?")
        acceptText: qsTr("Delete")
        dangerous: true
        onAccepted: {
            browser.remove(paths);
            pane.clearSelection();
            pane.focusList();
        }

        Column {
            width: Math.min(Theme.spacingXxl * 13, deleteDialog.maxWidth - deleteDialog.leftPadding - deleteDialog.rightPadding)
            spacing: Theme.spacingSm

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                text: browser.remote ? qsTr("Folders are deleted with everything in them. This can't be undone.")
                                     : qsTr("Folders are deleted with everything in them. They don't go to the trash, and this can't be undone.")
            }

            OsText {
                width: parent.width
                text: deleteDialog.paths.slice(0, 5).join("\n") + (deleteDialog.paths.length > 5 ? "\n…" : "") // lint-qml: allow (a list of paths)
                muted: true
                elide: Text.ElideMiddle
            }
        }
    }

    FolderDialog {
        id: downloadDialog

        title: qsTr("Download to")
        onAccepted: Transfers.copyToLocal(browser.paneId, pane.selectedPaths(), Platform.localPath(selectedFolder))
    }

    FileDialog {
        id: uploadDialog

        title: qsTr("Upload files")
        fileMode: FileDialog.OpenFiles
        onAccepted: Transfers.copyLocal(selectedFiles.map(url => Platform.localPath(url)), browser.paneId, browser.path)
    }
}
