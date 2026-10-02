pragma ComponentBehavior: Bound

// Shell integration (Sprint 8): a few lines for the server's ~/.bashrc or ~/.zshrc that tell the
// terminal the shell's folder before each prompt (OSC 7), so the side panel can follow `cd`.
// They can be copied, or added to the file when the user asks: OpenSesh never changes a remote
// rc file on its own.
// Functions: show(browser) (the side panel's file pane, whose connection adds the lines).
import QtQuick
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property SftpBrowser browser: null
    property string shell: "bash"
    property int token: 0
    readonly property string snippet: browser ? browser.shellIntegration(shell) : ""

    function show(source) {
        browser = source;
        token = 0;
        open();
    }

    title: qsTr("Follow the terminal's folder")
    acceptText: qsTr("Add to ~/.%1rc").arg(shell)
    acceptEnabled: browser !== null && browser.remote && token === 0
    closeOnAccept: false

    onAcceptClicked: token = browser.installShellIntegration(shell)

    Connections {
        target: dialog.browser

        function onDone(token, code, detail) {
            if (token !== dialog.token)
                return;
            dialog.token = 0;
            if (code.length === 0) {
                Toasts.show(qsTr("Added to ~/.%1rc. It works in new shells, or after `source ~/.%1rc`.").arg(dialog.shell), "success");
                dialog.accept();
            } else {
                Toasts.show(qsTr("The lines couldn't be added."), "danger", "", "", detail.length > 0 ? detail : code);
            }
        }
    }

    Column {
        width: Math.min(Theme.spacingXxl * 18, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsText {
            width: parent.width
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            text: qsTr("The shell on this server doesn't say which folder it is in, so the files can't follow `cd`. These lines make it say so before each prompt.")
        }

        SettingsChoice {
            width: parent.width
            values: ["bash", "zsh"]
            labels: ({
                    bash: qsTr("bash"),
                    zsh: qsTr("zsh")
                })
            value: dialog.shell
            Accessible.name: qsTr("Shell")
            onPicked: value => dialog.shell = value
        }

        Rectangle {
            width: parent.width
            height: code.implicitHeight + 2 * Theme.spacingSm
            radius: Theme.radiusControl
            color: Theme.surface2

            TextEdit {
                id: code

                anchors.fill: parent
                anchors.margins: Theme.spacingSm
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.WrapAnywhere
                textFormat: TextEdit.PlainText
                text: dialog.snippet
                color: Theme.text
                selectionColor: Theme.selection
                selectedTextColor: Theme.text
                font.family: Theme.monoFontFamily
                font.pixelSize: Theme.fontSizeSmall
                Accessible.role: Accessible.EditableText
                Accessible.name: qsTr("Shell integration lines")
            }
        }

        OsButton {
            text: qsTr("Copy the lines")
            iconName: "copy"
            onClicked: {
                Platform.copyText(dialog.snippet);
                Toasts.show(qsTr("Copied."), "success");
            }
        }

        OsText {
            width: parent.width
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            size: "small"
            muted: true
            text: qsTr("\"Add\" appends them to the file on the server unless they are there already.")
        }
    }
}
