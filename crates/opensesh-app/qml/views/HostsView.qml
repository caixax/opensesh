// Hosts view (PLAN §5.3, §5.4). Placeholder until Sprint 5 (hosts and sessions): the empty state
// with the §5.3 actions New host, Quick connect, Local terminal and Import.
import QtQuick
import cc.caixa.opensesh

Item {
    id: view

    ComingSoon {
        id: comingSoon
    }

    OsEmptyState {
        anchors.fill: parent
        iconName: "server"
        title: qsTr("No hosts yet")
        description: qsTr("Save the servers you connect to and open them with one click. Host management arrives in Sprint 5.")

        OsButton {
            text: qsTr("New host")
            iconName: "plus"
            variant: "primary"
            onClicked: comingSoon.notify(qsTr("Host management"), 5)
        }

        OsButton {
            text: qsTr("Quick connect")
            iconName: "plug-zap"
            onClicked: ActionRegistry.trigger("app.quickConnect")
        }

        OsButton {
            text: qsTr("Local terminal")
            iconName: "square-terminal"
            onClicked: ActionRegistry.trigger("tab.newLocal")
        }

        OsButton {
            text: qsTr("Import")
            iconName: "import"
            onClicked: comingSoon.notify(qsTr("Importing ~/.ssh/config"), 5)
        }
    }
}
