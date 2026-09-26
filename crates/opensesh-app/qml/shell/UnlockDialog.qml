pragma ComponentBehavior: Bound

// Unlocks a vault protected by a master password (PLAN §8, Sprint 6). After wrong passwords it
// counts down the wait before the next try (the Unlock button stays off meanwhile). "Forgot it?"
// offers to reset the vault.
// Functions: show(then) (`then` runs after a successful unlock).
import QtQuick
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property var then: null
    property string error: ""
    property bool working: false
    readonly property int waitSeconds: KeychainTasks.waitSeconds()

    signal resetRequested

    function show(then) {
        dialog.then = then ?? null;
        error = "";
        working = false;
        passwordField.text = "";
        passwordField.revealed = false;
        open();
        passwordField.forceActiveFocus();
    }

    function tryUnlock() {
        if (working || waitSeconds > 0 || passwordField.text.length === 0)
            return;
        working = true;
        error = "";
        const password = passwordField.text;
        passwordField.text = "";
        KeychainTasks.run(Keychain.unlock(password), (code, detail) => {
            dialog.working = false;
            if (code.length === 0) {
                const next = dialog.then;
                dialog.then = null;
                dialog.accept();
                if (next)
                    next();
            } else {
                dialog.error = KeychainTasks.message(code, detail);
                passwordField.forceActiveFocus();
            }
        });
    }

    title: qsTr("Unlock the vault")
    acceptText: qsTr("Unlock")
    acceptEnabled: !working && waitSeconds === 0 && passwordField.text.length > 0
    closeOnAccept: false

    onAcceptClicked: tryUnlock()

    Column {
        width: Math.min(Theme.spacingXxl * 12, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsText {
            width: parent.width
            text: qsTr("Your passwords and keys are encrypted with your master password.")
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }

        OsPasswordField {
            id: passwordField

            width: parent.width
            enabled: !dialog.working
            placeholderText: qsTr("Master password")
            Accessible.name: qsTr("Master password")
            onAccepted: dialog.tryUnlock()
        }

        OsProgress {
            width: parent.width
            visible: dialog.working
            indeterminate: true
        }

        OsText {
            width: parent.width
            visible: dialog.waitSeconds > 0
            text: qsTr("Too many wrong passwords. Try again in %n second(s).", "", dialog.waitSeconds)
            color: Theme.warning
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }

        OsText {
            width: parent.width
            visible: dialog.error.length > 0 && dialog.waitSeconds === 0
            text: dialog.error
            color: Theme.danger
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }

        OsButton {
            variant: "ghost"
            text: qsTr("Forgot it? Reset the vault…")
            onClicked: {
                dialog.close();
                dialog.resetRequested();
            }
        }
    }
}
