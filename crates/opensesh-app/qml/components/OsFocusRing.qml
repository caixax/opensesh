// Keyboard focus indicator. Put it inside a control (usually in its background) and point
// `target` at the control; it outlines its parent, outside its bounds, only when the control
// has focus from the keyboard (Tab, Backtab or a shortcut).
//   target: Item          the control whose focus is shown (defaults to the parent)
//   baseRadius: real      corner radius of the outlined shape
//   keyboardFocus: bool   for a target that doesn't know how it got the focus (a plain Item,
//                         unlike Controls and text fields): whether its focus came from the
//                         keyboard. The ring shows while the target has active focus and this is
//                         true (default false); e.g. `true` for an item that only takes the focus
//                         through Tab
import QtQuick
import cc.caixa.opensesh

Rectangle {
    id: ring

    property Item target: parent
    property real baseRadius: Theme.radiusControl
    property bool keyboardFocus: false

    readonly property bool shown: {
        if (!target)
            return false;
        if (target.visualFocus !== undefined)
            return target.visualFocus;
        if (target.focusReason === undefined)
            return target.activeFocus && keyboardFocus;
        return target.activeFocus && (target.focusReason === Qt.TabFocusReason
                                      || target.focusReason === Qt.BacktabFocusReason
                                      || target.focusReason === Qt.ShortcutFocusReason);
    }

    readonly property real gap: 2

    anchors.fill: parent
    anchors.margins: -(Theme.focusRingWidth + gap)
    radius: baseRadius + Theme.focusRingWidth + gap
    color: "transparent"
    border.color: Theme.focusRing
    border.width: Theme.focusRingWidth
    visible: shown
    z: 100

    Accessible.ignored: true
}
