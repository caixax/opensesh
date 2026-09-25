// Keychain view (PLAN §5.4): identities, SSH keys and known hosts. Placeholder until Sprint 6
// (known hosts in Sprint 7).
import QtQuick
import cc.caixa.opensesh

Item {
    id: view

    ComingSoon {
        id: comingSoon
    }

    OsEmptyState {
        anchors.fill: parent
        iconName: "key-round"
        title: qsTr("Your keychain is empty")
        description: qsTr("Identities, SSH keys and known hosts, protected by the vault or your system keyring. The keychain arrives in Sprint 6.")

        OsButton {
            text: qsTr("New identity")
            iconName: "user"
            variant: "primary"
            onClicked: comingSoon.notify(qsTr("Identities"), 6)
        }

        OsButton {
            text: qsTr("Generate key")
            iconName: "key-round"
            onClicked: comingSoon.notify(qsTr("SSH key generation"), 6)
        }
    }
}
