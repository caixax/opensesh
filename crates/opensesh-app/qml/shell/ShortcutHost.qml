// Binds one window Shortcut per ActionRegistry action that has a shortcut (PLAN §6.4), so every
// key binding comes from the registry. Put one in each window, inside its content (the window
// the Shortcut belongs to is found through its parents). Modal popups block these shortcuts.
//   nativeTexts: var   read-only; { actionId: text to show for its shortcut }
// Functions: nativeText(actionId) returns that text, or the portable text before the shortcuts
// exist.
// The text shown is the Shortcut's nativeText on macOS (Cmd, Option and Shift symbols) and the
// portable text elsewhere: there nativeText only differs by translating the key names into the
// OS language (e.g. "Control+Mayúsculas+P"), which would clash with an English-only UI.
import QtQuick
import cc.caixa.opensesh

Item {
    id: host

    // Registrations arrive one at a time at startup; bind the batch once per event loop turn
    // instead of rebuilding every Shortcut after each one.
    property var boundActions: []
    property var nativeTexts: ({})
    readonly property bool useNativeText: Qt.platform.os === "osx" || Qt.platform.os === "macos"

    function nativeText(actionId) {
        const text = nativeTexts[actionId];
        if (text !== undefined)
            return text;
        const action = ActionRegistry.find(actionId);
        return action ? action.shortcut : "";
    }

    function sync() {
        boundActions = ActionRegistry.actions.filter(action => action.shortcut.length > 0);
    }

    visible: false

    Component.onCompleted: sync()

    Connections {
        target: ActionRegistry

        function onActionsChanged() {
            Qt.callLater(host.sync);
        }
    }

    Instantiator {
        model: host.boundActions

        delegate: Shortcut {
            required property var modelData

            context: Qt.WindowShortcut
            // `sequence` (not `sequences`): Qt 6.8 only fills nativeText for a single sequence.
            sequence: modelData.shortcut
            enabled: modelData.enabled
            onActivated: ActionRegistry.trigger(modelData.actionId)
        }

        onObjectAdded: (index, object) => {
            const texts = Object.assign({}, host.nativeTexts);
            texts[object.modelData.actionId] = host.useNativeText ? object.nativeText : object.modelData.shortcut;
            host.nativeTexts = texts;
        }
    }
}
