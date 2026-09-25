// Numeric input with up/down buttons (see docs/design/components.md). Uses the Qt SpinBox API
// (`from`, `to`, `value`, `stepSize`, `editable`, `textFromValue`, `valueFromText`,
// `valueModified()`). Editable by default; the Up and Down keys step the value.
//   error: bool  outlines the box in Theme.danger
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.SpinBox {
    id: control

    property bool error: false

    readonly property real buttonWidth: Theme.controlHeightSmall
    readonly property bool canIncrease: wrap || value < Math.max(from, to)
    readonly property bool canDecrease: wrap || value > Math.min(from, to)
    readonly property color outlineColor: !enabled ? Theme.border
                                                   : error ? Theme.danger
                                                   : activeFocus ? Theme.accent : Theme.borderStrong

    // The width of the buttons is part of the padding.
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentItem.implicitWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    leftPadding: control.mirrored ? buttonWidth : Theme.controlPadding
    rightPadding: control.mirrored ? Theme.controlPadding : buttonWidth
    topPadding: 0
    bottomPadding: 0
    editable: true
    focusPolicy: Qt.StrongFocus
    hoverEnabled: true

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize

    validator: IntValidator {
        locale: control.locale.name
        bottom: Math.min(control.from, control.to)
        top: Math.max(control.from, control.to)
    }

    contentItem: TextInput {
        text: control.displayText
        clip: width < implicitWidth
        rightPadding: Theme.spacingSm
        font: control.font
        color: control.enabled ? Theme.text : Theme.textDisabled
        selectionColor: Theme.selection
        selectedTextColor: Theme.text
        horizontalAlignment: Qt.AlignLeft
        verticalAlignment: Qt.AlignVCenter
        readOnly: !control.editable
        validator: control.validator
        inputMethodHints: control.inputMethodHints
        selectByMouse: true
        Accessible.ignored: true
    }

    up.indicator: Item {
        x: control.mirrored ? 0 : control.width - width
        y: 0
        implicitWidth: control.buttonWidth
        implicitHeight: Theme.controlHeight / 2
        height: control.height / 2

        Rectangle {
            anchors.fill: parent
            anchors.margins: Theme.borderWidth
            anchors.bottomMargin: 0
            topRightRadius: control.mirrored ? 0 : Theme.radiusControl - Theme.borderWidth
            topLeftRadius: control.mirrored ? Theme.radiusControl - Theme.borderWidth : 0
            visible: control.enabled && control.canIncrease
            color: control.up.pressed ? Theme.pressed : control.up.hovered ? Theme.hover : "transparent"
        }

        OsIcon {
            anchors.centerIn: parent
            name: "chevron-up"
            size: Theme.iconSizeSmall - Theme.spacingXs / 2
            color: control.enabled && control.canIncrease
                   ? Theme.textMuted : Theme.textDisabled
        }
    }

    down.indicator: Item {
        x: control.mirrored ? 0 : control.width - width
        y: control.height / 2
        implicitWidth: control.buttonWidth
        implicitHeight: Theme.controlHeight / 2
        height: control.height / 2

        Rectangle {
            anchors.fill: parent
            anchors.margins: Theme.borderWidth
            anchors.topMargin: 0
            bottomRightRadius: control.mirrored ? 0 : Theme.radiusControl - Theme.borderWidth
            bottomLeftRadius: control.mirrored ? Theme.radiusControl - Theme.borderWidth : 0
            visible: control.enabled && control.canDecrease
            color: control.down.pressed ? Theme.pressed : control.down.hovered ? Theme.hover : "transparent"
        }

        OsIcon {
            anchors.centerIn: parent
            name: "chevron-down"
            size: Theme.iconSizeSmall - Theme.spacingXs / 2
            color: control.enabled && control.canDecrease
                   ? Theme.textMuted : Theme.textDisabled
        }
    }

    background: Rectangle {
        implicitWidth: Theme.controlHeight * 3.5
        implicitHeight: Theme.controlHeight
        radius: Theme.radiusControl
        color: Theme.surface2
        border.width: control.enabled && (control.activeFocus || control.error)
                      ? Theme.focusRingWidth : Theme.borderWidth
        border.color: control.outlineColor

        Behavior on border.color {
            ColorAnimation {
                duration: Theme.durationFast
            }
        }

        // Separator between the text and the buttons.
        Rectangle {
            x: control.mirrored ? control.buttonWidth : parent.width - control.buttonWidth
            y: Theme.spacingXs
            width: Theme.borderWidth
            height: parent.height - 2 * Theme.spacingXs
            color: Theme.border
        }

        OsFocusRing {
            target: control
            baseRadius: Theme.radiusControl
        }
    }
}
