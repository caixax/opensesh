// Inline notice inside a settings page: a status icon, a title and a few lines of text, on
// `surface2` with a status-colored outline. Screen readers get it as an alert.
//   kind: string    "warning" (default) | "danger" | "info"
//   title: string
//   lines: var      strings shown under the title, one per line (they wrap)
pragma ComponentBehavior: Bound

import QtQuick
import cc.caixa.opensesh

Rectangle {
    id: notice

    property string kind: "warning"
    property string title
    property var lines: []

    readonly property color statusColor: kind === "danger" ? Theme.danger
                                       : kind === "info" ? Theme.info : Theme.warning
    readonly property real innerPadding: Theme.spacingMd

    implicitWidth: Theme.spacingXxl * 10
    implicitHeight: content.implicitHeight + 2 * innerPadding
    radius: Theme.radiusControl
    color: Theme.surface2
    border.width: Theme.borderWidth
    border.color: statusColor

    Accessible.role: Accessible.AlertMessage
    Accessible.name: title
    Accessible.description: lines.join("\n")

    Row {
        id: content

        x: notice.innerPadding
        y: notice.innerPadding
        width: notice.width - x - notice.innerPadding
        spacing: Theme.spacingSm
        layoutDirection: notice.LayoutMirroring.enabled ? Qt.RightToLeft : Qt.LeftToRight

        OsIcon {
            id: icon

            y: Math.max(0, (titleText.lineHeightPx - height) / 2)
            name: notice.kind === "danger" ? "circle-x" : notice.kind === "info" ? "info" : "triangle-alert"
            size: Theme.iconSizeSmall
            color: notice.statusColor
        }

        Column {
            width: parent.width - icon.width - parent.spacing
            spacing: Theme.spacingXs

            OsText {
                id: titleText

                readonly property real lineHeightPx: lineCount > 0 ? implicitHeight / lineCount : implicitHeight

                width: parent.width
                text: notice.title
                font.weight: Font.Medium
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                horizontalAlignment: Text.AlignLeft
                Accessible.ignored: true
            }

            Repeater {
                model: notice.lines

                delegate: OsText {
                    required property string modelData

                    width: parent.width
                    text: modelData
                    size: "small"
                    muted: true
                    wrapMode: Text.Wrap
                    elide: Text.ElideNone
                    horizontalAlignment: Text.AlignLeft
                    Accessible.ignored: true
                }
            }
        }
    }
}
