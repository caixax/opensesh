pragma ComponentBehavior: Bound

// Tunnels view (PLAN §5.4, Sprint 9): the port forwarding manager. Each tunnel shows a switch,
// its route in words, what it is doing (running, connecting, waiting for a session of its host,
// retrying, failed) and its traffic, with a menu to edit, duplicate, copy its address or delete
// it. A tunnel that listens beyond this computer (or, for a remote one, beyond the server) is
// marked; one that asks something (a host key, a password) has an Answer button.
// Keys in the list: Up/Down, Space (switch), Enter (edit), Delete, Ctrl+N (new).
// Functions: newTunnel(), edit(id), answer(id), showImport(), closeDialogs(), smokeSteps(smoke).
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: view

    readonly property Item shell: WindowRegistry.mainShell
    readonly property var tunnels: JSON.parse(Tunnels.list || "[]")
    // What a row shows while its tunnel is going away.
    readonly property var blank: ({ id: "", name: "", kind: "local", host: "", target: "", hostName: "",
                                    bindAddress: "", bindPort: 0, destinationHost: "", destinationPort: 0,
                                    tied: false, autostart: false, reconnect: true, exposed: false, on: false,
                                    state: "stopped", port: 0, code: "", detail: "", retryIn: 0, sent: 0,
                                    received: 0, open: 0, total: 0, prompt: "" })

    // The rows follow the tunnels by id, so a row (and its open menu) stays while its tunnel's
    // counters change every second.
    function syncRows() {
        const ids = tunnels.map(entry => entry.id);
        for (let row = rows.count - 1; row >= 0; --row) {
            if (ids.indexOf(rows.get(row).tunnelId) < 0)
                rows.remove(row);
        }
        ids.forEach((id, index) => {
            let at = -1;
            for (let row = 0; row < rows.count; ++row) {
                if (rows.get(row).tunnelId === id) {
                    at = row;
                    break;
                }
            }
            if (at < 0)
                rows.insert(index, { tunnelId: id });
            else if (at !== index)
                rows.move(at, index, 1);
        });
    }

    onTunnelsChanged: syncRows()
    Component.onCompleted: syncRows()

    function tunnel(id) {
        return tunnels.find(entry => entry.id === id) ?? null;
    }

    function address(host, port) {
        return host.indexOf(":") >= 0 ? "[" + host + "]:" + port : host + ":" + port;
    }

    // Where it listens: the port it got once running.
    function listening(entry) {
        const host = entry.bindAddress.length > 0 ? entry.bindAddress : "*";
        return address(host, entry.port > 0 ? entry.port : entry.bindPort);
    }

    function route(entry) {
        switch (entry.kind) {
        case "local":
            return qsTr("%1 → %2 from %3").arg(listening(entry)).arg(address(entry.destinationHost, entry.destinationPort)).arg(entry.hostName);
        case "remote":
            return qsTr("%1 on %3 → %2 here").arg(listening(entry)).arg(address(entry.destinationHost, entry.destinationPort)).arg(entry.hostName);
        default:
            return qsTr("SOCKS proxy on %1 through %2").arg(listening(entry)).arg(entry.hostName);
        }
    }

    function title(entry) {
        return entry.name.length > 0 ? entry.name : route(entry);
    }

    function kindText(kind) {
        return kind === "local" ? qsTr("Local") : kind === "remote" ? qsTr("Remote") : qsTr("SOCKS");
    }

    function kindIcon(kind) {
        return kind === "local" ? "arrow-right" : kind === "remote" ? "arrow-left" : "globe";
    }

    function stateText(entry) {
        switch (entry.state) {
        case "running":
            return qsTr("Running");
        case "connecting":
            return qsTr("Connecting…");
        case "waiting":
            return entry.tied ? qsTr("Waiting for a session to %1").arg(entry.hostName) : qsTr("Waiting for the connection");
        case "retrying":
            return qsTr("Connection lost; trying again in %n s", "", entry.retryIn);
        case "failed":
            return failureText(entry);
        default:
            return qsTr("Stopped");
        }
    }

    function failureText(entry) {
        switch (entry.code) {
        case "auth":
            return qsTr("Authentication failed");
        case "host-key":
            return qsTr("The server's key wasn't accepted");
        case "cancelled":
            return qsTr("Cancelled");
        case "locked":
            return qsTr("The vault is locked");
        case "lost":
            return qsTr("The connection was lost");
        default:
            return entry.detail.length > 0 ? entry.detail : qsTr("It can't run");
        }
    }

    function stateColor(state) {
        return state === "running" ? Theme.success
             : state === "failed" ? Theme.danger
             : state === "stopped" ? Theme.textMuted
             : Theme.warning;
    }

    function traffic(entry) {
        if (entry.total === 0)
            return "";
        return qsTr("↑ %1 ↓ %2 · %n connection(s)", "", entry.total).arg(FileFormat.size(entry.sent)).arg(FileFormat.size(entry.received));
    }

    function newTunnel() {
        editor.show(null);
    }

    function edit(id) {
        const entry = tunnel(id);
        if (entry)
            editor.show(entry);
    }

    function answer(id) {
        question.show(id);
    }

    function showImport() {
        importDialog.show();
    }

    function closeDialogs() {
        for (const dialog of [editor, importDialog, question, deleteDialog])
            dialog.close();
    }

    function copyAddress(entry) {
        Platform.copyText(entry.kind === "dynamic" ? "socks5h://" + listening(entry) : listening(entry));
        Toasts.show(qsTr("Copied %1.").arg(listening(entry)), "success");
    }

    function askDelete(id) {
        deleteDialog.tunnelId = id;
        deleteDialog.open();
    }

    // Functions for SmokeTest.steps, after the SSH steps started the test server and loaded the
    // hosts fixture (H00000 goes to the test server): a tunnel of each kind on its own connection
    // (its questions answered in its row), HTTP through the local and remote ones to the test
    // HTTP server, a tunnel tied to H00000 that runs only while a terminal session to it is up,
    // the editor, the import dialog, duplicate and delete. Nothing is written.
    function smokeSteps(smoke) {
        const timeout = 20000;
        let deadline = 0;
        let web = 0;
        const ids = {};
        const answered = {};
        let body = "";
        let tabId = 0;
        let pane = null;
        const waitFor = (what, condition, next) => {
            const poll = () => {
                if (condition())
                    return next ? next() : [];
                if (Date.now() > deadline) {
                    smoke.fail("timed out after " + timeout / 1000 + " s waiting for " + what + ": " + Tunnels.list);
                    return [];
                }
                return [poll];
            };
            return poll;
        };
        const wait = (what, condition, next) => {
            deadline = Date.now() + timeout;
            return [waitFor(what, condition, next)];
        };
        const make = (name, fields) => {
            const id = Tunnels.save(JSON.stringify(Object.assign({
                name: name,
                host: "H00000",
                target: "",
                bindAddress: "127.0.0.1",
                bindPort: 0,
                destinationHost: "127.0.0.1",
                destinationPort: web,
                tied: false,
                autostart: false,
                reconnect: true
            }, fields)));
            if (id.length === 0)
                smoke.fail("the tunnel " + name + " wasn't saved");
            return id;
        };
        // Answers a tunnel's questions (host key, password) until it runs.
        const running = id => {
            const entry = view.tunnel(id);
            if (!entry)
                return false;
            if (entry.prompt.length > 0) {
                const question = JSON.parse(entry.prompt);
                if (answered[id] !== question.id) {
                    answered[id] = question.id;
                    if (question.kind === "hostKey")
                        Tunnels.answerPrompt(id, question.id, "trust-once", []);
                    else
                        Tunnels.answerPrompt(id, question.id, "submit", ["right password"]); // lint-qml: allow (the test server's password)
                }
            }
            if (entry.state === "failed")
                smoke.fail("the tunnel " + entry.name + " failed: " + entry.code + " " + entry.detail);
            return entry.state === "running" && entry.port > 0;
        };
        const get = port => {
            body = "";
            const request = new XMLHttpRequest();
            request.onreadystatechange = () => {
                if (request.readyState === XMLHttpRequest.DONE)
                    body = request.status === 200 ? request.responseText : String(request.status);
            };
            request.open("GET", "http://127.0.0.1:" + port + "/");
            request.send();
        };
        const fetched = () => body === "OpenSesh tunnel test"; // lint-qml: allow (the test HTTP server's answer)
        return [
            () => {
                web = AppInfo.startHttpTestServer();
                if (web <= 0)
                    smoke.fail("the HTTP test server didn't start");
                ids.local = make("Smoke local", { kind: "local" });
                ids.remote = make("Smoke remote", { kind: "remote" });
                ids.dynamic = make("Smoke SOCKS", { kind: "dynamic" });
                for (const key of ["local", "remote", "dynamic"])
                    Tunnels.setOn(ids[key], true);
                // The first question also shows in its dialog.
                return wait("a tunnel's question", () => tunnels.some(entry => entry.prompt.length > 0), () => {
                    view.answer(tunnels.find(entry => entry.prompt.length > 0).id);
                    return [];
                });
            },
            () => {
                question.close();
                return wait("the three tunnels to run", () => running(ids.local) && running(ids.remote) && running(ids.dynamic));
            },
            () => {
                get(view.tunnel(ids.local).port);
                return wait("HTTP through the local tunnel", fetched);
            },
            () => {
                get(view.tunnel(ids.remote).port);
                return wait("HTTP through the remote tunnel", fetched, () => {
                    console.info("smoke test: local, remote and dynamic tunnels ran on their own connection and carried HTTP");
                    return [];
                });
            },
            () => wait("the traffic counters", () => view.tunnel(ids.local).total >= 1 && view.tunnel(ids.local).received > 0),
            // More traffic: the row stays the same item (a menu open on it would stay open).
            () => {
                const row = list.itemAtIndex(0);
                const total = view.tunnel(ids.local).total;
                get(view.tunnel(ids.local).port);
                return wait("more traffic", () => fetched() && view.tunnel(ids.local).total > total, () => {
                    if (row === null || list.itemAtIndex(0) !== row)
                        smoke.fail("the tunnel's row was made again when its counters changed");
                    return [];
                });
            },
            // Tied to H00000: waits, runs with a terminal session, waits again when it closes.
            () => {
                ids.tied = make("Smoke tied", { kind: "local", tied: true });
                Tunnels.setOn(ids.tied, true);
                return wait("the tied tunnel to wait", () => view.tunnel(ids.tied).state === "waiting");
            },
            () => {
                if (!view.shell.connectHost("H00000", "tab"))
                    smoke.fail("connecting to H00000 opened nothing");
                tabId = view.shell.currentTabId;
                pane = view.shell.currentTerminal;
                const answeredPane = {};
                return wait("the tied tunnel to run with the session", () => {
                    const promptText = pane ? pane.terminal.prompt : "";
                    if (promptText.length > 0) {
                        const question = JSON.parse(promptText);
                        if (answeredPane.id !== question.id) {
                            answeredPane.id = question.id;
                            if (question.kind === "hostKey")
                                pane.terminal.answerPrompt(question.id, "trust-once", []);
                            else
                                pane.terminal.answerPrompt(question.id, "submit", ["right password"]); // lint-qml: allow (the test server's password)
                        }
                    }
                    return view.tunnel(ids.tied).state === "running";
                });
            },
            () => {
                get(view.tunnel(ids.tied).port);
                return wait("HTTP through the tied tunnel", fetched);
            },
            () => {
                view.shell.closeTabById(tabId);
                view.shell.showView("tunnels");
                return wait("the tied tunnel to wait again", () => view.tunnel(ids.tied).state === "waiting", () => {
                    console.info("smoke test: a tunnel tied to a host ran with its terminal session");
                    return [];
                });
            },
            () => {
                editor.show(view.tunnel(ids.local));
                if (editor.problem.length > 0)
                    smoke.fail("the editor finds a problem in a saved tunnel: " + editor.problem);
                editor.set("bindAddress", "0.0.0.0");
                if (!editor.exposed)
                    smoke.fail("the editor doesn't see 0.0.0.0 as exposed");
                editor.set("destinationPort", 0);
                if (editor.problem.length === 0)
                    smoke.fail("the editor accepts a destination port of 0");
            },
            () => {
                editor.close();
                importDialog.show();
            },
            () => {
                importDialog.close();
                const copy = Tunnels.duplicate(ids.dynamic);
                if (copy.length === 0 || view.tunnel(copy).on)
                    smoke.fail("duplicating a tunnel failed");
                for (const id of Object.values(ids).concat([copy]))
                    Tunnels.remove(id);
                return wait("the tunnels to go", () => Tunnels.count === 0, () => {
                    console.info("smoke test: the tunnel editor, the import dialog, duplicate and delete work");
                    return [];
                });
            }
        ];
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.spacingLg
        spacing: Theme.spacingMd
        visible: view.tunnels.length > 0

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingSm

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0

                OsText {
                    text: qsTr("Tunnels")
                    size: "title"
                }
                OsText {
                    Layout.fillWidth: true
                    text: qsTr("%n tunnel(s), %1 running", "", Tunnels.count).arg(Tunnels.running)
                    muted: true
                    elide: Text.ElideRight
                }
            }

            OsButton {
                text: qsTr("Import…")
                iconName: "import"
                onClicked: importDialog.show()
            }

            OsButton {
                text: qsTr("New tunnel")
                iconName: "plus"
                variant: "primary"
                onClicked: view.newTunnel()
            }
        }

        OsText {
            Layout.fillWidth: true
            visible: Tunnels.readOnly
            text: qsTr("tunnels.toml comes from a newer OpenSesh or can't be read: changes here are not saved.")
            color: Theme.warning
            wrapMode: Text.Wrap
        }

        ListView {
            id: list

            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            spacing: Theme.spacingXs
            model: ListModel {
                id: rows
            }
            currentIndex: 0
            activeFocusOnTab: true
            keyNavigationEnabled: true
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Tunnels")

            T.ScrollBar.vertical: OsScrollBar {}

            Keys.onPressed: event => {
                const entry = view.tunnels[currentIndex];
                if (event.key === Qt.Key_N && event.modifiers & Qt.ControlModifier) {
                    view.newTunnel();
                } else if (!entry) {
                    return;
                } else if (event.key === Qt.Key_Space) {
                    Tunnels.setOn(entry.id, !entry.on);
                } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                    view.edit(entry.id);
                } else if (event.key === Qt.Key_Delete) {
                    view.askDelete(entry.id);
                } else {
                    return;
                }
                event.accepted = true;
            }

            delegate: TunnelRow {}
        }
    }

    OsEmptyState {
        anchors.fill: parent
        visible: view.tunnels.length === 0
        iconName: "waypoints"
        title: qsTr("No tunnels")
        description: qsTr("Forward a port here to a server's network (local), a server's port back to this computer (remote), or run a SOCKS proxy through a server (dynamic).")

        Row {
            spacing: Theme.spacingSm

            OsButton {
                text: qsTr("Import from ~/.ssh/config…")
                iconName: "import"
                onClicked: importDialog.show()
            }
            OsButton {
                text: qsTr("New tunnel")
                iconName: "plus"
                variant: "primary"
                onClicked: view.newTunnel()
            }
        }
    }

    component TunnelRow: Rectangle {
        id: row

        required property string tunnelId
        required property int index
        readonly property var modelData: view.tunnel(tunnelId) ?? view.blank

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
            onDoubleClicked: view.edit(row.modelData.id)
        }

        RowLayout {
            id: rowLayout

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.leftMargin: Theme.spacingMd
            anchors.rightMargin: Theme.spacingSm
            spacing: Theme.spacingMd

            OsSwitch {
                checked: row.modelData.on
                focusPolicy: Qt.NoFocus
                Accessible.name: qsTr("Run %1").arg(view.title(row.modelData))
                onToggled: Tunnels.setOn(row.modelData.id, checked)
            }

            OsIcon {
                name: view.kindIcon(row.modelData.kind)
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
                        Layout.fillWidth: true
                        text: view.title(row.modelData)
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                    }
                    OsTag {
                        text: view.kindText(row.modelData.kind)
                    }
                }

                OsText {
                    Layout.fillWidth: true
                    visible: row.modelData.name.length > 0
                    text: view.route(row.modelData)
                    muted: true
                    size: "small"
                    elide: Text.ElideMiddle
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacingSm

                    Rectangle {
                        implicitWidth: Theme.spacingSm
                        implicitHeight: Theme.spacingSm
                        radius: width / 2
                        color: view.stateColor(row.modelData.state)
                    }
                    OsText {
                        Layout.fillWidth: true
                        text: {
                            const traffic = view.traffic(row.modelData);
                            const state = view.stateText(row.modelData);
                            return traffic.length > 0 ? qsTr("%1 · %2").arg(state).arg(traffic) : state;
                        }
                        muted: row.modelData.state !== "failed"
                        color: row.modelData.state === "failed" ? Theme.danger : Theme.textMuted
                        size: "small"
                        elide: Text.ElideRight
                    }
                }
            }

            OsIcon {
                visible: row.modelData.tied
                name: "link"
                size: Theme.iconSizeSmall
                color: Theme.textMuted
                OsTooltip {
                    text: qsTr("Runs while a terminal session to %1 is connected").arg(row.modelData.hostName)
                    visible: tiedHover.hovered
                }
                HoverHandler {
                    id: tiedHover
                }
            }

            OsIcon {
                visible: row.modelData.exposed
                name: "triangle-alert"
                size: Theme.iconSizeSmall
                color: Theme.warning
                OsTooltip {
                    text: row.modelData.kind === "remote" ? qsTr("Listens on every interface of the server: others on its network can use it.")
                                                          : qsTr("Listens beyond this computer: others on the network can use it.")
                    visible: exposedHover.hovered
                }
                HoverHandler {
                    id: exposedHover
                }
            }

            OsButton {
                visible: row.modelData.prompt.length > 0
                text: qsTr("Answer…")
                iconName: "circle-help"
                onClicked: view.answer(row.modelData.id)
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
                text: row.modelData.on ? qsTr("Stop") : qsTr("Start")
                iconName: row.modelData.on ? "pause" : "play"
                shortcutText: qsTr("Space")
                onTriggered: Tunnels.setOn(row.modelData.id, !row.modelData.on)
            }
            OsMenuItem {
                text: qsTr("Edit…")
                iconName: "pencil"
                shortcutText: qsTr("Enter")
                onTriggered: view.edit(row.modelData.id)
            }
            OsMenuItem {
                text: qsTr("Duplicate")
                iconName: "copy"
                onTriggered: Tunnels.duplicate(row.modelData.id)
            }
            OsMenuItem {
                visible: row.modelData.kind !== "remote"
                height: visible ? implicitHeight : 0
                text: qsTr("Copy the address")
                iconName: "link"
                onTriggered: view.copyAddress(row.modelData)
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

    TunnelEditorDialog {
        id: editor
    }

    TunnelImportDialog {
        id: importDialog
    }

    TunnelQuestionDialog {
        id: question
    }

    OsDialog {
        id: deleteDialog

        property string tunnelId: ""
        readonly property var entry: view.tunnel(tunnelId)

        title: qsTr("Delete this tunnel?")
        acceptText: qsTr("Delete")
        dangerous: true
        onAccepted: Tunnels.remove(tunnelId)

        OsText {
            width: Math.min(Theme.spacingXxl * 12, deleteDialog.maxWidth - deleteDialog.leftPadding - deleteDialog.rightPadding)
            text: deleteDialog.entry ? view.title(deleteDialog.entry) : ""
            wrapMode: Text.Wrap
        }
    }
}
