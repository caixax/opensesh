pragma ComponentBehavior: Bound

// Permissions of files (Sprint 8): read, write and execute for the owner, the group and others,
// setuid, setgid and sticky, and the same as an octal number; the grid and the number follow each
// other. Applies to every selected file.
// Functions: show(paths, mode) (the first file's mode).
// Signals: apply(paths, mode).
import QtQuick
import QtQuick.Layouts
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property var paths: []
    property int mode: 0o644

    signal apply(var paths, int mode)

    function show(list, current) {
        paths = list;
        mode = current & 0o7777;
        octal.text = mode.toString(8).padStart(4, "0");
        open();
    }

    function toggle(bit, on) {
        mode = on ? (mode | bit) : (mode & ~bit);
        octal.text = mode.toString(8).padStart(4, "0");
    }

    title: paths.length > 1 ? qsTr("Permissions of %n files", "", paths.length) : qsTr("Permissions")
    acceptText: qsTr("Apply")
    acceptEnabled: /^[0-7]{3,4}$/.test(octal.text)

    onAccepted: dialog.apply(dialog.paths, dialog.mode)

    Column {
        width: Math.min(Theme.spacingXxl * 12, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsText {
            width: parent.width
            visible: dialog.paths.length === 1
            text: dialog.paths.length === 1 ? dialog.paths[0] : ""
            elide: Text.ElideMiddle
            muted: true
        }

        GridLayout {
            width: parent.width
            columns: 4
            columnSpacing: Theme.spacingLg
            rowSpacing: Theme.spacingSm

            Item {
                Layout.preferredWidth: Theme.spacingXxl * 2
                implicitHeight: 1
            }
            OsText {
                text: qsTr("Read")
                muted: true
            }
            OsText {
                text: qsTr("Write")
                muted: true
            }
            OsText {
                text: qsTr("Execute")
                muted: true
            }

            Repeater {
                model: [
                    { label: qsTr("Owner"), shift: 6 },
                    { label: qsTr("Group"), shift: 3 },
                    { label: qsTr("Others"), shift: 0 }
                ]

                delegate: Item {
                    id: group

                    required property var modelData

                    Layout.columnSpan: 4
                    Layout.fillWidth: true
                    implicitHeight: bits.implicitHeight

                    RowLayout {
                        id: bits

                        anchors.left: parent.left
                        anchors.right: parent.right
                        spacing: Theme.spacingLg

                        OsText {
                            Layout.preferredWidth: Theme.spacingXxl * 2
                            text: group.modelData.label
                        }

                        Repeater {
                            model: [4, 2, 1]

                            delegate: OsCheckBox {
                                id: bit

                                required property int modelData
                                readonly property int mask: modelData << group.modelData.shift

                                checked: (dialog.mode & mask) !== 0
                                Accessible.name: qsTr("%1: %2").arg(group.modelData.label)
                                                                .arg(modelData === 4 ? qsTr("read") : modelData === 2 ? qsTr("write") : qsTr("execute"))
                                onToggled: dialog.toggle(mask, checked)
                            }
                        }
                    }
                }
            }
        }

        Flow {
            width: parent.width
            spacing: Theme.spacingLg

            OsCheckBox {
                text: qsTr("Set user ID")
                checked: (dialog.mode & 0o4000) !== 0
                onToggled: dialog.toggle(0o4000, checked)
            }
            OsCheckBox {
                text: qsTr("Set group ID")
                checked: (dialog.mode & 0o2000) !== 0
                onToggled: dialog.toggle(0o2000, checked)
            }
            OsCheckBox {
                text: qsTr("Sticky")
                checked: (dialog.mode & 0o1000) !== 0
                onToggled: dialog.toggle(0o1000, checked)
            }
        }

        OsFormRow {
            width: parent.width
            label: qsTr("Octal")

            OsTextField {
                id: octal

                width: Theme.spacingXxl * 4
                error: !/^[0-7]{3,4}$/.test(text)
                inputMethodHints: Qt.ImhDigitsOnly
                Accessible.name: qsTr("Permissions as an octal number")
                onTextEdited: {
                    if (/^[0-7]{3,4}$/.test(text))
                        dialog.mode = parseInt(text, 8);
                }
            }
        }
    }
}
