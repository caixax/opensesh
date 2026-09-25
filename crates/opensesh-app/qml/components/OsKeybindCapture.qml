// Records a keyboard shortcut (see docs/design/components.md). Click it, or press Enter or
// Space while it has focus, then press the key combination. Escape cancels, Backspace or
// Delete clears the shortcut. While recording, application shortcuts are suspended so the
// combination reaches this control.
//   sequence: string          the recorded combination as text ("" = none), built with
//                             Platform.keySequenceText, e.g. "Ctrl+Shift+P"
//   recording: bool           true while waiting for a combination (read-only for users)
//   placeholderText: string   shown when `sequence` is empty (default qsTr("Not set"))
//   signal sequenceEdited(string sequence)  the user recorded or cleared a combination
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.AbstractButton {
    id: control

    property string sequence: ""
    property string placeholderText: qsTr("Not set")
    readonly property alias recording: recorder.recording

    signal sequenceEdited(string sequence)

    function startRecording() {
        if (!enabled)
            return;
        recorder.recording = true;
        if (!activeFocus)
            forceActiveFocus(Qt.OtherFocusReason);
    }

    function stopRecording() {
        recorder.recording = false;
    }

    function commit(newSequence: string) {
        recorder.recording = false;
        if (newSequence === sequence)
            return;
        sequence = newSequence;
        sequenceEdited(newSequence);
    }

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    // A little more than a button: the pill's round ends eat into the padding.
    leftPadding: Theme.controlPadding
    rightPadding: Theme.controlPadding + Theme.spacingXs
    topPadding: 0
    bottomPadding: 0
    spacing: Theme.spacingSm
    focusPolicy: Qt.StrongFocus
    hoverEnabled: true

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize

    text: recording ? qsTr("Press a key combination…")
                    : sequence.length > 0 ? sequence : placeholderText

    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.description: qsTr("Keyboard shortcut. Press Enter or Space to record a new one, then press the key combination. Escape cancels, Backspace clears.")

    onClicked: recording ? stopRecording() : startRecording()
    onActiveFocusChanged: {
        if (!activeFocus)
            stopRecording();
    }
    onEnabledChanged: {
        if (!enabled)
            stopRecording();
    }

    // While recording, claim every key before application shortcuts see it (and Enter, which
    // starts a recording, so a dialog's default button does not take it).
    Keys.onShortcutOverride: event => {
        if (recording || event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
            event.accepted = true;
    }
    Keys.onPressed: event => {
        const plain = (event.modifiers & ~Qt.KeypadModifier) === Qt.NoModifier;
        if (!recording) {
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                startRecording();
                event.accepted = true;
            } else if (plain && (event.key === Qt.Key_Backspace || event.key === Qt.Key_Delete)
                       && sequence.length > 0) {
                commit("");
                event.accepted = true;
            }
            return;
        }
        event.accepted = true;
        if (event.isAutoRepeat || recorder.modifierKeys.indexOf(event.key) >= 0)
            return;
        recorder.swallowRelease = event.key;
        if (plain && event.key === Qt.Key_Escape)
            stopRecording();
        else if (plain && (event.key === Qt.Key_Backspace || event.key === Qt.Key_Delete))
            commit("");
        else
            commit(Platform.keySequenceText(event.key, event.modifiers));
    }
    // Swallow releases while recording, and the release of the key that ended the recording
    // (a recorded Space must not click the button again).
    Keys.onReleased: event => {
        if (recording) {
            event.accepted = true;
        } else if (event.key === recorder.swallowRelease) {
            recorder.swallowRelease = 0;
            event.accepted = true;
        }
    }

    QtObject {
        id: recorder

        property bool recording: false
        // Pressed alone, these wait for the rest of the combination.
        readonly property var modifierKeys: [Qt.Key_Control, Qt.Key_Shift, Qt.Key_Alt, Qt.Key_Meta,
            Qt.Key_AltGr, Qt.Key_Super_L, Qt.Key_Super_R, Qt.Key_Hyper_L, Qt.Key_Hyper_R,
            Qt.Key_CapsLock, Qt.Key_NumLock, Qt.Key_ScrollLock, Qt.Key_unknown]
        // Key whose release must not reach the button (it completed a recording).
        property int swallowRelease: 0
    }

    contentItem: Item {
        implicitWidth: row.implicitWidth
        implicitHeight: row.implicitHeight

        Row {
            id: row

            anchors.verticalCenter: parent.verticalCenter
            spacing: control.spacing

            OsIcon {
                anchors.verticalCenter: parent.verticalCenter
                name: "keyboard"
                size: Theme.iconSizeSmall
                color: !control.enabled ? Theme.textDisabled
                                        : control.recording ? Theme.accentFg : Theme.textMuted
            }

            OsText {
                anchors.verticalCenter: parent.verticalCenter
                text: control.text
                font: control.font
                color: !control.enabled ? Theme.textDisabled
                                        : control.recording ? Theme.accentFg
                                        : control.sequence.length > 0 ? Theme.text : Theme.textMuted
            }
        }
    }

    background: Rectangle {
        implicitWidth: Theme.controlHeight * 4
        implicitHeight: Theme.controlHeight
        radius: height / 2
        color: Theme.surface2
        border.width: control.recording ? Theme.focusRingWidth : Theme.borderWidth
        border.color: !control.enabled ? Theme.border
                                        : control.recording ? Theme.accent : Theme.borderStrong

        Behavior on border.color {
            ColorAnimation {
                duration: Theme.durationFast
            }
        }

        // Hover and press feedback, drawn over the fill.
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            visible: control.enabled && !control.recording
            color: control.down ? Theme.pressed : control.hovered ? Theme.hover : "transparent"

            Behavior on color {
                ColorAnimation {
                    duration: Theme.durationFast
                }
            }
        }

        OsFocusRing {
            target: control
            baseRadius: height / 2
        }
    }
}
