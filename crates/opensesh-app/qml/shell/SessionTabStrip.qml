pragma ComponentBehavior: Bound

// Session tab strip: the fixed Home tab, one tab per open session (its title, an activity dot
// for new output in the background, a bell icon after a bell) and a "+" button for a new local
// terminal. It shows the tab state of an AppShell and forwards the user's choices to it.
//   shell: Item   the AppShell (sessionModel, currentTab, tabBarSelected(), closeTab(), newTab())
import QtQuick
import cc.caixa.opensesh

Item {
    id: strip

    required property Item shell
    // Native text of the "new tab" shortcut, for the "+" tooltip.
    property string newTabShortcut: ""

    implicitWidth: tabs.implicitWidth + Theme.spacingXs + newTabButton.implicitWidth
    implicitHeight: Math.max(tabs.implicitHeight, newTabButton.implicitHeight)

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
            text: qsTr("Home")
            iconName: "house"
            closable: false
        }

        Repeater {
            model: strip.shell.sessionModel

            OsTabButton {
                required property int index
                required property string title
                required property bool newOutput
                required property bool bellRang

                text: title.length > 0 ? title : qsTr("Local terminal")
                iconName: bellRang ? "bell" : "square-terminal"
                activity: newOutput || bellRang
                Accessible.description: bellRang ? qsTr("The bell rang") : newOutput ? qsTr("New activity") : ""
                onCloseRequested: strip.shell.closeTab(index + 1)
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
}
