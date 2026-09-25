// Snippets view (PLAN §5.4): saved commands and macros. Placeholder until Sprint 10.
import QtQuick
import cc.caixa.opensesh

Item {
    id: view

    ComingSoon {
        id: comingSoon
    }

    OsEmptyState {
        anchors.fill: parent
        iconName: "scroll-text"
        title: qsTr("No snippets")
        description: qsTr("Save the commands you run often, with variables, and send them to one terminal or many at once. Snippets arrive in Sprint 10.")

        OsButton {
            text: qsTr("New snippet")
            iconName: "plus"
            variant: "primary"
            onClicked: comingSoon.notify(qsTr("Snippets"), 10)
        }
    }
}
