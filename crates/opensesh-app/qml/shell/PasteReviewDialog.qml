pragma ComponentBehavior: Bound

// The paste review (Sprint 10, PLAN §8): a paste the analyzer found something in, or the first
// paste into several panes while broadcasting, waits here. It lists what was found (a click
// selects it in the text), and the text itself, editable, is what gets pasted.
// Functions: show(text, findingsJson, broadcast, receivers). Signal: pasteRequested(text).
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property var findings: []
    property bool broadcast: false
    property int receivers: 0
    readonly property bool risky: findings.some(finding => finding.severity === "danger")

    signal pasteRequested(string text)

    function show(text, findingsJson, toBroadcast, receiverCount) {
        editor.text = text;
        findings = JSON.parse(findingsJson || "[]");
        broadcast = toBroadcast;
        receivers = receiverCount;
        open();
    }

    function message(finding) {
        switch (finding.kind) {
        case "runs-at-once":
            return qsTr("%n line(s) run as soon as they are pasted", "", Number(finding.detail));
        case "multiline":
            return qsTr("%n lines; they wait for Enter", "", Number(finding.detail));
        case "control":
            return finding.severity === "danger"
                   ? qsTr("An escape character (%1): it can end the paste early and run what follows").arg(finding.detail)
                   : qsTr("A control character (%1)").arg(finding.detail);
        case "invisible":
            return qsTr("An invisible character (%1)").arg(finding.detail);
        case "bidi":
            return qsTr("A character that reorders text (%1): what you see isn't what runs").arg(finding.detail);
        case "overwrite":
            return qsTr("A carriage return inside a line: what follows is printed over what came before");
        case "homoglyph":
            return qsTr("Letters of another alphabet in a Latin word: %1").arg(finding.detail);
        case "pipe-to-shell":
            return qsTr("Runs a download: %1").arg(finding.detail);
        case "decode-to-shell":
            return qsTr("Runs decoded or computed text: %1").arg(finding.detail);
        case "profile-write":
            return qsTr("Writes to a shell profile, SSH keys or a system file: %1").arg(finding.detail);
        case "sudo-pipe":
            return qsTr("sudo in a pipe or running a shell: %1").arg(finding.detail);
        case "destructive":
            return qsTr("Destroys data: %1").arg(finding.detail);
        default:
            return finding.detail;
        }
    }

    title: findings.length > 0 ? qsTr("Check this paste") : qsTr("Paste into %n panes?", "", receivers)
    acceptText: risky ? qsTr("Paste anyway") : qsTr("Paste")
    dangerous: risky
    onAccepted: pasteRequested(editor.text)

    ColumnLayout {
        width: Math.min(Theme.spacingXxl * 18, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsText {
            Layout.fillWidth: true
            visible: dialog.broadcast
            text: qsTr("Broadcast is on, so the text goes to every receiving pane of this tab (%n). You won't be asked again until broadcast is turned off, unless a paste looks risky.", "", dialog.receivers)
            wrapMode: Text.Wrap
        }

        ListView {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(contentHeight, Theme.spacingXxl * 4)
            visible: dialog.findings.length > 0
            clip: true
            spacing: Theme.spacingXs
            model: dialog.findings
            Accessible.role: Accessible.List
            Accessible.name: qsTr("What was found")

            T.ScrollBar.vertical: OsScrollBar {}

            delegate: T.AbstractButton {
                id: findingRow

                required property var modelData

                width: ListView.view.width
                implicitHeight: findingLayout.implicitHeight + Theme.spacingXs
                hoverEnabled: true
                Accessible.name: dialog.message(modelData)
                onClicked: {
                    editor.forceActiveFocus();
                    editor.select(modelData.start, modelData.end);
                }

                background: Rectangle {
                    radius: Theme.radiusControl
                    color: findingRow.hovered ? Theme.hover : "transparent"
                }

                contentItem: RowLayout {
                    id: findingLayout

                    spacing: Theme.spacingSm

                    OsIcon {
                        Layout.alignment: Qt.AlignTop
                        name: findingRow.modelData.severity === "info" ? "info" : "triangle-alert"
                        size: Theme.iconSizeSmall
                        color: findingRow.modelData.severity === "danger" ? Theme.danger
                             : findingRow.modelData.severity === "warning" ? Theme.warning : Theme.textMuted
                    }
                    OsText {
                        Layout.fillWidth: true
                        text: dialog.message(findingRow.modelData)
                        wrapMode: Text.Wrap
                        maximumLineCount: 3
                        elide: Text.ElideRight
                    }
                    OsText {
                        Layout.alignment: Qt.AlignTop
                        visible: findingRow.modelData.kind !== "runs-at-once" && findingRow.modelData.kind !== "multiline"
                        text: qsTr("line %1").arg(findingRow.modelData.line)
                        muted: true
                        size: "small"
                    }
                }
            }
        }

        OsText {
            Layout.fillWidth: true
            text: qsTr("What will be pasted (you can edit it):")
            muted: true
            size: "small"
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.spacingXxl * 5
            radius: Theme.radiusControl
            color: Theme.surface
            border.width: Theme.borderWidth
            border.color: editor.activeFocus ? Theme.focusRing : Theme.border

            Flickable {
                id: textFlick

                anchors.fill: parent
                anchors.margins: Theme.spacingSm
                clip: true
                contentWidth: width
                contentHeight: editor.implicitHeight
                boundsBehavior: Flickable.StopAtBounds

                T.TextArea {
                    id: editor

                    width: textFlick.width
                    wrapMode: TextEdit.WrapAnywhere
                    color: Theme.text
                    selectionColor: Theme.selection
                    selectedTextColor: Theme.text
                    font.family: Theme.monoFontFamily
                    font.pixelSize: Theme.fontSizeSmall
                    inputMethodHints: Qt.ImhNoPredictiveText
                    Accessible.name: qsTr("Text to paste")
                    onCursorRectangleChanged: {
                        if (cursorRectangle.y < textFlick.contentY)
                            textFlick.contentY = cursorRectangle.y;
                        else if (cursorRectangle.y + cursorRectangle.height > textFlick.contentY + textFlick.height)
                            textFlick.contentY = cursorRectangle.y + cursorRectangle.height - textFlick.height;
                    }
                }

                T.ScrollBar.vertical: OsScrollBar {}
            }
        }
    }
}
