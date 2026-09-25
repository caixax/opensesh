// A settings section that a later sprint implements: an empty state that says what the section
// will hold and which sprint brings it, centered in at least `minimumHeight`.
//   iconName, title, description: string   the section's icon, name and summary
//   sprint: int                             the sprint that implements it
//   minimumHeight: real                     usually the visible height of the page area
import QtQuick
import cc.caixa.opensesh

Item {
    id: page

    property string iconName
    property string title
    property string description
    property int sprint
    property real minimumHeight

    implicitHeight: Math.max(minimumHeight, emptyState.implicitHeight)

    OsEmptyState {
        id: emptyState

        anchors.fill: parent
        iconName: page.iconName
        title: page.title
        description: page.description

        OsTag {
            text: qsTr("Coming in Sprint %1").arg(page.sprint)
        }
    }
}
