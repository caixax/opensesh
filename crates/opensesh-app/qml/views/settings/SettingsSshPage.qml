// Settings > SSH (Sprint 7): what every SSH host uses unless its groups or the host itself set
// something else (the client, the authentication order, keepalive, reconnection, the language
// settings, OS detection, session logs), the folder session logs go to, and a way to the known
// hosts. Everything is `[ssh]` in config.toml.
import QtQuick
import QtQuick.Dialogs
import cc.caixa.opensesh

SettingsPage {
    id: page

    readonly property Item shell: WindowRegistry.mainShell
    // The order as typed; applied when it is valid.
    property string orderText: AppSettings.sshAuthOrder.join(", ")
    readonly property var typedOrder: orderText.split(",").map(method => method.trim()).filter(method => method.length > 0)
    readonly property bool orderValid: typedOrder.length > 0
                                       && typedOrder.every((method, index) => ["publickey", "keyboard-interactive", "password"].indexOf(method) >= 0
                                                                              && typedOrder.indexOf(method) === index)

    function applyOrder() {
        if (orderValid)
            AppSettings.sshAuthOrder = typedOrder;
    }

    // A file:// URL for a local folder.
    function folderUrl(path) {
        const slashed = path.replace(/\\/g, "/");
        return "file://" + (slashed.startsWith("/") ? "" : "/") + slashed;
    }

    title: qsTr("SSH")
    description: qsTr("What SSH hosts use unless their group or the host itself says otherwise. Quick connections use these too.")

    SettingsGroup {
        width: parent.width
        title: qsTr("Connections")

        SettingsRow {
            label: qsTr("SSH client")
            helpText: AppSettings.sshBackend === "openssh"
                      ? qsTr("The system's ssh runs in the terminal with its own settings (~/.ssh/config): most options here don't apply to it.")
                      : qsTr("The system's OpenSSH client is there for what the built-in one doesn't do (Kerberos, smart cards, Match exec).")

            SettingsChoice {
                width: parent.width
                values: AppSettings.choices("sshBackend")
                labels: ({
                        internal: qsTr("Built-in"),
                        openssh: qsTr("OpenSSH")
                    })
                value: AppSettings.sshBackend
                Accessible.name: qsTr("SSH client")
                onPicked: value => AppSettings.sshBackend = value
            }
        }

        SettingsRow {
            label: qsTr("Authentication order")
            helpText: qsTr("publickey (the identity's key, the key file, the agent), keyboard-interactive (one-time codes) and password, separated by commas.")
            errorText: page.orderValid ? "" : qsTr("Use each of publickey, keyboard-interactive and password at most once.")

            OsTextField {
                width: parent.width
                text: page.orderText
                error: !page.orderValid
                Accessible.name: qsTr("Authentication order")
                onTextEdited: page.orderText = text
                onEditingFinished: page.applyOrder()
            }
        }

        SettingsRow {
            label: qsTr("Keepalive")
            helpText: qsTr("Seconds between keepalive messages, which also notice a dead connection. 0 turns them off.")

            OsSpinBox {
                from: 0
                to: 3600
                stepSize: 5
                value: AppSettings.sshKeepaliveSecs
                Accessible.name: qsTr("Keepalive in seconds")
                onValueModified: AppSettings.sshKeepaliveSecs = value
            }
        }

        SettingsRow {
            label: qsTr("Reconnect by itself")
            helpText: qsTr("After the connection drops, tries again after 1, 2, 4, 8 and 16 seconds. Enter reconnects at any time.")

            OsSwitch {
                checked: AppSettings.sshAutoReconnect
                Accessible.name: qsTr("Reconnect by itself")
                onToggled: AppSettings.sshAutoReconnect = checked
            }
        }

        SettingsRow {
            label: qsTr("Send the language settings")
            helpText: qsTr("LANG and LC_* from this computer, as OpenSSH sends them; the server may ignore them.")

            OsSwitch {
                checked: AppSettings.sshSendLocale
                Accessible.name: qsTr("Send the language settings")
                onToggled: AppSettings.sshSendLocale = checked
            }
        }

        SettingsRow {
            label: qsTr("Detect the OS")
            helpText: qsTr("Reads /etc/os-release once connected, over a separate channel, for the icon of hosts set to Automatic.")

            OsSwitch {
                checked: AppSettings.sshDetectOs
                Accessible.name: qsTr("Detect the OS")
                onToggled: AppSettings.sshDetectOs = checked
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Session logs")
        description: qsTr("A file per connection with what the server sent. Logs keep whatever the screen showed, secrets printed there included.")

        SettingsRow {
            label: qsTr("Log sessions")

            SettingsChoice {
                width: parent.width
                values: AppSettings.choices("sshLog")
                labels: ({
                        off: qsTr("Off"),
                        text: qsTr("Text"),
                        raw: qsTr("Raw")
                    })
                value: AppSettings.sshLog
                Accessible.name: qsTr("Log sessions")
                onPicked: value => AppSettings.sshLog = value
            }
        }

        SettingsRow {
            label: qsTr("Folder")
            helpText: qsTr("Text logs leave out colors and other escape codes; raw logs keep everything, for replaying.")

            Column {
                width: parent.width
                spacing: Theme.spacingSm

                OsText {
                    width: parent.width
                    text: AppSettings.sshLogsFolder
                    elide: Text.ElideMiddle
                }

                Row {
                    spacing: Theme.spacingSm

                    OsButton {
                        text: qsTr("Choose…")
                        iconName: "folder-open"
                        onClicked: folderDialog.open()
                    }

                    OsButton {
                        visible: AppSettings.sshLogsDir.length > 0
                        variant: "ghost"
                        text: qsTr("Use the default")
                        onClicked: AppSettings.sshLogsDir = ""
                    }

                    OsButton {
                        variant: "ghost"
                        text: qsTr("Open")
                        iconName: "external-link"
                        onClicked: Qt.openUrlExternally(page.folderUrl(AppSettings.sshLogsFolder))
                    }
                }
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Known hosts")
        description: qsTr("Server keys are checked against ~/.ssh/known_hosts and OpenSesh's own file; the keys you trust are saved in OpenSesh's.")

        SettingsRow {
            label: qsTr("Known hosts")

            OsButton {
                text: qsTr("Show in the keychain")
                iconName: "key-round"
                onClicked: page.shell.showView("keychain")
            }
        }
    }

    FolderDialog {
        id: folderDialog

        title: qsTr("Folder for session logs")
        onAccepted: AppSettings.sshLogsDir = Platform.localPath(selectedFolder)
    }
}
