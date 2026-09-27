pragma ComponentBehavior: Bound

// History view (PLAN §5.4, Sprint 10): the recent connections (a click connects again), the
// session recordings (play in a tab, show the folder, delete) and the logs folders (session
// logs from Settings > SSH, and the app's own logs).
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: view

    readonly property Item shell: WindowRegistry.mainShell
    // Read again whenever Hosts changes.
    property var recent: []
    readonly property var recordings: JSON.parse(Recordings.list || "[]")

    function refreshRecent() {
        recent = JSON.parse(Hosts.recentConnections() || "[]");
    }

    function iconFor(host) {
        if (host.icon && host.icon !== "auto")
            return host.icon;
        if (host.detectedIcon)
            return host.detectedIcon;
        return host.protocol === "local" ? "square-terminal" : "server";
    }

    function connect(entry) {
        if (entry.kind === "host")
            shell.connectHost(entry.host.id, "tab");
        else
            shell.connectTarget(entry.target, "tab");
    }

    function askDelete(entry) {
        deleteDialog.entry = entry;
        deleteDialog.open();
    }

    Component.onCompleted: refreshRecent()
    onVisibleChanged: {
        if (visible)
            Recordings.refresh();
    }

    Connections {
        target: Hosts

        function onChanged() {
            view.refreshRecent();
        }
    }

    Connections {
        target: Recordings

        function onProblem(detail) {
            if (view.visible)
                Toasts.show(qsTr("The recording wasn't deleted: %1").arg(detail), "danger");
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.spacingLg
        spacing: Theme.spacingMd

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingSm

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0

                OsText {
                    text: qsTr("History")
                    size: "title"
                }
                OsText {
                    text: qsTr("Recent connections, session recordings and logs")
                    muted: true
                }
            }

            OsButton {
                text: qsTr("Session logs")
                iconName: "folder-open"
                onClicked: Qt.openUrlExternally(FileFormat.fileUrl(AppSettings.sshLogsFolder))
            }

            OsButton {
                text: qsTr("App logs")
                iconName: "folder-open"
                onClicked: ActionRegistry.trigger("app.openLogsFolder")
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: Theme.spacingLg

            // Recent connections.
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredWidth: 1
                spacing: Theme.spacingSm

                RowLayout {
                    Layout.fillWidth: true

                    OsText {
                        Layout.fillWidth: true
                        text: qsTr("Recent connections")
                        font.weight: Font.DemiBold
                    }
                    OsButton {
                        text: qsTr("Clear")
                        iconName: "trash-2"
                        enabled: view.recent.length > 0
                        onClicked: clearDialog.open()
                    }
                }

                ListView {
                    id: recentList

                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: view.recent
                    activeFocusOnTab: true
                    keyNavigationEnabled: true
                    Accessible.role: Accessible.List
                    Accessible.name: qsTr("Recent connections")

                    T.ScrollBar.vertical: OsScrollBar {}

                    Keys.onReturnPressed: {
                        const entry = view.recent[currentIndex];
                        if (entry)
                            view.connect(entry);
                    }

                    delegate: OsListRow {
                        required property var modelData
                        required property int index

                        width: ListView.view ? ListView.view.width : 0
                        highlighted: ListView.isCurrentItem && recentList.activeFocus
                        iconName: modelData.kind === "host" ? view.iconFor(modelData.host) : "globe"
                        text: modelData.kind === "host" ? modelData.host.name : modelData.target
                        subtitle: modelData.kind === "host" ? modelData.host.target : qsTr("Quick connect")
                        trailingText: FileFormat.time(modelData.at)
                        onClicked: view.connect(modelData)
                    }

                    OsText {
                        anchors.centerIn: parent
                        width: parent.width - 2 * Theme.spacingLg
                        visible: view.recent.length === 0
                        horizontalAlignment: Text.AlignHCenter
                        text: qsTr("The hosts you connect to show up here.")
                        muted: true
                        wrapMode: Text.Wrap
                    }
                }
            }

            // Recordings.
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredWidth: 1
                spacing: Theme.spacingSm

                RowLayout {
                    Layout.fillWidth: true

                    OsText {
                        Layout.fillWidth: true
                        text: qsTr("Recordings")
                        font.weight: Font.DemiBold
                    }
                    OsButton {
                        text: qsTr("Open folder")
                        iconName: "folder-open"
                        enabled: Recordings.count > 0
                        onClicked: Qt.openUrlExternally(FileFormat.fileUrl(Recordings.folder))
                    }
                }

                ListView {
                    id: recordingList

                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: view.recordings
                    activeFocusOnTab: true
                    keyNavigationEnabled: true
                    Accessible.role: Accessible.List
                    Accessible.name: qsTr("Recordings")

                    T.ScrollBar.vertical: OsScrollBar {}

                    Keys.onPressed: event => {
                        const entry = view.recordings[currentIndex];
                        if (!entry)
                            return;
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                            view.shell.playRecording(entry.path);
                        else if (event.key === Qt.Key_Delete && !entry.recording)
                            view.askDelete(entry);
                        else
                            return;
                        event.accepted = true;
                    }

                    delegate: OsListRow {
                        id: recordingRow

                        required property var modelData
                        required property int index

                        width: ListView.view ? ListView.view.width : 0
                        highlighted: ListView.isCurrentItem && recordingList.activeFocus
                        iconName: "film"
                        text: modelData.title.length > 0 ? modelData.title : modelData.name
                        subtitle: modelData.recording ? qsTr("Recording now")
                                                      : qsTr("%1 · %2").arg(FileFormat.time(modelData.modified)).arg(FileFormat.size(modelData.size))
                        onClicked: view.shell.playRecording(modelData.path)

                        OsButton {
                            text: qsTr("Play")
                            iconName: "play"
                            enabled: !recordingRow.modelData.recording
                            onClicked: view.shell.playRecording(recordingRow.modelData.path)
                        }

                        OsIconButton {
                            iconName: "trash-2"
                            toolTip: qsTr("Delete…")
                            enabled: !recordingRow.modelData.recording
                            onClicked: view.askDelete(recordingRow.modelData)
                        }
                    }

                    OsText {
                        anchors.centerIn: parent
                        width: parent.width - 2 * Theme.spacingLg
                        visible: view.recordings.length === 0
                        horizontalAlignment: Text.AlignHCenter
                        text: qsTr("Record a terminal from its menu (Record the session) to play it here later.")
                        muted: true
                        wrapMode: Text.Wrap
                    }
                }
            }
        }
    }

    OsDialog {
        id: deleteDialog

        property var entry: null

        title: qsTr("Delete this recording?")
        acceptText: qsTr("Delete")
        dangerous: true
        onAccepted: Recordings.remove(entry.path)

        OsText {
            width: Math.min(Theme.spacingXxl * 12, deleteDialog.maxWidth - deleteDialog.leftPadding - deleteDialog.rightPadding)
            text: deleteDialog.entry ? deleteDialog.entry.name : ""
            wrapMode: Text.WrapAnywhere
        }
    }

    OsDialog {
        id: clearDialog

        title: qsTr("Clear the recent connections?")
        acceptText: qsTr("Clear")
        dangerous: true
        onAccepted: Hosts.clearRecent()

        OsText {
            width: Math.min(Theme.spacingXxl * 12, clearDialog.maxWidth - clearDialog.leftPadding - clearDialog.rightPadding)
            text: qsTr("Your saved hosts stay; only the list of what you connected to goes.")
            wrapMode: Text.Wrap
        }
    }
}
