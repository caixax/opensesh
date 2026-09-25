// Title-bar session tab strip holding OsTabButton items. Tabs keep their natural width and the
// strip scrolls horizontally when they overflow. Left/Right (mirrored in RTL) move the current
// tab and the focus, Home/End jump to the first/last tab. No background: the title bar paints it.
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

    function moveCurrent(step) {
        const next = currentIndex + (mirrored ? -step : step);
        if (next < 0 || next >= count)
            return;
        setCurrentIndex(next);
        focusCurrent();
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
            setCurrentIndex(0);
            focusCurrent();
            event.accepted = true;
        } else if (event.key === Qt.Key_End) {
            setCurrentIndex(count - 1);
            focusCurrent();
            event.accepted = true;
        }
    }

    contentItem: ListView {
        model: control.contentModel
        currentIndex: control.currentIndex
        spacing: control.spacing
        orientation: ListView.Horizontal
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.AutoFlickIfNeeded
        clip: true
        // Arrow keys are handled by the bar, which also moves the focus.
        keyNavigationEnabled: false
        highlightMoveDuration: Theme.durationNormal
        highlightRangeMode: ListView.ApplyRange
        preferredHighlightBegin: Theme.spacingXxl
        preferredHighlightEnd: width - Theme.spacingXxl
    }

    background: null
}
