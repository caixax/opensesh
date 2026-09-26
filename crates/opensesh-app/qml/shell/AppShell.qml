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
// showWorkspaces(mode), focusInTabStrip(), cycleTab(step), gotoTab(n), toggleSidePanel(),
// togglePalette(), toggleNotifications(), toggleMaximize(), toggleFullScreen(),
// cycleRegion(step), shortcutText(actionId), smokeSteps(smoke), prepareScreenshot(),
// prepareSettingsScreenshot(), prepareTerminalScreenshot().
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
                iconName: row.broadcasting ? "radio-tower" : row.pinned ? "pin" : "square-terminal",
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
                shell.closeTabById(tabId);
                if (TerminalSessions.isOpen(paneId))
                    smoke.fail("the session of a closed tab is still open");
            },
            () => shell.splitSmokeSteps(smoke),
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
        const hasMarker = id => {
            const item = workspace.paneItem(id);
            return item !== null && item.terminal.screenText().indexOf(marker) >= 0;
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
                // A paste into several panes asks first (before reading the clipboard); cancel it.
                workspace.paneItem(a).terminal.paste();
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
                                () => workspace.paneItem(c).terminal.screenText().indexOf(marker + "-c") >= 0)];
            },
            () => {
                expect(workspace.paneItem(a).terminal.screenText().indexOf(marker + "-c") < 0,
                       "text typed in an excluded pane reached a receiving pane");
                workspace.toggleBroadcast();
                expect(!workspace.paneItem(a).receiving && workspace.participants.length === 0,
                       "turning broadcast off left a pane receiving");
                console.info("smoke test: splits, focus, zoom and broadcast work");
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
                // Closing a window ends the sessions of its tabs.
                const ids = other.currentWorkspace ? other.currentWorkspace.paneIds.slice() : [];
                expect(ids.length === 3, "the second window's tab doesn't have its three panes");
                other.window.close();
                deadline = Date.now() + timeout;
                return [waitFor("the second window to close and end its sessions",
                                () => WindowRegistry.shells.length === 1 && ids.every(id => !TerminalSessions.isOpen(id)),
                                () => console.info("smoke test: a two-window workspace opened, and closing a window ended its sessions"))];
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
        }
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
                        visible: shell.currentTab === 0 && !shell.detached
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
            workspace: shell.currentTab > 0 ? shell.currentWorkspace : null

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
