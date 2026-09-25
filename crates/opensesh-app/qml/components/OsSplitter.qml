// Split view with a themed handle: a 1 px `border` line that turns accent while hovered or
// dragged, with a wider invisible grab area. With `keyboardResizable`, each handle is a Tab stop:
// the arrow keys resize (Shift for bigger steps) the item on the non-fill side of the handle.
// Size the items with the SplitView attached properties (SplitView.preferredWidth, fillWidth...).
//   showGrip: bool             grip icon in the middle of each handle
//   keyboardResizable: bool    handles take keyboard focus (default true)
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.SplitView {
    id: control

    property bool showGrip: false
    property bool keyboardResizable: true

    readonly property bool horizontal: orientation === Qt.Horizontal

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    Accessible.role: Accessible.Splitter

    // The item a handle resizes: the one on the non-fill side of the handle. `sign` is +1 when
    // that item grows as the handle moves forward (right or down), -1 otherwise.
    function resizeTarget(handle) {
        let fill = count - 1;
        for (let i = 0; i < count; ++i) {
            const item = itemAt(i);
            if (item && (horizontal ? item.T.SplitView.fillWidth : item.T.SplitView.fillHeight)) {
                fill = i;
                break;
            }
        }
        const handleStart = horizontal ? handle.x : handle.y;
        let before = -1;
        for (let i = 0; i < count; ++i) {
            const item = itemAt(i);
            if (item && item.visible && (horizontal ? item.x + item.width : item.y + item.height) <= handleStart + 0.5)
                before = i;
        }
        if (before < 0)
            return null;
        if (before < fill)
            return { index: before, sign: 1, fill: fill };
        let after = before + 1;
        while (after < count && !itemAt(after).visible)
            ++after;
        return after < count ? { index: after, sign: -1, fill: fill } : null;
    }

    function resizeFromHandle(handle, delta) {
        const target = resizeTarget(handle);
        if (!target)
            return;
        const item = itemAt(target.index);
        const split = item.T.SplitView;
        // The preferred size is updated at once; the geometry only on the next polish.
        const preferred = horizontal ? split.preferredWidth : split.preferredHeight;
        const size = preferred >= 0 ? preferred : horizontal ? item.width : item.height;
        let next = size + target.sign * delta;
        const minimum = horizontal ? split.minimumWidth : split.minimumHeight;
        const maximum = horizontal ? split.maximumWidth : split.maximumHeight;
        next = Math.max(next, Math.max(0, minimum));
        if (maximum >= 0)
            next = Math.min(next, maximum);
        // Growing takes room from the fill item, down to its minimum size.
        const fillItem = itemAt(target.fill);
        if (next > size && fillItem && fillItem !== item) {
            const fillSplit = fillItem.T.SplitView;
            const fillMinimum = Math.max(0, horizontal ? fillSplit.minimumWidth : fillSplit.minimumHeight);
            const room = (horizontal ? fillItem.width : fillItem.height) - fillMinimum;
            next = Math.min(next, size + Math.max(0, room));
        }
        if (horizontal)
            split.preferredWidth = next;
        else
            split.preferredHeight = next;
    }

    handle: Item {
        id: handle

        readonly property bool pressed: T.SplitHandle.pressed
        readonly property bool hovered: T.SplitHandle.hovered
        readonly property bool active: pressed || hovered || handle.activeFocus
        readonly property real grabSize: control.showGrip ? grip.width : Theme.spacingSm

        implicitWidth: control.horizontal ? Theme.borderWidth : control.width
        implicitHeight: control.horizontal ? control.height : Theme.borderWidth
        activeFocusOnTab: control.keyboardResizable

        Accessible.role: Accessible.Separator
        Accessible.name: qsTr("Resize")
        Accessible.focusable: control.keyboardResizable

        Keys.onPressed: event => {
            const step = (event.modifiers & Qt.ShiftModifier) ? Theme.spacingXxl * 2 : Theme.spacingLg;
            const mirrored = control.horizontal && control.LayoutMirroring.enabled;
            let delta = 0;
            if (control.horizontal && event.key === Qt.Key_Left)
                delta = mirrored ? step : -step;
            else if (control.horizontal && event.key === Qt.Key_Right)
                delta = mirrored ? -step : step;
            else if (!control.horizontal && event.key === Qt.Key_Up)
                delta = -step;
            else if (!control.horizontal && event.key === Qt.Key_Down)
                delta = step;
            else
                return;
            control.resizeFromHandle(handle, delta);
            event.accepted = true;
        }

        // Wider hit area than the visible line.
        containmentMask: Item {
            x: control.horizontal ? (handle.width - width) / 2 : 0
            y: control.horizontal ? 0 : (handle.height - height) / 2
            width: control.horizontal ? handle.grabSize : handle.width
            height: control.horizontal ? handle.height : handle.grabSize
        }

        Rectangle {
            anchors.centerIn: parent
            width: control.horizontal ? (handle.active ? Theme.borderWidth * 2 : Theme.borderWidth) : parent.width
            height: control.horizontal ? parent.height : (handle.active ? Theme.borderWidth * 2 : Theme.borderWidth)
            color: handle.active ? Theme.accent : Theme.border

            Behavior on color {
                ColorAnimation {
                    duration: Theme.durationFast
                }
            }
        }

        Rectangle {
            id: grip

            anchors.centerIn: parent
            visible: control.showGrip
            width: control.horizontal ? Theme.iconSizeSmall + Theme.spacingXs : Theme.iconSizeSmall * 2
            height: control.horizontal ? Theme.iconSizeSmall * 2 : Theme.iconSizeSmall + Theme.spacingXs
            radius: Theme.radiusSmall
            color: Theme.surface2
            border.width: Theme.borderWidth
            border.color: handle.active ? Theme.accent : Theme.borderStrong

            OsIcon {
                anchors.centerIn: parent
                name: control.horizontal ? "grip-vertical" : "grip-horizontal"
                size: Theme.iconSizeSmall
                color: handle.active ? Theme.accentFg : Theme.textMuted
            }
        }

        // A handle only takes focus from the keyboard (Tab), never from a click.
        OsFocusRing {
            target: handle
            baseRadius: Theme.radiusSmall
            visible: handle.activeFocus
        }
    }
}
