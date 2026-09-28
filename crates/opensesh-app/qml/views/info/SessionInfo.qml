pragma ComponentBehavior: Bound

// The side panel's Info tab (PLAN Sprint 11): the host of the focused terminal: system, kernel,
// architecture, CPUs, uptime and load, memory, disks, addresses and logged-in users, read over
// the pane's SSH connection when the tab shows it (again after a reconnection, or with Refresh),
// with the remote monitor's live CPU, memory and network while it runs. "Copy as text" copies it
// all. Local terminals, OpenSSH panes and servers without a POSIX `sh` say why there is nothing.
//   terminalPane: Item   the focused TerminalPane, or null (null while the tab is hidden)
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: panel

    property Item terminalPane: null
    readonly property TerminalItem terminal: terminalPane ? terminalPane.terminal : null
    readonly property string sshState: terminal && terminal.connection.length > 0 ? (JSON.parse(terminal.connection).state ?? "") : ""
    readonly property var info: terminal && terminal.hostInfo.length > 0 ? JSON.parse(terminal.hostInfo) : null
    readonly property var monitor: terminal && terminal.monitor.length > 0 ? JSON.parse(terminal.monitor) : null
    readonly property var live: monitor && monitor.state === "reading" ? monitor : null
    // The live values while the monitor runs, else those read with the host info.
    readonly property var snapshot: live ?? (info && info.state === "ready" ? info.snapshot : null)
    // "none", "local", "openssh", "offline", "reading", "failed" or "ready".
    readonly property string mode: {
        if (!terminalPane)
            return "none";
        if (terminalPane.kind !== "ssh")
            return "local";
        if (terminal.command.length > 0)
            return "openssh";
        if (sshState !== "connected")
            return "offline";
        if (!info || info.state === "reading")
            return "reading";
        return info.state === "failed" ? "failed" : "ready";
    }
    // The connection each pane's info was read on (pane id -> connectionSerial).
    property var readOn: ({})

    // The pane's own item: `terminal` may not follow a new pane yet when this runs.
    function read() {
        const item = terminalPane ? terminalPane.terminal : null;
        if (!item || !item.readHostInfo())
            return;
        const next = Object.assign({}, readOn);
        next[terminalPane.paneId] = item.connectionSerial;
        readOn = next;
    }

    // Reads when the tab shows a connected pane not read on its current connection.
    function readIfNeeded() {
        const item = terminalPane ? terminalPane.terminal : null;
        if (!item || terminalPane.kind !== "ssh" || item.command.length > 0 || item.connection.length === 0)
            return;
        if (JSON.parse(item.connection).state === "connected" && readOn[terminalPane.paneId] !== item.connectionSerial)
            read();
    }

    function usedPercent(used, available) {
        return used + available > 0 ? Math.round(used * 100 / (used + available)) : 0;
    }

    function memoryText(memory) {
        return qsTr("%1 used of %2").arg(FileFormat.size(memory.total - memory.available)).arg(FileFormat.size(memory.total));
    }

    function loadText(load) {
        return load ? load.map(value => value.toFixed(2)).join("  ") : "";
    }

    // Everything shown, as plain text for the clipboard.
    function asText() {
        const lines = [];
        const ready = info && info.state === "ready" ? info : null;
        if (ready) {
            lines.push(qsTr("%1 (%2)").arg(ready.hostname).arg(ready.osName));
            lines.push(qsTr("Kernel: %1, %2").arg(ready.kernel).arg(ready.architecture));
            if (ready.cpus !== null)
                lines.push(qsTr("CPUs: %1").arg(ready.cpus));
        }
        const now = snapshot;
        if (now) {
            if (now.uptime !== null)
                lines.push(qsTr("Up %1").arg(FileFormat.uptime(now.uptime)));
            if (now.load)
                lines.push(qsTr("Load average: %1").arg(loadText(now.load)));
            if (now.cpu !== null)
                lines.push(qsTr("CPU: %1%").arg(now.cpu.toFixed(1)));
            if (now.memory) {
                lines.push(qsTr("Memory: %1").arg(memoryText(now.memory)));
                if (now.memory.swapTotal > 0)
                    lines.push(qsTr("Swap: %1 used of %2").arg(FileFormat.size(now.memory.swapTotal - now.memory.swapFree))
                                                          .arg(FileFormat.size(now.memory.swapTotal)));
            }
            if (now.received !== null && now.sent !== null)
                lines.push(qsTr("Network: %1/s in, %2/s out").arg(FileFormat.size(now.received)).arg(FileFormat.size(now.sent)));
            if (now.disks.length > 0) {
                lines.push(qsTr("Disks:"));
                for (const disk of now.disks)
                    lines.push(qsTr("  %1 (%2): %3 used of %4 (%5%)").arg(disk.mount).arg(disk.filesystem).arg(FileFormat.size(disk.used))
                                                                   .arg(FileFormat.size(disk.size)).arg(usedPercent(disk.used, disk.available)));
            }
        }
        if (ready && ready.addresses.length > 0) {
            lines.push(qsTr("Addresses:"));
            for (const address of ready.addresses)
                lines.push(qsTr("  %1 %2").arg(address.interface).arg(address.address));
        }
        if (now && now.users.length > 0) {
            lines.push(qsTr("Users:"));
            for (const user of now.users)
                lines.push(user.from.length > 0 ? qsTr("  %1 on %2 from %3").arg(user.name).arg(user.line).arg(user.from)
                                                : qsTr("  %1 on %2").arg(user.name).arg(user.line));
        }
        return lines.join("\n");
    }

    onTerminalPaneChanged: readIfNeeded()

    Connections {
        target: panel.terminal

        function onSshChanged() {
            panel.readIfNeeded();
        }
    }

    // A label and its value.
    component Pair: RowLayout {
        property string label
        property string value

        width: parent ? parent.width : 0
        visible: value.length > 0
        spacing: Theme.spacingSm

        OsText {
            Layout.preferredWidth: Theme.spacingXxl * 2.5
            Layout.alignment: Qt.AlignTop
            text: parent.label
            muted: true
            size: "small"
        }
        OsText {
            Layout.fillWidth: true
            text: parent.value
            size: "small"
            wrapMode: Text.Wrap
        }
    }

    // A section's title.
    component Heading: OsText {
        width: parent ? parent.width : 0
        topPadding: Theme.spacingMd
        muted: true
        size: "small"
        font.weight: Font.DemiBold
    }

    // A value with a bar under it.
    component Gauge: Column {
        property string label
        property string value
        property real fraction: 0

        width: parent ? parent.width : 0
        spacing: Theme.spacingXs / 2

        RowLayout {
            width: parent.width

            OsText {
                Layout.fillWidth: true
                text: parent.parent.label
                size: "small"
                elide: Text.ElideMiddle
            }
            OsText {
                text: parent.parent.value
                muted: true
                size: "small"
                font.features: { "tnum": 1 }
            }
        }

        OsProgress {
            width: parent.width
            from: 0
            to: 1
            value: parent.fraction
            Accessible.name: parent.label
        }
    }

    OsEmptyState {
        anchors.fill: parent
        visible: panel.mode !== "ready" && panel.mode !== "reading"
        iconName: panel.mode === "failed" ? "circle-alert" : "info"
        title: {
            switch (panel.mode) {
            case "local":
                return qsTr("A terminal on this computer");
            case "openssh":
                return qsTr("Connected with OpenSSH");
            case "offline":
                return qsTr("Not connected");
            case "failed":
                return qsTr("No host info");
            default:
                return qsTr("Host info");
            }
        }
        description: {
            switch (panel.mode) {
            case "local":
                return qsTr("Host info is for connections to servers with the built-in SSH client.");
            case "openssh":
                return qsTr("This host uses the system's OpenSSH, so OpenSesh has no connection to read it over. Choose the built-in client in the host's editor.");
            case "offline":
                return qsTr("It shows once the connection is up.");
            case "failed":
                return qsTr("This server couldn't be read: %1").arg(panel.info.reason);
            default:
                return qsTr("Open a terminal connected to a server to see how it is doing.");
            }
        }

        OsButton {
            visible: panel.mode === "failed"
            text: qsTr("Try again")
            iconName: "refresh-cw"
            onClicked: panel.read()
        }
    }

    Column {
        anchors.centerIn: parent
        width: parent.width - 2 * Theme.spacingLg
        visible: panel.mode === "reading"
        spacing: Theme.spacingSm

        OsProgress {
            width: parent.width
            indeterminate: true
            Accessible.name: qsTr("Reading the host")
        }
        OsText {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("Reading the host…")
            muted: true
            size: "small"
        }
    }

    Flickable {
        id: flick

        anchors.fill: parent
        visible: panel.mode === "ready"
        clip: true
        contentHeight: details.implicitHeight + 2 * Theme.spacingMd
        boundsBehavior: Flickable.StopAtBounds

        T.ScrollBar.vertical: OsScrollBar {}

        Column {
            id: details

            readonly property var ready: panel.info && panel.info.state === "ready" ? panel.info : null
            readonly property var now: panel.snapshot

            x: Theme.spacingMd
            y: Theme.spacingMd
            width: flick.width - 2 * Theme.spacingMd
            spacing: Theme.spacingXs

            RowLayout {
                width: parent.width
                spacing: Theme.spacingSm

                OsIcon {
                    name: "server"
                    size: Theme.iconSize
                    color: Theme.textMuted
                }
                Column {
                    Layout.fillWidth: true

                    OsText {
                        width: parent.width
                        text: details.ready ? details.ready.hostname : ""
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                    }
                    OsText {
                        width: parent.width
                        text: details.ready ? details.ready.osName : ""
                        muted: true
                        size: "small"
                        elide: Text.ElideRight
                    }
                }
            }

            Flow {
                width: parent.width
                spacing: Theme.spacingSm
                topPadding: Theme.spacingXs

                OsButton {
                    text: qsTr("Refresh")
                    iconName: "refresh-cw"
                    onClicked: panel.read()
                }
                OsButton {
                    text: qsTr("Copy as text")
                    iconName: "copy"
                    onClicked: {
                        Platform.copyText(panel.asText());
                        Toasts.show(qsTr("The host info was copied."), "success");
                    }
                }
            }

            Heading {
                text: qsTr("System")
            }
            Pair {
                label: qsTr("Kernel")
                value: details.ready ? details.ready.kernel : ""
            }
            Pair {
                label: qsTr("Architecture")
                value: details.ready ? details.ready.architecture : ""
            }
            Pair {
                label: qsTr("CPUs")
                value: details.ready && details.ready.cpus !== null ? String(details.ready.cpus) : ""
            }
            Pair {
                label: qsTr("Up")
                value: details.now && details.now.uptime !== null ? FileFormat.uptime(details.now.uptime) : ""
            }
            Pair {
                label: qsTr("Load")
                value: details.now ? panel.loadText(details.now.load) : ""
            }

            Heading {
                text: panel.live ? qsTr("Now (every %n second(s))", "", AppSettings.sshMonitorInterval) : qsTr("When read")
            }
            OsText {
                width: parent.width
                visible: panel.monitor !== null && panel.monitor.state === "unsupported"
                text: qsTr("The live monitor can't read this server: %1").arg(panel.monitor ? panel.monitor.reason ?? "" : "")
                color: Theme.warning
                size: "small"
                wrapMode: Text.Wrap
            }
            Gauge {
                visible: details.now !== null && details.now.cpu !== null
                label: qsTr("CPU")
                value: details.now && details.now.cpu !== null ? qsTr("%1%").arg(details.now.cpu.toFixed(1)) : ""
                fraction: details.now && details.now.cpu !== null ? details.now.cpu / 100 : 0
            }
            Gauge {
                visible: details.now !== null && details.now.memory !== null
                label: qsTr("Memory")
                value: details.now && details.now.memory ? panel.memoryText(details.now.memory) : ""
                fraction: details.now && details.now.memory && details.now.memory.total > 0
                          ? (details.now.memory.total - details.now.memory.available) / details.now.memory.total : 0
            }
            Gauge {
                visible: details.now !== null && details.now.memory !== null && details.now.memory.swapTotal > 0
                label: qsTr("Swap")
                value: details.now && details.now.memory
                       ? qsTr("%1 used of %2").arg(FileFormat.size(details.now.memory.swapTotal - details.now.memory.swapFree))
                                              .arg(FileFormat.size(details.now.memory.swapTotal)) : ""
                fraction: details.now && details.now.memory && details.now.memory.swapTotal > 0
                          ? (details.now.memory.swapTotal - details.now.memory.swapFree) / details.now.memory.swapTotal : 0
            }
            Pair {
                label: qsTr("Network")
                value: details.now && details.now.received !== null && details.now.sent !== null
                       ? qsTr("↓%1/s ↑%2/s").arg(FileFormat.size(details.now.received)).arg(FileFormat.size(details.now.sent)) : ""
            }

            Heading {
                visible: details.now !== null && details.now.disks.length > 0
                text: qsTr("Disks")
            }
            Repeater {
                model: details.now ? details.now.disks : []

                delegate: Gauge {
                    required property var modelData

                    label: modelData.mount
                    value: qsTr("%1 of %2").arg(FileFormat.size(modelData.used)).arg(FileFormat.size(modelData.size))
                    fraction: panel.usedPercent(modelData.used, modelData.available) / 100
                    Accessible.description: modelData.filesystem
                }
            }

            Heading {
                visible: details.ready !== null && details.ready.addresses.length > 0
                text: qsTr("Addresses")
            }
            Repeater {
                model: details.ready ? details.ready.addresses : []

                delegate: Pair {
                    required property var modelData

                    label: modelData.interface
                    value: modelData.address
                }
            }

            Heading {
                visible: details.now !== null && details.now.users.length > 0
                text: qsTr("Logged in")
            }
            Repeater {
                model: details.now ? details.now.users : []

                delegate: Pair {
                    required property var modelData

                    label: modelData.name
                    value: modelData.from.length > 0 ? qsTr("%1 from %2").arg(modelData.line).arg(modelData.from) : modelData.line
                }
            }
        }
    }
}
