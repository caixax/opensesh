pragma ComponentBehavior: Bound

// The tunnel editor (Sprint 9): a new tunnel, or changes to one. Kind, the host it goes through
// (a saved SSH host or `user@host` text), whether it runs on its own connection or with the
// host's terminal sessions, where it listens and where connections go, and whether it starts
// with OpenSesh and reconnects. `Tunnels.check` says what is missing as you type. Saving a
// tunnel that listens beyond the loopback asks once (PLAN §8).
// Functions: show(entry) (an entry of Tunnels.list, or null for a new one), save().
import QtQuick
import QtQuick.Layouts
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property string tunnelId: ""
    property var draft: ({})
    // Confirmed (or already listening beyond the loopback when it was opened).
    property bool exposureConfirmed: false
    readonly property string problem: Tunnels.check(JSON.stringify(draft))
    readonly property bool exposed: !Tunnels.isLoopback(draft.bindAddress ?? "")
    readonly property var hosts: Hosts.revision >= 0 ? JSON.parse(Hosts.search("", "all", "", "", "name") || "[]")
        .filter(host => host.protocol === "ssh") : []
    readonly property var hostChoices: hosts.map(host => ({ text: host.name, value: host.id }))
        .concat([{ text: qsTr("Other: user@host"), value: "" }])
    readonly property real fieldWidth: Math.min(Theme.spacingXxl * 16, maxWidth - leftPadding - rightPadding)

    function show(entry) {
        tunnelId = entry ? entry.id : "";
        draft = entry ? {
            id: entry.id,
            name: entry.name,
            kind: entry.kind,
            host: entry.host,
            target: entry.target,
            bindAddress: entry.bindAddress,
            bindPort: entry.bindPort,
            destinationHost: entry.destinationHost,
            destinationPort: entry.destinationPort,
            tied: entry.tied,
            autostart: entry.autostart,
            reconnect: entry.reconnect
        } : {
            id: "",
            name: "",
            kind: "local",
            host: hosts.length > 0 ? hosts[0].id : "",
            target: "",
            bindAddress: "127.0.0.1",
            bindPort: 0,
            destinationHost: "localhost",
            destinationPort: 0,
            tied: false,
            autostart: false,
            reconnect: true
        };
        exposureConfirmed = entry !== null && entry.exposed;
        open();
    }

    function set(key, value) {
        const next = Object.assign({}, draft);
        next[key] = value;
        if (key === "host" && value.length === 0)
            next.tied = false;
        draft = next;
    }

    function save() {
        if (problem.length > 0)
            return;
        if (exposed && !exposureConfirmed) {
            exposureDialog.open();
            return;
        }
        if (Tunnels.save(JSON.stringify(draft)).length > 0)
            accept();
    }

    title: tunnelId.length > 0 ? qsTr("Edit tunnel") : qsTr("New tunnel")
    acceptText: qsTr("Save")
    acceptEnabled: problem.length === 0
    closeOnAccept: false
    onAcceptClicked: save()

    Column {
        width: dialog.fieldWidth
        spacing: Theme.spacingMd

        OsFormRow {
            width: parent.width
            label: qsTr("Name")

            OsTextField {
                width: parent.width
                text: dialog.draft.name ?? ""
                placeholderText: qsTr("Optional")
                Accessible.name: qsTr("Name")
                onTextEdited: dialog.set("name", text)
            }
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Kind")
            helpText: dialog.draft.kind === "local" ? qsTr("A port on this computer reaches a host the server can reach (ssh -L).")
                    : dialog.draft.kind === "remote" ? qsTr("A port on the server reaches a host this computer can reach (ssh -R).")
                    : qsTr("A SOCKS5 proxy on this computer: programs that use it connect from the server (ssh -D).")

            OsComboBox {
                id: kindBox

                readonly property var choices: [
                    { text: qsTr("Local"), value: "local" },
                    { text: qsTr("Remote"), value: "remote" },
                    { text: qsTr("Dynamic (SOCKS)"), value: "dynamic" }
                ]

                width: parent.width
                model: choices
                textRole: "text"
                valueRole: "value"
                currentIndex: Math.max(0, choices.findIndex(choice => choice.value === dialog.draft.kind))
                Accessible.name: qsTr("Kind")
                onActivated: dialog.set("kind", currentValue)
            }
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Through")

            Column {
                width: parent.width
                spacing: Theme.spacingSm

                OsComboBox {
                    width: parent.width
                    model: dialog.hostChoices
                    textRole: "text"
                    valueRole: "value"
                    currentIndex: Math.max(0, dialog.hostChoices.findIndex(choice => choice.value === (dialog.draft.host ?? "")))
                    Accessible.name: qsTr("SSH host")
                    onActivated: dialog.set("host", currentValue)
                }

                OsTextField {
                    width: parent.width
                    visible: (dialog.draft.host ?? "").length === 0
                    text: dialog.draft.target ?? ""
                    placeholderText: qsTr("user@host:port")
                    Accessible.name: qsTr("SSH server")
                    onTextEdited: dialog.set("target", text)
                }
            }
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Runs")
            helpText: dialog.draft.tied ? qsTr("While a terminal session to the host is connected, on its connection.")
                                        : qsTr("On a connection of its own, while it is switched on.")

            OsComboBox {
                readonly property var choices: [
                    { text: qsTr("On its own"), value: false },
                    { text: qsTr("With the host's terminal sessions"), value: true }
                ]

                width: parent.width
                enabled: (dialog.draft.host ?? "").length > 0
                model: choices
                textRole: "text"
                valueRole: "value"
                currentIndex: dialog.draft.tied ? 1 : 0
                Accessible.name: qsTr("Runs")
                onActivated: dialog.set("tied", currentValue)
            }
        }

        OsFormRow {
            width: parent.width
            label: dialog.draft.kind === "remote" ? qsTr("Listen on the server") : qsTr("Listen on this computer")
            helpText: qsTr("Port 0 takes any free port.")
            errorText: dialog.exposed ? (dialog.draft.kind === "remote" ? qsTr("Every interface of the server: others on its network can use this tunnel (if the server allows it).")
                                                                         : qsTr("Beyond this computer: others on the network can use this tunnel.")) : ""

            EndpointFields {
                width: parent.width
                host: dialog.draft.bindAddress ?? ""
                port: dialog.draft.bindPort ?? 0
                hostName: qsTr("Listening address")
                portName: qsTr("Listening port")
                onHostEdited: value => dialog.set("bindAddress", value)
                onPortEdited: value => dialog.set("bindPort", value)
            }
        }

        OsFormRow {
            width: parent.width
            visible: dialog.draft.kind !== "dynamic"
            label: dialog.draft.kind === "remote" ? qsTr("Destination, from here") : qsTr("Destination, from the server")

            EndpointFields {
                width: parent.width
                host: dialog.draft.destinationHost ?? ""
                port: dialog.draft.destinationPort ?? 0
                hostName: qsTr("Destination host")
                portName: qsTr("Destination port")
                onHostEdited: value => dialog.set("destinationHost", value)
                onPortEdited: value => dialog.set("destinationPort", value)
            }
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Start with OpenSesh")

            OsSwitch {
                checked: dialog.draft.autostart ?? false
                Accessible.name: qsTr("Start with OpenSesh")
                onToggled: dialog.set("autostart", checked)
            }
        }

        OsFormRow {
            width: parent.width
            visible: !dialog.draft.tied
            label: qsTr("Reconnect")
            helpText: qsTr("When the connection is lost, try again after 1, 2, 4… up to 30 s.")

            OsSwitch {
                checked: dialog.draft.reconnect ?? true
                Accessible.name: qsTr("Reconnect")
                onToggled: dialog.set("reconnect", checked)
            }
        }

        OsText {
            width: parent.width
            visible: dialog.problem.length > 0
            text: dialog.problem
            color: Theme.danger
            wrapMode: Text.Wrap
        }
    }

    // A host or address and a port, side by side.
    component EndpointFields: RowLayout {
        id: fields

        required property string host
        required property int port
        required property string hostName
        required property string portName

        signal hostEdited(string value)
        signal portEdited(int value)

        spacing: Theme.spacingSm

        OsTextField {
            Layout.fillWidth: true
            text: fields.host
            Accessible.name: fields.hostName
            onTextEdited: fields.hostEdited(text.trim())
        }

        OsText {
            text: qsTr(":")
            muted: true
        }

        OsTextField {
            Layout.preferredWidth: Theme.spacingXxl * 2
            // An empty field while typing, rather than a 0 that comes back.
            text: fields.port > 0 || !activeFocus ? String(fields.port) : ""
            inputMethodHints: Qt.ImhDigitsOnly
            validator: IntValidator {
                bottom: 0
                top: 65535
            }
            Accessible.name: fields.portName
            onTextEdited: fields.portEdited(text.length > 0 ? parseInt(text, 10) : 0)
        }
    }

    OsDialog {
        id: exposureDialog

        title: qsTr("Listen beyond this computer?")
        acceptText: qsTr("Save anyway")
        dangerous: true
        onAccepted: {
            dialog.exposureConfirmed = true;
            dialog.save();
        }

        OsText {
            width: Math.min(Theme.spacingXxl * 13, exposureDialog.maxWidth - exposureDialog.leftPadding - exposureDialog.rightPadding)
            wrapMode: Text.Wrap
            text: dialog.draft.kind === "remote"
                  ? qsTr("The server will listen on %1 on every interface it allows: anyone who can reach it there reaches %2 on this computer.")
                    .arg(dialog.draft.bindPort ?? 0).arg(dialog.draft.destinationHost ?? "")
                  : qsTr("OpenSesh will listen on %1 beyond this computer: anyone on the network who can reach it can use the tunnel into the server's network.")
                    .arg(dialog.draft.bindAddress ?? "")
        }
    }
}
