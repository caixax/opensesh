// Terminal workspace view (PLAN §5.4). Terminal tabs show their own content (TabWorkspace); this
// view shows when the Terminal view is restored at startup with no tab open: open a terminal, or
// a saved workspace.
import QtQuick
import cc.caixa.opensesh

Item {
    id: view

    OsEmptyState {
        anchors.fill: parent
        iconName: "square-terminal"
        title: qsTr("No terminal open")
        description: qsTr("Open a local terminal, a saved workspace or connect to a host.")

        OsButton {
            text: qsTr("Open local terminal")
            iconName: "plus"
            variant: "primary"
            onClicked: ActionRegistry.trigger("tab.newLocal")
        }

        OsButton {
            text: qsTr("Open workspace…")
            iconName: "layout-grid"
            onClicked: ActionRegistry.trigger("workspace.open")
        }

        OsButton {
            text: qsTr("Quick connect")
            iconName: "plug-zap"
            onClicked: ActionRegistry.trigger("app.quickConnect")
        }
    }
}
