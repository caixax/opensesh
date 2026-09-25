// Square, icon-only button (see docs/design/components.md).
//   iconName: string  icon to show (a name from assets/icons/icons.toml)
//   toolTip: string   translated label: shown as a tooltip on hover and used as the accessible
//                     name. Always set it (or `text`): an icon alone has no name.
//   variant: string   "ghost" (default, no fill until hovered) | "secondary" (surface2 fill)
//   iconSize: real    defaults to Theme.iconSizeSmall
// Set `checkable: true` for a toggle; the checked state gets an accent tint and accent icon.
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.Button {
    id: control

    property string iconName: ""
    property string toolTip: ""
    property string variant: "ghost"
    property real iconSize: Theme.iconSizeSmall

    readonly property color inkColor: !enabled ? Theme.textDisabled
                                               : checked ? Theme.accentFg : Theme.text

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 0
    focusPolicy: Qt.StrongFocus
    hoverEnabled: true

    Accessible.role: Accessible.Button
    Accessible.name: toolTip.length > 0 ? toolTip : text

    contentItem: Item {
        implicitWidth: control.iconSize
        implicitHeight: control.iconSize

        OsIcon {
            anchors.centerIn: parent
            name: control.iconName
            color: control.inkColor
            size: control.iconSize
        }
    }

    background: Rectangle {
        implicitWidth: Theme.controlHeight
        implicitHeight: Theme.controlHeight
        radius: Theme.radiusControl
        color: control.checked ? Theme.selection
                               : control.variant === "secondary" ? Theme.surface2 : "transparent"
        border.width: control.variant === "secondary" && !control.checked ? Theme.borderWidth : 0
        border.color: Theme.border

        Behavior on color {
            ColorAnimation {
                duration: Theme.durationFast
            }
        }

        // Hover and press feedback, drawn over the fill.
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            visible: control.enabled
            // No hover on the checked state: it would pull muted text under AA (ADR 0006).
            color: control.down ? Theme.pressed : control.hovered && !control.checked ? Theme.hover : "transparent"

            Behavior on color {
                ColorAnimation {
                    duration: Theme.durationFast
                }
            }
        }

        OsFocusRing {
            target: control
            baseRadius: Theme.radiusControl
        }
    }

    T.ToolTip {
        id: tip

        parent: control
        x: Math.round((control.width - width) / 2)
        y: control.height + Theme.spacingXs
        margins: Theme.spacingSm
        leftPadding: Theme.spacingSm
        rightPadding: Theme.spacingSm
        topPadding: Theme.spacingXs
        bottomPadding: Theme.spacingXs
        implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                                implicitContentWidth + leftPadding + rightPadding)
        implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                                 implicitContentHeight + topPadding + bottomPadding)
        delay: 600
        text: control.toolTip
        visible: control.hovered && !control.down && control.toolTip.length > 0
        closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutsideParent
                     | T.Popup.CloseOnReleaseOutsideParent

        contentItem: OsText {
            text: tip.text
            size: "small"
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }

        background: Rectangle {
            color: Theme.surface2
            radius: Theme.radiusSmall
            border.width: Theme.borderWidth
            border.color: Theme.borderStrong
        }
    }
}
