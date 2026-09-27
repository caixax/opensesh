pragma ComponentBehavior: Bound

// The snippet editor (Sprint 10): a new snippet, changes to one, or a macro just recorded.
// Name, folder, tags, description, a shortcut, and either text (with `{{name}}` asked when it
// runs and `{{secret:identity}}` typed from the keychain) or macro steps: type text, wait for a
// pattern in the output (with a timeout), pause. `Snippets.check` says what is missing.
// Functions: show(entry) (an entry of Snippets.list, or null), showRecorded(steps).
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property var draft: ({})
    property bool recorded: false
    readonly property string problem: Snippets.check(JSON.stringify(draft))
    readonly property real fieldWidth: Math.min(Theme.spacingXxl * 18, maxWidth - leftPadding - rightPadding)

    function blank() {
        return { id: "", name: "", folder: "", tags: [], description: "", shortcut: "", text: "", steps: [], macro: false };
    }

    function show(entry) {
        recorded = false;
        draft = entry ? JSON.parse(JSON.stringify(entry)) : blank();
        textArea.text = draft.text ?? "";
        open();
    }

    function showRecorded(steps) {
        recorded = true;
        draft = Object.assign(blank(), { name: qsTr("Recorded macro"), macro: true, steps: steps });
        textArea.text = "";
        open();
    }

    function set(key, value) {
        const next = Object.assign({}, draft);
        next[key] = value;
        draft = next;
    }

    function setStep(index, key, value) {
        const steps = draft.steps.slice();
        steps[index] = Object.assign({}, steps[index]);
        steps[index][key] = value;
        set("steps", steps);
    }

    function addStep(kind) {
        const step = kind === "send" ? { kind: "send", text: "" }
                   : kind === "wait" ? { kind: "wait", pattern: "", timeout: 10000 }
                   : { kind: "delay", ms: 1000 };
        set("steps", draft.steps.concat([step]));
    }

    function moveStep(index, by) {
        const steps = draft.steps.slice();
        const target = index + by;
        if (target < 0 || target >= steps.length)
            return;
        const moved = steps.splice(index, 1)[0];
        steps.splice(target, 0, moved);
        set("steps", steps);
    }

    function removeStep(index) {
        const steps = draft.steps.slice();
        steps.splice(index, 1);
        set("steps", steps);
    }

    title: draft.id ? qsTr("Edit snippet") : recorded ? qsTr("Save the recorded macro") : qsTr("New snippet")
    acceptText: qsTr("Save")
    acceptEnabled: problem.length === 0
    closeOnAccept: false
    onAcceptClicked: {
        if (Snippets.save(JSON.stringify(draft)).length > 0)
            accept();
    }

    Flickable {
        id: form

        implicitWidth: dialog.fieldWidth
        // A fixed cap: the dialog's maxHeight falls back to its own height before it has a window.
        implicitHeight: Math.min(formColumn.implicitHeight, Theme.spacingXxl * 12)
        contentHeight: formColumn.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        T.ScrollBar.vertical: OsScrollBar {}

        // A fixed width (not the Flickable's): its height then never feeds back into it.
        Column {
            id: formColumn

            width: dialog.fieldWidth
            spacing: Theme.spacingMd

            OsText {
                width: parent.width
                visible: dialog.recorded
                text: qsTr("Everything typed was recorded as text, passwords too: remove them, or type them with {{secret:identity}} from the keychain.")
                color: Theme.warning
                wrapMode: Text.Wrap
            }

            OsFormRow {
                width: parent.width
                label: qsTr("Name")

                OsTextField {
                    width: parent.width
                    text: dialog.draft.name ?? ""
                    Accessible.name: qsTr("Name")
                    onTextEdited: dialog.set("name", text)
                }
            }

            OsFormRow {
                width: parent.width
                label: qsTr("Folder")

                OsTextField {
                    width: parent.width
                    text: dialog.draft.folder ?? ""
                    placeholderText: qsTr("Ops/Web")
                    Accessible.name: qsTr("Folder")
                    onTextEdited: dialog.set("folder", text)
                }
            }

            OsFormRow {
                width: parent.width
                label: qsTr("Tags")
                helpText: qsTr("Separated by commas.")

                OsTextField {
                    width: parent.width
                    text: (dialog.draft.tags ?? []).join(", ")
                    Accessible.name: qsTr("Tags")
                    onTextEdited: dialog.set("tags", text.split(",").map(tag => tag.trim()).filter(tag => tag.length > 0))
                }
            }

            OsFormRow {
                width: parent.width
                label: qsTr("Description")

                OsTextField {
                    width: parent.width
                    text: dialog.draft.description ?? ""
                    placeholderText: qsTr("Optional")
                    Accessible.name: qsTr("Description")
                    onTextEdited: dialog.set("description", text)
                }
            }

            OsFormRow {
                width: parent.width
                label: qsTr("Shortcut")

                OsKeybindCapture {
                    sequence: dialog.draft.shortcut ?? ""
                    Accessible.name: qsTr("Shortcut")
                    onSequenceEdited: sequence => dialog.set("shortcut", sequence)
                }
            }

            OsFormRow {
                width: parent.width
                label: qsTr("Type")

                OsComboBox {
                    readonly property var choices: [
                        { text: qsTr("Text"), value: false },
                        { text: qsTr("Macro (steps)"), value: true }
                    ]

                    width: parent.width
                    model: choices
                    textRole: "text"
                    valueRole: "value"
                    currentIndex: dialog.draft.macro ? 1 : 0
                    Accessible.name: qsTr("Type")
                    onActivated: {
                        dialog.set("macro", currentValue);
                        if (currentValue && (dialog.draft.steps ?? []).length === 0 && (dialog.draft.text ?? "").length > 0)
                            dialog.set("steps", [{ kind: "send", text: dialog.draft.text }]);
                    }
                }
            }

            // Text.
            Column {
                width: parent.width
                visible: !dialog.draft.macro
                spacing: Theme.spacingXs

                Rectangle {
                    width: parent.width
                    height: Theme.spacingXxl * 4
                    radius: Theme.radiusControl
                    color: Theme.surface
                    border.width: Theme.borderWidth
                    border.color: textArea.activeFocus ? Theme.focusRing : Theme.border

                    Flickable {
                        id: textFlick

                        anchors.fill: parent
                        anchors.margins: Theme.spacingSm
                        clip: true
                        contentWidth: width
                        contentHeight: textArea.implicitHeight
                        boundsBehavior: Flickable.StopAtBounds

                        T.TextArea {
                            id: textArea

                            width: textFlick.width
                            wrapMode: TextEdit.WrapAnywhere
                            color: Theme.text
                            selectionColor: Theme.selection
                            selectedTextColor: Theme.text
                            font.family: Theme.monoFontFamily
                            font.pixelSize: Theme.fontSizeSmall
                            inputMethodHints: Qt.ImhNoPredictiveText
                            placeholderText: qsTr("sudo systemctl restart {{service}}")
                            placeholderTextColor: Theme.textMuted
                            Accessible.name: qsTr("Text")
                            onTextChanged: {
                                if (text !== (dialog.draft.text ?? ""))
                                    dialog.set("text", text);
                            }
                        }
                    }
                }

                OsText {
                    width: parent.width
                    text: qsTr("{{name}} asks for a value when it runs; {{secret:identity}} types the password of a keychain identity. A new line is Enter.")
                    muted: true
                    size: "small"
                    wrapMode: Text.Wrap
                }
            }

            // Steps.
            Column {
                width: parent.width
                visible: dialog.draft.macro === true
                spacing: Theme.spacingSm

                Repeater {
                    model: dialog.draft.steps ?? []

                    delegate: RowLayout {
                        id: stepRow

                        required property var modelData
                        required property int index

                        width: parent ? parent.width : 0
                        spacing: Theme.spacingSm

                        OsText {
                            Layout.preferredWidth: Theme.spacingXxl * 2
                            text: stepRow.modelData.kind === "send" ? qsTr("Type")
                                : stepRow.modelData.kind === "wait" ? qsTr("Wait for")
                                : qsTr("Pause")
                            muted: true
                        }

                        OsTextField {
                            Layout.fillWidth: true
                            visible: stepRow.modelData.kind !== "delay"
                            font.family: Theme.monoFontFamily
                            text: stepRow.modelData.kind === "send" ? (stepRow.modelData.text ?? "").replace(/\n/g, "\\n")
                                                                    : (stepRow.modelData.pattern ?? "")
                            placeholderText: stepRow.modelData.kind === "send" ? qsTr("text; \\n is Enter") : qsTr("a pattern (regular expression)")
                            Accessible.name: stepRow.modelData.kind === "send" ? qsTr("Text of step %1").arg(stepRow.index + 1)
                                                                               : qsTr("Pattern of step %1").arg(stepRow.index + 1)
                            onTextEdited: {
                                if (stepRow.modelData.kind === "send")
                                    dialog.setStep(stepRow.index, "text", text.replace(/\\n/g, "\n"));
                                else
                                    dialog.setStep(stepRow.index, "pattern", text);
                            }
                        }

                        OsSpinBox {
                            visible: stepRow.modelData.kind !== "send"
                            Layout.fillWidth: stepRow.modelData.kind === "delay"
                            from: 1
                            to: 600000
                            stepSize: 100
                            value: stepRow.modelData.kind === "wait" ? stepRow.modelData.timeout : stepRow.modelData.ms
                            Accessible.name: stepRow.modelData.kind === "wait" ? qsTr("Timeout of step %1 in milliseconds").arg(stepRow.index + 1)
                                                                               : qsTr("Pause of step %1 in milliseconds").arg(stepRow.index + 1)
                            onValueModified: dialog.setStep(stepRow.index, stepRow.modelData.kind === "wait" ? "timeout" : "ms", value)
                        }

                        OsText {
                            visible: stepRow.modelData.kind !== "send"
                            text: qsTr("ms")
                            muted: true
                        }

                        OsIconButton {
                            iconName: "chevron-up"
                            toolTip: qsTr("Move up")
                            enabled: stepRow.index > 0
                            onClicked: dialog.moveStep(stepRow.index, -1)
                        }
                        OsIconButton {
                            iconName: "chevron-down"
                            toolTip: qsTr("Move down")
                            enabled: stepRow.index < (dialog.draft.steps ?? []).length - 1
                            onClicked: dialog.moveStep(stepRow.index, 1)
                        }
                        OsIconButton {
                            iconName: "x"
                            toolTip: qsTr("Remove the step")
                            onClicked: dialog.removeStep(stepRow.index)
                        }
                    }
                }

                Flow {
                    width: parent.width
                    spacing: Theme.spacingSm

                    OsButton {
                        text: qsTr("Type text")
                        iconName: "plus"
                        onClicked: dialog.addStep("send")
                    }
                    OsButton {
                        text: qsTr("Wait for text")
                        iconName: "plus"
                        onClicked: dialog.addStep("wait")
                    }
                    OsButton {
                        text: qsTr("Pause")
                        iconName: "plus"
                        onClicked: dialog.addStep("delay")
                    }
                }
            }

            OsText {
                width: parent.width
                visible: dialog.problem.length > 0
                text: dialog.problem
                color: Theme.danger
                wrapMode: Text.Wrap
            }
        }
    }
}
