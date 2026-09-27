pragma ComponentBehavior: Bound

// A file already at the destination of a transfer (Sprint 8, Transfers.question): the two files'
// sizes and times, and what to do: replace, replace if newer, continue (a shorter file that
// matches so far), skip, keep both (a new name), or cancel the transfer; for this file or, with
// "for every file", the rest of the job too. It opens by itself while a question waits.
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    readonly property var question: Transfers.question.length > 0 ? JSON.parse(Transfers.question) : ({})
    readonly property bool asking: question.job !== undefined

    function choose(choice) {
        const job = question.job;
        close();
        Transfers.answer(job, choice, forAll.checked);
        forAll.checked = false;
    }

    title: qsTr("%1 is already there").arg(question.name ?? "")
    showReject: false
    acceptText: qsTr("Replace")
    closePolicy: T.Popup.NoAutoClose

    onAskingChanged: {
        if (asking)
            open();
        else
            close();
    }
    onAcceptClicked: choose("overwrite")

    Column {
        width: Math.min(Theme.spacingXxl * 15, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsText {
            width: parent.width
            text: dialog.question.path ?? ""
            elide: Text.ElideMiddle
            muted: true
        }

        GridLayout {
            width: parent.width
            columns: 3
            columnSpacing: Theme.spacingLg
            rowSpacing: Theme.spacingXs

            Item {
                implicitWidth: 1
                implicitHeight: 1
            }
            OsText {
                text: qsTr("Size")
                muted: true
                size: "small"
            }
            OsText {
                text: qsTr("Modified")
                muted: true
                size: "small"
            }

            OsText {
                text: qsTr("Being copied")
            }
            OsText {
                text: FileFormat.size(dialog.question.sourceSize ?? 0)
            }
            OsText {
                text: FileFormat.time(dialog.question.sourceModified)
            }

            OsText {
                text: qsTr("Already there")
            }
            OsText {
                text: FileFormat.size(dialog.question.existingSize ?? 0)
            }
            OsText {
                text: FileFormat.time(dialog.question.existingModified)
            }
        }

        Flow {
            width: parent.width
            spacing: Theme.spacingSm

            OsButton {
                visible: dialog.question.resumable === true
                text: qsTr("Continue it")
                onClicked: dialog.choose("resume")
            }
            OsButton {
                text: qsTr("Replace if newer")
                onClicked: dialog.choose("newer")
            }
            OsButton {
                text: qsTr("Keep both")
                onClicked: dialog.choose("rename")
            }
            OsButton {
                text: qsTr("Skip")
                onClicked: dialog.choose("skip")
            }
            OsButton {
                variant: "ghost"
                text: qsTr("Cancel the transfer")
                onClicked: dialog.choose("cancel")
            }
        }

        OsCheckBox {
            id: forAll

            text: qsTr("Do this for every file of this transfer")
        }
    }
}
