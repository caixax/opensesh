// Live preview for Settings > Appearance: a few real components (list rows, tag, badge, buttons,
// switch, check box, field and progress bar) that follow the theme, accent, density, scale and
// font as they change. They stay out of the Tab chain: the preview is only for looking.
import QtQuick
import cc.caixa.opensesh

Column {
    id: preview

    spacing: Theme.spacingMd

    Accessible.role: Accessible.Grouping
    Accessible.name: qsTr("Preview")

    Column {
        width: parent.width
        spacing: Theme.spacingXs

        OsListRow {
            width: parent.width
            focusPolicy: Qt.NoFocus
            iconName: "server"
            text: qsTr("web-01")
            subtitle: qsTr("deploy@web-01.example.org")
            selected: true

            OsTag {
                text: qsTr("production")
                variant: "accent"
            }
        }

        OsListRow {
            width: parent.width
            focusPolicy: Qt.NoFocus
            iconName: "server"
            text: qsTr("db-02")
            subtitle: qsTr("postgres@db-02.internal")
            trailingText: qsTr("2 sessions")

            OsBadge {
                dot: true
                variant: "success"
            }
        }
    }

    Flow {
        width: parent.width
        spacing: Theme.spacingSm

        OsButton {
            focusPolicy: Qt.NoFocus
            text: qsTr("Connect")
            iconName: "plug-zap"
            variant: "primary"
        }

        OsButton {
            focusPolicy: Qt.NoFocus
            text: qsTr("Edit")
            iconName: "pencil"
        }

        OsButton {
            focusPolicy: Qt.NoFocus
            text: qsTr("Delete")
            iconName: "trash-2"
            variant: "ghost"
        }

        OsSwitch {
            height: Theme.controlHeight
            focusPolicy: Qt.NoFocus
            text: qsTr("Reconnect")
            checked: true
        }

        OsCheckBox {
            height: Theme.controlHeight
            focusPolicy: Qt.NoFocus
            text: qsTr("Favorite")
            checked: true
        }
    }

    Row {
        width: parent.width
        spacing: Theme.spacingMd

        OsTextField {
            id: previewField

            width: Math.min(Theme.spacingXxl * 7, (parent.width - parent.spacing) / 2)
            activeFocusOnTab: false
            placeholderText: qsTr("Search hosts")
        }

        Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - previewField.width - parent.spacing
            spacing: Theme.spacingXs

            OsText {
                width: parent.width
                text: qsTr("Uploading backup.tar.gz")
                size: "small"
                muted: true
            }

            OsProgress {
                width: parent.width
                value: 0.6
                Accessible.name: qsTr("Upload progress")
            }
        }
    }
}
