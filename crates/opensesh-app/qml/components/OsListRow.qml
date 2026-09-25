// List row: optional leading icon, title (`text`), optional subtitle and trailing content.
// Children go into the trailing slot (e.g. an OsBadge, OsTag or OsButton).
//   iconName: string       optional leading icon
//   subtitle: string       optional second line (muted)
//   trailingText: string   optional muted text before the trailing slot
//   selected: bool         selection fill with an accent bar (`highlighted` is the lighter
//                          "current item" fill, e.g. `highlighted: ListView.isCurrentItem`)
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.ItemDelegate {
    id: control

    property string iconName: ""
    property string subtitle: ""
    property string trailingText: ""
    property bool selected: false

    default property alias trailing: trailingRow.data

    readonly property color inkColor: enabled ? Theme.text : Theme.textDisabled
    readonly property color mutedInkColor: enabled ? Theme.textMuted : Theme.textDisabled

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    leftPadding: Theme.controlPadding
    rightPadding: Theme.controlPadding
    topPadding: Theme.spacingXs
    bottomPadding: Theme.spacingXs
    spacing: Theme.spacingMd
    hoverEnabled: true

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize

    Accessible.role: Accessible.ListItem
    Accessible.name: text
    Accessible.description: subtitle
    Accessible.selected: selected

    contentItem: Item {
        implicitWidth: (icon.visible ? icon.width + control.spacing : 0)
                       + Math.max(titleText.implicitWidth, subtitleText.visible ? subtitleText.implicitWidth : 0)
                       + (trailingLabel.visible ? control.spacing + trailingLabel.implicitWidth : 0)
                       + (trailingRow.implicitWidth > 0 ? control.spacing + trailingRow.implicitWidth : 0)
        implicitHeight: Math.max(texts.implicitHeight, trailingRow.implicitHeight, icon.height)

        OsIcon {
            id: icon

            anchors.verticalCenter: parent.verticalCenter
            x: control.mirrored ? parent.width - width : 0
            visible: control.iconName.length > 0
            name: control.iconName
            size: Theme.iconSize
            color: control.selected && control.enabled ? Theme.accentFg : control.mutedInkColor
        }

        Column {
            id: texts

            readonly property real start: icon.visible ? icon.width + control.spacing : 0
            readonly property real end: (trailingLabel.visible ? trailingLabel.width + control.spacing : 0)
                                        + (trailingRow.width > 0 ? trailingRow.width + control.spacing : 0)

            anchors.verticalCenter: parent.verticalCenter
            x: control.mirrored ? end : start
            width: Math.max(0, parent.width - start - end)

            OsText {
                id: titleText

                width: parent.width
                text: control.text
                font: control.font
                color: control.inkColor
                horizontalAlignment: Text.AlignLeft
            }

            OsText {
                id: subtitleText

                width: parent.width
                visible: control.subtitle.length > 0
                text: control.subtitle
                size: "small"
                color: control.mutedInkColor
                horizontalAlignment: Text.AlignLeft
            }
        }

        OsText {
            id: trailingLabel

            anchors.verticalCenter: parent.verticalCenter
            x: control.mirrored ? (trailingRow.width > 0 ? trailingRow.width + control.spacing : 0)
                                : parent.width - width - (trailingRow.width > 0 ? trailingRow.width + control.spacing : 0)
            visible: control.trailingText.length > 0
            text: control.trailingText
            size: "small"
            color: control.mutedInkColor
        }

        Row {
            id: trailingRow

            anchors.verticalCenter: parent.verticalCenter
            x: control.mirrored ? 0 : parent.width - width
            spacing: Theme.spacingSm
        }
    }

    background: Rectangle {
        implicitWidth: Theme.spacingXxl * 6
        implicitHeight: Theme.rowHeight
        radius: Theme.radiusControl
        color: control.selected ? Theme.selection : "transparent"

        Behavior on color {
            ColorAnimation {
                duration: Theme.durationFast
            }
        }

        // Hover, press and "current item" feedback, drawn over the fill.
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            visible: control.enabled
            color: control.down ? Theme.pressed
                 : control.hovered || control.highlighted ? Theme.hover
                 : "transparent"

            Behavior on color {
                ColorAnimation {
                    duration: Theme.durationFast
                }
            }
        }

        // Selected indicator.
        Rectangle {
            x: control.mirrored ? parent.width - width : 0
            anchors.verticalCenter: parent.verticalCenter
            width: Theme.borderWidth * 3
            height: parent.height - 2 * Theme.spacingSm
            radius: width / 2
            color: control.enabled ? Theme.accent : Theme.textDisabled
            visible: control.selected
        }

        // Inset ring: rows usually sit in clipped lists.
        OsFocusRing {
            anchors.margins: 0
            target: control
            baseRadius: Theme.radiusControl - Theme.focusRingWidth - gap
        }
    }
}
