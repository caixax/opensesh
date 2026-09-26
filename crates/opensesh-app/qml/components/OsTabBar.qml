// Title-bar session tab strip holding OsTabButton items. Tabs keep their natural width and the
// strip scrolls horizontally when they overflow (with the wheel, and to keep the current tab in
// view; it doesn't flick, so tabs can be dragged). Left/Right (mirrored in RTL) move the current
// tab and the focus, Home/End jump to the first/last tab; disabled tabs are skipped. No
// background: the title bar paints it.
// Functions: tabAt(index) returns the tab item at `index`, or null.
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.TabBar {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    spacing: Theme.spacingXs
    padding: 0

    Accessible.role: Accessible.PageTabList

    function focusCurrent() {
        if (!currentItem)
            return;
        currentItem.forceActiveFocus(Qt.TabFocusReason);
        // The ListView may have focused the new current tab first (OtherFocusReason), which
        // hides the focus ring; the arrow keys are keyboard navigation.
        currentItem.focusReason = Qt.TabFocusReason;
    }

    // Makes the first enabled tab from `index` in the direction of `step` (+1 or -1) current and
    // focuses it; nothing happens if there is none. A disabled tab can't take the focus, and as
    // the current tab it would leave the bar without a Tab stop.
    function selectEnabled(index, step) {
        for (let i = index; i >= 0 && i < count; i += step) {
            const item = itemAt(i);
            if (item && item.enabled) {
                setCurrentIndex(i);
                focusCurrent();
                return;
            }
        }
    }

    function tabAt(index) {
        return itemAt(index);
    }

    function moveCurrent(step) {
        const direction = mirrored ? -step : step;
        selectEnabled(currentIndex + direction, direction);
    }

    Keys.onLeftPressed: event => {
        control.moveCurrent(-1);
        event.accepted = true;
    }
    Keys.onRightPressed: event => {
        control.moveCurrent(1);
        event.accepted = true;
    }
    Keys.onPressed: event => {
        if (count === 0)
            return;
        if (event.key === Qt.Key_Home) {
            selectEnabled(0, 1);
            event.accepted = true;
        } else if (event.key === Qt.Key_End) {
            selectEnabled(count - 1, -1);
            event.accepted = true;
        }
    }

    contentItem: ListView {
        id: list

        model: control.contentModel
        currentIndex: control.currentIndex
        spacing: control.spacing
        orientation: ListView.Horizontal
        boundsBehavior: Flickable.StopAtBounds
        // Dragging reorders tabs instead of flicking the strip.
        interactive: false
        clip: true
        // Arrow keys are handled by the bar, which also moves the focus.
        keyNavigationEnabled: false
        highlightMoveDuration: Theme.durationNormal
        highlightRangeMode: ListView.ApplyRange
        preferredHighlightBegin: Theme.spacingXxl
        preferredHighlightEnd: width - Theme.spacingXxl

        // Keep the current tab in view when the strip gets narrower (e.g. the window shrinks).
        onWidthChanged: {
            if (currentIndex >= 0)
                positionViewAtIndex(currentIndex, ListView.Contain);
        }

        // The wheel scrolls the tabs sideways when they overflow.
        WheelHandler {
            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
            enabled: list.contentWidth > list.width
            onWheel: event => {
                const delta = event.angleDelta.x !== 0 ? event.angleDelta.x : event.angleDelta.y;
                const pixels = event.pixelDelta.x !== 0 ? event.pixelDelta.x : event.pixelDelta.y;
                const step = pixels !== 0 ? pixels : delta / 120 * Theme.spacingXxl;
                list.contentX = Math.max(list.originX, Math.min(list.originX + list.contentWidth - list.width,
                                                                list.contentX - step));
            }
        }
    }

    background: null
}
