// Feeds the Theme singleton from AppSettings and the OS color scheme. Screenshot and gallery
// modes can override the mode and density without touching the user's settings.
import QtQuick
import cc.caixa.opensesh

Item {
    id: binder

    property bool overrideActive: false
    property string overrideMode: "dark"
    property string overrideDensity: "comfortable"

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
        value: AppSettings.accent
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
        value: AppSettings.reduceMotion
    }
}
