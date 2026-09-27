pragma ComponentBehavior: Bound

// SFTP view (PLAN §5.4, Sprint 8): two file panes side by side, each showing this computer or a
// server (a saved host, or quick-connect text, on a connection of its own), and the transfer
// queue below. F5 and F6 copy or move the selection to the other pane; files drag between the
// panes and in from the file manager.
// Functions: setSource(side, source) (`source`: {mode, hostId, target, title}), pane(side),
// smokeSteps(smoke).
import QtQuick
import QtQuick.Layouts
import cc.caixa.opensesh

Item {
    id: view

    readonly property Item shell: WindowRegistry.mainShell
    property bool showTransfers: true

    function pane(side) {
        return side === 0 ? leftSide.pane : rightSide.pane;
    }

    function setSource(side, source) {
        (side === 0 ? leftSide : rightSide).use(source);
    }

    RowLayout {
        id: sides

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: transfersArea.top
        spacing: 0

        Side {
            id: leftSide

            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            other: rightSide
            initial: ({ mode: "local", hostId: "", target: "", title: qsTr("This computer") })
        }

        Rectangle {
            Layout.fillHeight: true
            implicitWidth: Theme.borderWidth
            color: Theme.border
        }

        Side {
            id: rightSide

            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            other: leftSide
            initial: ({ mode: "remote", hostId: "", target: "", title: "" })
        }
    }

    // The queue, below a line with its switch.
    Rectangle {
        id: transfersArea

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: view.showTransfers ? Math.min(Theme.spacingXxl * 7, view.height / 3) : transfersToggle.height
        color: Theme.surface

        Rectangle {
            width: parent.width
            height: Theme.borderWidth
            color: Theme.border
        }

        OsIconButton {
            id: transfersToggle

            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: Theme.spacingXs
            z: 1
            iconName: view.showTransfers ? "chevron-down" : "chevron-up"
            toolTip: view.showTransfers ? qsTr("Hide the transfers") : qsTr("Show the transfers")
            onClicked: view.showTransfers = !view.showTransfers
        }

        TransfersPanel {
            anchors.fill: parent
            anchors.margins: Theme.spacingXs
            anchors.rightMargin: transfersToggle.width + Theme.spacingSm
            visible: view.showTransfers
        }
    }

    // One side: where its files come from, and the pane.
    component Side: ColumnLayout {
        id: side

        required property Item other
        required property var initial
        property var source: initial
        readonly property Item pane: loader.item
        // Saved hosts with files to browse (the list follows edits of hosts.toml).
        readonly property var hosts: Hosts.revision >= 0 ? JSON.parse(Hosts.search("", "all", "", "", "name") || "[]")
            .filter(host => host.protocol === "ssh" || host.protocol === "sftp") : []
        readonly property var choices: [{ text: qsTr("This computer"), value: "local" }]
            .concat(hosts.map(host => ({ text: host.name, value: "host:" + host.id })))
            .concat([{ text: qsTr("Connect to user@host…"), value: "other" }])

        function use(next) {
            source = next;
            loader.active = false;
            loader.active = true;
        }

        function choose(value) {
            if (value === "local") {
                use({ mode: "local", hostId: "", target: "", title: qsTr("This computer") });
            } else if (value === "other") {
                targetDialog.side = side;
                targetDialog.open();
            } else if (value.startsWith("host:")) {
                const id = value.slice(5);
                const host = side.hosts.find(entry => entry.id === id);
                use({ mode: "remote", hostId: id, target: "", title: host ? host.name : id });
            }
        }

        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.spacingXs
            spacing: Theme.spacingSm

            OsIcon {
                name: side.source.mode === "local" ? "hard-drive" : "server"
                size: Theme.iconSize
                color: Theme.textMuted
            }

            OsComboBox {
                id: picker

                Layout.fillWidth: true
                model: side.choices
                textRole: "text"
                valueRole: "value"
                displayText: side.source.mode === "local" ? qsTr("This computer")
                           : side.source.title.length > 0 ? side.source.title : qsTr("Choose a host")
                Accessible.name: qsTr("Files of")
                onActivated: index => side.choose(side.choices[index].value)
            }

            OsIconButton {
                iconName: "arrow-left-right"
                toolTip: side.other === rightSide ? qsTr("Copy the selection to the right (F5)") : qsTr("Copy the selection to the left (F5)")
                enabled: side.pane !== null && side.pane.ready && side.other.pane !== null && side.other.pane.ready
                onClicked: side.pane.copyToPeer(false)
            }
        }

        Loader {
            id: loader

            Layout.fillWidth: true
            Layout.fillHeight: true

            sourceComponent: FilePane {
                mode: side.source.mode
                hostId: side.source.hostId
                target: side.source.target
                title: side.source.title
                startPath: side.source.startPath ?? ""
                peer: side.other.pane
                onChooseSource: picker.popup.open()
            }
        }
    }

    OsDialog {
        id: targetDialog

        property Item side: null

        title: qsTr("Connect to")
        acceptText: qsTr("Connect")
        acceptEnabled: targetField.text.trim().length > 0

        onOpened: {
            targetField.text = "";
            targetField.forceActiveFocus();
        }
        onAccepted: {
            const text = targetField.text.trim();
            if (side)
                side.use({ mode: "remote", hostId: "", target: text, title: text });
        }

        OsTextField {
            id: targetField

            implicitWidth: Math.min(Theme.spacingXxl * 10, targetDialog.maxWidth - targetDialog.leftPadding - targetDialog.rightPadding)
            placeholderText: qsTr("user@host:port")
            Accessible.name: qsTr("Where to connect")
            onAccepted: {
                if (targetDialog.acceptEnabled)
                    targetDialog.accept();
            }
        }
    }
}
