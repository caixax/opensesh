// Section title (fontSizeLarge, DemiBold) with an optional muted description underneath.
// Children go into a trailing row for actions (e.g. OsButton), centered vertically.
//   title: string
//   description: string   optional, wraps
import QtQuick
import cc.caixa.opensesh

Item {
    id: control

    property string title
    property string description

    default property alias actions: actionsRow.data

    readonly property real actionsSpace: actionsRow.implicitWidth > 0 ? actionsRow.implicitWidth + Theme.spacingLg : 0

    implicitWidth: Math.max(titleText.implicitWidth, descriptionText.visible ? descriptionText.implicitWidth : 0)
                   + actionsSpace
    implicitHeight: Math.max(texts.implicitHeight, actionsRow.implicitHeight)

    Accessible.role: Accessible.Heading
    Accessible.name: title
    Accessible.description: description

    Column {
        id: texts

        anchors.verticalCenter: parent.verticalCenter
        x: control.LayoutMirroring.enabled ? control.actionsSpace : 0
        width: Math.max(0, parent.width - control.actionsSpace)
        spacing: Theme.spacingXs

        OsText {
            id: titleText

            width: parent.width
            text: control.title
            size: "large"
            horizontalAlignment: Text.AlignLeft
            Accessible.ignored: true
        }

        OsText {
            id: descriptionText

            width: parent.width
            visible: control.description.length > 0
            text: control.description
            muted: true
            horizontalAlignment: Text.AlignLeft
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            Accessible.ignored: true
        }
    }

    Row {
        id: actionsRow

        anchors.verticalCenter: parent.verticalCenter
        x: control.LayoutMirroring.enabled ? 0 : parent.width - width
        spacing: Theme.spacingSm
    }
}
