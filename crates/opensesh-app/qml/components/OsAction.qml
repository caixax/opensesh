// A user-facing command: the unit of the action registry, the command palette and the
// keyboard shortcuts (PLAN §6.4). Declare it where its handler lives, register it with
// `ActionRegistry.register(this)` and react to `triggered`.
//   actionId: string          stable identifier, e.g. "app.settings" (the keybindings.toml key)
//   text: string              translated title shown in the palette
//   defaultShortcut: string   portable key sequence, e.g. "Ctrl+Shift+P" (empty for none)
//   shortcut: string          read-only; the user's shortcut (Keybindings) or the default
//   category: string   translated group name shown in the palette
//   iconName: string   optional icon
//   showInPalette: bool
import QtQuick
import cc.caixa.opensesh

QtObject {
    id: action

    property string actionId
    property string text
    property string defaultShortcut
    readonly property string shortcut: Keybindings.revision >= 0 && actionId.length > 0
                                       ? Keybindings.shortcut(actionId, defaultShortcut) : defaultShortcut
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
