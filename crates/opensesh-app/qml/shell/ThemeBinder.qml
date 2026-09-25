// Feeds the Theme singleton from AppSettings and the OS color scheme. Screenshot and gallery
// modes can override the mode and density without touching the user's settings.
//   overrideActive: bool        use the override properties below instead of the settings
//   overrideMode: string        "dark" | "light" | "system"
//   overrideDensity: string     "comfortable" | "compact"
//   overrideAccent: string      "" keeps the user's accent; "default" or "#RRGGBB" replaces it
//   overrideReduceMotion: var   null keeps the user's setting; true or false replaces it
import QtQuick
import cc.caixa.opensesh

Item {
    id: binder

    property bool overrideActive: false
    property string overrideMode: "dark"
    property string overrideDensity: "comfortable"
    property string overrideAccent: ""
    property var overrideReduceMotion: null

    visible: false

    Binding {
        target: Theme
        property: "systemDark"
        value: Application.styleHints.colorScheme !== Qt.Light
    }
    Binding {
        target: Theme
        property: "requestedMode"
        value: binder.overrideActive ? binder.overrideMode : AppSettings.theme
    }
    Binding {
        target: Theme
        property: "requestedDensity"
        value: binder.overrideActive ? binder.overrideDensity : AppSettings.density
    }
    Binding {
        target: Theme
        property: "requestedAccent"
        value: binder.overrideActive && binder.overrideAccent.length > 0 ? binder.overrideAccent
                                                                         : AppSettings.accent
    }
    Binding {
        target: Theme
        property: "uiScale"
        value: AppSettings.uiScale
    }
    Binding {
        target: Theme
        property: "uiFontFamily"
        value: AppSettings.uiFont
    }
    Binding {
        target: Theme
        property: "reduceMotion"
        value: binder.overrideActive && binder.overrideReduceMotion !== null
               ? binder.overrideReduceMotion === true : AppSettings.reduceMotion
    }
}
