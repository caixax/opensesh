pragma ComponentBehavior: Bound

// Imports a private key into the vault (Sprint 6): an OpenSSH key or a PuTTY .ppk (versions 2
// and 3), from a file or pasted. A key with a passphrase needs it once, to decrypt it; the vault
// keeps it encrypted from then on. Asks to unlock the vault first when it is locked.
// Functions: show(path) ("" to start empty).
import QtQuick
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    required property Item shell
    property bool pasting: false
    property bool needsPassphrase: false
    property string error: ""
    property string errorCode: ""
    property bool working: false

    function show(path) {
        pasting = false;
        pathField.text = path ?? "";
        keyText.text = "";
        nameField.text = "";
        passphraseField.text = "";
        needsPassphrase = false;
        error = "";
        errorCode = "";
        working = false;
        open();
        pathField.forceActiveFocus();
    }

    function submit() {
        if (working || !ready())
            return;
        if (Keychain.vaultStatus === "locked") {
            shell.unlockVault(() => dialog.submit());
            return;
        }
        working = true;
        error = "";
        errorCode = "";
        const token = pasting ? Keychain.importKeyText(keyText.text, passphraseField.text, nameField.text)
                              : Keychain.importKeyFile(pathField.text, passphraseField.text, nameField.text);
        KeychainTasks.run(token, (code, detail) => {
            dialog.working = false;
            if (code.length === 0) {
                keyText.text = "";
                passphraseField.text = "";
                dialog.accept();
                Toasts.show(qsTr("Key imported."), "success");
                return;
            }
            dialog.errorCode = code;
            dialog.error = KeychainTasks.message(code, detail);
            if (code === "needs-passphrase" || code === "wrong-passphrase") {
                dialog.needsPassphrase = true;
                passphraseField.forceActiveFocus();
            }
        });
    }

    function ready() {
        return pasting ? keyText.text.trim().length > 0 : pathField.text.trim().length > 0;
    }

    title: qsTr("Import a key")
    acceptText: qsTr("Import")
    acceptEnabled: !working && pathField.text.length + keyText.text.length > 0
    closeOnAccept: false

    onAcceptClicked: submit()
    onClosed: {
        keyText.text = "";
        passphraseField.text = "";
    }

    Column {
        width: Math.min(Theme.spacingXxl * 17, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingSm

        OsText {
            width: parent.width
            text: qsTr("OpenSSH private keys and PuTTY .ppk files (versions 2 and 3) are supported.")
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }

        OsSwitch {
            text: qsTr("Paste the key instead of choosing a file")
            checked: dialog.pasting
            enabled: !dialog.working
            onToggled: dialog.pasting = checked
        }

        RowLayout {
            width: parent.width
            visible: !dialog.pasting
            spacing: Theme.spacingSm

            OsTextField {
                id: pathField

                Layout.fillWidth: true
                enabled: !dialog.working
                placeholderText: qsTr("Key file")
                Accessible.name: qsTr("Key file")
                onAccepted: dialog.submit()
            }

            OsButton {
                text: qsTr("Choose…")
                iconName: "folder-open"
                enabled: !dialog.working
                onClicked: fileDialog.open()
            }
        }

        Rectangle {
            width: parent.width
            height: Theme.rowHeight * 5
            visible: dialog.pasting
            radius: Theme.radiusControl
            color: Theme.surface2
            border.width: Theme.borderWidth
            border.color: keyText.activeFocus ? Theme.accent : Theme.borderStrong

            Flickable {
                id: keyFlick

                anchors.fill: parent
                anchors.margins: Theme.spacingSm
                clip: true
                contentWidth: width
                contentHeight: keyText.implicitHeight
                boundsBehavior: Flickable.StopAtBounds

                T.TextArea {
                    id: keyText

                    width: keyFlick.width
                    // A template TextArea keeps a one-line implicit height: it grows with its text.
                    implicitHeight: contentHeight + topPadding + bottomPadding
                    enabled: !dialog.working
                    wrapMode: TextEdit.WrapAnywhere
                    color: Theme.text
                    selectionColor: Theme.selection
                    selectedTextColor: Theme.text
                    font.family: Theme.monoFontFamily
                    font.pixelSize: Theme.fontSizeSmall
                    inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText
                    placeholderText: qsTr("-----BEGIN OPENSSH PRIVATE KEY-----")
                    placeholderTextColor: Theme.textMuted
                    Accessible.name: qsTr("Private key")
                }

                T.ScrollBar.vertical: OsScrollBar {}
            }
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Name")

            OsTextField {
                id: nameField

                width: parent.width
                enabled: !dialog.working
                placeholderText: qsTr("The key's comment")
                Accessible.name: qsTr("Name")
            }
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Passphrase")
            helpText: dialog.needsPassphrase ? "" : qsTr("Only if the key has one.")

            OsPasswordField {
                id: passphraseField

                width: parent.width
                enabled: !dialog.working
                error: dialog.errorCode === "wrong-passphrase"
                placeholderText: qsTr("Passphrase")
                Accessible.name: qsTr("Passphrase of the key")
                onAccepted: dialog.submit()
            }
        }

        OsProgress {
            width: parent.width
            visible: dialog.working
            indeterminate: true
        }

        RowLayout {
            width: parent.width
            visible: dialog.error.length > 0
            spacing: Theme.spacingSm

            OsText {
                Layout.fillWidth: true
                text: dialog.error
                color: dialog.errorCode === "needs-passphrase" ? Theme.warning : Theme.danger
                wrapMode: Text.Wrap
                elide: Text.ElideNone
            }

            OsButton {
                visible: dialog.errorCode === "needs-vault"
                text: qsTr("Set a master password…")
                onClicked: dialog.shell.showMasterPassword("create")
            }
        }
    }

    FileDialog {
        id: fileDialog

        title: qsTr("Choose a private key")
        nameFilters: [qsTr("Private keys (id_* *.pem *.key *.ppk)"), qsTr("All files (*)")]
        onAccepted: pathField.text = Platform.localPath(selectedFile)
    }
}
