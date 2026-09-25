// Password input (see docs/design/components.md): an OsTextField that masks its text, with a
// trailing eye button that reveals it.
//   revealed: bool  whether the text is shown in the clear (the eye button toggles it)
import QtQuick
import cc.caixa.opensesh

OsTextField {
    id: control

    property bool revealed: false

    echoMode: revealed ? TextInput.Normal : TextInput.Password
    rightPadding: revealButton.visible ? revealButton.width + Theme.spacingXs * 2 : Theme.controlPadding
    inputMethodHints: Qt.ImhHiddenText | Qt.ImhSensitiveData | Qt.ImhNoPredictiveText
                      | Qt.ImhNoAutoUppercase
    placeholderText: qsTr("Password")

    Accessible.role: Accessible.EditableText
    Accessible.passwordEdit: !revealed

    OsIconButton {
        id: revealButton

        x: control.width - width - Theme.spacingXs
        anchors.verticalCenter: parent.verticalCenter
        width: Theme.controlHeightSmall
        height: Theme.controlHeightSmall
        visible: control.enabled
        iconName: control.revealed ? "eye-off" : "eye"
        iconSize: Theme.iconSizeSmall
        toolTip: control.revealed ? qsTr("Hide password") : qsTr("Show password")
        // Reachable with Tab right after the field, so keyboard users can reveal the text too.
        focusPolicy: Qt.TabFocus
        Accessible.description: qsTr("Toggles whether the password is shown in plain text.")
        onClicked: control.revealed = !control.revealed
    }
}
