pragma ComponentBehavior: Bound

// Tunnels view (PLAN §5.4, Sprint 9): the port forwarding manager. Each tunnel shows a switch,
// its route in words, what it is doing (running, connecting, waiting for a session of its host,
// retrying, failed) and its traffic, with a menu to edit, duplicate, copy its address or delete
// it. A tunnel that listens beyond this computer (or, for a remote one, beyond the server) is
// marked; one that asks something (a host key, a password) has an Answer button.
// Keys in the list: Up/Down, Space (switch), Enter (edit), Delete, Ctrl+N (new).
// Functions: newTunnel(), edit(id), answer(id), smokeSteps(smoke), showSample(page).
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: view

    readonly property Item shell: WindowRegistry.mainShell
    readonly property var tunnels: JSON.parse(Tunnels.list || "[]")

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

    function copyAddress(entry) {
        Platform.copyText(entry.kind === "dynamic" ? "socks5h://" + listening(entry) : listening(entry));
        Toasts.show(qsTr("Copied %1.").arg(listening(entry)), "success");
    }

    function askDelete(id) {
        deleteDialog.tunnelId = id;
        deleteDialog.open();
    }

    Connections {
        target: Tunnels

        function onNeedsAnswer(id, name) {
            Toasts.show(qsTr("The tunnel %1 needs an answer to connect.").arg(name), "warning");
        }
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
            model: view.tunnels
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
