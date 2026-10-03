// A settings section without a page of its own: an empty state that says what the section
// holds, centered in at least `minimumHeight`.
//   iconName, title, description: string   the section's icon, name and summary
//   minimumHeight: real                     usually the visible height of the page area
import QtQuick
import cc.caixa.opensesh

Item {
    id: page

    property string iconName
    property string title
    property string description
    property real minimumHeight

    implicitHeight: Math.max(minimumHeight, emptyState.implicitHeight)

    OsEmptyState {
        id: emptyState

        anchors.fill: parent
        iconName: page.iconName
        title: page.title
        description: page.description
    }
}
