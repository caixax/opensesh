pragma ComponentBehavior: Bound

// The side panel's files (Sprint 8): the files of the current tab's focused terminal.
// - An SSH pane of the built-in client: its server, on a new channel of the same connection (no
//   second login), again after each reconnection.
// - An SSH pane on OpenSSH: its host, on a connection of its own.
// - A local shell: this computer.
// With "Follow the terminal" on, the pane goes where the shell goes (OSC 7). A server's shell
// that doesn't say where it is gets an offer to set that up (ShellIntegrationDialog).
//   terminalPane: TerminalPane   the focused pane of the current tab, or null
import QtQuick
import QtQuick.Layouts
import cc.caixa.opensesh

Item {
    id: files

    property Item terminalPane: null
    property bool follow: true
    readonly property TerminalItem terminal: terminalPane ? terminalPane.terminal : null
    readonly property bool builtInSsh: terminalPane !== null && terminalPane.kind === "ssh" && terminal !== null
                                       && terminal.command.length === 0
    readonly property string shellFolder: terminal === null ? ""
                                          : terminalPane.kind === "ssh" ? terminal.shellDirectory : terminal.workingDirectory
    readonly property Item pane: loader.item

    // The terminal changed: a new pane for it.
    onTerminalPaneChanged: {
        loader.active = false;
        loader.active = terminalPane !== null;
    }
    onShellFolderChanged: {
        if (follow && pane && pane.ready && shellFolder.length > 0 && shellFolder !== pane.browser.path)
            pane.navigate(shellFolder);
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacingSm
            Layout.rightMargin: Theme.spacingSm
            Layout.topMargin: Theme.spacingXs
            visible: files.terminalPane !== null
            spacing: Theme.spacingSm

            OsSwitch {
                id: followSwitch

                text: qsTr("Follow the terminal")
                checked: files.follow
                onToggled: {
                    files.follow = checked;
                    if (checked && files.pane && files.shellFolder.length > 0)
                        files.pane.navigate(files.shellFolder);
                }
            }

            Item {
                Layout.fillWidth: true
            }

            OsIconButton {
                iconName: "arrow-right"
                toolTip: qsTr("Go to the terminal's folder")
                enabled: files.shellFolder.length > 0 && files.pane !== null && files.pane.ready
                onClicked: files.pane.navigate(files.shellFolder)
            }
        }

        // The shell of a server doesn't say where it is.
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacingSm
            Layout.rightMargin: Theme.spacingSm
            visible: files.follow && files.builtInSsh && files.pane !== null && files.pane.ready && files.terminal.shellDirectory.length === 0
            spacing: Theme.spacingSm

            OsText {
                Layout.fillWidth: true
                text: qsTr("This shell doesn't tell its folder.")
                size: "small"
                muted: true
                elide: Text.ElideRight
            }

            OsButton {
                variant: "ghost"
                text: qsTr("Set up…")
                onClicked: integrationDialog.show(files.pane.browser)
            }
        }

        Loader {
            id: loader

            Layout.fillWidth: true
            Layout.fillHeight: true
            active: files.terminalPane !== null

            sourceComponent: FilePane {
                compact: true
                mode: files.terminalPane.kind === "ssh" ? "remote" : "local"
                terminalSession: files.builtInSsh ? files.terminalPane.paneId : 0
                connectionSerial: files.builtInSsh ? files.terminal.connectionSerial : 0
                hostId: files.builtInSsh ? "" : files.terminalPane.host
                target: files.builtInSsh ? "" : files.terminalPane.target
                startPath: files.follow ? files.shellFolder : ""
                title: files.terminalPane.label.length > 0 ? files.terminalPane.label : qsTr("This computer")
            }
        }
    }

    OsEmptyState {
        anchors.fill: parent
        visible: files.terminalPane === null
        iconName: "folder-sync"
        title: qsTr("No terminal")
        description: qsTr("Open a terminal or an SSH connection: its files show here and follow its folder.")
    }

    ShellIntegrationDialog {
        id: integrationDialog
    }
}
