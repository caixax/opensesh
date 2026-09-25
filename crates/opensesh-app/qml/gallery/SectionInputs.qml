// Gallery section: input controls in every state (default, checked/on, error, disabled) plus
// one control with keyboard focus. Put it in a scrolling column and give it a width; its height
// is implicit.
//   focusDemo: bool  give the "Keyboard focus" text field Tab focus once loaded (default true)
pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

Column {
    id: section

    property bool focusDemo: true

    // A control with a small caption under it.
    component Cell: Column {
        property string caption
        default property alias content: holder.data

        spacing: Theme.spacingXs

        Item {
            id: holder

            implicitWidth: childrenRect.width
            implicitHeight: childrenRect.height
            width: implicitWidth
            height: implicitHeight
        }

        OsText {
            text: parent.caption
            size: "small"
            muted: true
        }
    }

    // A titled group of cells that wraps to the available width.
    component Group: Column {
        property string title
        default property alias content: flow.data

        width: parent ? parent.width : implicitWidth
        spacing: Theme.spacingSm

        OsText {
            text: parent.title
            font.weight: Font.DemiBold
        }

        Flow {
            id: flow

            width: parent.width
            spacing: Theme.spacingLg
        }
    }

    spacing: Theme.spacingXl

    Component.onCompleted: {
        if (focusDemo)
            Qt.callLater(() => focusedField.forceActiveFocus(Qt.TabFocusReason));
    }

    OsText {
        text: qsTr("Inputs")
        size: "title"
    }

    Group {
        title: qsTr("Icon button")

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
        title: qsTr("Switch and check box")

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
        title: qsTr("Color picker and shortcut")

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
