// A terminal color option that follows the theme unless the profile sets its own (cursor and
// selection colors): a switch, and a color picker while a custom color is used.
//   page: Item          the SettingsTerminalPage (`values`, `set()`, `themeColors`)
//   key: string         the option, e.g. "cursor_color"
//   themeColor: string  the theme's own color for it ("#RRGGBB"), the starting custom color
//   label: string       what the color is for (accessible names)
pragma ComponentBehavior: Bound

import QtQuick
import cc.caixa.opensesh

Column {
    id: option

    required property Item page
    required property string key
    property string themeColor
    property string label

    readonly property string value: page.values[key] !== undefined ? page.values[key] : "theme"
    readonly property bool custom: value !== "theme"

    spacing: Theme.spacingSm

    OsSwitch {
        text: qsTr("Use the theme's color")
        checked: !option.custom
        Accessible.name: qsTr("%1: use the theme's color").arg(option.label)
        onToggled: option.page.set(option.key, checked ? "theme" : (option.themeColor.length > 0 ? option.themeColor : String(Theme.accent)))
    }

    OsColorPicker {
        id: picker

        visible: option.custom
        presets: option.page.themeSwatches
        Accessible.name: option.label
        onAccepted: picked => option.page.set(option.key, picker.hexOf(picked))

        Binding on value {
            value: option.custom ? option.value : option.themeColor
            when: option.value.length > 0
            restoreMode: Binding.RestoreNone
        }
    }
}
