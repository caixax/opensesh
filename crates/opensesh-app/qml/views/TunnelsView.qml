// Tunnels view (PLAN §5.4): port forwarding manager. Placeholder until Sprint 9.
import QtQuick
import cc.caixa.opensesh

Item {
    id: view

    ComingSoon {
        id: comingSoon
    }

    OsEmptyState {
        anchors.fill: parent
        iconName: "waypoints"
        title: qsTr("No tunnels")
        description: qsTr("Forward local, remote and dynamic (SOCKS) ports through SSH, start them with the app and see their traffic. Tunnels arrive in Sprint 9.")

        OsButton {
            text: qsTr("New tunnel")
            iconName: "plus"
            variant: "primary"
            onClicked: comingSoon.notify(qsTr("Tunnels"), 9)
        }
    }
}
