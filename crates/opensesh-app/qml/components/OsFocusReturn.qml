// Gives the keyboard focus back to where it was when a popup closes (PLAN §10). Qt refocuses the
// opener itself, but with a non-keyboard reason that hides its focus ring; this helper also
// remembers how the item got the focus. The popup calls save() when it is about to show and
// restore() once it has closed; OsDialog, OsDrawer, OsContextMenu and OsCommandPalette do.
// restore() leaves the focus alone when something else took it in between (for example a menu
// action that opened a dialog).
//   popup: T.Popup   the popup to follow (required)
//   item: Item       the item to give the focus back to; save() sets it, null for none
// Functions: save(), restore().
import QtQuick
import QtQuick.Templates as T

QtObject {
    id: helper

    required property T.Popup popup
    property Item item: null
    property int reason: Qt.OtherFocusReason

    // `ancestor` is `candidate` or contains it.
    function isWithin(candidate: Item, ancestor: Item): bool {
        for (let at = candidate; at; at = at.parent) {
            if (at === ancestor)
                return true;
        }
        return false;
    }

    function save() {
        const anchor = popup.parent ? popup.parent : popup.contentItem;
        const window = anchor ? anchor.Window.window : null;
        const focused = window ? window.activeFocusItem : null;
        const popupItem = popup.contentItem ? popup.contentItem.parent : null;
        item = focused && !(popupItem && isWithin(focused, popupItem)) ? focused : null;
        // Controls know how they got the focus: keep the focus ring if it came from the keyboard.
        reason = item && item.focusReason !== undefined ? item.focusReason : Qt.OtherFocusReason;
    }

    function restore() {
        const target = item;
        item = null;
        if (!target || !target.visible || !target.enabled)
            return;
        const window = target.Window.window;
        const current = window ? window.activeFocusItem : null;
        // Where closing leaves the focus: back on the target (Qt's own restore), still inside
        // the closed popup, or on an ancestor of the target (its focus scope, the window content).
        const popupItem = popup.contentItem ? popup.contentItem.parent : null;
        if (current && current !== target && !(popupItem && isWithin(current, popupItem))
                && !isWithin(target, current))
            return;
        target.forceActiveFocus(reason);
    }
}
