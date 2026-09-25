// SFTP view (PLAN §5.4): dual-pane file explorer. Placeholder until Sprint 8.
import QtQuick
import cc.caixa.opensesh

Item {
    id: view

    ComingSoon {
        id: comingSoon
    }

    OsEmptyState {
        anchors.fill: parent
        iconName: "folder-sync"
        title: qsTr("SFTP")
        description: qsTr("Browse, edit and transfer files on your servers in a dual-pane explorer, local on one side and remote on the other. Coming in Sprint 8.")

        OsButton {
            text: qsTr("New SFTP connection")
            iconName: "plus"
            variant: "primary"
            onClicked: comingSoon.notify(qsTr("SFTP"), 8)
        }
    }
}
