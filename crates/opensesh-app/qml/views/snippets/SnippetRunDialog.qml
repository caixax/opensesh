pragma ComponentBehavior: Bound

// Running a snippet (Sprint 10): the values of its variables (the last ones offered), asked
// once for every pane, and where it runs: the focused pane, every pane of the tab, or the
// broadcast panes. Secrets are never asked here: they come from the keychain.
// Functions: show(snippet, workspace, where) (`where`: "pane", "tab" or "broadcast" to start with).
import QtQuick
import QtQuick.Layouts
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property var snippet: ({ id: "", name: "", variables: [], secrets: [] })
    property Item workspace: null
    property string where: "pane"
    property var values: ({})
    readonly property var targets: !workspace ? []
        : where === "tab" ? workspace.paneIds
        : where === "broadcast" ? workspace.participants
        : [workspace.focusedPane]
    readonly property bool complete: (snippet.variables ?? []).every(name => (values[name] ?? "").length > 0)

    function show(entry, target, start) {
        snippet = entry;
        workspace = target;
        where = start;
        values = JSON.parse(Snippets.lastValues(entry.id) || "{}");
        open();
    }

    function set(name, value) {
        const next = Object.assign({}, values);
        next[name] = value;
        values = next;
    }

    title: qsTr("Run %1").arg(snippet.name)
    acceptText: qsTr("Run")
    acceptEnabled: complete && targets.length > 0
    onAccepted: Snippets.run(snippet.id, JSON.stringify(values), JSON.stringify(targets))

    Column {
        width: Math.min(Theme.spacingXxl * 14, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        Repeater {
            model: dialog.snippet.variables ?? []

            delegate: OsFormRow {
                id: variableRow

                required property string modelData
                required property int index

                width: parent ? parent.width : 0
                label: modelData

                OsTextField {
                    width: parent.width
                    text: dialog.values[variableRow.modelData] ?? ""
                    focus: variableRow.index === 0
                    Accessible.name: variableRow.modelData
                    onTextEdited: dialog.set(variableRow.modelData, text)
                    onAccepted: {
                        if (dialog.acceptEnabled)
                            dialog.accept();
                    }
                }
            }
        }

        OsText {
            width: parent.width
            visible: (dialog.snippet.secrets ?? []).length > 0
            text: qsTr("Types the password of %1 from the keychain.").arg((dialog.snippet.secrets ?? []).join(", "))
            muted: true
            size: "small"
            wrapMode: Text.Wrap
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Run in")

            OsComboBox {
                readonly property var choices: {
                    const out = [{ text: qsTr("The focused pane"), value: "pane" }];
                    if (dialog.workspace && dialog.workspace.paneIds.length > 1)
                        out.push({ text: qsTr("Every pane of this tab (%1)").arg(dialog.workspace.paneIds.length), value: "tab" });
                    if (dialog.workspace && dialog.workspace.broadcast)
                        out.push({ text: qsTr("The broadcast panes (%1)").arg(dialog.workspace.participants.length), value: "broadcast" });
                    return out;
                }

                width: parent.width
                model: choices
                textRole: "text"
                valueRole: "value"
                currentIndex: Math.max(0, choices.findIndex(choice => choice.value === dialog.where))
                Accessible.name: qsTr("Run in")
                onActivated: dialog.where = currentValue
            }
        }
    }
}
