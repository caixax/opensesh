pragma ComponentBehavior: Bound

// The main window's actions (PLAN §6.4 defaults), registered in ActionRegistry: the command
// palette lists them and ShortcutHost binds their shortcuts. App shortcuts use Shift or Alt
// combinations so they never take keys terminal programs need (Ctrl+A, Ctrl+B, Ctrl+K, Ctrl+R...).
// The exception is F6 / Shift+F6 to move between the window regions (ADR 0011): from Sprint 2 the
// terminal keeps function keys for its programs, and Ctrl+F6 / Ctrl+Shift+F6 always work.
// Debug builds add a few "Debug:" actions. Conflicting shortcuts are reported with console.warn,
// which fails the smoke test. There is one set of actions for the app: they act on the shell of
// the window in use (WindowRegistry.activeShell). Pane actions that take keys terminal programs
// also use (Alt+arrows) are only enabled while the tab has several panes.
//   shell: Item      the AppShell of the window in use
//   window: Window   the main window
import QtQuick
import cc.caixa.opensesh

QtObject {
    id: root

    required property Item shell
    required property Window window

    readonly property string categoryApp: qsTr("Application")
    readonly property string categoryTabs: qsTr("Tabs")
    readonly property string categoryView: qsTr("View")
    readonly property string categoryAppearance: qsTr("Appearance")
    readonly property string categorySessions: qsTr("Sessions")
    readonly property string categoryTerminal: qsTr("Terminal")
    readonly property string categoryPanes: qsTr("Panes")
    readonly property string categoryWorkspaces: qsTr("Workspaces")
    readonly property string categoryDebug: qsTr("Debug")

    readonly property ComingSoon comingSoon: ComingSoon {}
    // The terminal tab shown in the window in use, or null.
    readonly property Item workspace: shell.currentTab > 0 ? shell.currentWorkspace : null
    readonly property bool severalPanes: workspace !== null && workspace.paneCount > 1
    readonly property bool currentPinned: shell.tabsRevision >= 0 && shell.currentTab > 0
                                          && shell.currentTab <= shell.sessionCount
                                          && shell.sessionModel.get(shell.currentTab - 1).pinned

    readonly property list<OsAction> actions: [
        OsAction {
            actionId: "app.commandPalette"
            text: qsTr("Command palette")
            defaultShortcut: "Ctrl+Shift+P"
            category: root.categoryApp
            iconName: "command"
            onTriggered: root.shell.togglePalette()
        },
        OsAction {
            actionId: "app.quickConnect"
            text: qsTr("Quick connect")
            defaultShortcut: "Ctrl+Shift+O"
            category: root.categorySessions
            iconName: "plug-zap"
            onTriggered: root.comingSoon.notify(qsTr("Quick connect"), 5)
        },
        OsAction {
            actionId: "tab.newLocal"
            text: qsTr("New local terminal tab")
            defaultShortcut: "Ctrl+Shift+T"
            category: root.categoryTabs
            iconName: "square-terminal"
            onTriggered: root.shell.newTab()
        },
        OsAction {
            actionId: "tab.duplicate"
            text: qsTr("Duplicate tab")
            defaultShortcut: "Ctrl+Shift+D"
            category: root.categoryTabs
            iconName: "copy"
            enabled: root.workspace !== null
            onTriggered: root.shell.duplicateTab(root.shell.currentTab)
        },
        // Ctrl+Shift+W closes the focused pane (pane.close), and the tab with its last pane.
        OsAction {
            actionId: "tab.close"
            text: qsTr("Close tab")
            category: root.categoryTabs
            iconName: "x"
            enabled: root.shell.currentTab > 0
            onTriggered: root.shell.closeTab(root.shell.currentTab)
        },
        OsAction {
            actionId: "tab.reopenClosed"
            text: qsTr("Reopen closed tab")
            defaultShortcut: "Ctrl+Alt+Shift+T"
            category: root.categoryTabs
            iconName: "rotate-ccw"
            enabled: WindowRegistry.closedTabs.length > 0
            onTriggered: root.shell.reopenClosedTab()
        },
        // Ctrl+Tab goes through the tabs in the order they were used (a list while Ctrl is held).
        OsAction {
            actionId: "tab.next"
            text: qsTr("Switch to the previously used tab")
            defaultShortcut: "Ctrl+Tab"
            category: root.categoryTabs
            iconName: "chevron-right"
            enabled: root.shell.sessionCount > 0
            onTriggered: root.shell.showSwitcher(1)
        },
        OsAction {
            actionId: "tab.previous"
            text: qsTr("Switch to the least recently used tab")
            defaultShortcut: "Ctrl+Shift+Tab"
            category: root.categoryTabs
            iconName: "chevron-left"
            enabled: root.shell.sessionCount > 0
            onTriggered: root.shell.showSwitcher(-1)
        },
        // The tab strip's order (PLAN §6.4).
        OsAction {
            actionId: "tab.nextAlt"
            text: qsTr("Next tab")
            defaultShortcut: "Ctrl+PgDown"
            category: root.categoryTabs
            iconName: "chevron-right"
            enabled: root.shell.sessionCount > 0
            onTriggered: root.shell.cycleTab(1)
        },
        OsAction {
            actionId: "tab.previousAlt"
            text: qsTr("Previous tab")
            defaultShortcut: "Ctrl+PgUp"
            category: root.categoryTabs
            iconName: "chevron-left"
            enabled: root.shell.sessionCount > 0
            onTriggered: root.shell.cycleTab(-1)
        },
        OsAction {
            actionId: "tab.moveLeft"
            text: qsTr("Move tab left")
            defaultShortcut: "Ctrl+Shift+PgUp"
            category: root.categoryTabs
            iconName: "chevron-left"
            enabled: root.shell.currentTab > 1
            onTriggered: root.shell.moveTab(root.shell.currentTab, root.shell.currentTab - 1)
        },
        OsAction {
            actionId: "tab.moveRight"
            text: qsTr("Move tab right")
            defaultShortcut: "Ctrl+Shift+PgDown"
            category: root.categoryTabs
            iconName: "chevron-right"
            enabled: root.shell.currentTab > 0 && root.shell.currentTab < root.shell.sessionCount
            onTriggered: root.shell.moveTab(root.shell.currentTab, root.shell.currentTab + 1)
        },
        OsAction {
            actionId: "tab.rename"
            text: qsTr("Rename tab")
            category: root.categoryTabs
            iconName: "pencil"
            enabled: root.workspace !== null
            onTriggered: root.shell.askRenameTab(root.shell.currentTab)
        },
        OsAction {
            actionId: "tab.pin"
            text: root.currentPinned ? qsTr("Unpin tab") : qsTr("Pin tab")
            category: root.categoryTabs
            iconName: root.currentPinned ? "pin-off" : "pin"
            enabled: root.workspace !== null
            onTriggered: root.shell.setTabPinned(root.shell.currentTab, !root.currentPinned)
        },
        OsAction {
            actionId: "tab.closeOthers"
            text: qsTr("Close other tabs")
            category: root.categoryTabs
            enabled: root.workspace !== null && root.shell.sessionCount > 1
            onTriggered: root.shell.closeOtherTabs(root.shell.currentTab, "others")
        },
        OsAction {
            actionId: "tab.closeLeft"
            text: qsTr("Close tabs to the left")
            category: root.categoryTabs
            enabled: root.shell.currentTab > 1
            onTriggered: root.shell.closeOtherTabs(root.shell.currentTab, "left")
        },
        OsAction {
            actionId: "tab.closeRight"
            text: qsTr("Close tabs to the right")
            category: root.categoryTabs
            enabled: root.shell.currentTab > 0 && root.shell.currentTab < root.shell.sessionCount
            onTriggered: root.shell.closeOtherTabs(root.shell.currentTab, "right")
        },
        OsAction {
            actionId: "tab.moveToNewWindow"
            text: qsTr("Move tab to a new window")
            category: root.categoryTabs
            iconName: "app-window"
            enabled: root.workspace !== null && (!root.shell.detached || root.shell.sessionCount > 1)
            onTriggered: root.shell.moveTabToNewWindow(root.shell.currentTab)
        },
        OsAction {
            actionId: "tab.moveToMainWindow"
            text: qsTr("Move tab to the main window")
            category: root.categoryTabs
            iconName: "app-window"
            enabled: root.workspace !== null && root.shell.detached && WindowRegistry.mainShell !== null
            onTriggered: root.shell.moveTabToShell(root.shell.currentTab, WindowRegistry.mainShell)
        },
        // Split panes (PLAN §6.4).
        OsAction {
            actionId: "pane.splitRight"
            text: qsTr("Split right")
            defaultShortcut: "Alt+Shift+="
            category: root.categoryPanes
            iconName: "columns-2"
            enabled: root.workspace !== null
            onTriggered: root.workspace.splitPane(0, "horizontal")
        },
        OsAction {
            actionId: "pane.splitDown"
            text: qsTr("Split down")
            defaultShortcut: "Alt+Shift+-"
            category: root.categoryPanes
            iconName: "rows-2"
            enabled: root.workspace !== null
            onTriggered: root.workspace.splitPane(0, "vertical")
        },
        OsAction {
            actionId: "pane.close"
            text: qsTr("Close pane")
            defaultShortcut: "Ctrl+Shift+W"
            category: root.categoryPanes
            iconName: "x"
            enabled: root.workspace !== null
            onTriggered: root.workspace.closePane(root.workspace.focusedPane)
        },
        OsAction {
            actionId: "pane.zoom"
            text: root.workspace && root.workspace.zoomedPane !== 0 ? qsTr("Restore pane size") : qsTr("Maximize pane")
            defaultShortcut: "Ctrl+Shift+Z"
            category: root.categoryPanes
            iconName: "maximize-2"
            enabled: root.severalPanes
            onTriggered: root.workspace.toggleZoom(0)
        },
        OsAction {
            actionId: "pane.equalize"
            text: qsTr("Make all panes the same size")
            category: root.categoryPanes
            iconName: "layout-grid"
            enabled: root.severalPanes
            onTriggered: root.workspace.equalize()
        },
        OsAction {
            actionId: "pane.broadcast"
            text: root.workspace && root.workspace.broadcast ? qsTr("Stop broadcasting input") : qsTr("Broadcast input to all panes")
            defaultShortcut: "Ctrl+Shift+B"
            category: root.categoryPanes
            iconName: "radio-tower"
            enabled: root.workspace !== null
            onTriggered: root.workspace.toggleBroadcast()
        },
        OsAction {
            actionId: "pane.toggleReceiving"
            text: root.workspace && root.workspace.focusedItem && root.workspace.focusedItem.participant
                  ? qsTr("Stop receiving broadcast input in this pane") : qsTr("Receive broadcast input in this pane")
            category: root.categoryPanes
            iconName: "radio-tower"
            enabled: root.workspace !== null && root.workspace.broadcast && root.workspace.focusedItem !== null
            onTriggered: root.workspace.setPaneReceiving(root.workspace.focusedPane, !root.workspace.focusedItem.participant)
        },
        OsAction {
            actionId: "pane.syncScroll"
            text: root.workspace && root.workspace.syncScroll ? qsTr("Stop synchronized scrolling")
                                                             : qsTr("Synchronize scrolling of the receiving panes")
            category: root.categoryPanes
            iconName: "rows-2"
            enabled: root.workspace !== null && root.workspace.broadcast
            onTriggered: root.workspace.syncScroll = !root.workspace.syncScroll
        },
        // Workspaces.
        OsAction {
            actionId: "workspace.save"
            text: qsTr("Save workspace…")
            category: root.categoryWorkspaces
            iconName: "save"
            onTriggered: root.shell.showWorkspaces("save")
        },
        OsAction {
            actionId: "workspace.open"
            text: qsTr("Open workspace…")
            category: root.categoryWorkspaces
            iconName: "layout-grid"
            onTriggered: root.shell.showWorkspaces("open")
        },
        // The current terminal tab (PLAN §6.4: Copy / Paste, Find in terminal).
        OsAction {
            actionId: "terminal.copy"
            text: qsTr("Copy")
            defaultShortcut: "Ctrl+Shift+C"
            category: root.categoryTerminal
            iconName: "copy"
            enabled: root.shell.currentTerminal !== null
            onTriggered: root.shell.currentTerminal.copy()
        },
        OsAction {
            actionId: "terminal.paste"
            text: qsTr("Paste")
            defaultShortcut: "Ctrl+Shift+V"
            category: root.categoryTerminal
            enabled: root.shell.currentTerminal !== null
            onTriggered: root.shell.currentTerminal.paste()
        },
        OsAction {
            actionId: "terminal.find"
            text: qsTr("Find in terminal")
            defaultShortcut: "Ctrl+Shift+F"
            category: root.categoryTerminal
            iconName: "search"
            enabled: root.shell.currentTerminal !== null
            onTriggered: root.shell.currentTerminal.openSearch()
        },
        OsAction {
            actionId: "terminal.selectAll"
            text: qsTr("Select all in terminal")
            category: root.categoryTerminal
            enabled: root.shell.currentTerminal !== null
            onTriggered: root.shell.currentTerminal.selectAll()
        },
        OsAction {
            actionId: "terminal.clearScrollback"
            text: qsTr("Clear scrollback")
            category: root.categoryTerminal
            iconName: "trash-2"
            enabled: root.shell.currentTerminal !== null
            onTriggered: root.shell.currentTerminal.clearScrollback()
        },
        // Font zoom of the current tab (PLAN §6.4).
        OsAction {
            actionId: "terminal.zoomIn"
            text: qsTr("Make the terminal text bigger")
            defaultShortcut: "Ctrl+="
            category: root.categoryTerminal
            iconName: "plus"
            enabled: root.shell.currentTerminal !== null
            onTriggered: root.shell.currentTerminal.zoom(1)
        },
        OsAction {
            actionId: "terminal.zoomOut"
            text: qsTr("Make the terminal text smaller")
            defaultShortcut: "Ctrl+-"
            category: root.categoryTerminal
            iconName: "minus"
            enabled: root.shell.currentTerminal !== null
            onTriggered: root.shell.currentTerminal.zoom(-1)
        },
        OsAction {
            actionId: "terminal.zoomReset"
            text: qsTr("Reset the terminal text size")
            defaultShortcut: "Ctrl+0"
            category: root.categoryTerminal
            iconName: "rotate-ccw"
            enabled: root.shell.currentTerminal !== null
            onTriggered: root.shell.currentTerminal.zoom(0)
        },
        OsAction {
            actionId: "terminal.toggleHighlight"
            text: qsTr("Toggle keyword highlighting in this pane")
            category: root.categoryTerminal
            iconName: "zap"
            enabled: root.shell.currentTerminal !== null
            onTriggered: root.shell.currentTerminal.toggleHighlight()
        },
        OsAction {
            actionId: "terminal.settings"
            text: qsTr("Terminal settings")
            category: root.categoryTerminal
            iconName: "square-terminal"
            onTriggered: root.shell.openSettings("terminal")
        },
        OsAction {
            actionId: "view.sidePanel"
            text: qsTr("Toggle side panel")
            defaultShortcut: "Ctrl+Shift+E"
            category: root.categoryView
            iconName: "panel-right"
            onTriggered: root.shell.toggleSidePanel()
        },
        OsAction {
            actionId: "app.settings"
            text: qsTr("Open settings")
            defaultShortcut: "Ctrl+,"
            category: root.categoryApp
            iconName: "settings"
            onTriggered: root.shell.showView("settings")
        },
        OsAction {
            actionId: "app.fullscreen"
            text: qsTr("Toggle full screen")
            defaultShortcut: "F11"
            category: root.categoryView
            iconName: "maximize-2"
            onTriggered: root.shell.toggleFullScreen()
        },
        OsAction {
            actionId: "view.hosts"
            text: qsTr("Go to Hosts")
            category: root.categoryView
            iconName: "server"
            onTriggered: root.shell.showView("hosts")
        },
        OsAction {
            actionId: "view.terminal"
            text: qsTr("Go to Terminal")
            category: root.categoryView
            iconName: "square-terminal"
            onTriggered: root.shell.openTerminal()
        },
        OsAction {
            actionId: "view.sftp"
            text: qsTr("Go to SFTP")
            category: root.categoryView
            iconName: "folder-sync"
            onTriggered: root.shell.showView("sftp")
        },
        OsAction {
            actionId: "view.tunnels"
            text: qsTr("Go to Tunnels")
            category: root.categoryView
            iconName: "waypoints"
            onTriggered: root.shell.showView("tunnels")
        },
        OsAction {
            actionId: "view.snippets"
            text: qsTr("Go to Snippets")
            category: root.categoryView
            iconName: "scroll-text"
            onTriggered: root.shell.showView("snippets")
        },
        OsAction {
            actionId: "view.keychain"
            text: qsTr("Go to Keychain")
            category: root.categoryView
            iconName: "key-round"
            onTriggered: root.shell.showView("keychain")
        },
        OsAction {
            actionId: "view.history"
            text: qsTr("Go to History")
            category: root.categoryView
            iconName: "history"
            onTriggered: root.shell.showView("history")
        },
        OsAction {
            actionId: "view.toggleStatusBar"
            text: AppSettings.showStatusBar ? qsTr("Hide status bar") : qsTr("Show status bar")
            category: root.categoryView
            iconName: "panel-bottom"
            onTriggered: AppSettings.showStatusBar = !AppSettings.showStatusBar
        },
        OsAction {
            actionId: "appearance.toggleTheme"
            text: qsTr("Switch theme (System, Dark, Light)")
            category: root.categoryAppearance
            iconName: "sun-moon"
            onTriggered: {
                const order = ["system", "dark", "light"];
                AppSettings.theme = order[(order.indexOf(AppSettings.theme) + 1) % order.length];
            }
        },
        OsAction {
            actionId: "appearance.toggleDensity"
            text: AppSettings.density === "compact" ? qsTr("Use comfortable density") : qsTr("Use compact density")
            category: root.categoryAppearance
            iconName: "rows-2"
            onTriggered: AppSettings.density = AppSettings.density === "compact" ? "comfortable" : "compact"
        },
        OsAction {
            actionId: "app.notifications"
            text: qsTr("Show notifications")
            category: root.categoryApp
            iconName: "bell"
            onTriggered: root.shell.toggleNotifications()
        },
        OsAction {
            actionId: "app.checkForUpdates"
            text: qsTr("Check for updates")
            category: root.categoryApp
            iconName: "refresh-cw"
            onTriggered: {
                Updater.check();
                Toasts.show(qsTr("Checking for updates…"), "info");
            }
        },
        OsAction {
            actionId: "app.update"
            text: Updater.canInstall ? qsTr("Update OpenSesh and restart") : qsTr("Open the download page")
            category: root.categoryApp
            iconName: "download"
            enabled: Updater.state === "available"
            onTriggered: {
                if (Updater.canInstall)
                    Updater.install();
                else
                    Qt.openUrlExternally(Updater.releaseUrl);
            }
        },
        OsAction {
            actionId: "app.openLogsFolder"
            text: qsTr("Open logs folder")
            category: root.categoryApp
            iconName: "folder-open"
            onTriggered: Qt.openUrlExternally(AppInfo.logsFolder)
        },
        OsAction {
            actionId: "app.openConfigFolder"
            text: qsTr("Open configuration folder")
            category: root.categoryApp
            iconName: "folder-open"
            onTriggered: Qt.openUrlExternally(AppInfo.configFolder)
        },
        OsAction {
            actionId: "app.quit"
            text: qsTr("Quit OpenSesh")
            defaultShortcut: "Ctrl+Shift+Q"
            category: root.categoryApp
            iconName: "log-out"
            onTriggered: root.window.close()
        },
        // Keyboard navigation between the window regions (not listed in the palette).
        OsAction {
            actionId: "focus.nextRegion"
            text: qsTr("Focus the next region")
            defaultShortcut: "F6"
            category: root.categoryView
            showInPalette: false
            onTriggered: root.shell.cycleRegion(1)
        },
        OsAction {
            actionId: "focus.previousRegion"
            text: qsTr("Focus the previous region")
            defaultShortcut: "Shift+F6"
            category: root.categoryView
            showInPalette: false
            onTriggered: root.shell.cycleRegion(-1)
        },
        // The same with Ctrl, which the terminal never takes: the keyboard way out of it (ADR 0011).
        OsAction {
            actionId: "focus.nextRegionAlt"
            text: qsTr("Focus the next region")
            defaultShortcut: "Ctrl+F6"
            category: root.categoryView
            showInPalette: false
            onTriggered: root.shell.cycleRegion(1)
        },
        OsAction {
            actionId: "focus.previousRegionAlt"
            text: qsTr("Focus the previous region")
            defaultShortcut: "Ctrl+Shift+F6"
            category: root.categoryView
            showInPalette: false
            onTriggered: root.shell.cycleRegion(-1)
        }
    ]

    // Debug builds only.
    readonly property list<OsAction> debugActions: [
        OsAction {
            actionId: "debug.panic"
            text: qsTr("Debug: trigger a panic")
            category: root.categoryDebug
            iconName: "bug"
            onTriggered: {
                Platform.debugPanic();
                // Still running: the panic hook is off unless the variable is set.
                Toasts.show(qsTr("No panic: start OpenSesh with OPENSESH_DEBUG_PANIC=1 to enable it."), "warning");
            }
        },
        OsAction {
            actionId: "debug.toast"
            text: qsTr("Debug: show sample notifications")
            category: root.categoryDebug
            iconName: "bug"
            onTriggered: {
                Toasts.show(qsTr("Connected to deploy@web-01."), "info");
                Toasts.show(qsTr("Settings saved."), "success");
                Toasts.show(qsTr("The host key changed since the last connection. Verify it before you continue."),
                            "warning");
                Toasts.show(qsTr("Connection to db-02 lost."), "danger", qsTr("Open logs"), "app.openLogsFolder");
            }
        }
    ]

    // Focus, resize and swap in each direction, created below.
    readonly property Component paneComponent: Component {
        OsAction {
            property string kind
            property string direction

            category: root.categoryPanes
            enabled: root.severalPanes
            onTriggered: {
                if (kind === "focus")
                    root.workspace.moveFocus(direction);
                else if (kind === "resize")
                    root.workspace.resizePane(direction);
                else
                    root.workspace.swapPane(direction);
            }
        }
    }

    // "Go to tab N" (Alt+1...Alt+9; Alt+9 is the last tab, like browsers), created below.
    readonly property Component gotoComponent: Component {
        OsAction {
            property int tabNumber

            category: root.categoryTabs
            showInPalette: false
            onTriggered: root.shell.gotoTab(tabNumber)
        }
    }

    Component.onCompleted: {
        for (const action of actions)
            ActionRegistry.register(action);
        const directions = [
            {
                id: "Left",
                focus: qsTr("Focus the pane on the left"),
                resize: qsTr("Resize the pane to the left"),
                swap: qsTr("Swap with the pane on the left"),
                icon: "chevron-left"
            },
            {
                id: "Right",
                focus: qsTr("Focus the pane on the right"),
                resize: qsTr("Resize the pane to the right"),
                swap: qsTr("Swap with the pane on the right"),
                icon: "chevron-right"
            },
            {
                id: "Up",
                focus: qsTr("Focus the pane above"),
                resize: qsTr("Resize the pane upward"),
                swap: qsTr("Swap with the pane above"),
                icon: "chevron-up"
            },
            {
                id: "Down",
                focus: qsTr("Focus the pane below"),
                resize: qsTr("Resize the pane downward"),
                swap: qsTr("Swap with the pane below"),
                icon: "chevron-down"
            }
        ];
        for (const d of directions) {
            const specs = [
                { kind: "focus", text: d.focus, shortcut: "Alt+" + d.id },
                { kind: "resize", text: d.resize, shortcut: "Alt+Shift+" + d.id },
                { kind: "swap", text: d.swap, shortcut: "" }
            ];
            for (const spec of specs) {
                ActionRegistry.register(paneComponent.createObject(root, {
                    actionId: "pane." + spec.kind + d.id,
                    text: spec.text,
                    defaultShortcut: spec.shortcut,
                    iconName: spec.kind === "swap" ? "arrow-left-right" : d.icon,
                    kind: spec.kind,
                    direction: d.id.toLowerCase()
                }));
            }
        }
        for (let n = 1; n <= 9; ++n) {
            const action = gotoComponent.createObject(root, {
                actionId: "tab.goto" + n,
                text: n === 9 ? qsTr("Go to the last tab") : qsTr("Go to tab %1").arg(n),
                defaultShortcut: "Alt+" + n,
                tabNumber: n
            });
            ActionRegistry.register(action);
        }
        if (Platform.debugBuild) {
            for (const action of debugActions)
                ActionRegistry.register(action);
        }
        // The defaults must never clash; the user's own choices are shown in Settings > Shortcuts.
        const seen = {};
        for (const action of ActionRegistry.actions) {
            const key = action.defaultShortcut.toLowerCase();
            if (key.length === 0)
                continue;
            if (seen[key])
                console.warn("AppActions: conflicting default shortcuts:", seen[key], action.actionId);
            seen[key] = action.actionId;
        }
    }
}
