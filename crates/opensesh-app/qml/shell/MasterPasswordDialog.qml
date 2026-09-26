pragma ComponentBehavior: Bound

// The master password (PLAN §8, Sprint 6), in one of four modes:
//   "create"  a new vault protected by a master password (no vault yet)
//   "set"     protect a vault the system keyring holds with a master password
//   "change"  change it (asks for the current one)
//   "remove"  remove it: the system keyring holds the vault key again (asks for the current one)
// "Remember on this computer" keeps a copy of the vault key in the system keyring, so the vault
// opens without the password here. New passwords need at least 8 characters, typed twice.
// Functions: show(mode).
import QtQuick
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property string mode: "set"
    property string error: ""
    property bool working: false
    readonly property int minimumLength: 8
    readonly property bool asksCurrent: mode === "change" || mode === "remove"
    readonly property bool asksNew: mode !== "remove"
    readonly property bool newTooShort: asksNew && newField.text.length > 0 && newField.text.length < minimumLength
    readonly property bool mismatch: asksNew && confirmField.text.length > 0 && confirmField.text !== newField.text
    readonly property int waitSeconds: KeychainTasks.waitSeconds()
    readonly property bool ready: !working
                                  && (!asksCurrent || (currentField.text.length > 0 && waitSeconds === 0))
                                  && (!asksNew || (newField.text.length >= minimumLength
                                                   && confirmField.text === newField.text))

    function show(mode) {
        dialog.mode = mode;
        error = "";
        working = false;
        currentField.text = "";
        newField.text = "";
        confirmField.text = "";
        rememberBox.checked = false;
        open();
        (asksCurrent ? currentField : newField).forceActiveFocus();
    }

    function submit() {
        if (!ready)
            return;
        working = true;
        error = "";
        let token = 0;
        switch (mode) {
        case "create":
            token = Keychain.createVault(newField.text, rememberBox.checked);
            break;
        case "set":
            token = Keychain.setMasterPassword(newField.text, rememberBox.checked);
            break;
        case "change":
            token = Keychain.changeMasterPassword(currentField.text, newField.text);
            break;
        default:
            token = Keychain.removeMasterPassword(currentField.text);
            break;
        }
        currentField.text = "";
        KeychainTasks.run(token, (code, detail) => {
            dialog.working = false;
            if (code.length === 0) {
                newField.text = "";
                confirmField.text = "";
                dialog.accept();
                Toasts.show(dialog.mode === "remove" ? qsTr("The master password was removed: the system keyring holds the vault key.")
                          : dialog.mode === "change" ? qsTr("The master password was changed.")
                          : qsTr("The vault is protected by your master password."), "success");
            } else {
                dialog.error = KeychainTasks.message(code, detail);
            }
        });
    }

    title: mode === "create" ? qsTr("Create the vault")
         : mode === "set" ? qsTr("Set a master password")
         : mode === "change" ? qsTr("Change the master password")
         : qsTr("Remove the master password")
    acceptText: mode === "remove" ? qsTr("Remove") : mode === "change" ? qsTr("Change") : qsTr("Set password")
    dangerous: mode === "remove"
    acceptEnabled: ready
    closeOnAccept: false

    onAcceptClicked: submit()

    Column {
        width: Math.min(Theme.spacingXxl * 14, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsText {
            width: parent.width
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            text: dialog.mode === "remove"
                  ? qsTr("Without a master password, the system keyring holds the key of the vault: anyone who can use your account on this computer can use your saved passwords and keys.")
                  : dialog.mode === "change"
                    ? qsTr("The vault is encrypted again with the new password.")
                    : qsTr("Your passwords and keys are encrypted with a key derived from this password (Argon2id). If you forget it, they can't be recovered.")
        }

        OsFormRow {
            width: parent.width
            visible: dialog.asksCurrent
            label: qsTr("Current password")

            OsPasswordField {
                id: currentField

                width: parent.width
                enabled: !dialog.working
                placeholderText: qsTr("Current master password")
                Accessible.name: qsTr("Current master password")
                onAccepted: dialog.asksNew ? newField.forceActiveFocus() : dialog.submit()
            }
        }

        OsFormRow {
            width: parent.width
            visible: dialog.asksNew
            label: qsTr("New password")
            errorText: dialog.newTooShort ? qsTr("Use at least %n character(s).", "", dialog.minimumLength) : ""

            OsPasswordField {
                id: newField

                width: parent.width
                enabled: !dialog.working
                error: dialog.newTooShort
                placeholderText: qsTr("New master password")
                Accessible.name: qsTr("New master password")
                onAccepted: confirmField.forceActiveFocus()
            }
        }

        OsFormRow {
            width: parent.width
            visible: dialog.asksNew
            label: qsTr("Repeat it")
            errorText: dialog.mismatch ? qsTr("The passwords don't match.") : ""

            OsPasswordField {
                id: confirmField

                width: parent.width
                enabled: !dialog.working
                error: dialog.mismatch
                placeholderText: qsTr("The same password again")
                Accessible.name: qsTr("Repeat the new master password")
                onAccepted: dialog.submit()
            }
        }

        OsCheckBox {
            id: rememberBox

            width: parent.width
            visible: dialog.mode === "create" || dialog.mode === "set"
            enabled: Keychain.keyringAvailable && !dialog.working
            text: qsTr("Remember on this computer (the system keyring keeps the vault key)")
        }

        OsProgress {
            width: parent.width
            visible: dialog.working
            indeterminate: true
        }

        OsText {
            width: parent.width
            visible: dialog.asksCurrent && dialog.waitSeconds > 0
            text: qsTr("Too many wrong passwords. Try again in %n second(s).", "", dialog.waitSeconds)
            color: Theme.warning
            wrapMode: Text.Wrap
            elide: Text.ElideNone
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
}
