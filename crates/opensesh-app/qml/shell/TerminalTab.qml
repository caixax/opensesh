pragma ComponentBehavior: Bound

// Content of one local terminal tab: the TerminalItem attached to the tab's session, with the
// tab's profile (PLAN §6.2) and font zoom, the profile's background image, a thin scroll bar,
// the search bar (Ctrl+Shift+F), the context menu (right click or the Menu key), the bell in the
// profile's style and a banner when the shell ends with an error. A shell that exits with code 0
// closes its tab. Every tab stays alive while hidden, so its shell keeps running.
//   shell: Item            the AppShell (sessionModel, currentTab, closeTabById(), updateTab(),
//                          shortcutText(), focusInTabStrip(), currentTerminal)
//   tabId: int             the tab's id, which is also its session id
//   index: int             the tab's row in shell.sessionModel
//   profile: string        the tab's profile id (model role)
//   startSession: bool     false: no shell (screenshot runs keep their tab titles stable)
//   edgeInset: real        room kept free at the right edge (a frameless window's resize grip)
//   terminal: TerminalItem read-only
//   current: bool          read-only; this is the tab shown
//   fontZoom: real         points added to the profile's font size (Ctrl+= / Ctrl+- / Ctrl+0)
//   highlightOn: bool      keyword highlighting in this tab
// Functions: focusTerminal(), openSearch(), closeSearch(), findNext(forward), copy(), paste(),
// selectAll(), clearScrollback(), restart(), closeTab(), zoom(step) (0 resets),
// toggleHighlight(), useProfile(id).
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: tab

    required property Item shell
    required property int tabId
    required property int index
    required property string profile
    required property bool startSession
    property real edgeInset: 0
    property real fontZoom: 0
    property bool highlightOn: true
    readonly property var profileList: JSON.parse(TerminalProfiles.profiles || "[]")

    readonly property alias terminal: terminal
    readonly property bool current: shell.currentTabId !== 0 && shell.currentTabId === tabId
    property bool searchOpen: false
    // "none", "nomatch" or "error".
    property string searchState: "none"
    // The shell ended with an error (a code other than 0, or killed, or it didn't start).
    property bool failed: false

    function focusTerminal() {
        terminal.forceActiveFocus(Qt.OtherFocusReason);
    }

    function closeTab() {
        shell.closeTabById(tabId);
    }

    function restart() {
        failed = false;
        if (terminal.restart())
            focusTerminal();
        else
            failed = true;
    }

    function openSearch() {
        searchOpen = true;
        searchField.forceActiveFocus(Qt.ShortcutFocusReason);
        searchField.selectAll();
    }

    function closeSearch() {
        searchTimer.stop();
        searchOpen = false;
        searchState = "none";
        terminal.clearSearch();
        focusTerminal();
    }

    // forward: toward newer output (down); Enter searches up, toward older output.
    function findNext(forward) {
        searchTimer.stop();
        const pattern = searchField.text;
        if (pattern.length === 0) {
            terminal.clearSearch();
            searchState = "none";
            return;
        }
        const found = terminal.find(pattern, forward);
        searchState = terminal.searchError.length > 0 ? "error" : found ? "none" : "nomatch";
    }

    // Copy and paste act on the search field while it has the focus.
    function copy() {
        if (searchField.activeFocus)
            searchField.copy();
        else
            terminal.copy();
    }

    function paste() {
        if (searchField.activeFocus)
            searchField.paste();
        else
            terminal.paste();
    }

    function selectAll() {
        if (searchField.activeFocus)
            searchField.selectAll();
        else
            terminal.selectAll();
    }

    function clearScrollback() {
        terminal.clearScrollback();
    }

    // step: +1 bigger, -1 smaller, 0 back to the profile's size.
    function zoom(step) {
        fontZoom = step === 0 ? 0 : Math.max(-20, Math.min(40, fontZoom + step));
    }

    function toggleHighlight() {
        highlightOn = !highlightOn;
    }

    function useProfile(id) {
        shell.updateTab(tabId, "profile", id);
    }

    function ringBell() {
        const style = terminal.bellStyle;
        if (style === "none")
            return;
        if (!tab.current) {
            tab.shell.updateTab(tab.tabId, "bellRang", true);
            return;
        }
        const window = tab.Window.window;
        if (style === "sound" && Platform.beep())
            return;
        if (style === "notification" && window && !window.active) {
            window.alert(0);
            Toasts.show(qsTr("The bell rang in %1.").arg(terminal.title.length > 0 ? terminal.title : qsTr("a terminal")), "info");
            return;
        }
        if (!Theme.reduceMotion)
            bellAnimation.restart();
    }

    // A Windows status such as 0xC000013A reads better in hexadecimal.
    function exitCodeText(code) {
        return code < 0 || code > 255 ? "0x" + (code >>> 0).toString(16).toUpperCase() : String(code);
    }

    function becameCurrent() {
        shell.updateTab(tabId, "newOutput", false);
        shell.updateTab(tabId, "bellRang", false);
        shell.currentTerminal = tab;
        // Keyboard users moving along the tab strip keep their place there.
        if (!shell.focusInTabStrip())
            Qt.callLater(() => {
                if (tab.current)
                    tab.focusTerminal();
            });
    }

    visible: current

    onCurrentChanged: {
        if (current)
            becameCurrent();
        else if (shell.currentTerminal === tab)
            shell.currentTerminal = null;
    }
    Component.onCompleted: {
        if (current)
            becameCurrent();
    }
    Component.onDestruction: {
        if (shell.currentTerminal === tab)
            shell.currentTerminal = null;
    }

    // The profile's background image, under the terminal, dimmed with the theme's background.
    Item {
        anchors.fill: terminal
        visible: terminal.backgroundImage.length > 0

        Image {
            anchors.fill: parent
            source: terminal.backgroundImage
            asynchronous: true
            cache: false
            fillMode: {
                switch (terminal.backgroundImageFit) {
                case "contain":
                    return Image.PreserveAspectFit;
                case "stretch":
                    return Image.Stretch;
                case "tile":
                    return Image.Tile;
                case "center":
                    return Image.Pad;
                default:
                    return Image.PreserveAspectCrop;
                }
            }
            horizontalAlignment: Image.AlignHCenter
            verticalAlignment: Image.AlignVCenter
            clip: true
        }

        Rectangle {
            anchors.fill: parent
            color: terminal.backgroundColor.length > 0 ? terminal.backgroundColor : Theme.bg
            opacity: terminal.backgroundImageDim
        }
    }

    TerminalItem {
        id: terminal

        anchors.fill: parent
        sessionId: tab.startSession ? tab.tabId : 0
        dark: Theme.dark
        profileId: tab.profile
        settingsRevision: TerminalProfiles.revision
        fontZoom: tab.fontZoom
        highlightEnabled: tab.highlightOn
        reduceMotion: Theme.reduceMotion
        Accessible.role: Accessible.Terminal
        Accessible.name: title.length > 0 ? title : qsTr("Local terminal")

        onTitleChanged: tab.shell.updateTab(tab.tabId, "title", title)
        onActivity: {
            if (!tab.current)
                tab.shell.updateTab(tab.tabId, "newOutput", true);
        }
        onBell: tab.ringBell()
        onClipboardSet: Toasts.show(qsTr("A program in this terminal copied text to the clipboard."), "info")
        onExited: code => {
            if (exitCodeKnown && code === 0) {
                // Not from inside this signal: closing the tab destroys this item.
                Qt.callLater(tab.closeTab);
                return;
            }
            const hadFocus = activeFocus;
            tab.failed = true;
            // The keyboard user lands on Restart, with its focus ring.
            if (hadFocus)
                Qt.callLater(() => restartButton.forceActiveFocus(Qt.TabFocusReason));
        }
        onRunningChanged: {
            if (running)
                tab.failed = false;
        }
        onContextMenuRequested: (x, y) => contextMenu.popup(terminal, x, y)
    }

    // The visual bell: a short flash over the terminal (none with reduce motion).
    Rectangle {
        id: bellFlash

        anchors.fill: terminal
        color: Theme.hover
        opacity: 0
        visible: opacity > 0

        SequentialAnimation {
            id: bellAnimation

            NumberAnimation {
                target: bellFlash
                property: "opacity"
                to: 1
                duration: Theme.durationFast
            }
            NumberAnimation {
                target: bellFlash
                property: "opacity"
                to: 0
                duration: Theme.durationNormal
            }
        }
    }

    // Thin scroll bar over the right padding, bound to the terminal's history.
    OsScrollBar {
        id: scrollBar

        readonly property int total: terminal.historySize + terminal.lines

        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        anchors.rightMargin: tab.edgeInset
        orientation: Qt.Vertical
        policy: terminal.historySize > 0 ? T.ScrollBar.AsNeeded : T.ScrollBar.AlwaysOff
        size: total > 0 ? terminal.lines / total : 1
        active: hovered || pressed || scrollActivity.running
        focusPolicy: Qt.NoFocus
        Accessible.name: qsTr("Scrollback")

        onPositionChanged: {
            if (pressed)
                terminal.scrollTo(Math.round(terminal.historySize - position * total));
        }

        Binding on position {
            when: !scrollBar.pressed
            value: scrollBar.total > 0 ? (terminal.historySize - terminal.displayOffset) / scrollBar.total : 0
            restoreMode: Binding.RestoreNone
        }

        // Shows the bar for a moment after the view scrolled.
        Timer {
            id: scrollActivity

            interval: 1000
        }

        Connections {
            target: terminal

            function onViewChanged() {
                if (terminal.historySize > 0)
                    scrollActivity.restart();
            }
        }
    }

    // Search bar (Ctrl+Shift+F): Enter searches up, Shift+Enter down, Escape closes it.
    Rectangle {
        id: searchBar

        anchors.top: parent.top
        anchors.right: parent.right
        anchors.topMargin: Theme.spacingSm
        anchors.rightMargin: Theme.spacingLg
        width: searchRow.implicitWidth + 2 * Theme.spacingXs
        height: searchRow.implicitHeight + 2 * Theme.spacingXs
        visible: tab.searchOpen
        radius: Theme.radiusControl
        color: Theme.surface
        border.width: Theme.borderWidth
        border.color: Theme.borderStrong

        Accessible.role: Accessible.Grouping
        Accessible.name: qsTr("Find in terminal")

        onVisibleChanged: {
            if (!visible && searchField.activeFocus)
                tab.focusTerminal();
        }

        Row {
            id: searchRow

            anchors.centerIn: parent
            spacing: Theme.spacingXs

            OsSearchField {
                id: searchField

                anchors.verticalCenter: parent.verticalCenter
                width: Theme.spacingXxl * 8
                placeholderText: qsTr("Find (regular expression)")
                error: tab.searchState === "error"
                Accessible.description: tab.searchState === "error"
                                        ? qsTr("Invalid regular expression: %1").arg(terminal.searchError)
                                        : qsTr("Enter searches up, Shift+Enter searches down, Escape closes.")

                onTextChanged: {
                    if (tab.searchOpen)
                        searchTimer.restart();
                }
                // Escape closes the bar at once, even with text in the field.
                Keys.onShortcutOverride: event => {
                    if (event.key === Qt.Key_Escape)
                        event.accepted = true;
                }
                Keys.onEscapePressed: event => {
                    tab.closeSearch();
                    event.accepted = true;
                }
                Keys.onReturnPressed: event => {
                    tab.findNext((event.modifiers & Qt.ShiftModifier) !== 0);
                    event.accepted = true;
                }
                Keys.onEnterPressed: event => {
                    tab.findNext((event.modifiers & Qt.ShiftModifier) !== 0);
                    event.accepted = true;
                }
            }

            OsText {
                anchors.verticalCenter: parent.verticalCenter
                visible: tab.searchState !== "none"
                text: tab.searchState === "error" ? qsTr("Invalid pattern") : qsTr("No match")
                size: "small"
                muted: true
                Accessible.role: Accessible.StaticText
                Accessible.name: text
            }

            OsIconButton {
                anchors.verticalCenter: parent.verticalCenter
                implicitWidth: Theme.controlHeightSmall
                implicitHeight: Theme.controlHeightSmall
                iconName: "chevron-up"
                toolTip: qsTr("Search up (Enter)")
                onClicked: tab.findNext(false)
            }

            OsIconButton {
                anchors.verticalCenter: parent.verticalCenter
                implicitWidth: Theme.controlHeightSmall
                implicitHeight: Theme.controlHeightSmall
                iconName: "chevron-down"
                toolTip: qsTr("Search down (Shift+Enter)")
                onClicked: tab.findNext(true)
            }

            OsIconButton {
                anchors.verticalCenter: parent.verticalCenter
                implicitWidth: Theme.controlHeightSmall
                implicitHeight: Theme.controlHeightSmall
                iconName: "x"
                toolTip: qsTr("Close search (Escape)")
                onClicked: tab.closeSearch()
            }
        }

        // Search as you type, once typing pauses (a search step can scan the whole history).
        Timer {
            id: searchTimer

            interval: 200
            onTriggered: tab.findNext(false)
        }
    }

    // The shell ended with an error: say so, and offer to restart it or close the tab.
    Rectangle {
        id: exitBanner

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: Theme.spacingLg
        height: bannerRow.implicitHeight + 2 * Theme.spacingMd
        visible: tab.failed && !terminal.running
        radius: Theme.radiusCard
        color: Theme.surface
        border.width: Theme.borderWidth
        border.color: Theme.danger

        Accessible.role: Accessible.AlertMessage
        Accessible.name: bannerText.text

        RowLayout {
            id: bannerRow

            anchors.fill: parent
            anchors.leftMargin: Theme.spacingLg
            anchors.rightMargin: Theme.spacingMd
            spacing: Theme.spacingMd

            OsIcon {
                name: "circle-alert"
                color: Theme.danger
                size: Theme.iconSize
            }

            OsText {
                id: bannerText

                Layout.fillWidth: true
                text: terminal.exitCodeKnown
                      ? qsTr("The shell exited with code %1.").arg(tab.exitCodeText(terminal.exitCode))
                      : qsTr("The shell was ended or could not start.")
                wrapMode: Text.WordWrap
            }

            OsButton {
                id: restartButton

                text: qsTr("Restart")
                iconName: "refresh-cw"
                variant: "primary"
                onClicked: tab.restart()
                Keys.onReturnPressed: tab.restart()
                Keys.onEnterPressed: tab.restart()
            }

            OsButton {
                text: qsTr("Close tab")
                iconName: "x"
                onClicked: tab.closeTab()
                Keys.onReturnPressed: tab.closeTab()
                Keys.onEnterPressed: tab.closeTab()
            }
        }
    }

    OsContextMenu {
        id: contextMenu

        OsMenuItem {
            text: qsTr("Copy")
            iconName: "copy"
            shortcutText: tab.shell.shortcutText("terminal.copy")
            enabled: terminal.hasSelection
            onTriggered: terminal.copy()
        }

        OsMenuItem {
            text: qsTr("Paste")
            shortcutText: tab.shell.shortcutText("terminal.paste")
            enabled: terminal.running
            onTriggered: terminal.paste()
        }

        OsMenuItem {
            text: qsTr("Select all")
            onTriggered: terminal.selectAll()
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Find…")
            iconName: "search"
            shortcutText: tab.shell.shortcutText("terminal.find")
            onTriggered: tab.openSearch()
        }

        OsMenuItem {
            text: qsTr("Clear scrollback")
            iconName: "trash-2"
            onTriggered: terminal.clearScrollback()
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Highlight keywords")
            checkable: true
            checked: tab.highlightOn
            onTriggered: tab.toggleHighlight()
        }

        OsContextMenu {
            id: profileMenu

            title: qsTr("Profile")

            Instantiator {
                model: tab.profileList

                delegate: OsMenuItem {
                    required property var modelData

                    text: modelData.name
                    checkable: true
                    checked: modelData.id === tab.profile
                    onTriggered: tab.useProfile(modelData.id)
                }

                onObjectAdded: (index, object) => profileMenu.insertItem(index, object)
                onObjectRemoved: (index, object) => profileMenu.removeItem(object)
            }
        }

        OsMenuItem {
            text: qsTr("Terminal settings…")
            iconName: "sliders-horizontal"
            onTriggered: tab.shell.openSettings("terminal")
        }
    }
}
