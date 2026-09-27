pragma ComponentBehavior: Bound

// A tunnel's question (Sprint 9): its own connection asks for a host key decision, a password,
// a passphrase or a one-time code. The same cards as a terminal pane (SshOverlay), in a dialog;
// it closes by itself once nothing is asked.
// Functions: show(id).
import QtQuick
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property string tunnelId: ""
    readonly property var entry: JSON.parse(Tunnels.list || "[]").find(tunnel => tunnel.id === tunnelId) ?? null
    readonly property string promptText: entry ? entry.prompt : ""

    function show(id) {
        tunnelId = id;
        open();
        overlay.focusPrompt();
    }

    title: entry ? (entry.name.length > 0 ? entry.name : entry.hostName) : ""
    acceptText: ""
    rejectText: qsTr("Close")

    onPromptTextChanged: {
        if (visible && promptText.length === 0)
            close();
    }

    // What SshOverlay talks to: the tunnel's question and its answers.
    QtObject {
        id: source

        readonly property string connection: ""
        readonly property string prompt: dialog.promptText
        readonly property bool activeFocus: false

        function answerPrompt(id, action, secrets) {
            return Tunnels.answerPrompt(dialog.tunnelId, id, action, secrets);
        }

        function sendText(text) {
        }

        function forceActiveFocus(reason) {
        }
    }

    Item {
        implicitWidth: Math.min(Theme.spacingXxl * 14, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        implicitHeight: Theme.spacingXxl * 9

        SshOverlay {
            id: overlay

            anchors.fill: parent
            terminal: source
            shell: WindowRegistry.mainShell ?? dialog.contentItem
            label: dialog.entry ? dialog.entry.hostName : ""
            banner: false
        }
    }
}
