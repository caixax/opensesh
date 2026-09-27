// Settings > SFTP (Sprint 8): the transfer queue (files at once, what to do with files already at
// the destination, keeping times and permissions), the file panes (hidden files, confirming
// deletes) and the editor for remote files. Everything is `[sftp]` in config.toml.
import QtQuick
import cc.caixa.opensesh

SettingsPage {
    id: page

    title: qsTr("SFTP")
    description: qsTr("How files are transferred, shown and edited, in the SFTP view and in the side panel.")

    SettingsGroup {
        width: parent.width
        title: qsTr("Transfers")

        SettingsRow {
            label: qsTr("Files at once")
            helpText: qsTr("How many files are copied at the same time, across every transfer.")

            OsSpinBox {
                from: 1
                to: 16
                value: AppSettings.sftpParallel
                Accessible.name: qsTr("Files at once")
                onValueModified: AppSettings.sftpParallel = value
            }
        }

        SettingsRow {
            label: qsTr("When a file is already there")

            OsComboBox {
                id: policyBox

                readonly property var choices: [
                    { value: "ask", text: qsTr("Ask") },
                    { value: "overwrite", text: qsTr("Replace it") },
                    { value: "newer", text: qsTr("Replace it if older") },
                    { value: "resume", text: qsTr("Continue it if it is shorter") },
                    { value: "skip", text: qsTr("Skip it") },
                    { value: "rename", text: qsTr("Keep both") }
                ]

                width: Math.min(parent.width, Theme.spacingXxl * 9)
                model: choices
                textRole: "text"
                valueRole: "value"
                currentIndex: Math.max(0, choices.findIndex(choice => choice.value === AppSettings.sftpPolicy))
                Accessible.name: qsTr("When a file is already there")
                onActivated: AppSettings.sftpPolicy = currentValue
            }
        }

        SettingsRow {
            label: qsTr("Keep modification times")

            OsSwitch {
                checked: AppSettings.sftpPreserveTimes
                Accessible.name: qsTr("Keep modification times")
                onToggled: AppSettings.sftpPreserveTimes = checked
            }
        }

        SettingsRow {
            label: qsTr("Keep permissions")
            helpText: qsTr("The permission bits of each file go with it (on Windows, only read-only means something).")

            OsSwitch {
                checked: AppSettings.sftpPreservePermissions
                Accessible.name: qsTr("Keep permissions")
                onToggled: AppSettings.sftpPreservePermissions = checked
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("File panes")

        SettingsRow {
            label: qsTr("Show hidden files")
            helpText: qsTr("Files whose name starts with a dot. Ctrl+H switches it in a pane.")

            OsSwitch {
                checked: AppSettings.sftpShowHidden
                Accessible.name: qsTr("Show hidden files")
                onToggled: AppSettings.sftpShowHidden = checked
            }
        }

        SettingsRow {
            label: qsTr("Confirm deleting")

            OsSwitch {
                checked: AppSettings.sftpConfirmDelete
                Accessible.name: qsTr("Confirm deleting")
                onToggled: AppSettings.sftpConfirmDelete = checked
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Editing server files")
        description: qsTr("Opening a server's file downloads a private copy and opens it; each save is uploaded, after checking nobody changed the server's copy since.")

        SettingsRow {
            label: qsTr("Editor command")
            helpText: qsTr("For example: code --wait {file}. {file} is the copy's path; empty uses the system's editor for the file's type.")

            OsTextField {
                width: parent.width
                text: AppSettings.sftpEditorCommand
                placeholderText: qsTr("the system's editor")
                Accessible.name: qsTr("Editor command")
                onEditingFinished: AppSettings.sftpEditorCommand = text
            }
        }
    }
}
