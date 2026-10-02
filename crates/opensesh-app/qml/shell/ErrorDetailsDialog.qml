pragma ComponentBehavior: Bound

// The technical details behind a message (Sprint 17): what a library, the system or a server
// said, a path, a code. The message stays human; the details are here, selectable and copyable,
// for a bug report or a search.
// Functions: show(toast) (an entry of Toasts.history with `details`).
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property string message: ""
    property string details: ""

    function show(toast) {
        message = toast.text ?? "";
        details = toast.details ?? "";
        open();
    }

    title: qsTr("Details")
    acceptText: qsTr("Copy")
    rejectText: qsTr("Close")
    closeOnAccept: false
    onAcceptClicked: {
        Platform.copyText(details);
        Toasts.show(qsTr("The details are on the clipboard."), "info");
    }

    Column {
        width: Math.min(Theme.spacingXxl * 16, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsText {
            width: parent.width
            text: dialog.message
            wrapMode: Text.Wrap
        }

        Rectangle {
            width: parent.width
            height: Math.min(detailsText.implicitHeight + 2 * Theme.spacingSm, Theme.rowHeight * 8)
            color: Theme.surface2
            radius: Theme.radiusControl
            border.color: Theme.border
            border.width: Theme.borderWidth

            Flickable {
                id: scroller

                anchors.fill: parent
                anchors.margins: Theme.spacingSm
                contentWidth: width
                contentHeight: detailsText.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds

                TextEdit {
                    id: detailsText

                    width: scroller.width
                    text: dialog.details
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextEdit.WrapAnywhere
                    color: Theme.text
                    selectionColor: Theme.selection
                    selectedTextColor: Theme.text
                    font.family: Theme.monoFontFamily
                    font.pixelSize: Theme.fontSizeSmall
                    Accessible.role: Accessible.EditableText
                    Accessible.name: qsTr("Technical details")
                    Accessible.readOnly: true
                }

                T.ScrollBar.vertical: OsScrollBar {}
            }
        }
    }
}
