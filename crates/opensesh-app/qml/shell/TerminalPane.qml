pragma ComponentBehavior: Bound

// One pane of a tab (TabWorkspace): the TerminalItem attached to the pane's session, with the
// pane's profile (PLAN §6.2) and font zoom, the profile's background image, a thin scroll bar,
// the search bar (Ctrl+Shift+F), the context menu (right click or the Menu key), the bell in the
// profile's style and a banner when the shell ends with an error. A shell that exits with code 0
// closes its pane. Panes stay alive while hidden (another tab, or another pane maximized), so
// their shells keep running.
//
// Broadcast (MultiExec): while the tab broadcasts, what is typed or pasted in a pane that
// receives broadcast input also goes to the other receiving panes. Those panes, and only those,
// get a border in Theme.danger; a chip in the top right corner (under the search bar while it is
// open) shows whether the pane receives and turns it on or off.
//   workspace: Item        the TabWorkspace (shell, participants, pasteConfirmed, closePane(),
//                          setFocusedPane(), paneActivity(), paneBell(), confirmPaste())
//   paneId: int            the pane's id, which is also its session id
//   kind: string           `local` or `ssh` (model role)
//   host: string           the saved host it connects to, if any (model role)
//   target: string         the quick-connect target it connects to, if any (model role)
//   commandJson: string    the program and arguments to run instead of a shell, as a JSON list
//   label: string          what it connects to, for titles (the host's name or the target)
//   profile: string        the pane's profile id (model role; empty lets a host decide)
//   directory: string      where a new shell starts (model role; empty for home)
//   startSession: bool     false: no shell, the renderer's demo frame instead (screenshot runs)
//   edgeInset: real        room kept free at the right edge (a frameless window's resize grip)
//   terminal: TerminalItem read-only
//   focused: bool          read-only; the tab's focused pane
//   receiving: bool        read-only; input typed here also reaches other panes
//   fontZoom: real         points added to the profile's font size (Ctrl+= / Ctrl+- / Ctrl+0)
//   highlightOn: bool      keyword highlighting in this pane
// Functions: focusTerminal(), openSearch(), closeSearch(), findNext(forward), copy(), paste(),
// selectAll(), clearScrollback(), restart(), closePane(), zoom(step) (0 resets),
// toggleHighlight(), useProfile(id), currentDirectory().
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: pane

    required property Item workspace
    required property int paneId
    required property string kind
    required property string host
    required property string target
    required property string commandJson
    required property string label
    required property string profile
    required property string directory
    required property bool startSession
    property real edgeInset: 0
    property real fontZoom: 0
    property bool highlightOn: true
    readonly property var profileList: JSON.parse(TerminalProfiles.profiles || "[]")

    readonly property alias terminal: terminal
    readonly property Item shell: workspace.shell
    readonly property bool focused: workspace.focusedPane === paneId
    readonly property bool participant: workspace.broadcast && workspace.broadcastExcluded.indexOf(paneId) < 0
    readonly property bool receiving: participant && workspace.participants.length > 1
    property bool searchOpen: false
    // "none", "nomatch" or "error".
    property string searchState: "none"
    // The shell ended with an error (a code other than 0, or killed, or it didn't start).
    property bool failed: false

    function focusTerminal() {
        terminal.forceActiveFocus(Qt.OtherFocusReason);
    }

    function closePane() {
        workspace.closePane(paneId);
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
        workspace.setPaneProfile(paneId, id);
    }

    // Where a copy of this pane should start: the shell's directory (OSC 7) or the start one.
    function currentDirectory() {
        return terminal.workingDirectory.length > 0 ? terminal.workingDirectory : directory;
    }

    function ringBell() {
        const style = terminal.bellStyle;
        if (style === "none")
            return;
        if (!workspace.current) {
            workspace.paneBell(paneId);
            return;
        }
        const window = pane.Window.window;
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

    Component.onCompleted: {
        if (host.length > 0)
            WindowRegistry.hostOpened(host);
    }
    Component.onDestruction: {
        if (host.length > 0)
            WindowRegistry.hostClosed(host);
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
        sessionId: pane.startSession ? pane.paneId : 0
        hostId: pane.host
        command: JSON.parse(pane.commandJson || "[]")
        demo: !pane.startSession
        demoDark: Theme.dark
        dark: Theme.dark
        profileId: pane.profile
        settingsRevision: TerminalProfiles.revision + Hosts.revision
        fontZoom: pane.fontZoom
        highlightEnabled: pane.highlightOn
        reduceMotion: Theme.reduceMotion
        startDirectory: pane.directory
        // Only the receiving panes, and only while more than one receives.
        broadcastTargets: pane.receiving ? pane.workspace.participants : []
        scrollSyncTargets: pane.receiving && pane.workspace.syncScroll ? pane.workspace.participants : []
        pasteGuard: pane.receiving && !pane.workspace.pasteConfirmed
        Accessible.role: Accessible.Terminal
        Accessible.name: title.length > 0 ? title : pane.label.length > 0 ? pane.label : qsTr("Local terminal")
        Accessible.description: pane.receiving ? qsTr("Broadcasting input to %n panes", "", pane.workspace.participants.length) : ""

        onActivity: {
            if (!pane.workspace.current)
                pane.workspace.paneActivity(pane.paneId);
        }
        onBell: pane.ringBell()
        onClipboardSet: Toasts.show(qsTr("A program in this terminal copied text to the clipboard."), "info")
        onPasteConfirmationNeeded: selection => pane.workspace.confirmPaste(pane.paneId, selection)
        onActiveFocusChanged: {
            if (activeFocus)
                pane.workspace.setFocusedPane(pane.paneId);
        }
        onExited: code => {
            if (exitCodeKnown && code === 0) {
                // Not from inside this signal: closing the pane destroys this item.
                Qt.callLater(pane.closePane);
                return;
            }
            const hadFocus = activeFocus;
            pane.failed = true;
            // The keyboard user lands on Restart, with its focus ring.
            if (hadFocus)
                Qt.callLater(() => restartButton.forceActiveFocus(Qt.TabFocusReason));
        }
        onRunningChanged: {
            if (running)
                pane.failed = false;
        }
        onContextMenuRequested: (x, y) => contextMenu.popup(terminal, x, y)
    }

    // The demo frame skips the profile (screenshot runs): it needs its own font, as in the gallery.
    Binding {
        target: terminal
        property: "fontFamily"
        when: !pane.startSession
        value: Theme.monoFontFamily
    }

    Binding {
        target: terminal
        property: "fontPointSize"
        when: !pane.startSession
        value: 10
    }

    Binding {
        target: terminal
        property: "padding"
        when: !pane.startSession
        value: Theme.spacingSm
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
        anchors.rightMargin: pane.edgeInset
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

    // Which pane has the focus, when the tab shows several; the broadcast border wins.
    Rectangle {
        anchors.fill: parent
        visible: pane.receiving || (pane.focused && pane.workspace.paneCount > 1 && pane.workspace.zoomedPane === 0)
        color: "transparent"
        border.width: pane.receiving ? Theme.borderWidth * 2 : Theme.borderWidth
        border.color: pane.receiving ? Theme.danger : Theme.accent
        z: 2
    }

    // While the tab broadcasts: whether this pane receives, and the switch to change it.
    T.AbstractButton {
        id: broadcastChip

        anchors.top: pane.searchOpen ? searchBar.bottom : parent.top
        anchors.right: parent.right
        anchors.topMargin: Theme.spacingSm
        anchors.rightMargin: Theme.spacingLg + pane.edgeInset
        visible: pane.workspace.broadcast
        z: 3
        implicitWidth: chipRow.implicitWidth + 2 * Theme.spacingSm
        implicitHeight: Theme.controlHeightSmall - Theme.spacingXs
        focusPolicy: Qt.NoFocus
        hoverEnabled: true
        checkable: true
        checked: pane.participant
        Accessible.role: Accessible.CheckBox
        Accessible.name: qsTr("Receive broadcast input")
        Accessible.checked: checked

        onToggled: pane.workspace.setPaneReceiving(pane.paneId, checked)

        background: Rectangle {
            radius: height / 2
            color: broadcastChip.checked ? Theme.danger : Theme.surface2
            border.width: broadcastChip.checked ? 0 : Theme.borderWidth
            border.color: Theme.borderStrong

            Rectangle {
                anchors.fill: parent
                radius: parent.radius
                color: broadcastChip.down ? Theme.pressed : broadcastChip.hovered ? Theme.hover : "transparent"
            }
        }

        contentItem: Item {
            Row {
                id: chipRow

                anchors.centerIn: parent
                spacing: Theme.spacingXs

                OsIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "radio-tower"
                    size: Theme.iconSizeSmall
                    color: broadcastChip.checked ? Theme.bg : Theme.textMuted
                }

                OsText {
                    anchors.verticalCenter: parent.verticalCenter
                    text: broadcastChip.checked ? qsTr("Receiving") : qsTr("Not receiving")
                    size: "small"
                    color: broadcastChip.checked ? Theme.bg : Theme.textMuted
                }
            }
        }

        OsTooltip {
            visible: broadcastChip.hovered
            text: broadcastChip.checked ? qsTr("This pane receives what is typed in the other receiving panes. Click to leave.")
                                        : qsTr("This pane only gets its own input. Click to receive broadcast input.")
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
        visible: pane.searchOpen
        z: 3
        radius: Theme.radiusControl
        color: Theme.surface
        border.width: Theme.borderWidth
        border.color: Theme.borderStrong

        Accessible.role: Accessible.Grouping
        Accessible.name: qsTr("Find in terminal")

        onVisibleChanged: {
            if (!visible && searchField.activeFocus)
                pane.focusTerminal();
        }

        Row {
            id: searchRow

            anchors.centerIn: parent
            spacing: Theme.spacingXs

            OsSearchField {
                id: searchField

                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(Theme.spacingXxl * 8, Math.max(Theme.spacingXxl * 3, pane.width - Theme.spacingXxl * 4))
                placeholderText: qsTr("Find (regular expression)")
                error: pane.searchState === "error"
                Accessible.description: pane.searchState === "error"
                                        ? qsTr("Invalid regular expression: %1").arg(terminal.searchError)
                                        : qsTr("Enter searches up, Shift+Enter searches down, Escape closes.")

                onTextChanged: {
                    if (pane.searchOpen)
                        searchTimer.restart();
                }
                // Escape closes the bar at once, even with text in the field.
                Keys.onShortcutOverride: event => {
                    if (event.key === Qt.Key_Escape)
                        event.accepted = true;
                }
                Keys.onEscapePressed: event => {
                    pane.closeSearch();
                    event.accepted = true;
                }
                Keys.onReturnPressed: event => {
                    pane.findNext((event.modifiers & Qt.ShiftModifier) !== 0);
                    event.accepted = true;
                }
                Keys.onEnterPressed: event => {
                    pane.findNext((event.modifiers & Qt.ShiftModifier) !== 0);
                    event.accepted = true;
                }
            }

            OsText {
                anchors.verticalCenter: parent.verticalCenter
                visible: pane.searchState !== "none"
                text: pane.searchState === "error" ? qsTr("Invalid pattern") : qsTr("No match")
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
                onClicked: pane.findNext(false)
            }

            OsIconButton {
                anchors.verticalCenter: parent.verticalCenter
                implicitWidth: Theme.controlHeightSmall
                implicitHeight: Theme.controlHeightSmall
                iconName: "chevron-down"
                toolTip: qsTr("Search down (Shift+Enter)")
                onClicked: pane.findNext(true)
            }

            OsIconButton {
                anchors.verticalCenter: parent.verticalCenter
                implicitWidth: Theme.controlHeightSmall
                implicitHeight: Theme.controlHeightSmall
                iconName: "x"
                toolTip: qsTr("Close search (Escape)")
                onClicked: pane.closeSearch()
            }
        }

        // Search as you type, once typing pauses (a search step can scan the whole history).
        Timer {
            id: searchTimer

            interval: 200
            onTriggered: pane.findNext(false)
        }
    }

    // The shell ended with an error: say so, and offer to restart it or close the pane.
    Rectangle {
        id: exitBanner

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: Theme.spacingLg
        height: bannerRow.implicitHeight + 2 * Theme.spacingMd
        visible: pane.failed && !terminal.running
        z: 3
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
                text: {
                    if (terminal.startError.length > 0)
                        return pane.kind === "ssh" ? qsTr("ssh could not start: %1. Install the OpenSSH client; the built-in one arrives in a later version.").arg(terminal.startError)
                                                   : qsTr("The shell could not start: %1").arg(terminal.startError);
                    if (pane.kind === "ssh")
                        return terminal.exitCodeKnown ? qsTr("The connection to %1 ended (code %2).").arg(pane.label).arg(pane.exitCodeText(terminal.exitCode))
                                                      : qsTr("The connection to %1 was ended.").arg(pane.label);
                    return terminal.exitCodeKnown ? qsTr("The shell exited with code %1.").arg(pane.exitCodeText(terminal.exitCode))
                                                  : qsTr("The shell was ended or could not start.");
                }
                wrapMode: Text.WordWrap
            }

            OsButton {
                id: restartButton

                text: pane.kind === "ssh" ? qsTr("Reconnect") : qsTr("Restart")
                iconName: "refresh-cw"
                variant: "primary"
                onClicked: pane.restart()
                Keys.onReturnPressed: pane.restart()
                Keys.onEnterPressed: pane.restart()
            }

            OsButton {
                text: pane.workspace.paneCount > 1 ? qsTr("Close pane") : qsTr("Close tab")
                iconName: "x"
                onClicked: pane.closePane()
                Keys.onReturnPressed: pane.closePane()
                Keys.onEnterPressed: pane.closePane()
            }
        }
    }

    OsContextMenu {
        id: contextMenu

        OsMenuItem {
            text: qsTr("Copy")
            iconName: "copy"
            shortcutText: pane.shell.shortcutText("terminal.copy")
            enabled: terminal.hasSelection
            onTriggered: terminal.copy()
        }

        OsMenuItem {
            text: qsTr("Paste")
            shortcutText: pane.shell.shortcutText("terminal.paste")
            enabled: terminal.running
            onTriggered: terminal.paste()
        }

        OsMenuItem {
            text: qsTr("Select all")
            onTriggered: terminal.selectAll()
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Split right")
            iconName: "columns-2"
            shortcutText: pane.shell.shortcutText("pane.splitRight")
            onTriggered: pane.workspace.splitPane(pane.paneId, "horizontal")
        }

        OsMenuItem {
            text: qsTr("Split down")
            iconName: "rows-2"
            shortcutText: pane.shell.shortcutText("pane.splitDown")
            onTriggered: pane.workspace.splitPane(pane.paneId, "vertical")
        }

        OsMenuItem {
            text: pane.workspace.zoomedPane === pane.paneId ? qsTr("Restore pane size") : qsTr("Maximize pane")
            iconName: pane.workspace.zoomedPane === pane.paneId ? "minimize-2" : "maximize-2"
            shortcutText: pane.shell.shortcutText("pane.zoom")
            enabled: pane.workspace.paneCount > 1
            onTriggered: pane.workspace.toggleZoom(pane.paneId)
        }

        OsMenuItem {
            text: qsTr("Close pane")
            iconName: "x"
            shortcutText: pane.shell.shortcutText("pane.close")
            onTriggered: pane.closePane()
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Broadcast input to all panes")
            iconName: "radio-tower"
            shortcutText: pane.shell.shortcutText("pane.broadcast")
            checkable: true
            checked: pane.workspace.broadcast
            onTriggered: pane.workspace.toggleBroadcast()
        }

        OsMenuItem {
            text: qsTr("Receive broadcast input")
            checkable: true
            checked: pane.participant
            enabled: pane.workspace.broadcast
            onTriggered: pane.workspace.setPaneReceiving(pane.paneId, !pane.participant)
        }

        OsMenuItem {
            text: qsTr("Synchronize scrolling")
            checkable: true
            checked: pane.workspace.syncScroll
            enabled: pane.workspace.broadcast
            onTriggered: pane.workspace.syncScroll = !pane.workspace.syncScroll
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Find…")
            iconName: "search"
            shortcutText: pane.shell.shortcutText("terminal.find")
            onTriggered: pane.openSearch()
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
            checked: pane.highlightOn
            onTriggered: pane.toggleHighlight()
        }

        OsContextMenu {
            id: profileMenu

            title: qsTr("Profile")

            Instantiator {
                model: pane.profileList

                delegate: OsMenuItem {
                    required property var modelData

                    text: modelData.name
                    checkable: true
                    checked: modelData.id === pane.profile
                    onTriggered: pane.useProfile(modelData.id)
                }

                onObjectAdded: (index, object) => profileMenu.insertItem(index, object)
                onObjectRemoved: (index, object) => profileMenu.removeItem(object)
            }
        }

        OsMenuItem {
            text: qsTr("Terminal settings…")
            iconName: "sliders-horizontal"
            onTriggered: pane.shell.openSettings("terminal")
        }
    }
}
