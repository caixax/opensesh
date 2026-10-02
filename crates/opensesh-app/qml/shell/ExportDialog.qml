pragma ComponentBehavior: Bound

// Export (Sprint 16): an OpenSesh bundle (hosts, snippets, profiles and themes, and with a
// password the keychain, sealed under it) or an OpenSSH config file of the SSH hosts. The
// bundle is written by the keychain worker, so secrets never pass through the UI.
// Functions: show(ids) (host ids for an OpenSSH file of those hosts; empty for everything).
import QtQuick
import QtQuick.Dialogs
import QtQuick.Layouts
import cc.caixa.opensesh

OsDialog {
    id: dialog

    required property Item shell
    property string format: "bundle"
    property var ids: []
    property bool withKeychain: false
    property bool working: false
    property string error: ""
    readonly property bool mismatch: withKeychain && confirmField.text !== passwordField.text
    // As the master password's.
    readonly property int minimumLength: 8
    readonly property bool tooShort: withKeychain && passwordField.text.length < minimumLength
    readonly property var formats: [
        { value: "bundle", text: qsTr("OpenSesh bundle") },
        { value: "ssh_config", text: qsTr("OpenSSH config file") }
    ]

    function show(hostIds) {
        ids = hostIds ?? [];
        format = ids.length > 0 ? "ssh_config" : "bundle";
        withKeychain = false;
        working = false;
        error = "";
        pathField.text = "";
        passwordField.text = "";
        confirmField.text = "";
        open();
        pathField.forceActiveFocus();
    }

    function submit() {
        const path = pathField.text.trim();
        if (working || path.length === 0 || mismatch)
            return;
        error = "";
        if (format === "ssh_config") {
            const result = JSON.parse(Hosts.exportSshConfig(path, JSON.stringify(ids)) || "{}");
            if (result.error) {
                error = qsTr("Could not save: %1").arg(result.error);
                return;
            }
            Toasts.show(qsTr("%n host(s) written to %1.", "", result.count).arg(path), "success");
            accept();
            return;
        }
        if (tooShort) {
            error = qsTr("Choose a password of at least %n character(s) for the keychain, or leave it out.", "", minimumLength);
            return;
        }
        if (withKeychain && Keychain.vaultStatus === "locked") {
            shell.unlockVault(() => dialog.submit());
            return;
        }
        working = true;
        const token = Keychain.exportBundle(path, withKeychain ? passwordField.text : "");
        passwordField.text = "";
        confirmField.text = "";
        KeychainTasks.run(token, (code, detail, value) => {
            dialog.working = false;
            if (code.length > 0) {
                dialog.error = KeychainTasks.message(code, detail);
                return;
            }
            const counts = JSON.parse(value || "{}");
            Toasts.show(qsTr("%n host(s), %1 snippet(s) and %2 profile and theme file(s) saved to %3.", "", counts.hosts ?? 0)
                        .arg(counts.snippets ?? 0).arg(counts.files ?? 0).arg(path), "success");
            dialog.accept();
        });
    }

    title: qsTr("Export")
    acceptText: working ? qsTr("Saving…") : qsTr("Export")
    acceptEnabled: !working && pathField.text.trim().length > 0 && !mismatch && !tooShort
    closeOnAccept: false
    onAcceptClicked: submit()

    Column {
        width: Math.min(Theme.spacingXxl * 15, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsFormRow {
            width: parent.width
            label: qsTr("Format")

            OsComboBox {
                width: parent.width
                model: dialog.formats
                textRole: "text"
                valueRole: "value"
                currentIndex: dialog.formats.findIndex(entry => entry.value === dialog.format)
                Accessible.name: qsTr("Export format")
                onActivated: index => {
                    dialog.format = dialog.formats[index].value;
                    pathField.text = "";
                }
            }
        }

        OsText {
            width: parent.width
            wrapMode: Text.Wrap
            muted: true
            size: "small"
            text: {
                if (dialog.format === "bundle")
                    return qsTr("Your hosts and groups, snippets, terminal profiles and themes, in one file to import on another computer or keep as a backup.");
                return dialog.ids.length > 0 ? qsTr("The %n selected host(s) that use SSH, as Host blocks for ssh and the tools that read ~/.ssh/config.", "", dialog.ids.length)
                                             : qsTr("Every host that uses SSH, as Host blocks for ssh and the tools that read ~/.ssh/config.");
            }
        }

        RowLayout {
            width: parent.width
            spacing: Theme.spacingSm

            OsTextField {
                id: pathField

                Layout.fillWidth: true
                placeholderText: dialog.format === "bundle" ? qsTr("hosts.opensesh") : qsTr("config")
                Accessible.name: qsTr("File to write")
                onAccepted: dialog.submit()
            }

            OsButton {
                text: qsTr("Choose…")
                iconName: "folder-open"
                onClicked: saveDialog.open()
            }
        }

        Column {
            width: parent.width
            visible: dialog.format === "bundle"
            spacing: Theme.spacingSm

            OsCheckBox {
                width: parent.width
                text: qsTr("Include the keychain: identities, keys and their passwords")
                checked: dialog.withKeychain
                onToggled: dialog.withKeychain = checked
            }

            OsText {
                width: parent.width
                visible: dialog.withKeychain
                wrapMode: Text.Wrap
                muted: true
                size: "small"
                text: qsTr("They are sealed with this password (as the vault is): anyone with the file and the password has your keys. Without them, hosts keep their identities only where those exist already.")
            }

            OsPasswordField {
                id: passwordField

                width: parent.width
                visible: dialog.withKeychain
                placeholderText: qsTr("Export password")
                Accessible.name: qsTr("Export password")
            }

            OsPasswordField {
                id: confirmField

                width: parent.width
                visible: dialog.withKeychain
                placeholderText: qsTr("Confirm the password")
                Accessible.name: qsTr("Confirm the export password")
                onAccepted: dialog.submit()
            }

            OsText {
                width: parent.width
                visible: dialog.withKeychain && passwordField.text.length > 0 && dialog.tooShort
                text: qsTr("Use at least %n character(s).", "", dialog.minimumLength)
                color: Theme.danger
                size: "small"
            }

            OsText {
                width: parent.width
                visible: dialog.mismatch && confirmField.text.length > 0
                text: qsTr("The passwords don't match.")
                color: Theme.danger
                size: "small"
            }
        }

        OsText {
            width: parent.width
            visible: dialog.error.length > 0
            text: dialog.error
            color: Theme.danger
            wrapMode: Text.Wrap
        }
    }

    FileDialog {
        id: saveDialog

        title: qsTr("Export to")
        fileMode: FileDialog.SaveFile
        defaultSuffix: dialog.format === "bundle" ? "opensesh" : ""
        nameFilters: dialog.format === "bundle" ? [qsTr("OpenSesh bundles (*.opensesh)")] : [qsTr("All files (*)")]
        onAccepted: pathField.text = Platform.localPath(selectedFile)
    }
}
