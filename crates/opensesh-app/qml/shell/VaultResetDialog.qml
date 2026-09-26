// Deletes the vault after a lost master password (Sprint 6): every saved password and private
// key is gone; identities and the public halves of keys stay. The user must tick that they
// understand before the button works.
// Functions: show().
import QtQuick
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property bool working: false

    function show() {
        understood.checked = false;
        working = false;
        open();
    }

    title: qsTr("Reset the vault?")
    acceptText: qsTr("Delete every secret")
    dangerous: true
    acceptEnabled: understood.checked && !working
    closeOnAccept: false

    onAcceptClicked: {
        working = true;
        KeychainTasks.run(Keychain.resetVault(), (code, detail) => {
            dialog.working = false;
            dialog.accept();
            if (code.length === 0)
                Toasts.show(qsTr("The vault was reset. Its passwords and private keys are gone."), "warning");
            else
                Toasts.show(KeychainTasks.message(code, detail), "danger");
        });
    }

    Column {
        width: Math.min(Theme.spacingXxl * 13, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsText {
            width: parent.width
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            text: qsTr("Nobody can open the vault without its master password, not even OpenSesh. Resetting deletes it: every saved password and private key is lost. Identities, hosts and public keys stay.")
        }

        OsCheckBox {
            id: understood

            width: parent.width
            text: qsTr("I understand that my saved passwords and private keys will be deleted")
        }
    }
}
