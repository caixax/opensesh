// A user-facing command: the unit of the action registry, the command palette and the
// keyboard shortcuts (PLAN §6.4). Declare it where its handler lives, register it with
// `ActionRegistry.register(this)` and react to `triggered`.
//   actionId: string   stable identifier, e.g. "app.settings" (future keybindings.toml key)
//   text: string       translated title shown in the palette
//   shortcut: string   portable key sequence, e.g. "Ctrl+Shift+P" (empty for none)
//   category: string   translated group name shown in the palette
//   iconName: string   optional icon
//   showInPalette: bool
import QtQuick

QtObject {
    id: action

    property string actionId
    property string text
    property string shortcut
    property string category
    property string iconName
    property bool enabled: true
    property bool showInPalette: true

    signal triggered

    function trigger() {
        if (enabled)
            triggered();
    }
}
