// Terminal workspace view (PLAN §5.4). Session tabs show their own content; this view is what the
// Terminal rail entry shows while no session tab is open. The terminal engine arrives in Sprint 2,
// tabs and split panes are completed in Sprint 4.
import QtQuick
import cc.caixa.opensesh

Item {
    id: view

    OsEmptyState {
        anchors.fill: parent
        iconName: "square-terminal"
        title: qsTr("No terminal open")
        description: qsTr("Open a local terminal or connect to a host. The terminal engine arrives in Sprint 2, and tabs with split panes in Sprint 4.")

        OsButton {
            text: qsTr("Open local terminal")
            iconName: "plus"
            variant: "primary"
            onClicked: ActionRegistry.trigger("tab.newLocal")
        }

        OsButton {
            text: qsTr("Quick connect")
            iconName: "plug-zap"
            onClicked: ActionRegistry.trigger("app.quickConnect")
        }
    }
}
