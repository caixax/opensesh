// A titled group of settings on a card. Children go into the card's column, one under the other
// (usually SettingsRow items, which fill the width by themselves).
//   title: string
//   description: string   optional muted text under the title
import QtQuick
import cc.caixa.opensesh

Column {
    id: group

    property string title
    property string description

    default property alias rows: body.data

    spacing: Theme.spacingSm

    Accessible.role: Accessible.Grouping
    Accessible.name: title

    OsSectionHeader {
        width: parent.width
        visible: group.title.length > 0
        title: group.title
        description: group.description
    }

    OsCard {
        width: parent.width
        padding: Theme.spacingLg

        Column {
            id: body

            width: parent.width
            spacing: Theme.spacingLg
        }
    }
}
