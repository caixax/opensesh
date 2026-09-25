// Search/filter field (see docs/design/components.md): an OsTextField with a leading search
// icon and a clear button while it has text. Escape clears the text (and is passed on when the
// field is already empty, so an enclosing popup or dialog can still close).
//   signal cleared()  emitted when the text is cleared with the button or Escape
import QtQuick
import cc.caixa.opensesh

OsTextField {
    id: control

    signal cleared

    function clearText() {
        if (text.length === 0)
            return;
        clear();
        cleared();
    }

    leftPadding: Theme.controlPadding + Theme.iconSizeSmall + Theme.spacingSm
    rightPadding: clearButton.visible ? clearButton.width + Theme.spacingXs * 2 : Theme.controlPadding
    placeholderText: qsTr("Search")
    inputMethodHints: Qt.ImhNoPredictiveText

    Accessible.role: Accessible.EditableText
    Accessible.searchEdit: true
    Accessible.description: qsTr("Type to filter. Press Escape to clear.")

    // Take Escape before any application shortcut when there is something to clear.
    Keys.onShortcutOverride: event => {
        if (event.key === Qt.Key_Escape && text.length > 0)
            event.accepted = true;
    }
    Keys.onEscapePressed: event => {
        if (text.length > 0) {
            clearText();
            event.accepted = true;
        } else {
            event.accepted = false;
        }
    }

    OsIcon {
        x: Theme.controlPadding
        anchors.verticalCenter: parent.verticalCenter
        name: "search"
        size: Theme.iconSizeSmall
        color: control.enabled ? Theme.textMuted : Theme.textDisabled
    }

    OsIconButton {
        id: clearButton

        x: control.width - width - Theme.spacingXs
        anchors.verticalCenter: parent.verticalCenter
        width: Theme.controlHeightSmall
        height: Theme.controlHeightSmall
        visible: control.length > 0 && control.enabled && !control.readOnly
        iconName: "x"
        iconSize: Theme.iconSizeSmall
        toolTip: qsTr("Clear")
        // Escape clears from the keyboard, so the button stays out of the tab order.
        focusPolicy: Qt.NoFocus
        onClicked: {
            control.clearText();
            control.forceActiveFocus(Qt.MouseFocusReason);
        }
    }
}
