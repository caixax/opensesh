pragma ComponentBehavior: Bound

// Session tab strip: the fixed Home tab (not in a detached window), one tab per terminal tab and
// a "+" button for a new local terminal. A tab shows its name (the one the user gave, else the
// focused terminal's title), its color, a pin when pinned, a radio tower while it broadcasts, an
// activity dot for new output in the background and a bell icon after a bell. It shows the tab
// state of an AppShell and forwards the user's choices to it.
//
// Right click (or the Menu key) opens the tab menu: rename, color, pin, duplicate, move to a new
// or another window, and the close commands. Dragging a tab along the strip reorders it (pinned
// tabs stay first); dropping it outside the window moves it to the window under the pointer, or
// to a new window. A drop elsewhere in its own window changes nothing.
//   shell: Item   the AppShell (sessionModel, currentTab, tabBarSelected(), closeTab(), newTab(),
//                 moveTab(), dropTabOutside() and the other tab functions)
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: strip

    required property Item shell
    // Native text of the "new tab" shortcut, for the "+" tooltip.
    property string newTabShortcut: ""
    // While a tab is dragged: its index, and the index it would move to (0: none).
    property int dragFrom: 0
    property int dropTo: 0
    property bool dragOutside: false

    implicitWidth: tabs.implicitWidth + Theme.spacingXs + newTabButton.implicitWidth
    implicitHeight: Math.max(tabs.implicitHeight, newTabButton.implicitHeight)

    // The tab index a pointer at x (strip coordinates) would drop at, for a tab from `from`.
    function dropIndexAt(x, from) {
        let slot = strip.shell.sessionCount + 1;
        for (let i = 1; i <= strip.shell.sessionCount; ++i) {
            const item = tabs.tabAt(i);
            if (!item)
                continue;
            const left = item.mapToItem(strip, 0, 0).x;
            if (x < left + item.width / 2) {
                slot = i;
                break;
            }
        }
        // A slot after the dragged tab is one less once it is taken out.
        return slot > from ? slot - 1 : slot;
    }

    function isOutside(point) {
        const margin = Theme.spacingXl;
        return point.y < -margin || point.y > strip.height + margin || point.x < -margin || point.x > strip.width + margin;
    }

    function inWindow(scenePoint) {
        const window = strip.Window.window;
        return window !== null && scenePoint.x >= 0 && scenePoint.y >= 0 && scenePoint.x < window.width
               && scenePoint.y < window.height;
    }

    function dragMoved(index, scenePoint) {
        const point = strip.mapFromItem(null, scenePoint.x, scenePoint.y);
        dragFrom = index;
        dragOutside = isOutside(point);
        dropTo = dragOutside ? 0 : dropIndexAt(point.x, index);
        ghost.x = scenePoint.x + Theme.spacingSm;
        ghost.y = scenePoint.y + Theme.spacingSm;
    }

    function dragEnded(index, scenePoint) {
        const point = strip.mapFromItem(null, scenePoint.x, scenePoint.y);
        const outside = isOutside(point);
        const target = dropIndexAt(point.x, index);
        dragFrom = 0;
        dropTo = 0;
        dragOutside = false;
        if (!outside) {
            strip.shell.moveTab(index, target);
            return;
        }
        if (inWindow(scenePoint))
            return;
        strip.shell.dropTabOutside(index, strip.mapToGlobal(point.x, point.y));
    }

    // Removing a tab moves the bar's own current index; re-read the shell's even when that
    // value didn't change (so the binding alone wouldn't fire).
    Connections {
        target: strip.shell

        function onTabsUpdated() {
            tabs.setCurrentIndex(strip.shell.currentTab);
        }
    }

    OsTabBar {
        id: tabs

        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(0, Math.min(implicitWidth, strip.width - Theme.spacingXs - newTabButton.width))
        currentIndex: strip.shell.currentTab
        Accessible.name: qsTr("Tabs")

        onCurrentIndexChanged: strip.shell.tabBarSelected(currentIndex)

        OsTabButton {
            // A detached window has no Home.
            visible: !strip.shell.detached
            enabled: !strip.shell.detached
            implicitWidth: visible ? implicitContentWidth + leftPadding + rightPadding : 0
            text: qsTr("Home")
            iconName: "house"
            closable: false
        }

        Repeater {
            model: strip.shell.sessionModel

            OsTabButton {
                id: tabButton

                required property int index
                required property string title
                required property string customTitle
                required property string color
                required property bool pinned
                required property bool broadcasting
                required property bool newOutput
                required property bool bellRang
                readonly property int colorIndex: Theme.tabColorNames.indexOf(color)
                readonly property int tabIndex: index + 1

                text: customTitle.length > 0 ? customTitle : title.length > 0 ? title : qsTr("Local terminal")
                iconName: bellRang ? "bell" : broadcasting ? "radio-tower" : pinned ? "pin" : "square-terminal"
                markColor: colorIndex >= 0 ? Theme.tabColors[colorIndex] : "transparent"
                // Pinned tabs close from their menu or the shortcut only.
                closable: !pinned
                activity: newOutput || bellRang
                opacity: strip.dragFrom === tabIndex ? 0.5 : 1
                Accessible.description: [
                    pinned ? qsTr("Pinned") : "",
                    broadcasting ? qsTr("Broadcasting input") : "",
                    bellRang ? qsTr("The bell rang") : newOutput ? qsTr("New activity") : ""
                ].filter(part => part.length > 0).join(", ")

                onCloseRequested: strip.shell.closeTab(tabIndex)

                TapHandler {
                    acceptedButtons: Qt.RightButton
                    onTapped: (eventPoint, button) => tabMenu.popupFor(tabButton.tabIndex, tabButton,
                                                                      eventPoint.position.x, eventPoint.position.y)
                }

                Keys.onMenuPressed: tabMenu.popupFor(tabButton.tabIndex, tabButton, 0, tabButton.height)

                DragHandler {
                    id: dragHandler

                    target: null
                    // Keeps the drag once it started (the title bar would move the window).
                    grabPermissions: PointerHandler.CanTakeOverFromAnything

                    onActiveChanged: {
                        if (active)
                            strip.dragMoved(tabButton.tabIndex, centroid.scenePosition);
                        else
                            strip.dragEnded(tabButton.tabIndex, centroid.scenePosition);
                    }
                    onCentroidChanged: {
                        if (active)
                            strip.dragMoved(tabButton.tabIndex, centroid.scenePosition);
                    }
                }
            }
        }
    }

    OsIconButton {
        id: newTabButton

        anchors.left: tabs.right
        anchors.leftMargin: Theme.spacingXs
        anchors.verticalCenter: parent.verticalCenter
        implicitWidth: Theme.controlHeightSmall
        implicitHeight: Theme.controlHeightSmall
        iconName: "plus"
        toolTip: strip.newTabShortcut.length > 0 ? qsTr("New tab (%1)").arg(strip.newTabShortcut) : qsTr("New tab")
        onClicked: strip.shell.newTab()
    }

    // Where a dragged tab would land.
    Rectangle {
        readonly property Item before: strip.dropTo > 0 ? tabs.tabAt(strip.dropTo >= strip.dragFrom ? strip.dropTo + 1 : strip.dropTo) : null
        readonly property Item last: tabs.tabAt(strip.shell.sessionCount)

        visible: strip.dragFrom > 0 && strip.dropTo > 0 && strip.dropTo !== strip.dragFrom
        x: before ? before.mapToItem(strip, 0, 0).x - width - 1 : last ? last.mapToItem(strip, last.width, 0).x + 1 : 0
        y: (strip.height - height) / 2
        width: Theme.borderWidth * 2
        height: strip.height - Theme.spacingSm
        radius: width / 2
        color: Theme.accent
    }

    // Follows the pointer while a tab is dragged out of the strip.
    Rectangle {
        id: ghost

        parent: T.Overlay.overlay
        visible: strip.dragFrom > 0 && strip.dragOutside
        z: 10000
        width: ghostRow.implicitWidth + 2 * Theme.spacingMd
        height: Theme.controlHeightSmall
        radius: Theme.radiusControl
        color: Theme.surface
        border.width: Theme.borderWidth
        border.color: Theme.borderStrong

        Row {
            id: ghostRow

            anchors.centerIn: parent
            spacing: Theme.spacingSm

            OsIcon {
                anchors.verticalCenter: parent.verticalCenter
                name: "app-window"
                size: Theme.iconSizeSmall
                color: Theme.textMuted
            }

            OsText {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Drop outside this window to move the tab to another one")
                size: "small"
            }
        }
    }

    OsContextMenu {
        id: tabMenu

        property int index: 0
        readonly property var row: index > 0 && index <= strip.shell.sessionCount ? strip.shell.sessionModel.get(index - 1) : null
        readonly property var otherShells: WindowRegistry.shells.filter(shell => shell !== strip.shell)

        function popupFor(index, item, x, y) {
            tabMenu.index = index;
            popup(item, x, y);
        }

        OsMenuItem {
            text: qsTr("Rename…")
            iconName: "pencil"
            onTriggered: strip.shell.askRenameTab(tabMenu.index)
        }

        OsContextMenu {
            id: colorMenu

            title: qsTr("Color")

            OsMenuItem {
                text: qsTr("None")
                checkable: true
                checked: tabMenu.row !== null && tabMenu.row.color.length === 0
                onTriggered: strip.shell.setTabColor(tabMenu.index, "")
            }

            Instantiator {
                model: Theme.tabColorNames

                delegate: OsMenuItem {
                    id: colorItem

                    required property string modelData
                    required property int index

                    text: {
                        switch (modelData) {
                        case "red":
                            return qsTr("Red");
                        case "orange":
                            return qsTr("Orange");
                        case "yellow":
                            return qsTr("Yellow");
                        case "green":
                            return qsTr("Green");
                        case "teal":
                            return qsTr("Teal");
                        case "blue":
                            return qsTr("Blue");
                        case "purple":
                            return qsTr("Purple");
                        case "pink":
                            return qsTr("Pink");
                        default:
                            return modelData;
                        }
                    }
                    checkable: true
                    checked: tabMenu.row !== null && tabMenu.row.color === modelData
                    onTriggered: strip.shell.setTabColor(tabMenu.index, modelData)

                    Rectangle {
                        anchors.right: parent.right
                        anchors.rightMargin: Theme.spacingMd
                        anchors.verticalCenter: parent.verticalCenter
                        width: Theme.spacingSm * 1.5
                        height: width
                        radius: width / 2
                        color: Theme.tabColors[colorItem.index] ?? Theme.border
                    }
                }

                onObjectAdded: (index, object) => colorMenu.insertItem(index + 1, object)
                onObjectRemoved: (index, object) => colorMenu.removeItem(object)
            }
        }

        OsMenuItem {
            text: tabMenu.row !== null && tabMenu.row.pinned ? qsTr("Unpin") : qsTr("Pin")
            iconName: tabMenu.row !== null && tabMenu.row.pinned ? "pin-off" : "pin"
            onTriggered: strip.shell.setTabPinned(tabMenu.index, !(tabMenu.row !== null && tabMenu.row.pinned))
        }

        OsMenuItem {
            text: qsTr("Duplicate")
            iconName: "copy"
            shortcutText: strip.shell.shortcutText("tab.duplicate")
            onTriggered: strip.shell.duplicateTab(tabMenu.index)
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Move to a new window")
            iconName: "app-window"
            enabled: !strip.shell.detached || strip.shell.sessionCount > 1
            onTriggered: strip.shell.moveTabToNewWindow(tabMenu.index)
        }

        OsContextMenu {
            id: windowMenu

            title: qsTr("Move to window")
            enabled: tabMenu.otherShells.length > 0

            Instantiator {
                model: tabMenu.otherShells

                delegate: OsMenuItem {
                    required property var modelData

                    text: WindowRegistry.windowName(modelData)
                    onTriggered: strip.shell.moveTabToShell(tabMenu.index, modelData)
                }

                onObjectAdded: (index, object) => windowMenu.insertItem(index, object)
                onObjectRemoved: (index, object) => windowMenu.removeItem(object)
            }
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Close")
            iconName: "x"
            onTriggered: strip.shell.closeTab(tabMenu.index)
        }

        OsMenuItem {
            text: qsTr("Close other tabs")
            enabled: strip.shell.sessionCount > 1
            onTriggered: strip.shell.closeOtherTabs(tabMenu.index, "others")
        }

        OsMenuItem {
            text: qsTr("Close tabs to the left")
            enabled: tabMenu.index > 1
            onTriggered: strip.shell.closeOtherTabs(tabMenu.index, "left")
        }

        OsMenuItem {
            text: qsTr("Close tabs to the right")
            enabled: tabMenu.index < strip.shell.sessionCount
            onTriggered: strip.shell.closeOtherTabs(tabMenu.index, "right")
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Reopen closed tab")
            iconName: "rotate-ccw"
            shortcutText: strip.shell.shortcutText("tab.reopenClosed")
            enabled: WindowRegistry.closedTabs.length > 0
            onTriggered: strip.shell.reopenClosedTab()
        }
    }
}
