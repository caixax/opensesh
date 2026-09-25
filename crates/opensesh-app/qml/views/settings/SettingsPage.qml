// One page of the Settings view (PLAN §5.4): a title, a muted description and the page content
// (children, usually SettingsGroup items with `width: parent.width`), stacked in a column.
//   title: string
//   description: string   optional, wraps
import QtQuick
import cc.caixa.opensesh

Column {
    id: page

    property string title
    property string description

    spacing: Theme.spacingXl

    Column {
        width: parent.width
        spacing: Theme.spacingXs

        OsText {
            width: parent.width
            text: page.title
            size: "title"
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignLeft
            Accessible.role: Accessible.Heading
        }

        OsText {
            width: parent.width
            visible: text.length > 0
            text: page.description
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignLeft
        }
    }
}
