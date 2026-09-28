pragma ComponentBehavior: Bound

// Asks before a window closes with work still running (Settings > General, "Confirm before
// closing with active sessions", on by default): what ends with it (terminal sessions, and for the
// whole app its tunnels and file transfers), Cancel or Close, and "Don't ask again".
// Functions: show(wholeApp, sessions, tunnels, transfers). Signal: confirmed().
import QtQuick
import cc.caixa.opensesh

OsDialog {
    id: dialog

    // The main window (every window, tunnel and transfer ends) or one detached window.
    property bool wholeApp: true
    property int sessions: 0
    property int tunnels: 0
    property int transfers: 0
    readonly property var lines: [
        sessions > 0 ? qsTr("%n terminal session(s)", "", sessions) : "",
        tunnels > 0 ? qsTr("%n tunnel(s)", "", tunnels) : "",
        transfers > 0 ? qsTr("%n file transfer(s)", "", transfers) : ""
    ].filter(line => line.length > 0)

    signal confirmed

    function show(all, sessionCount, tunnelCount, transferCount) {
        wholeApp = all;
        sessions = sessionCount;
        tunnels = tunnelCount;
        transfers = transferCount;
        dontAsk.checked = false;
        open();
    }

    title: wholeApp ? qsTr("Close OpenSesh?") : qsTr("Close this window?")
    acceptText: wholeApp ? qsTr("Close OpenSesh") : qsTr("Close window")
    dangerous: true
    onAccepted: {
        if (dontAsk.checked)
            AppSettings.confirmCloseWithSessions = false;
        confirmed();
    }

    Column {
        width: Math.min(Theme.spacingXxl * 12, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingSm

        OsText {
            width: parent.width
            text: dialog.wholeApp ? qsTr("These end when OpenSesh closes:") : qsTr("These end when the window closes:")
            wrapMode: Text.Wrap
        }

        Repeater {
            model: dialog.lines

            delegate: OsText {
                required property string modelData

                width: parent ? parent.width : 0
                leftPadding: Theme.spacingMd
                text: qsTr("• %1").arg(modelData)
                wrapMode: Text.Wrap
            }
        }

        OsCheckBox {
            id: dontAsk

            topPadding: Theme.spacingSm
            text: qsTr("Don't ask again")
        }
    }
}
