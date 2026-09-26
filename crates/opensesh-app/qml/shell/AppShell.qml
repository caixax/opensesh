pragma ComponentBehavior: Bound

// Main window content (PLAN §5.3, §5.4): title bar with the session tabs, navigation rail,
// content area with the views, collapsible side panel and status bar, plus the command palette,
// the notifications panel, the app actions and their shortcuts.
//
// Tabs: tab 0 is Home, which shows the view picked in the rail; tabs 1..n are local terminal
// sessions (TerminalTab, one per sessionModel row, all kept alive) and show their own content,
// with the rail on Terminal. The Terminal rail entry goes back to the last session tab, or opens
// a local terminal while none is open. A tab's id is also the id of its session in the Rust
// registry (TerminalSessions); closing the tab ends the session.
//
// Keyboard: Tab follows the regions in order (title bar, rail, content, side panel, status bar),
// and F6 / Shift+F6 (or Ctrl+F6 / Ctrl+Shift+F6, ADR 0011) jump between them. When a view, tab
// or region is hidden, the keyboard focus moves off it to the content shown now.
//
//   window: Window          the main window
//   persistState: bool      remember the view and side panel in UiState (off in smoke and
//                           screenshot runs, which must not write the user's files)
//   layoutOverride: var     { tabsPosition, railPosition, railLabels, sidePanelPosition,
//                           showStatusBar } values that win over AppSettings (tests only)
//   commandPalette: OsCommandPalette   read-only
//   currentTerminal: TerminalTab       the terminal tab shown, or null (set by the tabs)
// Functions: showView(id), openTerminal(), selectTab(index), newTab(), closeTab(index),
// closeTabById(tabId), tabIndexOf(tabId), updateTab(tabId, role, value), focusInTabStrip(),
// cycleTab(step), gotoTab(n), toggleSidePanel(), togglePalette(), toggleNotifications(),
// toggleMaximize(), toggleFullScreen(), cycleRegion(step), shortcutText(actionId),
// smokeSteps(smoke), prepareScreenshot(), prepareSettingsScreenshot().
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: shell

    required property Window window
    property bool persistState: true
    property var layoutOverride: ({})

    readonly property string tabsPosition: layoutOverride.tabsPosition ?? AppSettings.tabsPosition
    readonly property string railPosition: layoutOverride.railPosition ?? AppSettings.railPosition
    readonly property bool railLabels: layoutOverride.railLabels ?? AppSettings.railLabels
    readonly property string sidePanelPosition: layoutOverride.sidePanelPosition ?? AppSettings.sidePanelPosition
    readonly property bool showStatusBar: layoutOverride.showStatusBar ?? AppSettings.showStatusBar
    readonly property string decorations: Platform.effectiveDecorations(AppSettings.windowDecorations)
    readonly property bool frameless: decorations === "custom" || decorations === "none"
    readonly property bool sidePanelLeft: sidePanelPosition === "left"
    // The resize grip of a frameless window (WindowResizeHandles.grip) covers the right edge of
    // the content while nothing sits between them: the terminal's scroll bar stays clear of it.
    readonly property real contentEdgeInset: frameless && window.visibility === Window.Windowed
                                             && railPosition !== "right"
                                             && !(sidePanelOpen && !sidePanelLeft)
                                             ? Math.round(Theme.spacingXs * 1.5) : 0

    readonly property var viewIds: ["hosts", "terminal", "sftp", "tunnels", "snippets", "keychain", "history",
        "settings"]
    property string activeView: "hosts"
    // Last view other than Terminal: what Home shows while sessions are open.
    property string homeView: "hosts"
    property int currentTab: 0
    // The tab id of currentTab (0 for Home). Tabs compare against this stable id, not the
    // index: removing a row renumbers the delegates before currentTab is adjusted.
    property int currentTabId: 0
    property int lastSessionTab: 1
    property alias sessionModel: sessionModel
    readonly property int sessionCount: sessionModel.count
    property int nextTabId: 1
    property Item currentTerminal: null
    // The terminal shown has a translucent background and the window has an alpha channel:
    // Main.qml stops painting the window background.
    readonly property bool translucentTerminal: AppInfo.windowAlpha && currentTab > 0 && currentTerminal !== null
                                                && currentTerminal.terminal.backgroundImage.length === 0
                                                && currentTerminal.terminal.backgroundOpacity < 0.999

    onCurrentTabChanged: syncCurrentTabId()
    // Set while the tab model changes, so the tab bar's own index adjustments are ignored.
    property bool updatingTabs: false

    property bool sidePanelOpen: false
    property real sidePanelWidth: 320
    readonly property real sidePanelMinimumWidth: Theme.spacingXxl * 7
    readonly property real contentMinimumWidth: Theme.spacingXxl * 8

    property bool restoreMaximized: false

    readonly property alias commandPalette: palette

    // The tab strip re-reads currentTab after the tab model changed.
    signal tabsUpdated

    function scheduleSave() {
        if (persistState)
            saveTimer.restart();
    }

    function setActiveView(id) {
        activeView = id;
        if (id !== "terminal")
            homeView = id;
        if (persistState && UiState.activeView !== id) {
            UiState.activeView = id;
            scheduleSave();
        }
    }

    function showView(id) {
        if (viewIds.indexOf(id) < 0) {
            console.warn("AppShell: unknown view", id);
            return;
        }
        if (id === "terminal" && sessionModel.count > 0) {
            selectTab(Math.min(Math.max(lastSessionTab, 1), sessionModel.count));
            return;
        }
        currentTab = 0;
        setActiveView(id);
        tabsUpdated();
        moveFocusOffHiddenItem();
    }

    function syncCurrentTabId() {
        currentTabId = currentTab > 0 && currentTab <= sessionModel.count
                       ? sessionModel.get(currentTab - 1).tabId : 0;
    }

    function selectTab(index) {
        index = Math.max(0, Math.min(index, sessionModel.count));
        currentTab = index;
        // Also when the index didn't change (the current tab was closed and the next one moved
        // into its place).
        syncCurrentTabId();
        if (index > 0) {
            lastSessionTab = index;
            setActiveView("terminal");
        } else if (activeView === "terminal" && sessionModel.count > 0) {
            setActiveView(homeView);
        }
        tabsUpdated();
        moveFocusOffHiddenItem();
    }

    // The tab bar's current index changed (a click or the arrow keys).
    function tabBarSelected(index) {
        if (!updatingTabs && index >= 0 && index !== currentTab)
            selectTab(index);
    }

    // startSession false: a tab without a shell (screenshot runs, whose tab titles must not
    // depend on the shell).
    function appendSession(startSession) {
        updatingTabs = true;
        sessionModel.append({
            tabId: nextTabId,
            kind: "local",
            title: "",
            profile: AppSettings.terminalProfile,
            newOutput: false,
            bellRang: false,
            startSession: startSession ?? true
        });
        nextTabId += 1;
        updatingTabs = false;
    }

    // The Terminal rail entry: the last session tab, or a new local terminal.
    function openTerminal() {
        if (sessionModel.count > 0)
            showView("terminal");
        else
            newTab();
    }

    // 1-based tab index of a session tab, or -1.
    function tabIndexOf(tabId) {
        for (let i = 0; i < sessionModel.count; ++i) {
            if (sessionModel.get(i).tabId === tabId)
                return i + 1;
        }
        return -1;
    }

    function closeTabById(tabId) {
        const index = tabIndexOf(tabId);
        if (index > 0)
            closeTab(index);
    }

    // Sets a role of a session tab: title, newOutput or bellRang.
    function updateTab(tabId, role, value) {
        const index = tabIndexOf(tabId);
        if (index > 0 && sessionModel.get(index - 1)[role] !== value)
            sessionModel.setProperty(index - 1, role, value);
    }

    function focusInTabStrip() {
        return isInside(window.activeFocusItem, titleRegion);
    }

    function newTab() {
        appendSession();
        selectTab(sessionModel.count);
    }

    function closeTab(index) {
        if (index < 1 || index > sessionModel.count)
            return;
        const wasCurrent = index === currentTab;
        // Closing the focused tab destroys the focused item; the focus then goes to the new
        // current tab.
        const focusInTabs = isInside(window.activeFocusItem, titleRegion);
        // The shell ends in the background; the tab's item only lets go of it.
        TerminalSessions.close(sessionModel.get(index - 1).tabId);
        updatingTabs = true;
        sessionModel.remove(index - 1);
        updatingTabs = false;
        if (lastSessionTab >= index)
            lastSessionTab = Math.max(1, lastSessionTab - 1);
        if (sessionModel.count === 0) {
            currentTab = 0;
            if (activeView === "terminal")
                setActiveView(homeView);
            tabsUpdated();
        } else if (wasCurrent) {
            selectTab(Math.min(index, sessionModel.count));
        } else {
            if (currentTab > index)
                currentTab -= 1;
            tabsUpdated();
        }
        if (focusInTabs && !isInside(window.activeFocusItem, titleRegion))
            focusRegion(titleRegion);
        moveFocusOffHiddenItem();
        // "Keep the window" shows Home; "quit" closes it (never in test runs).
        if (sessionModel.count === 0 && AppSettings.onLastTabClosed === "quit" && persistState)
            window.close();
    }

    function cycleTab(step) {
        const count = sessionModel.count + 1;
        selectTab((currentTab + step + count) % count);
    }

    // n is 1-based and counts Home; 9 is always the last tab.
    function gotoTab(n) {
        const count = sessionModel.count + 1;
        const index = n === 9 ? count - 1 : n - 1;
        if (index < count)
            selectTab(index);
    }

    function setSidePanelOpen(open) {
        // Don't leave the keyboard focus on a panel that disappears.
        if (!open && isInside(window.activeFocusItem, sidePanel))
            focusRegion(contentArea);
        sidePanelOpen = open;
        if (persistState && UiState.sidePanelOpen !== open) {
            UiState.sidePanelOpen = open;
            scheduleSave();
        }
    }

    function toggleSidePanel() {
        setSidePanelOpen(!sidePanelOpen);
    }

    // Remember the width the user gives the panel (dragging or keyboard resizing the handle), but
    // not the smaller width the split view forces while the window is too narrow for it.
    function sidePanelSlotResized(slot) {
        const preferred = slot.T.SplitView.preferredWidth;
        if (slot.visible && (splitter.resizing || Math.abs(slot.width - preferred) < 1))
            recordSidePanelWidth(slot.width);
    }

    function recordSidePanelWidth(width) {
        const rounded = Math.round(width);
        if (rounded < sidePanelMinimumWidth || rounded === Math.round(sidePanelWidth))
            return;
        sidePanelWidth = rounded;
        if (persistState) {
            UiState.sidePanelWidth = rounded;
            scheduleSave();
        }
    }

    // OsSplitter's keyboard resizing assigns a slot's preferred width, which drops its binding to
    // sidePanelWidth; bind both slots again (e.g. before the panel changes sides).
    function syncSidePanelWidth() {
        leftSlot.T.SplitView.preferredWidth = Qt.binding(() => shell.sidePanelWidth);
        rightSlot.T.SplitView.preferredWidth = Qt.binding(() => shell.sidePanelWidth);
    }

    function togglePalette() {
        if (palette.opened)
            palette.close();
        else
            palette.open();
    }

    function toggleNotifications() {
        if (notifications.opened)
            notifications.close();
        else
            notifications.open();
    }

    function toggleMaximize() {
        if (window.visibility === Window.Maximized)
            window.showNormal();
        else
            window.showMaximized();
    }

    function toggleFullScreen() {
        if (window.visibility === Window.FullScreen) {
            if (restoreMaximized)
                window.showMaximized();
            else
                window.showNormal();
        } else {
            restoreMaximized = window.visibility === Window.Maximized;
            window.showFullScreen();
        }
    }

    function shortcutText(actionId) {
        return shortcutHost.nativeText(actionId);
    }

    // Focus regions, in Tab order.
    function regions() {
        return [titleRegion, rail, contentArea, sidePanel, statusBar];
    }

    function isInside(item, region) {
        for (let current = item; current; current = current.parent) {
            if (current === region)
                return true;
        }
        return false;
    }

    function firstFocusable(region) {
        if (!region || !region.visible)
            return null;
        const next = region.nextItemInFocusChain(true);
        return next && next !== region && isInside(next, region) ? next : null;
    }

    function focusRegion(region, reason) {
        const target = firstFocusable(region);
        if (target)
            target.forceActiveFocus(reason ?? Qt.TabFocusReason);
        return target !== null;
    }

    // Hiding an item doesn't take its keyboard focus away, and key events still reach it (a
    // hidden button would still press, a hidden slider would still change its setting). Call this
    // after the visible content changed: it moves the focus from a hidden item to the content
    // shown now, else to the rail or the title bar. The focus reason is kept, so the focus ring
    // shows for keyboard users but not after a click.
    function moveFocusOffHiddenItem() {
        const focused = window.activeFocusItem;
        if (!focused || focused.visible)
            return;
        const reason = focused.focusReason ?? Qt.OtherFocusReason;
        if (!focusRegion(contentArea, reason) && !focusRegion(rail, reason))
            focusRegion(titleRegion, reason);
    }

    function cycleRegion(step) {
        const list = regions();
        const focused = window.activeFocusItem;
        let index = -1;
        for (let i = 0; i < list.length; ++i) {
            if (focused && isInside(focused, list[i])) {
                index = i;
                break;
            }
        }
        if (index < 0)
            index = step > 0 ? -1 : list.length;
        for (let k = 0; k < list.length; ++k) {
            index = (index + step + list.length) % list.length;
            if (focusRegion(list[index]))
                return;
        }
    }

    // Functions for SmokeTest.steps: a real local terminal (shell output, typed input, closing
    // the tab ends the session, a shell that exits closes its tab). `smoke` is the SmokeTest.
    function terminalSmokeSteps(smoke) {
        const timeout = 15000;
        const marker = "opensesh-smoke-" + Math.floor(Math.random() * 1e9);
        let tab = null;
        let tabId = 0;
        let deadline = 0;
        let profileId = "";
        // A step that polls `condition` until it holds, then runs `next` (which may return steps).
        const waitFor = (what, condition, next) => {
            const poll = () => {
                if (condition())
                    return next ? next() : [];
                if (Date.now() > deadline) {
                    smoke.fail("timed out after " + timeout / 1000 + " s waiting for " + what);
                    return [];
                }
                return [poll];
            };
            return poll;
        };
        const openTab = what => {
            shell.newTab();
            tab = shell.currentTerminal;
            if (!tab) {
                smoke.fail("no terminal tab after opening one");
                return [];
            }
            tabId = tab.tabId;
            deadline = Date.now() + timeout;
            return [waitFor(what, () => tab.terminal.screenText().trim().length > 0)];
        };
        return [
            () => openTab("the shell's first output"),
            () => {
                tab.terminal.sendText("echo " + marker + "\r");
                deadline = Date.now() + timeout;
                return [waitFor("the echoed marker " + marker,
                                () => tab.terminal.screenText().split("\n").some(line => line.trim() === marker))];
            },
            () => {
                console.info("smoke test: the local terminal echoed", marker);
                // A profile change reaches the open terminal at once (test runs keep profiles
                // in memory and never write them).
                profileId = TerminalProfiles.createProfile("Smoke test", "");
                if (profileId.length === 0) {
                    smoke.fail("could not create a profile");
                    return [];
                }
                TerminalProfiles.setOption(profileId, "font_size", "20");
                tab.useProfile(profileId);
                deadline = Date.now() + timeout;
                return [waitFor("the tab to use the new profile", () => tab.terminal.fontSize === 20)];
            },
            () => {
                TerminalProfiles.setOption(profileId, "font_size", "18");
                TerminalProfiles.setOption(profileId, "highlight_sets", JSON.stringify(["logs", "network"]));
                deadline = Date.now() + timeout;
                return [waitFor("a live profile edit to reach the terminal", () => tab.terminal.fontSize === 18)];
            },
            () => {
                tab.zoom(1);
                tab.toggleHighlight();
                deadline = Date.now() + timeout;
                return [waitFor("the tab's zoom", () => tab.terminal.fontSize === 19)];
            },
            () => {
                const find = ActionRegistry.find("terminal.find");
                Keybindings.setShortcut("terminal.find", "Ctrl+Alt+F", find.defaultShortcut);
                if (find.shortcut !== "Ctrl+Alt+F")
                    smoke.fail("a changed shortcut did not reach its action");
                Keybindings.reset("terminal.find");
                if (find.shortcut !== find.defaultShortcut)
                    smoke.fail("a reset shortcut did not go back to the default");
                TerminalProfiles.deleteProfile(profileId);
                deadline = Date.now() + timeout;
                return [waitFor("the tab to fall back to the default profile", () => tab.terminal.fontSize !== 19 && tab.terminal.fontSize > 0,
                                () => console.info("smoke test: profile changes reached the terminal live"))];
            },
            () => {
                shell.closeTabById(tabId);
                if (TerminalSessions.isOpen(tabId))
                    smoke.fail("the session of a closed tab is still open");
            },
            () => openTab("the second shell's first output"),
            () => {
                tab.terminal.sendText("exit\r");
                deadline = Date.now() + timeout;
                return [waitFor("the shell to exit and close its tab",
                                () => shell.tabIndexOf(tabId) < 0 && !TerminalSessions.isOpen(tabId),
                                () => console.info("smoke test: the shell exited and closed its tab"))];
            }
        ];
    }

    // Functions for SmokeTest.steps: instantiate every view, overlay and layout variant, then
    // run the terminal steps.
    function smokeSteps(smoke) {
        const steps = [];
        let initialView = "hosts";
        const expectVisibleFocus = what => {
            const focused = shell.window.activeFocusItem;
            if (focused && !focused.visible)
                console.warn("AppShell: the keyboard focus stayed on a hidden item after", what);
        };
        steps.push(() => initialView = shell.activeView);
        for (const id of viewIds)
            steps.push(() => shell.showView(id));
        steps.push(() => {
            const settings = settingsLoader.item;
            return settings && typeof settings.smokeSteps === "function" ? settings.smokeSteps() : [];
        });
        steps.push(() => shell.togglePalette());
        steps.push(() => palette.setQuery("tab"));
        steps.push(() => palette.setQuery("zzzz"));
        steps.push(() => shell.togglePalette());
        steps.push(() => Toasts.show(qsTr("Smoke test notification."), "info"));
        steps.push(() => shell.toggleNotifications());
        steps.push(() => shell.toggleNotifications());
        steps.push(() => shell.toggleSidePanel());
        steps.push(() => sidePanel.currentIndex = 1);
        steps.push(() => sidePanel.currentIndex = 2);
        steps.push(() => shell.toggleSidePanel());
        steps.push(() => shell.newTab());
        steps.push(() => shell.newTab());
        steps.push(() => shell.cycleTab(1));
        steps.push(() => shell.gotoTab(9));
        steps.push(() => shell.showView("terminal"));
        steps.push(() => shell.closeTab(shell.currentTab));
        steps.push(() => shell.closeTab(1));
        steps.push(() => shell.cycleRegion(1));
        steps.push(() => shell.cycleRegion(1));
        steps.push(() => shell.cycleRegion(-1));
        // The focus never stays on a hidden view, tab or region.
        steps.push(() => shell.focusRegion(contentArea));
        steps.push(() => shell.showView("history"));
        steps.push(() => expectVisibleFocus("a view switch"));
        steps.push(() => shell.newTab());
        steps.push(() => expectVisibleFocus("opening a tab"));
        steps.push(() => shell.closeTab(shell.currentTab));
        steps.push(() => expectVisibleFocus("closing the last tab"));
        steps.push(() => shell.focusRegion(statusBar));
        steps.push(() => shell.layoutOverride = {
            tabsPosition: "below_title_bar",
            railPosition: "right",
            railLabels: true,
            sidePanelPosition: "left",
            showStatusBar: false
        });
        steps.push(() => expectVisibleFocus("hiding the status bar"));
        steps.push(() => shell.toggleSidePanel());
        steps.push(() => shell.toggleSidePanel());
        steps.push(() => shell.layoutOverride = {});
        steps.push(() => shell.showView(initialView));
        if (smoke)
            steps.push(() => shell.terminalSmokeSteps(smoke));
        return steps;
    }

    // --screenshots: the Hosts view, with one session tab (without a shell) and no popups.
    function prepareScreenshot() {
        palette.close();
        notifications.close();
        sidePanelOpen = false;
        if (sessionModel.count === 0)
            appendSession(false);
        showView("hosts");
    }

    // Shows Settings at `section` (e.g. "terminal").
    function openSettings(section) {
        showView("settings");
        const settings = settingsLoader.item;
        if (settings && typeof settings.showSection === "function")
            settings.showSection(section);
    }

    function prepareSettingsScreenshot(page) {
        showView("settings");
        const settings = settingsLoader.item;
        if (settings && typeof settings.showSection === "function")
            settings.showSection(page && page.length > 0 ? page : "appearance");
    }

    Component.onCompleted: {
        const stored = UiState.activeView;
        const view = viewIds.indexOf(stored) >= 0 ? stored : "hosts";
        activeView = view;
        if (view !== "terminal")
            homeView = view;
        sidePanelOpen = UiState.sidePanelOpen;
        sidePanelWidth = Math.max(sidePanelMinimumWidth, Math.min(1200, UiState.sidePanelWidth));
    }

    onSidePanelLeftChanged: syncSidePanelWidth()

    // UiState.save() is debounced off the GUI thread too; this only batches a drag.
    Timer {
        id: saveTimer

        interval: 400
        onTriggered: UiState.save()
    }

    ListModel {
        id: sessionModel
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Column {
            id: titleRegion

            Layout.fillWidth: true

            TitleBar {
                width: parent.width
                window: shell.window
                shell: shell
                showTabs: shell.tabsPosition !== "below_title_bar"
                frameless: shell.frameless
                showWindowButtons: shell.decorations === "custom"
            }

            // Tabs in their own row (tabsPosition "below_title_bar").
            Rectangle {
                width: parent.width
                height: Theme.titleBarHeight
                visible: shell.tabsPosition === "below_title_bar"
                color: Theme.bg

                Loader {
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.spacingSm
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.spacingSm
                    anchors.verticalCenter: parent.verticalCenter
                    active: parent.visible

                    sourceComponent: SessionTabStrip {
                        shell: shell
                        newTabShortcut: shell.shortcutText("tab.newLocal")
                    }
                }

                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: Theme.borderWidth
                    color: Theme.border
                }
            }
        }

        Item {
            id: body

            Layout.fillWidth: true
            Layout.fillHeight: true

            OsRail {
                id: rail

                readonly property bool onRight: shell.railPosition === "right"

                x: onRight ? body.width - width : 0
                width: implicitWidth
                height: body.height
                visible: shell.railPosition !== "hidden"
                showLabels: shell.railLabels
                // A right-hand rail puts its edge line on the left. Without labels it is the
                // full mirror image (selection bar on the outer edge, tooltips to the left); with
                // labels the entries stay left to right so the text reads normally.
                LayoutMirroring.enabled: onRight
                LayoutMirroring.childrenInherit: !showLabels
                model: [
                    { id: "hosts", text: qsTr("Hosts"), iconName: "server" },
                    { id: "terminal", text: qsTr("Terminal"), iconName: "square-terminal" },
                    { id: "sftp", text: qsTr("SFTP"), iconName: "folder-sync" },
                    { id: "tunnels", text: qsTr("Tunnels"), iconName: "waypoints" },
                    { id: "snippets", text: qsTr("Snippets"), iconName: "scroll-text" },
                    { id: "keychain", text: qsTr("Keychain"), iconName: "key-round" },
                    { id: "history", text: qsTr("History"), iconName: "history" }
                ]
                footerModel: [
                    { id: "settings", text: qsTr("Settings"), iconName: "settings" }
                ]

                onActivated: id => id === "terminal" ? shell.openTerminal() : shell.showView(id)
                onVisibleChanged: {
                    if (!visible)
                        shell.moveFocusOffHiddenItem();
                }

                // OsRail assigns currentId itself when activated; keep it following the shell.
                Binding on currentId {
                    value: shell.currentTab > 0 ? "terminal" : shell.activeView
                }
            }

            OsSplitter {
                id: splitter

                x: rail.visible && !rail.onRight ? rail.width : 0
                width: body.width - (rail.visible ? rail.width : 0)
                height: body.height

                // Side panel slot on the left (sidePanelPosition "left").
                Item {
                    id: leftSlot

                    visible: shell.sidePanelOpen && shell.sidePanelLeft
                    T.SplitView.preferredWidth: shell.sidePanelWidth
                    T.SplitView.minimumWidth: shell.sidePanelMinimumWidth
                    T.SplitView.maximumWidth: Math.max(shell.sidePanelMinimumWidth,
                                                       Math.min(1200, splitter.width - shell.contentMinimumWidth))

                    onWidthChanged: shell.sidePanelSlotResized(leftSlot)
                }

                Item {
                    id: contentArea

                    // Views never paint over the side panel when the window is narrow.
                    clip: true
                    T.SplitView.fillWidth: true
                    T.SplitView.minimumWidth: shell.contentMinimumWidth

                    StackLayout {
                        anchors.fill: parent
                        visible: shell.currentTab === 0
                        currentIndex: Math.max(0, shell.viewIds.indexOf(shell.activeView))

                        ViewLoader {
                            viewId: "hosts"
                            currentView: shell.activeView
                            sourceComponent: HostsView {}
                        }
                        ViewLoader {
                            viewId: "terminal"
                            currentView: shell.activeView
                            sourceComponent: TerminalView {}
                        }
                        ViewLoader {
                            viewId: "sftp"
                            currentView: shell.activeView
                            sourceComponent: SftpView {}
                        }
                        ViewLoader {
                            viewId: "tunnels"
                            currentView: shell.activeView
                            sourceComponent: TunnelsView {}
                        }
                        ViewLoader {
                            viewId: "snippets"
                            currentView: shell.activeView
                            sourceComponent: SnippetsView {}
                        }
                        ViewLoader {
                            viewId: "keychain"
                            currentView: shell.activeView
                            sourceComponent: KeychainView {}
                        }
                        ViewLoader {
                            viewId: "history"
                            currentView: shell.activeView
                            sourceComponent: HistoryView {}
                        }
                        ViewLoader {
                            id: settingsLoader

                            viewId: "settings"
                            currentView: shell.activeView
                            sourceComponent: SettingsView {}
                        }
                    }

                    // One tab per session; only the current one is visible.
                    Repeater {
                        model: sessionModel

                        TerminalTab {
                            anchors.fill: parent
                            shell: shell
                            edgeInset: shell.contentEdgeInset
                        }
                    }
                }

                // Side panel slot on the right (the default).
                Item {
                    id: rightSlot

                    visible: shell.sidePanelOpen && !shell.sidePanelLeft
                    T.SplitView.preferredWidth: shell.sidePanelWidth
                    T.SplitView.minimumWidth: shell.sidePanelMinimumWidth
                    T.SplitView.maximumWidth: Math.max(shell.sidePanelMinimumWidth,
                                                       Math.min(1200, splitter.width - shell.contentMinimumWidth))

                    onWidthChanged: shell.sidePanelSlotResized(rightSlot)
                }
            }

            SidePanel {
                id: sidePanel

                parent: shell.sidePanelLeft ? leftSlot : rightSlot
                anchors.fill: parent
                onCloseRequested: shell.setSidePanelOpen(false)
            }

            OsToastHost {}
        }

        StatusBar {
            id: statusBar

            Layout.fillWidth: true
            visible: shell.showStatusBar
            terminal: shell.currentTerminal ? shell.currentTerminal.terminal : null

            onVisibleChanged: {
                if (!visible)
                    shell.moveFocusOffHiddenItem();
            }
        }
    }

    OsCommandPalette {
        id: palette

        topOffset: titleRegion.height + Theme.spacingLg
        shortcutText: action => shell.shortcutText(action.actionId)
    }

    NotificationsPanel {
        id: notifications
    }

    ShortcutHost {
        id: shortcutHost
    }

    AppActions {
        shell: shell
        window: shell.window
    }

    // Loads a view the first time it is shown and keeps it afterwards.
    component ViewLoader: Loader {
        required property string viewId
        property string currentView
        property bool keep: false

        active: keep || currentView === viewId
        // Not in onLoaded: that runs inside the `active` binding and would loop.
        onCurrentViewChanged: {
            if (currentView === viewId)
                keep = true;
        }
        Component.onCompleted: {
            if (currentView === viewId)
                keep = true;
        }
    }
}
