// Single-line text input (see docs/design/components.md).
//   error: bool  outlines the field in Theme.danger; pair it with an error message nearby
// The accessible name defaults to the placeholder; set `Accessible.name` when a label exists.
// OsSearchField and OsPasswordField build on this: they widen the paddings and add children.
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.TextField {
    id: control

    property bool error: false

    readonly property color outlineColor: !enabled ? Theme.border
                                                   : error ? Theme.danger
                                                   : activeFocus ? Theme.accent : Theme.borderStrong

    implicitWidth: implicitBackgroundWidth + leftInset + rightInset
                   || Math.max(contentWidth, placeholder.implicitWidth) + leftPadding + rightPadding
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding,
                             placeholder.implicitHeight + topPadding + bottomPadding)

    leftPadding: Theme.controlPadding
    rightPadding: Theme.controlPadding
    topPadding: 0
    bottomPadding: 0

    color: enabled ? Theme.text : Theme.textDisabled
    selectionColor: Theme.selection
    selectedTextColor: Theme.text
    placeholderTextColor: enabled ? Theme.textMuted : Theme.textDisabled
    verticalAlignment: TextInput.AlignVCenter
    selectByMouse: true
    hoverEnabled: true

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize

    Accessible.name: placeholderText

    OsText {
        id: placeholder

        x: control.leftPadding
        y: control.topPadding
        width: control.width - (control.leftPadding + control.rightPadding)
        height: control.height - (control.topPadding + control.bottomPadding)
        text: control.placeholderText
        font: control.font
        color: control.placeholderTextColor
        verticalAlignment: control.verticalAlignment
        horizontalAlignment: control.effectiveHorizontalAlignment
        visible: !control.length && !control.preeditText
                 && (!control.activeFocus || control.horizontalAlignment !== Qt.AlignHCenter)
        renderType: control.renderType
        Accessible.ignored: true
    }

    background: Rectangle {
        implicitWidth: Theme.controlHeight * 6
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

        // Hover feedback (not while typing: the accent outline already marks the field).
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            visible: control.enabled && control.hovered && !control.activeFocus
            color: Theme.hover
        }

        OsFocusRing {
            target: control
            baseRadius: Theme.radiusControl
        }
    }
}
