// Content of a placeholder session tab ("Local terminal") until the terminal engine lands in
// Sprint 2.
//   title: string   the tab title
import QtQuick
import cc.caixa.opensesh

Item {
    id: placeholder

    property string title

    OsEmptyState {
        anchors.fill: parent
        iconName: "square-terminal"
        title: placeholder.title
        description: qsTr("The terminal engine arrives in Sprint 2. Until then this tab is a placeholder to try the tab bar and its shortcuts.")

        OsButton {
            text: qsTr("Close tab")
            iconName: "x"
            onClicked: ActionRegistry.trigger("tab.close")
        }

        OsButton {
            text: qsTr("New tab")
            iconName: "plus"
            onClicked: ActionRegistry.trigger("tab.newLocal")
        }
    }
}
