pragma ComponentBehavior: Bound

// "Install my key on the server" (Sprint 7), like ssh-copy-id: pick a public key (from the
// keychain or a running agent) and OpenSesh connects to the host in a new tab as usual (host key,
// password and one-time code prompts in the pane), then adds the key to ~/.ssh/authorized_keys
// unless it is there already. A toast says how it went.
// Functions: show(hostId).
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    required property Item shell
    property string hostId: ""
    property string hostName: ""
    property int selected: -1
    // [{name, detail, line}]: the keychain's keys, then the agents' keys.
    readonly property var choices: {
        const out = [];
        for (const key of JSON.parse(Keychain.keys || "[]"))
            out.push({ name: key.name, detail: key.label + " · " + key.fingerprint, line: key.publicKey });
        for (const agent of JSON.parse(Keychain.agents || "[]")) {
            for (const key of agent.keys) {
                if (!out.some(entry => entry.line.split(" ")[1] === key.publicKey.split(" ")[1]))
                    out.push({
                        name: key.comment.length > 0 ? key.comment : key.fingerprint,
                        detail: qsTr("In an agent · %1 · %2").arg(key.label).arg(key.fingerprint),
                        line: key.publicKey
                    });
            }
        }
        return out;
    }

    function show(id) {
        const host = JSON.parse(Hosts.hostJson(id) || "{}");
        if (!host.id)
            return;
        hostId = id;
        hostName = host.name;
        selected = -1;
        Keychain.refreshAgents();
        open();
        list.forceActiveFocus();
    }

    function install() {
        const choice = choices[selected];
        if (!choice)
            return;
        accept();
        Hosts.recordHost(hostId);
        shell.openConnection({ kind: "ssh", host: hostId, installKey: choice.line }, "tab");
    }

    title: qsTr("Install a key on %1").arg(hostName)
    acceptText: qsTr("Connect and install")
    acceptEnabled: selected >= 0 && selected < choices.length
    closeOnAccept: false

    onAcceptClicked: install()

    Column {
        width: Math.min(Theme.spacingXxl * 14, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsText {
            width: parent.width
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            text: qsTr("OpenSesh connects to the host in a new tab and adds the public key to ~/.ssh/authorized_keys there, unless it is there already. Next time, the key logs you in.")
        }

        ListView {
            id: list

            width: parent.width
            height: Math.min(contentHeight, Theme.spacingXxl * 6)
            visible: dialog.choices.length > 0
            clip: true
            model: dialog.choices
            currentIndex: dialog.selected
            keyNavigationEnabled: true
            boundsBehavior: Flickable.StopAtBounds
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Keys")

            onCurrentIndexChanged: dialog.selected = currentIndex

            delegate: OsListRow {
                required property var modelData
                required property int index

                width: ListView.view.width
                iconName: "key-round"
                text: modelData.name
                subtitle: modelData.detail
                selected: index === dialog.selected
                focusPolicy: Qt.NoFocus
                onClicked: dialog.selected = index
                onDoubleClicked: {
                    dialog.selected = index;
                    dialog.install();
                }
            }

            Keys.onReturnPressed: dialog.install()
            Keys.onEnterPressed: dialog.install()

            T.ScrollBar.vertical: OsScrollBar {}
        }

        OsText {
            width: parent.width
            visible: dialog.choices.length === 0
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            muted: true
            text: qsTr("No keys yet: generate or import one in the keychain, or start an SSH agent with your key.")
        }

        OsButton {
            visible: dialog.choices.length === 0
            text: qsTr("Open the keychain")
            iconName: "key-round"
            onClicked: {
                dialog.close();
                dialog.shell.showView("keychain");
            }
        }
    }
}
