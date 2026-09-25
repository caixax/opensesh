// Empty state: a large muted icon in a `surface2` circle, a title, a body text and a row of
// actions (children, e.g. OsButton), centered in the available space.
//   iconName: string
//   title: string
//   description: string        body text, wraps
//   maximumTextWidth: real     the texts wrap at this width
import QtQuick
import cc.caixa.opensesh

Item {
    id: control

    property string iconName: ""
    property string title
    property string description
    property real maximumTextWidth: Theme.spacingXxl * 12

    default property alias actions: actionsRow.data

    implicitWidth: Math.min(maximumTextWidth, Math.max(titleText.implicitWidth, bodyText.implicitWidth,
                                                       actionsRow.implicitWidth, circle.width))
                   + 2 * Theme.spacingXl
    implicitHeight: column.implicitHeight + 2 * Theme.spacingXl

    Accessible.role: Accessible.Grouping
    Accessible.name: title
    Accessible.description: description

    Column {
        id: column

        anchors.centerIn: parent
        width: Math.max(0, Math.min(control.width - 2 * Theme.spacingXl, control.maximumTextWidth))
        spacing: Theme.spacingSm

        Rectangle {
            id: circle

            anchors.horizontalCenter: parent.horizontalCenter
            visible: control.iconName.length > 0
            width: Theme.spacingXxl * 2
            height: width
            radius: width / 2
            color: Theme.surface2
            border.width: Theme.borderWidth
            border.color: Theme.border

            OsIcon {
                anchors.centerIn: parent
                name: control.iconName
                size: Theme.spacingXxl
                color: control.enabled ? Theme.textMuted : Theme.textDisabled
            }
        }

        // Extra room between the icon and the title.
        Item {
            width: 1
            height: Theme.spacingXs
            visible: circle.visible
        }

        OsText {
            id: titleText

            width: parent.width
            visible: text.length > 0
            text: control.title
            size: "large"
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignHCenter
            Accessible.ignored: true
        }

        OsText {
            id: bodyText

            width: parent.width
            visible: text.length > 0
            text: control.description
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignHCenter
            Accessible.ignored: true
        }

        Item {
            width: 1
            height: Theme.spacingSm
            visible: actionsRow.implicitWidth > 0
        }

        Row {
            id: actionsRow

            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Theme.spacingSm
        }
    }
}
