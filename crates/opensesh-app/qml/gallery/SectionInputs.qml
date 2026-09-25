// Gallery section: input controls in every state (default, checked/on, error, disabled). Put it
// in a scrolling column and give it a width; its height is implicit.
// Functions: showFocus()  gives the "Keyboard focus" text field Tab focus (screenshots).
pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

Column {
    id: section

    function showFocus() {
        focusedField.forceActiveFocus(Qt.TabFocusReason);
    }

    // A control with a small caption under it. A control lower than Theme.controlHeight is
    // centered in a slot of that height, so the captions of a row line up.
    component Cell: Column {
        property string caption
        default property alias content: holder.data

        spacing: Theme.spacingXs

        Item {
            id: holder

            implicitWidth: {
                let widest = 0;
                for (let i = 0; i < children.length; ++i)
                    widest = Math.max(widest, children[i].width);
                return widest;
            }
            implicitHeight: {
                let tallest = Theme.controlHeight;
                for (let i = 0; i < children.length; ++i)
                    tallest = Math.max(tallest, children[i].height);
                return tallest;
            }
            width: implicitWidth
            height: implicitHeight

            Component.onCompleted: {
                for (let i = 0; i < children.length; ++i) {
                    const child = children[i];
                    child.y = Qt.binding(() => Math.round((holder.height - child.height) / 2));
                }
            }
        }

        OsText {
            text: parent.caption
            size: "small"
            muted: true
        }
    }

    // A titled group of cells that wraps to the available width.
    component Group: Column {
        id: group

        property string title
        property string description
        default property alias content: flow.data

        width: parent ? parent.width : implicitWidth
        spacing: Theme.spacingMd

        OsSectionHeader {
            width: parent.width
            title: group.title
            description: group.description
        }

        Flow {
            id: flow

            width: parent.width
            spacing: Theme.spacingLg
        }
    }

    spacing: Theme.spacingXl

    Column {
        width: parent.width
        spacing: Theme.spacingSm

        OsText {
            text: qsTr("Inputs")
            size: "title"
            Accessible.role: Accessible.Heading
        }

        OsText {
            width: parent.width
            text: qsTr("Buttons, fields, pickers and toggles in their states: default, on or checked, error, disabled and keyboard focus.")
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }
    }

    Group {
        title: qsTr("Button")
        description: qsTr("OsButton variants, with and without an icon, and disabled.")

        Cell {
            caption: qsTr("Primary")

            OsButton {
                text: qsTr("Connect")
                variant: "primary"
            }
        }
        Cell {
            caption: qsTr("Primary, icon")

            OsButton {
                text: qsTr("New host")
                variant: "primary"
                iconName: "plus"
            }
        }
        Cell {
            caption: qsTr("Secondary")

            OsButton {
                text: qsTr("Import")
                iconName: "import"
            }
        }
        Cell {
            caption: qsTr("Ghost")

            OsButton {
                text: qsTr("Cancel")
                variant: "ghost"
            }
        }
        Cell {
            caption: qsTr("Danger")

            OsButton {
                text: qsTr("Delete")
                variant: "danger"
                iconName: "trash-2"
            }
        }
        Cell {
            caption: qsTr("Primary, disabled")

            OsButton {
                text: qsTr("Connect")
                variant: "primary"
                enabled: false
            }
        }
        Cell {
            caption: qsTr("Secondary, disabled")

            OsButton {
                text: qsTr("Import")
                iconName: "import"
                enabled: false
            }
        }
        Cell {
            caption: qsTr("Danger, disabled")

            OsButton {
                text: qsTr("Delete")
                variant: "danger"
                iconName: "trash-2"
                enabled: false
            }
        }
    }

    Group {
        title: qsTr("Icon button")
        description: qsTr("Square, icon only; the tooltip is also the accessible name.")

        Cell {
            caption: qsTr("Ghost")

            OsIconButton {
                iconName: "settings"
                toolTip: qsTr("Settings")
            }
        }
        Cell {
            caption: qsTr("Secondary")

            OsIconButton {
                iconName: "plus"
                toolTip: qsTr("Add")
                variant: "secondary"
            }
        }
        Cell {
            caption: qsTr("Checkable, on")

            OsIconButton {
                iconName: "panel-left"
                toolTip: qsTr("Show sidebar")
                checkable: true
                checked: true
            }
        }
        Cell {
            caption: qsTr("Checkable, off")

            OsIconButton {
                iconName: "panel-left"
                toolTip: qsTr("Show sidebar")
                checkable: true
            }
        }
        Cell {
            caption: qsTr("Disabled")

            OsIconButton {
                iconName: "trash-2"
                toolTip: qsTr("Delete")
                enabled: false
            }
        }
    }

    Group {
        title: qsTr("Text field")
        description: qsTr("Placeholder, text, keyboard focus, error and disabled.")

        Cell {
            caption: qsTr("Placeholder")

            OsTextField {
                placeholderText: qsTr("Host name")
            }
        }
        Cell {
            caption: qsTr("With text")

            OsTextField {
                text: qsTr("sesame.example.org")
            }
        }
        Cell {
            caption: qsTr("Keyboard focus")

            OsTextField {
                id: focusedField

                placeholderText: qsTr("User name")
            }
        }
        Cell {
            caption: qsTr("Error")

            OsTextField {
                text: qsTr("host name:22")
                error: true
            }
        }
        Cell {
            caption: qsTr("Disabled")

            OsTextField {
                text: qsTr("Read from the profile")
                enabled: false
            }
        }
    }

    Group {
        title: qsTr("Search and password fields")
        description: qsTr("Escape clears the search; the eye button reveals the password.")

        Cell {
            caption: qsTr("Search, empty")

            OsSearchField {}
        }
        Cell {
            caption: qsTr("Search, with text")

            OsSearchField {
                text: qsTr("prod")
            }
        }
        Cell {
            caption: qsTr("Password, hidden")

            OsPasswordField {
                text: qsTr("open sesame")
            }
        }
        Cell {
            caption: qsTr("Password, revealed")

            OsPasswordField {
                text: qsTr("open sesame")
                revealed: true
            }
        }
        Cell {
            caption: qsTr("Disabled")

            OsPasswordField {
                text: qsTr("open sesame")
                enabled: false
            }
        }
    }

    Group {
        title: qsTr("Combo box and font picker")
        description: qsTr("Themed popups; the font pickers draw every family in itself.")

        Cell {
            caption: qsTr("Strings")

            OsComboBox {
                model: [qsTr("Comfortable"), qsTr("Compact")]
            }
        }
        Cell {
            caption: qsTr("Objects (textRole, valueRole)")

            OsComboBox {
                textRole: "text"
                valueRole: "value"
                model: [
                    {
                        value: "system",
                        text: qsTr("Follow the system")
                    },
                    {
                        value: "dark",
                        text: qsTr("Dark")
                    },
                    {
                        value: "light",
                        text: qsTr("Light")
                    }
                ]
            }
        }
        Cell {
            caption: qsTr("Disabled")

            OsComboBox {
                model: [qsTr("Unavailable")]
                enabled: false
            }
        }
        Cell {
            caption: qsTr("Font")

            OsFontPicker {
                currentFamily: Theme.fontFamily
            }
        }
        Cell {
            caption: qsTr("Monospace font")

            OsFontPicker {
                monospaceOnly: true
                currentFamily: Theme.monoFontFamily
            }
        }
    }

    Group {
        title: qsTr("Switch")
        description: qsTr("On/off settings that apply at once.")

        Cell {
            caption: qsTr("Off")

            OsSwitch {
                text: qsTr("Reconnect")
            }
        }
        Cell {
            caption: qsTr("On")

            OsSwitch {
                text: qsTr("Reconnect")
                checked: true
            }
        }
        Cell {
            caption: qsTr("Disabled, off")

            OsSwitch {
                text: qsTr("Reconnect")
                enabled: false
            }
        }
        Cell {
            caption: qsTr("Disabled, on")

            OsSwitch {
                text: qsTr("Reconnect")
                checked: true
                enabled: false
            }
        }
    }

    Group {
        title: qsTr("Check box")
        description: qsTr("Includes the partially checked state of a \"select all\" box.")

        Cell {
            caption: qsTr("Unchecked")

            OsCheckBox {
                text: qsTr("Agent forwarding")
            }
        }
        Cell {
            caption: qsTr("Checked")

            OsCheckBox {
                text: qsTr("Agent forwarding")
                checked: true
            }
        }
        Cell {
            caption: qsTr("Partially checked")

            OsCheckBox {
                text: qsTr("All hosts")
                tristate: true
                checkState: Qt.PartiallyChecked
            }
        }
        Cell {
            caption: qsTr("Disabled")

            OsCheckBox {
                text: qsTr("Agent forwarding")
                enabled: false
            }
        }
        Cell {
            caption: qsTr("Disabled, checked")

            OsCheckBox {
                text: qsTr("Agent forwarding")
                checked: true
                enabled: false
            }
        }
    }

    Group {
        title: qsTr("Slider and spin box")
        description: qsTr("The arrow keys step the value; the spin box also takes typed numbers.")

        Cell {
            caption: qsTr("Slider")

            OsSlider {
                from: 0
                to: 100
                value: 40
                Accessible.name: qsTr("Volume")
            }
        }
        Cell {
            caption: qsTr("Slider, disabled")

            OsSlider {
                from: 0
                to: 100
                value: 70
                enabled: false
                Accessible.name: qsTr("Volume")
            }
        }
        Cell {
            caption: qsTr("Spin box")

            OsSpinBox {
                from: 1
                to: 65535
                value: 22
                Accessible.name: qsTr("Port")
            }
        }
        Cell {
            caption: qsTr("Spin box, error")

            OsSpinBox {
                from: 1
                to: 65535
                value: 1
                error: true
                Accessible.name: qsTr("Port")
            }
        }
        Cell {
            caption: qsTr("Spin box, disabled")

            OsSpinBox {
                from: 1
                to: 65535
                value: 22
                enabled: false
                Accessible.name: qsTr("Port")
            }
        }
    }

    Group {
        title: qsTr("Color picker")
        description: qsTr("Preset swatches and a hex field.")

        Cell {
            caption: qsTr("Color picker")

            OsColorPicker {}
        }
        Cell {
            caption: qsTr("Color picker, disabled")

            OsColorPicker {
                enabled: false
            }
        }
    }

    Group {
        title: qsTr("Shortcut capture")
        description: qsTr("Click, then press a key combination. Escape cancels, Backspace clears.")

        Cell {
            caption: qsTr("Shortcut")

            OsKeybindCapture {
                sequence: Platform.keySequenceText(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier)
                Accessible.name: qsTr("Command palette shortcut")
            }
        }
        Cell {
            caption: qsTr("Shortcut, not set")

            OsKeybindCapture {}
        }
        Cell {
            caption: qsTr("Shortcut, disabled")

            OsKeybindCapture {
                sequence: Platform.keySequenceText(Qt.Key_T, Qt.ControlModifier)
                enabled: false
            }
        }
    }

    Group {
        title: qsTr("Scroll bar")
        description: qsTr("Thin, themed bars for any Flickable.")

        Cell {
            caption: qsTr("Always on")

            Rectangle {
                width: Theme.controlHeight * 6
                height: Theme.controlHeight * 3
                color: Theme.surface
                border.width: Theme.borderWidth
                border.color: Theme.border
                radius: Theme.radiusSmall
                clip: true

                ListView {
                    id: scrollDemo

                    anchors.fill: parent
                    anchors.margins: Theme.borderWidth
                    model: 20
                    boundsBehavior: Flickable.StopAtBounds
                    delegate: OsText {
                        required property int index

                        width: ListView.view.width
                        height: Theme.controlHeightSmall
                        leftPadding: Theme.spacingSm
                        text: qsTr("Row %1").arg(index + 1)
                    }

                    T.ScrollBar.vertical: OsScrollBar {
                        policy: T.ScrollBar.AlwaysOn
                    }
                }
            }
        }
        Cell {
            caption: qsTr("As needed (shows while scrolling)")

            Rectangle {
                width: Theme.controlHeight * 6
                height: Theme.controlHeight * 3
                color: Theme.surface
                border.width: Theme.borderWidth
                border.color: Theme.border
                radius: Theme.radiusSmall
                clip: true

                ListView {
                    anchors.fill: parent
                    anchors.margins: Theme.borderWidth
                    model: 20
                    boundsBehavior: Flickable.StopAtBounds
                    delegate: OsText {
                        required property int index

                        width: ListView.view.width
                        height: Theme.controlHeightSmall
                        leftPadding: Theme.spacingSm
                        text: qsTr("Row %1").arg(index + 1)
                    }

                    T.ScrollBar.vertical: OsScrollBar {}
                }
            }
        }
    }
}
