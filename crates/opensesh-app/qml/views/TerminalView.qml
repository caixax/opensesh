// Terminal workspace view (PLAN §5.4). Session tabs show their own content (TerminalTab); this
// view shows when the Terminal view is restored at startup with no tab open. Split panes arrive
// in Sprint 4.
import QtQuick
import cc.caixa.opensesh

Item {
    id: view

    OsEmptyState {
        anchors.fill: parent
        iconName: "square-terminal"
        title: qsTr("No terminal open")
        description: qsTr("Open a local terminal or connect to a host.")

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
