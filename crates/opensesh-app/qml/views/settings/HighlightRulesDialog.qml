// Editor of one keyword highlighting rule set (PLAN §6.5): its name and its rules, each a regular
// expression with a style (text and background color from the theme's ANSI colors, bold,
// underline). Built-in sets are shown read-only, with a button that copies them into an editable
// set. Patterns are checked as they are typed; Save stays disabled while one is invalid.
// Functions: openSet(id) (an empty id starts a new set).
pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property string setId
    property bool builtin: false
    property int invalidCount: 0

    readonly property var colorNames: [""].concat(TerminalProfiles.choices("highlight_color"))
    readonly property var colorLabels: ({
            "": qsTr("Unchanged"),
            black: qsTr("Black"),
            red: qsTr("Red"),
            green: qsTr("Green"),
            yellow: qsTr("Yellow"),
            blue: qsTr("Blue"),
            magenta: qsTr("Magenta"),
            cyan: qsTr("Cyan"),
            white: qsTr("White"),
            "bright-black": qsTr("Bright black"),
            "bright-red": qsTr("Bright red"),
            "bright-green": qsTr("Bright green"),
            "bright-yellow": qsTr("Bright yellow"),
            "bright-blue": qsTr("Bright blue"),
            "bright-magenta": qsTr("Bright magenta"),
            "bright-cyan": qsTr("Bright cyan"),
            "bright-white": qsTr("Bright white")
        })

    function colorLabel(name) {
        return colorLabels[name] !== undefined ? colorLabels[name] : name;
    }

    function openSet(id) {
        const sets = JSON.parse(TerminalProfiles.highlightSets || "[]");
        const found = sets.find(set => set.id === id);
        rules.clear();
        if (found) {
            setId = found.id;
            builtin = found.builtin;
            nameField.text = found.name;
            for (const rule of found.rules)
                rules.append({
                    pattern: rule.pattern,
                    ignoreCase: rule.ignoreCase,
                    fg: rule.foreground,
                    bg: rule.background,
                    bold: rule.bold,
                    underline: rule.underline
                });
        } else {
            setId = "";
            builtin = false;
            nameField.text = qsTr("My rules");
            rules.append({
                pattern: "",
                ignoreCase: false,
                fg: "red",
                bg: "",
                bold: false,
                underline: false
            });
        }
        open();
    }

    function rulesArray() {
        const out = [];
        for (let i = 0; i < rules.count; ++i) {
            const rule = rules.get(i);
            out.push({
                pattern: rule.pattern,
                ignoreCase: rule.ignoreCase,
                foreground: rule.fg,
                background: rule.bg,
                bold: rule.bold,
                underline: rule.underline
            });
        }
        return out;
    }

    function recount() {
        let invalid = 0;
        for (let i = 0; i < rules.count; ++i) {
            const rule = rules.get(i);
            if (TerminalProfiles.checkPattern(rule.pattern, rule.ignoreCase).length > 0)
                ++invalid;
        }
        invalidCount = invalid;
    }

    title: builtin ? qsTr("Built-in rule set") : setId.length > 0 ? qsTr("Edit rule set") : qsTr("New rule set")
    acceptText: builtin ? "" : qsTr("Save")
    rejectText: builtin ? qsTr("Close") : qsTr("Cancel")
    acceptEnabled: !builtin && invalidCount === 0 && rules.count > 0 && nameField.text.trim().length > 0

    onAccepted: {
        const reply = JSON.parse(TerminalProfiles.saveHighlightSet(setId, nameField.text, JSON.stringify(rulesArray())));
        if (reply.error.length > 0)
            Toasts.show(qsTr("Could not save the rules: %1").arg(reply.error), "warning");
    }

    ListModel {
        id: rules

        onCountChanged: dialog.recount()
    }

    Column {
        width: Math.min(Theme.spacingXxl * 22, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsText {
            width: parent.width
            visible: dialog.builtin
            text: qsTr("Built-in sets can't be changed. Make a copy to adjust it.")
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignLeft
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Name")
            labelWidth: Theme.spacingXxl * 3

            OsTextField {
                id: nameField

                width: parent.width
                readOnly: dialog.builtin
                Accessible.name: qsTr("Rule set name")
            }
        }

        OsText {
            width: parent.width
            text: qsTr("Each rule is a regular expression. With a group in parentheses, only the group is styled. Later rules win where rules overlap.")
            size: "small"
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignLeft
        }

        Flickable {
            id: flick

            width: parent.width
            height: Math.min(ruleColumn.implicitHeight, Math.max(Theme.spacingXxl * 6, dialog.maxHeight - Theme.spacingXxl * 9))
            contentHeight: ruleColumn.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            T.ScrollBar.vertical: OsScrollBar {}

            Column {
                id: ruleColumn

                width: flick.width - Theme.spacingMd
                spacing: Theme.spacingMd

                Repeater {
                    model: rules

                    delegate: OsCard {
                        id: ruleCard

                        required property int index
                        required property string pattern
                        required property bool ignoreCase
                        required property string fg
                        required property string bg
                        required property bool bold
                        required property bool underline

                        readonly property string error: TerminalProfiles.checkPattern(pattern, ignoreCase)

                        width: ruleColumn.width
                        padding: Theme.spacingMd

                        Column {
                            width: parent.width
                            spacing: Theme.spacingSm

                            Row {
                                width: parent.width
                                spacing: Theme.spacingSm

                                OsTextField {
                                    width: parent.width - removeRule.width - parent.spacing
                                    text: ruleCard.pattern
                                    readOnly: dialog.builtin
                                    error: ruleCard.error.length > 0
                                    font.family: Theme.monoFontFamily
                                    placeholderText: qsTr("Regular expression, e.g. \\bERROR\\b")
                                    Accessible.name: qsTr("Pattern of rule %1").arg(ruleCard.index + 1)
                                    onTextEdited: {
                                        rules.setProperty(ruleCard.index, "pattern", text);
                                        dialog.recount();
                                    }
                                }

                                OsIconButton {
                                    id: removeRule

                                    anchors.verticalCenter: parent.verticalCenter
                                    visible: !dialog.builtin
                                    iconName: "trash-2"
                                    toolTip: qsTr("Remove this rule")
                                    onClicked: rules.remove(ruleCard.index)
                                }
                            }

                            OsText {
                                width: parent.width
                                visible: ruleCard.error.length > 0 && ruleCard.pattern.length > 0
                                text: qsTr("Invalid pattern: %1").arg(ruleCard.error)
                                size: "small"
                                color: Theme.danger
                                wrapMode: Text.Wrap
                                elide: Text.ElideNone
                                horizontalAlignment: Text.AlignLeft
                            }

                            Flow {
                                width: parent.width
                                spacing: Theme.spacingSm

                                OsComboBox {
                                    width: Theme.spacingXxl * 5
                                    enabled: !dialog.builtin
                                    textRole: "text"
                                    valueRole: "value"
                                    model: dialog.colorNames.map(name => ({
                                                value: name,
                                                text: qsTr("Text: %1").arg(dialog.colorLabel(name))
                                            }))
                                    currentIndex: Math.max(0, dialog.colorNames.indexOf(ruleCard.fg))
                                    Accessible.name: qsTr("Text color of rule %1").arg(ruleCard.index + 1)
                                    onActivated: index => rules.setProperty(ruleCard.index, "fg", dialog.colorNames[index])
                                }

                                OsComboBox {
                                    width: Theme.spacingXxl * 5
                                    enabled: !dialog.builtin
                                    textRole: "text"
                                    valueRole: "value"
                                    model: dialog.colorNames.map(name => ({
                                                value: name,
                                                text: qsTr("Background: %1").arg(dialog.colorLabel(name))
                                            }))
                                    currentIndex: Math.max(0, dialog.colorNames.indexOf(ruleCard.bg))
                                    Accessible.name: qsTr("Background color of rule %1").arg(ruleCard.index + 1)
                                    onActivated: index => rules.setProperty(ruleCard.index, "bg", dialog.colorNames[index])
                                }

                                OsCheckBox {
                                    text: qsTr("Bold")
                                    enabled: !dialog.builtin
                                    checked: ruleCard.bold
                                    onToggled: rules.setProperty(ruleCard.index, "bold", checked)
                                }

                                OsCheckBox {
                                    text: qsTr("Underline")
                                    enabled: !dialog.builtin
                                    checked: ruleCard.underline
                                    onToggled: rules.setProperty(ruleCard.index, "underline", checked)
                                }

                                OsCheckBox {
                                    text: qsTr("Ignore case")
                                    enabled: !dialog.builtin
                                    checked: ruleCard.ignoreCase
                                    onToggled: {
                                        rules.setProperty(ruleCard.index, "ignoreCase", checked);
                                        dialog.recount();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Row {
            spacing: Theme.spacingSm

            OsButton {
                visible: !dialog.builtin
                text: qsTr("Add rule")
                iconName: "plus"
                onClicked: rules.append({
                    pattern: "",
                    ignoreCase: false,
                    fg: "red",
                    bg: "",
                    bold: false,
                    underline: false
                })
            }

            OsButton {
                text: qsTr("Make a copy")
                iconName: "copy"
                visible: dialog.setId.length > 0
                onClicked: {
                    const id = TerminalProfiles.duplicateHighlightSet(dialog.setId);
                    if (id.length > 0)
                        Qt.callLater(() => dialog.openSet(id));
                }
            }

            OsButton {
                text: qsTr("Delete set")
                iconName: "trash-2"
                variant: "danger"
                visible: !dialog.builtin && dialog.setId.length > 0
                onClicked: {
                    TerminalProfiles.deleteHighlightSet(dialog.setId);
                    dialog.close();
                }
            }
        }
    }
}
