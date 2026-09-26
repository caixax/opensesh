pragma Singleton

// The tab colors (Theme.tabColorNames) for menus and marks: translated names, and the color to
// draw for a name. Hosts and groups may also hold a `#RRGGBB` color from hosts.toml, drawn as it
// is.
// Functions: label(name), color(name) (transparent for none or an unknown name).
import QtQuick
import cc.caixa.opensesh

QtObject {
    // [{text, value}] for a drop-down, without "None".
    readonly property var options: Theme.tabColorNames.map(name => ({ text: label(name), value: name }))

    function label(name) {
        switch (name) {
        case "red":
            return qsTr("Red");
        case "orange":
            return qsTr("Orange");
        case "yellow":
            return qsTr("Yellow");
        case "green":
            return qsTr("Green");
        case "teal":
            return qsTr("Teal");
        case "blue":
            return qsTr("Blue");
        case "purple":
            return qsTr("Purple");
        case "pink":
            return qsTr("Pink");
        default:
            return name;
        }
    }

    function color(name) {
        const index = Theme.tabColorNames.indexOf(name ?? "");
        if (index >= 0)
            return Theme.tabColors[index];
        if (/^#[0-9a-fA-F]{6}$/.test(name ?? ""))
            return name;
        return "transparent";
    }
}
