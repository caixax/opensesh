pragma ComponentBehavior: Bound

// Import from ~/.ssh/config (Sprint 9): its LocalForward, RemoteForward and DynamicForward lines,
// each as a tunnel tied to its host (as in OpenSSH, up while a session is). Forwards of hosts
// that aren't in OpenSesh can't be picked (import the hosts first); ones that are tunnels
// already are left unticked.
// Functions: show().
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property var candidates: []
    property var picked: ({})
    readonly property int pickedCount: Object.keys(picked).filter(key => picked[key]).length

    function show() {
        candidates = JSON.parse(Tunnels.importCandidates() || "[]");
        const next = {};
        for (const candidate of candidates)
            next[candidate.key] = candidate.host.length > 0 && !candidate.known;
        picked = next;
        open();
    }

    function toggle(key, on) {
        const next = Object.assign({}, picked);
        next[key] = on;
        picked = next;
    }

    function describe(candidate) {
        const bind = (candidate.bindAddress.indexOf(":") >= 0 ? "[" + candidate.bindAddress + "]" : candidate.bindAddress) + ":" + candidate.bindPort;
        const to = candidate.destinationHost + ":" + candidate.destinationPort;
        return candidate.kind === "local" ? qsTr("LocalForward %1 → %2").arg(bind).arg(to)
             : candidate.kind === "remote" ? qsTr("RemoteForward %1 → %2").arg(bind).arg(to)
             : qsTr("DynamicForward %1").arg(bind);
    }

    title: qsTr("Import tunnels from ~/.ssh/config")
    acceptText: qsTr("Import %n", "", pickedCount)
    acceptEnabled: pickedCount > 0
    onAccepted: {
        const keys = Object.keys(picked).filter(key => picked[key]);
        const imported = Tunnels.importForwards(keys);
        Toasts.show(qsTr("Imported %n tunnel(s).", "", imported), "success");
    }

    ColumnLayout {
        width: Math.min(Theme.spacingXxl * 16, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingSm

        OsText {
            Layout.fillWidth: true
            text: dialog.candidates.length > 0
                  ? qsTr("Each forward becomes a tunnel of its host that runs while a terminal session to it is connected, as with OpenSSH.")
                  : qsTr("~/.ssh/config has no LocalForward, RemoteForward or DynamicForward lines in Host blocks OpenSesh reads.")
            muted: true
            wrapMode: Text.Wrap
        }

        ListView {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(contentHeight, Theme.spacingXxl * 8)
            visible: dialog.candidates.length > 0
            clip: true
            model: dialog.candidates
            spacing: Theme.spacingXs

            T.ScrollBar.vertical: OsScrollBar {}

            delegate: RowLayout {
                id: candidateRow

                required property var modelData

                width: ListView.view.width
                spacing: Theme.spacingSm

                OsCheckBox {
                    enabled: candidateRow.modelData.host.length > 0
                    checked: dialog.picked[candidateRow.modelData.key] === true
                    text: candidateRow.modelData.alias
                    onToggled: dialog.toggle(candidateRow.modelData.key, checked)
                }

                OsText {
                    Layout.fillWidth: true
                    text: dialog.describe(candidateRow.modelData)
                    font.family: Theme.monoFontFamily
                    size: "small"
                    elide: Text.ElideRight
                }

                OsText {
                    visible: candidateRow.modelData.host.length === 0 || candidateRow.modelData.known
                    text: candidateRow.modelData.host.length === 0 ? qsTr("Host not in OpenSesh") : qsTr("Already a tunnel")
                    muted: true
                    size: "small"
                }
            }
        }
    }
}
