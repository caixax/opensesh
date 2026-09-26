pragma ComponentBehavior: Bound

// Generates an SSH key into the vault (Sprint 6): Ed25519 (the default), ECDSA P-256, P-384 or
// P-521, or RSA 4096 (which takes a few seconds). The name labels it in the keychain; the
// comment goes into the public key. Asks to unlock the vault first when it is locked.
// Functions: show().
import QtQuick
import QtQuick.Layouts
import cc.caixa.opensesh

OsDialog {
    id: dialog

    required property Item shell
    property string error: ""
    property string errorCode: ""
    property bool working: false
    readonly property var types: [
        { text: qsTr("Ed25519 (recommended)"), value: "ed25519" },
        { text: qsTr("ECDSA P-256"), value: "ecdsa-p256" },
        { text: qsTr("ECDSA P-384"), value: "ecdsa-p384" },
        { text: qsTr("ECDSA P-521"), value: "ecdsa-p521" },
        { text: qsTr("RSA 4096"), value: "rsa-4096" }
    ]

    // Emitted with the new key's id.
    signal generated(string id)

    function show() {
        typeBox.currentIndex = 0;
        nameField.text = "";
        commentField.text = "";
        error = "";
        errorCode = "";
        working = false;
        open();
        nameField.forceActiveFocus();
    }

    function generate() {
        if (working)
            return;
        if (Keychain.vaultStatus === "locked") {
            shell.unlockVault(() => dialog.generate());
            return;
        }
        working = true;
        error = "";
        errorCode = "";
        KeychainTasks.run(Keychain.generateKey(typeBox.currentValue, nameField.text, commentField.text), (code, detail, value) => {
            dialog.working = false;
            if (code.length === 0) {
                dialog.accept();
                Toasts.show(qsTr("Key generated."), "success");
                dialog.generated(value);
            } else {
                dialog.errorCode = code;
                dialog.error = KeychainTasks.message(code, detail);
            }
        });
    }

    title: qsTr("Generate a key")
    acceptText: qsTr("Generate")
    acceptEnabled: !working
    closeOnAccept: false

    onAcceptClicked: generate()

    Column {
        width: Math.min(Theme.spacingXxl * 15, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingSm

        OsFormRow {
            width: parent.width
            label: qsTr("Type")
            helpText: typeBox.currentValue === "rsa-4096" ? qsTr("RSA keys take a few seconds to generate. Prefer Ed25519 unless a server needs RSA.")
                                                         : ""

            OsComboBox {
                id: typeBox

                width: parent.width
                model: dialog.types
                textRole: "text"
                valueRole: "value"
                enabled: !dialog.working
                Accessible.name: qsTr("Key type")
            }
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Name")

            OsTextField {
                id: nameField

                width: parent.width
                enabled: !dialog.working
                placeholderText: qsTr("e.g. Laptop")
                Accessible.name: qsTr("Name")
            }
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Comment")
            helpText: qsTr("Written into the public key, e.g. you@this-computer.")

            OsTextField {
                id: commentField

                width: parent.width
                enabled: !dialog.working
                Accessible.name: qsTr("Comment")
                onAccepted: dialog.generate()
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
                color: Theme.danger
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
}
