pragma ComponentBehavior: Bound

// Exports a key (Sprint 6): the public key (an OpenSSH line) or the private key as an OpenSSH
// key file, optionally encrypted with a new passphrase. The private key is written by the
// keychain worker straight to the chosen file (private to the user on Linux); it never passes
// through the UI.
// Functions: show(id, which) (`which`: "public" or "private").
import QtQuick
import QtQuick.Dialogs
import QtQuick.Layouts
import cc.caixa.opensesh

OsDialog {
    id: dialog

    required property Item shell
    property string keyId: ""
    property string keyName: ""
    property bool privateKey: false
    property string error: ""
    property bool working: false
    readonly property bool mismatch: privateKey && confirmField.text !== passphraseField.text

    function show(id, which) {
        const key = JSON.parse(Keychain.keys || "[]").find(entry => entry.id === id);
        if (!key)
            return;
        keyId = id;
        keyName = key.name;
        privateKey = which === "private";
        pathField.text = "";
        passphraseField.text = "";
        confirmField.text = "";
        error = "";
        working = false;
        open();
        pathField.forceActiveFocus();
    }

    function submit() {
        if (working || pathField.text.trim().length === 0 || mismatch)
            return;
        if (privateKey && Keychain.vaultStatus === "locked") {
            shell.unlockVault(() => dialog.submit());
            return;
        }
        working = true;
        error = "";
        const token = privateKey ? Keychain.exportPrivateKey(keyId, pathField.text, passphraseField.text)
                                 : Keychain.exportPublicKey(keyId, pathField.text);
        passphraseField.text = "";
        confirmField.text = "";
        KeychainTasks.run(token, (code, detail) => {
            dialog.working = false;
            if (code.length === 0) {
                dialog.accept();
                Toasts.show(qsTr("Saved to %1.").arg(pathField.text), "success");
            } else {
                dialog.error = KeychainTasks.message(code, detail);
            }
        });
    }

    title: privateKey ? qsTr("Export the private key") : qsTr("Export the public key")
    acceptText: qsTr("Export")
    acceptEnabled: !working && pathField.text.trim().length > 0 && !mismatch
    closeOnAccept: false

    onAcceptClicked: submit()
    onClosed: {
        passphraseField.text = "";
        confirmField.text = "";
    }

    Column {
        width: Math.min(Theme.spacingXxl * 15, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingSm

        OsText {
            width: parent.width
            text: dialog.privateKey ? qsTr("“%1” is written as an OpenSSH private key. Anyone with the file can use the key, so give it a passphrase unless the file stays somewhere safe.").arg(dialog.keyName)
                                    : qsTr("“%1” is written as one line, ready for authorized_keys.").arg(dialog.keyName)
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }

        RowLayout {
            width: parent.width
            spacing: Theme.spacingSm

            OsTextField {
                id: pathField

                Layout.fillWidth: true
                enabled: !dialog.working
                placeholderText: qsTr("File")
                Accessible.name: qsTr("File to write")
            }

            OsButton {
                text: qsTr("Choose…")
                iconName: "folder-open"
                enabled: !dialog.working
                onClicked: saveDialog.open()
            }
        }

        OsFormRow {
            width: parent.width
            visible: dialog.privateKey
            label: qsTr("Passphrase")
            helpText: qsTr("Optional. Empty writes the key without one.")

            OsPasswordField {
                id: passphraseField

                width: parent.width
                enabled: !dialog.working
                placeholderText: qsTr("New passphrase")
                Accessible.name: qsTr("Passphrase for the file")
            }
        }

        OsFormRow {
            width: parent.width
            visible: dialog.privateKey && passphraseField.text.length > 0
            label: qsTr("Repeat it")
            errorText: dialog.mismatch && confirmField.text.length > 0 ? qsTr("The passphrases don't match.") : ""

            OsPasswordField {
                id: confirmField

                width: parent.width
                enabled: !dialog.working
                error: dialog.mismatch && text.length > 0
                placeholderText: qsTr("The same passphrase again")
                Accessible.name: qsTr("Repeat the passphrase")
                onAccepted: dialog.submit()
            }
        }

        OsProgress {
            width: parent.width
            visible: dialog.working
            indeterminate: true
        }

        OsText {
            width: parent.width
            visible: dialog.error.length > 0
            text: dialog.error
            color: Theme.danger
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }
    }

    FileDialog {
        id: saveDialog

        title: dialog.privateKey ? qsTr("Save the private key") : qsTr("Save the public key")
        fileMode: FileDialog.SaveFile
        onAccepted: pathField.text = Platform.localPath(selectedFile)
    }
}
