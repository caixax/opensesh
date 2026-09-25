// History and logs view (PLAN §5.4). Placeholder: recent connections come with hosts and
// sessions (Sprint 5), session logs with SSH (Sprint 7) and recordings in Sprint 10. The
// application logs folder already exists.
import QtQuick
import cc.caixa.opensesh

Item {
    id: view

    OsEmptyState {
        anchors.fill: parent
        iconName: "history"
        title: qsTr("No history yet")
        description: qsTr("Your recent connections appear here with hosts and sessions in Sprint 5. Session logs arrive in Sprint 7 and recordings in Sprint 10.")

        OsButton {
            text: qsTr("Open logs folder")
            iconName: "folder-open"
            onClicked: ActionRegistry.trigger("app.openLogsFolder")
        }
    }
}
