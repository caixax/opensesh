// Themed popup menu for context menus, overflow menus and submenus. Fill it with OsMenuItem,
// OsMenuSeparator and nested OsContextMenu objects (a nested menu becomes a submenu whose item
// text is its `title`). Open it like T.Menu: `popup()` at the mouse position, `popup(x, y)`,
// `popup(parentItem, x, y)` or `open()`. T.Menu provides Accessible.role PopupMenu and the
// keyboard handling (arrows, Enter, Escape, Left/Right for submenus).
// It is always drawn inside the window (`popupType: Popup.Item`), so it is themed and captured
// by screenshots on every platform. Closing a top-level menu gives the focus back to where it
// was, with its focus ring (OsFocusReturn); a submenu leaves that to its parent menu.
//   hasLeadingColumn: bool  read-only; some item shows an icon or is checkable, so every
//                           OsMenuItem reserves the leading column and the labels line up
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.Menu {
    id: control

    readonly property bool hasLeadingColumn: {
        for (let i = 0; i < count; ++i) {
            const item = itemAt(i) as T.MenuItem;
            if (item && item.checkable)
                return true;
            const osItem = item as OsMenuItem;
            if (osItem && osItem.iconName.length > 0)
                return true;
        }
        return false;
    }
    readonly property OsFocusReturn focusReturn: OsFocusReturn {
        popup: control
    }

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    popupType: T.Popup.Item
    // Distance kept from the window edges, and how far a submenu overlaps its parent menu.
    margins: Theme.spacingSm
    overlap: Theme.spacingXs
    padding: Theme.spacingXs

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize

    delegate: OsMenuItem {}

    onAboutToShow: {
        focusReturn.save();
        // A submenu opens from an item of its parent menu, which takes the focus back itself.
        if (focusReturn.item as T.MenuItem)
            focusReturn.item = null;
    }
    onClosed: focusReturn.restore()

    // Fade in only. Closing is instant like native menus, and because T.Menu ignores `popup()`
    // while an exit transition runs, an exit fade would swallow a right-click made to reopen the
    // menu somewhere else while it is still fading out.
    enter: Transition {
        NumberAnimation {
            property: "opacity"
            from: 0
            to: 1
            duration: Theme.durationFast
        }
    }

    contentItem: ListView {
        // As wide as the widest item, so labels and shortcuts are never elided needlessly.
        implicitWidth: {
            let widest = 0;
            for (let i = 0; i < control.count; ++i) {
                const item = control.itemAt(i);
                if (item)
                    widest = Math.max(widest, item.implicitWidth);
            }
            return widest;
        }
        implicitHeight: contentHeight
        model: control.contentModel
        interactive: Window.window ? contentHeight + control.topPadding + control.bottomPadding > control.height
                                   : false
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        currentIndex: control.currentIndex
    }

    background: Rectangle {
        implicitWidth: Theme.spacingXs * 45
        implicitHeight: Theme.controlHeightSmall
        color: Theme.surface
        radius: Theme.radiusControl
        border.color: Theme.borderStrong
        border.width: Theme.borderWidth
    }
}
