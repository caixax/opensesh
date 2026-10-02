pragma ComponentBehavior: Bound

// Settings > Data and sync (Sprint 16): importing from other programs and exporting; the
// settings folder, which can live in a Git repository or a Syncthing folder (used from the next
// start); the conflicts that syncing left in it; and a Git helper for a folder that is a
// repository. Every file and Git operation runs in SettingsSync's worker and ends with
// SettingsSync.finished; the dialogs live in the shell.
import QtQuick
import QtQuick.Dialogs
import cc.caixa.opensesh

SettingsPage {
    id: page

    readonly property Item shell: WindowRegistry.mainShell
    readonly property var git: JSON.parse(SettingsSync.git || "{}")
    readonly property var conflicts: JSON.parse(SettingsSync.conflicts || "[]")
    property string chosen: ""

    function chooseFolder(path) {
        const info = JSON.parse(SettingsSync.folderInfo(path) || "{}");
        if (info.current)
            return;
        chosen = path;
        if (info.settings)
            folderChoice.open();
        else
            SettingsSync.setFolder(path, "copy");
    }

    function gitText() {
        if (!SettingsSync.gitAvailable)
            return qsTr("Git isn't installed on this computer.");
        if (!git.repo)
            return qsTr("This folder isn't a Git repository.");
        const parts = [git.upstream.length > 0 ? qsTr("Branch %1, following %2.").arg(git.branch).arg(git.upstream)
                                               : qsTr("Branch %1, with no remote branch to follow.").arg(git.branch || qsTr("(none)"))];
        parts.push(git.changes > 0 ? qsTr("%n file(s) changed.", "", git.changes) : qsTr("Nothing changed."));
        if (git.ahead > 0)
            parts.push(qsTr("%n commit(s) to push.", "", git.ahead));
        if (git.behind > 0)
            parts.push(qsTr("%n commit(s) to pull.", "", git.behind));
        return parts.join(" ");
    }

    title: qsTr("Data and sync")
    description: qsTr("Bring your hosts from other programs, export them, and keep your settings in a folder that Git or Syncthing syncs between your computers.")

    Connections {
        target: SettingsSync

        function onFinished(action, code, detail) {
            if (action === "resolve")
                return;
            if (code === "test-run")
                return;
            if (code === "busy") {
                Toasts.show(qsTr("Wait for the operation in progress to end."), "info");
                return;
            }
            if (code === "nothing") {
                Toasts.show(qsTr("Nothing to commit."), "info");
                return;
            }
            if (code.length > 0) {
                Toasts.show(qsTr("It didn't work: %1").arg(detail), "danger");
                return;
            }
            switch (action) {
            case "folder":
                Toasts.show(qsTr("OpenSesh uses the new folder from its next start."), "success");
                break;
            case "git-commit":
                Toasts.show(qsTr("Committed."), "success");
                break;
            case "git-pull":
                Toasts.show(qsTr("Pulled."), "success");
                break;
            case "git-push":
                Toasts.show(qsTr("Pushed."), "success");
                break;
            case "git-init":
                Toasts.show(qsTr("The folder is a Git repository now."), "success");
                break;
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Import and export")

        SettingsRow {
            label: qsTr("Import")
            helpText: qsTr("From MobaXterm, PuTTY, Remmina, a CSV file, an OpenSesh bundle or ~/.ssh/config.")

            Row {
                spacing: Theme.spacingSm

                OsButton {
                    text: qsTr("Import hosts…")
                    iconName: "import"
                    onClicked: page.shell.showImport("")
                }
            }
        }

        SettingsRow {
            label: qsTr("Export")
            helpText: qsTr("An OpenSesh bundle, to move to another computer or keep as a backup, or an OpenSSH config file.")

            OsButton {
                text: qsTr("Export…")
                iconName: "upload"
                onClicked: page.shell.showExport([])
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Settings folder")

        SettingsRow {
            label: qsTr("Folder")
            helpText: SettingsSync.moved ? qsTr("A folder you chose. Hosts, profiles, themes, snippets, tunnels, shortcuts and settings are there.")
                                         : qsTr("The default folder. Choose one that Git or Syncthing syncs to share your settings between computers.")

            OsText {
                width: parent.width
                text: SettingsSync.folder
                wrapMode: Text.WrapAnywhere
                elide: Text.ElideNone
            }
        }

        SettingsNotice {
            width: parent.width
            visible: SettingsSync.restartNeeded
            kind: "info"
            title: qsTr("From the next start")
            lines: [qsTr("OpenSesh will use %1.").arg(SettingsSync.nextFolder)]
        }

        SettingsRow {
            label: qsTr("Change")
            helpText: qsTr("The keychain's identities and keys, and the vault, stay on each computer: bring them with a bundle exported with its keychain.")

            Row {
                spacing: Theme.spacingSm

                OsButton {
                    text: qsTr("Choose a folder…")
                    iconName: "folder-open"
                    enabled: !SettingsSync.busy
                    onClicked: folderDialog.open()
                }

                OsButton {
                    visible: SettingsSync.nextFolder !== SettingsSync.defaultFolder
                    variant: "ghost"
                    text: qsTr("Use the default folder")
                    enabled: !SettingsSync.busy
                    onClicked: SettingsSync.setFolder("", "default")
                }
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Conflicts")

        SettingsRow {
            visible: page.conflicts.length === 0
            label: qsTr("Status")

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                text: qsTr("No conflicts. When two computers change a file before they sync, it shows up here.")
                muted: true
            }
        }

        Repeater {
            model: page.conflicts

            delegate: SettingsRow {
                id: conflictRow

                required property var modelData
                required property int index

                label: modelData.name
                helpText: modelData.source === "git" ? qsTr("Git's conflict markers") : qsTr("A Syncthing conflict copy")

                OsButton {
                    text: qsTr("Resolve…")
                    iconName: "check"
                    onClicked: page.shell.showConflict(conflictRow.index)
                }
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Git")

        SettingsRow {
            label: qsTr("Repository")

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                text: page.gitText()
            }
        }

        SettingsRow {
            visible: SettingsSync.gitAvailable && !page.git.repo
            label: qsTr("Start")
            helpText: qsTr("Makes the folder a repository, ignoring OpenSesh's backups and temporary files. Add a remote with Git to push it.")

            OsButton {
                text: qsTr("Make it a repository")
                enabled: !SettingsSync.busy
                onClicked: SettingsSync.gitInit()
            }
        }

        SettingsRow {
            visible: SettingsSync.gitAvailable && page.git.repo === true
            label: qsTr("Commit")
            helpText: qsTr("Pull and push reach the network; nothing else does. Git asks for no password here: use an SSH agent or a credential helper.")

            Column {
                width: parent.width
                spacing: Theme.spacingSm

                OsTextField {
                    id: messageField

                    width: parent.width
                    placeholderText: qsTr("OpenSesh settings")
                    Accessible.name: qsTr("Commit message")
                }

                Row {
                    spacing: Theme.spacingSm

                    OsButton {
                        text: qsTr("Commit")
                        iconName: "save"
                        enabled: !SettingsSync.busy
                        onClicked: SettingsSync.gitCommit(messageField.text)
                    }

                    OsButton {
                        text: qsTr("Pull")
                        iconName: "download"
                        enabled: !SettingsSync.busy && page.git.upstream.length > 0
                        onClicked: SettingsSync.gitPull()
                    }

                    OsButton {
                        text: qsTr("Push")
                        iconName: "upload"
                        enabled: !SettingsSync.busy && page.git.upstream.length > 0
                        onClicked: SettingsSync.gitPush()
                    }
                }
            }
        }
    }

    FolderDialog {
        id: folderDialog

        title: qsTr("Choose the settings folder")
        onAccepted: page.chooseFolder(Platform.localPath(selectedFolder))
    }

    OsDialog {
        id: folderChoice

        title: qsTr("This folder has settings")
        acceptText: qsTr("Use them")
        onAccepted: SettingsSync.setFolder(page.chosen, "use")

        Column {
            width: Math.min(Theme.spacingXxl * 14, folderChoice.maxWidth - folderChoice.leftPadding - folderChoice.rightPadding)
            spacing: Theme.spacingMd

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                text: qsTr("%1 has OpenSesh settings already (from another computer?). Use them as they are, or add the ones from this computer that aren't there (nothing there is overwritten).").arg(page.chosen)
            }

            OsButton {
                text: qsTr("Add mine, then use them")
                onClicked: {
                    folderChoice.close();
                    SettingsSync.setFolder(page.chosen, "copy");
                }
            }
        }
    }
}
