pragma ComponentBehavior: Bound

// What editing a server's file may need to ask (Sprint 8, Transfers.editEvent): the server's copy
// changed since it was opened (upload anyway, take the server's copy, or keep waiting), and the
// server refused the save (try again through `sudo tee`, with the password typed here once).
// It also opens a downloaded copy with the system's editor when no editor command is set, and
// says when a save reached the server.
import QtQuick
import cc.caixa.opensesh

Item {
    id: dialogs

    Connections {
        target: Transfers

        function onEditReady(id, local, opened) {
            if (!opened)
                Qt.openUrlExternally(FileFormat.fileUrl(local));
        }

        function onEditEvent(id, what, name, detail) {
            switch (what) {
            case "saved":
                // A conflict settled some other way needs no answer any more.
                if (conflictDialog.visible && conflictDialog.edit === id)
                    conflictDialog.close();
                Toasts.show(qsTr("%1 saved to the server.").arg(name), "success");
                break;
            case "conflict":
                conflictDialog.edit = id;
                conflictDialog.name = name;
                conflictDialog.open();
                break;
            case "denied":
                sudoDialog.edit = id;
                sudoDialog.name = name;
                sudoDialog.open();
                break;
            case "failed":
                Toasts.show(qsTr("%1 couldn't be saved to the server: %2").arg(name).arg(detail), "danger");
                break;
            }
        }
    }

    OsDialog {
        id: conflictDialog

        property int edit: 0
        property string name: ""

        title: qsTr("%1 changed on the server").arg(name)
        acceptText: qsTr("Replace it with mine")
        rejectText: qsTr("Not now")
        dangerous: true

        onAccepted: Transfers.resolveEdit(edit, "overwrite")
        onRejected: Transfers.resolveEdit(edit, "wait")

        Column {
            width: Math.min(Theme.spacingXxl * 14, conflictDialog.maxWidth - conflictDialog.leftPadding - conflictDialog.rightPadding)
            spacing: Theme.spacingMd

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                text: qsTr("Someone or something changed the file on the server after you opened it. Your save would replace their changes.")
            }

            OsButton {
                text: qsTr("Take the server's copy (my changes are lost)")
                onClicked: {
                    conflictDialog.close();
                    Transfers.resolveEdit(conflictDialog.edit, "discard");
                }
            }
        }
    }

    OsDialog {
        id: sudoDialog

        property int edit: 0
        property string name: ""

        title: qsTr("Save %1 with sudo?").arg(name)
        acceptText: qsTr("Save with sudo")
        dangerous: true

        onOpened: {
            password.text = "";
            password.forceActiveFocus();
        }
        onAccepted: {
            Transfers.saveWithSudo(edit, password.text);
            password.text = "";
        }
        onClosed: password.text = ""

        Column {
            width: Math.min(Theme.spacingXxl * 14, sudoDialog.maxWidth - sudoDialog.leftPadding - sudoDialog.rightPadding)
            spacing: Theme.spacingMd

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                text: qsTr("The server didn't let your user write this file. With sudo, the file is written as root (`sudo tee`): make sure you mean to change it.")
            }

            OsPasswordField {
                id: password

                width: parent.width
                placeholderText: qsTr("sudo password (empty if it asks for none)")
                Accessible.name: qsTr("sudo password")
                onAccepted: sudoDialog.accept()
            }
        }
    }
}
