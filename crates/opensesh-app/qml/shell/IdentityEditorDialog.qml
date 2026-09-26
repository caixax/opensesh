pragma ComponentBehavior: Bound

// Identity editor (Sprint 6): a name, a user name, a password kept in the vault and/or a key
// from the keychain. An identity is given to hosts and groups in their editors.
// The password field starts empty: typing replaces the saved password, "Remove" deletes it, and
// leaving it alone keeps it. Saving a password asks to unlock the vault first when it is locked.
// Functions: create(), edit(id).
import QtQuick
import QtQuick.Layouts
import cc.caixa.opensesh

OsDialog {
    id: dialog

    required property Item shell
    property string identityId: ""
    property bool hadPassword: false
    property bool removePassword: false
    property string error: ""
    property string errorCode: ""
    property bool working: false
    readonly property var keys: JSON.parse(Keychain.keys || "[]")
    readonly property var keyOptions: [{ text: qsTr("No key"), value: "" }].concat(
        keys.map(key => ({ text: qsTr("%1 · %2").arg(key.name).arg(key.label), value: key.id })))

    function create() {
        load({ id: "", name: "", user: "", key: "", notes: "", hasPassword: false });
    }

    function edit(id) {
        const found = JSON.parse(Keychain.identities || "[]").find(identity => identity.id === id);
        if (found)
            load(found);
    }

    function load(identity) {
        identityId = identity.id;
        nameField.text = identity.name;
        userField.text = identity.user;
        notesField.text = identity.notes;
        passwordField.text = "";
        passwordField.revealed = false;
        hadPassword = identity.hasPassword;
        removePassword = false;
        keyBox.currentIndex = Math.max(0, keyOptions.findIndex(option => option.value === identity.key));
        error = "";
        errorCode = "";
        working = false;
        open();
        nameField.forceActiveFocus();
    }

    function passwordMode() {
        if (passwordField.text.length > 0)
            return "set";
        return removePassword ? "clear" : "keep";
    }

    function save() {
        if (working)
            return;
        const mode = passwordMode();
        // A new password goes into the vault: it must be open.
        if (mode === "set" && Keychain.vaultStatus === "locked") {
            shell.unlockVault(() => dialog.save());
            return;
        }
        working = true;
        error = "";
        errorCode = "";
        const identity = {
            id: identityId,
            name: nameField.text,
            user: userField.text,
            key: keyBox.currentValue ?? "",
            notes: notesField.text
        };
        KeychainTasks.run(Keychain.saveIdentity(JSON.stringify(identity), mode, passwordField.text), (code, detail) => {
            dialog.working = false;
            if (code.length === 0) {
                passwordField.text = "";
                dialog.accept();
            } else {
                dialog.errorCode = code;
                dialog.error = KeychainTasks.message(code, detail);
            }
        });
    }

    title: identityId.length > 0 ? qsTr("Edit identity") : qsTr("New identity")
    acceptText: qsTr("Save")
    acceptEnabled: !working && (nameField.text.trim().length > 0 || userField.text.trim().length > 0)
    closeOnAccept: false

    onAcceptClicked: save()
    onClosed: passwordField.text = ""

    Column {
        width: Math.min(Theme.spacingXxl * 16, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingSm

        OsFormRow {
            width: parent.width
            label: qsTr("Name")

            OsTextField {
                id: nameField

                width: parent.width
                placeholderText: qsTr("e.g. deploy on production")
                Accessible.name: qsTr("Name")
            }
        }

        OsFormRow {
            width: parent.width
            label: qsTr("User name")

            OsTextField {
                id: userField

                width: parent.width
                placeholderText: qsTr("e.g. deploy")
                Accessible.name: qsTr("User name")
            }
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Password")
            helpText: passwordField.text.length > 0 ? qsTr("Saved in the encrypted vault.")
                    : dialog.removePassword ? qsTr("The saved password will be removed.")
                    : dialog.hadPassword ? qsTr("A password is saved in the vault. Type a new one to replace it.")
                    : qsTr("Optional. Kept in the encrypted vault, never in a file in clear.")

            RowLayout {
                width: parent.width
                spacing: Theme.spacingSm

                OsPasswordField {
                    id: passwordField

                    Layout.fillWidth: true
                    placeholderText: dialog.hadPassword && !dialog.removePassword ? qsTr("Saved (unchanged)") : qsTr("Password")
                    Accessible.name: qsTr("Password")
                    onTextChanged: {
                        if (text.length > 0)
                            dialog.removePassword = false;
                    }
                }

                OsButton {
                    visible: dialog.hadPassword
                    variant: "ghost"
                    text: dialog.removePassword ? qsTr("Keep") : qsTr("Remove")
                    onClicked: {
                        passwordField.text = "";
                        dialog.removePassword = !dialog.removePassword;
                    }
                }
            }
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Key")
            helpText: dialog.keys.length === 0 ? qsTr("Generate or import keys in the Keys section of the keychain.") : ""

            OsComboBox {
                id: keyBox

                width: parent.width
                model: dialog.keyOptions
                textRole: "text"
                valueRole: "value"
                Accessible.name: qsTr("Key")
            }
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Notes")

            OsTextField {
                id: notesField

                width: parent.width
                Accessible.name: qsTr("Notes")
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
