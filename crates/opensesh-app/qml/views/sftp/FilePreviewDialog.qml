pragma ComponentBehavior: Bound

// A quick look at a file (Sprint 8): the start of a text file in the terminal font, or an image.
// Binary files and images too big to show say so.
// Functions: show(browser, row).
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property SftpBrowser browser: null
    property int token: 0
    property string name: ""
    property string kind: "loading"
    property string content: ""

    function show(source, row) {
        browser = source;
        const entry = JSON.parse(source.entryJson(row) || "{}");
        name = entry.name ?? "";
        kind = "loading";
        content = "";
        token = source.preview(row);
        open();
    }

    title: name
    acceptText: qsTr("Close")
    showReject: false

    Connections {
        target: dialog.browser

        function onPreviewReady(token, kind, content) {
            if (token !== dialog.token)
                return;
            dialog.kind = kind;
            dialog.content = content;
        }
    }

    Item {
        implicitWidth: Math.min(Theme.spacingXxl * 22, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        implicitHeight: Math.min(Theme.spacingXxl * 14, dialog.maxHeight - Theme.spacingXxl * 3)

        OsProgress {
            anchors.centerIn: parent
            visible: dialog.kind === "loading"
            indeterminate: true
        }

        OsText {
            anchors.centerIn: parent
            width: parent.width
            visible: dialog.kind !== "loading" && dialog.kind !== "text" && dialog.kind !== "image"
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            muted: true
            text: {
                switch (dialog.kind) {
                case "binary":
                    return qsTr("This file isn't text.");
                case "too-big":
                    return qsTr("This image is too big for a quick look (over 8 MB).");
                default:
                    return qsTr("The file can't be read: %1").arg(dialog.content);
                }
            }
        }

        Image {
            anchors.fill: parent
            visible: dialog.kind === "image"
            source: dialog.kind === "image" ? dialog.content : ""
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            smooth: true
        }

        Flickable {
            id: flick

            anchors.fill: parent
            visible: dialog.kind === "text"
            clip: true
            contentWidth: text.implicitWidth
            contentHeight: text.implicitHeight
            boundsBehavior: Flickable.StopAtBounds

            TextEdit {
                id: text

                readOnly: true
                selectByMouse: true
                textFormat: TextEdit.PlainText
                text: dialog.kind === "text" ? dialog.content : ""
                color: Theme.text
                selectionColor: Theme.selection
                selectedTextColor: Theme.text
                font.family: Theme.monoFontFamily
                font.pixelSize: Theme.fontSizeSmall
                Accessible.role: Accessible.EditableText
                Accessible.name: dialog.name
            }

            T.ScrollBar.vertical: OsScrollBar {}
            T.ScrollBar.horizontal: OsScrollBar {}
        }
    }
}
