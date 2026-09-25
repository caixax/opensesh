// Entry of an OsContextMenu: leading icon or check mark, label, muted shortcut text on the right
// and a chevron when it opens a submenu. T.MenuItem provides Accessible.role MenuItem, the
// checkable state and `text` as the accessible name.
//   iconName: string       optional leading icon
//   shortcutText: string   key sequence shown on the right, e.g. "Ctrl+C" (display only; the
//                          shortcut itself belongs to an OsAction or a Shortcut)
//   reserveLeading: bool   keep the leading column even without an icon; defaults to the menu's
//                          `hasLeadingColumn` so labels line up (outside a menu: own icon/check)
// A checkable item shows a check mark in the leading column when checked, instead of its icon.
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.MenuItem {
    id: control

    property string iconName
    property string shortcutText
    property bool reserveLeading: ownerMenu && ownerMenu.hasLeadingColumn !== undefined
                                  ? ownerMenu.hasLeadingColumn : checkable || iconName.length > 0

    // The menu as an untyped object: naming OsContextMenu here would make the two files depend on
    // each other, so its `hasLeadingColumn` is looked up dynamically (other menus lack it).
    readonly property var ownerMenu: menu

    readonly property string leadingIcon: checkable ? (checked ? "check" : "") : iconName
    // Layout conditions; never the children's `visible`, which is false while the menu is closed.
    readonly property bool hasShortcut: shortcutText.length > 0
    readonly property bool hasSubMenu: subMenu !== null
    readonly property color inkColor: enabled ? Theme.text : Theme.textDisabled
    // Outside a menu nothing sets `highlighted`, so hover highlights directly.
    readonly property bool showHighlight: enabled && (highlighted || visualFocus || (menu === null && hovered))

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    leftPadding: Theme.spacingSm
    rightPadding: Theme.spacingSm
    topPadding: 0
    bottomPadding: 0
    spacing: Theme.spacingSm

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize

    Accessible.name: text
    Accessible.description: shortcutText

    contentItem: Item {
        implicitWidth: (control.reserveLeading ? leading.width + control.spacing : 0) + label.implicitWidth
                       + (control.hasShortcut ? Theme.spacingXl + shortcut.implicitWidth : 0)
                       + (control.hasSubMenu ? control.spacing + arrow.width : 0)
        implicitHeight: Math.max(label.implicitHeight, Theme.iconSizeSmall)

        OsIcon {
            id: leading

            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            visible: control.reserveLeading
            name: control.leadingIcon
            color: control.inkColor
            size: Theme.iconSizeSmall
        }

        OsText {
            id: label

            anchors.left: control.reserveLeading ? leading.right : parent.left
            anchors.leftMargin: control.reserveLeading ? control.spacing : 0
            anchors.right: control.hasShortcut ? shortcut.left : control.hasSubMenu ? arrow.left : parent.right
            anchors.rightMargin: control.hasShortcut ? Theme.spacingXl : control.hasSubMenu ? control.spacing : 0
            anchors.verticalCenter: parent.verticalCenter
            text: control.text
            font: control.font
            color: control.inkColor
            Accessible.ignored: true
        }

        OsText {
            id: shortcut

            anchors.right: control.hasSubMenu ? arrow.left : parent.right
            anchors.rightMargin: control.hasSubMenu ? control.spacing : 0
            anchors.verticalCenter: parent.verticalCenter
            visible: control.hasShortcut
            text: control.shortcutText
            font: control.font
            color: control.enabled ? Theme.textMuted : Theme.textDisabled
            Accessible.ignored: true
        }

        OsIcon {
            id: arrow

            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: control.hasSubMenu
            name: control.mirrored ? "chevron-left" : "chevron-right"
            color: control.enabled ? Theme.textMuted : Theme.textDisabled
            size: Theme.iconSizeSmall
        }
    }

    background: Rectangle {
        implicitWidth: Theme.spacingXs * 30
        implicitHeight: Theme.controlHeightSmall + Theme.spacingXs
        radius: Theme.radiusSmall
        color: !control.showHighlight ? "transparent" : control.down ? Theme.pressed : Theme.hover

        Behavior on color {
            ColorAnimation {
                duration: Theme.durationFast
            }
        }
    }
}
