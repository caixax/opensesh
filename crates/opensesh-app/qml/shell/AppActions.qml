pragma ComponentBehavior: Bound

// The main window's actions (PLAN §6.4 defaults), registered in ActionRegistry: the command
// palette lists them and ShortcutHost binds their shortcuts. App shortcuts use Shift or Alt
// combinations so they never take keys terminal programs need (Ctrl+A, Ctrl+B, Ctrl+K, Ctrl+R...).
// Debug builds add a few "Debug:" actions. Conflicting shortcuts are reported with console.warn,
// which fails the smoke test.
//   shell: Item      the AppShell
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
    readonly property string categoryDebug: qsTr("Debug")

    readonly property ComingSoon comingSoon: ComingSoon {}

    readonly property list<OsAction> actions: [
        OsAction {
            actionId: "app.commandPalette"
            text: qsTr("Command palette")
            shortcut: "Ctrl+Shift+P"
            category: root.categoryApp
            iconName: "command"
            onTriggered: root.shell.togglePalette()
        },
        OsAction {
            actionId: "app.quickConnect"
            text: qsTr("Quick connect")
            shortcut: "Ctrl+Shift+O"
            category: root.categorySessions
            iconName: "plug-zap"
            onTriggered: root.comingSoon.notify(qsTr("Quick connect"), 5)
        },
        OsAction {
            actionId: "tab.newLocal"
            text: qsTr("New local terminal tab")
            shortcut: "Ctrl+Shift+T"
            category: root.categoryTabs
            iconName: "square-terminal"
            onTriggered: root.shell.newTab()
        },
        OsAction {
            actionId: "tab.duplicate"
            text: qsTr("Duplicate tab")
            shortcut: "Ctrl+Shift+D"
            category: root.categoryTabs
            iconName: "copy"
            onTriggered: root.comingSoon.notify(qsTr("Duplicating tabs"), 4)
        },
        OsAction {
            actionId: "tab.close"
            text: qsTr("Close tab")
            shortcut: "Ctrl+Shift+W"
            category: root.categoryTabs
            iconName: "x"
            enabled: root.shell.currentTab > 0
            onTriggered: root.shell.closeTab(root.shell.currentTab)
        },
        OsAction {
            actionId: "tab.reopenClosed"
            text: qsTr("Reopen closed tab")
            shortcut: "Ctrl+Alt+Shift+T"
            category: root.categoryTabs
            iconName: "rotate-ccw"
            onTriggered: root.comingSoon.notify(qsTr("Reopening closed tabs"), 4)
        },
        OsAction {
            actionId: "tab.next"
            text: qsTr("Next tab")
            shortcut: "Ctrl+Tab"
            category: root.categoryTabs
            iconName: "chevron-right"
            enabled: root.shell.sessionCount > 0
            onTriggered: root.shell.cycleTab(1)
        },
        OsAction {
            actionId: "tab.previous"
            text: qsTr("Previous tab")
            shortcut: "Ctrl+Shift+Tab"
            category: root.categoryTabs
            iconName: "chevron-left"
            enabled: root.shell.sessionCount > 0
            onTriggered: root.shell.cycleTab(-1)
        },
        // Alternative keys for the same two commands (PLAN §6.4).
        OsAction {
            actionId: "tab.nextAlt"
            text: qsTr("Next tab")
            shortcut: "Ctrl+PgDown"
            category: root.categoryTabs
            iconName: "chevron-right"
            showInPalette: false
            enabled: root.shell.sessionCount > 0
            onTriggered: root.shell.cycleTab(1)
        },
        OsAction {
            actionId: "tab.previousAlt"
            text: qsTr("Previous tab")
            shortcut: "Ctrl+PgUp"
            category: root.categoryTabs
            iconName: "chevron-left"
            showInPalette: false
            enabled: root.shell.sessionCount > 0
            onTriggered: root.shell.cycleTab(-1)
        },
        OsAction {
            actionId: "view.sidePanel"
            text: qsTr("Toggle side panel")
            shortcut: "Ctrl+Shift+E"
            category: root.categoryView
            iconName: "panel-right"
            onTriggered: root.shell.toggleSidePanel()
        },
        OsAction {
            actionId: "app.settings"
            text: qsTr("Open settings")
            shortcut: "Ctrl+,"
            category: root.categoryApp
            iconName: "settings"
            onTriggered: root.shell.showView("settings")
        },
        OsAction {
            actionId: "app.fullscreen"
            text: qsTr("Toggle full screen")
            shortcut: "F11"
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
            onTriggered: root.shell.showView("terminal")
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
            shortcut: "Ctrl+Shift+Q"
            category: root.categoryApp
            iconName: "log-out"
            onTriggered: root.window.close()
        },
        // Keyboard navigation between the window regions (not listed in the palette).
        OsAction {
            actionId: "focus.nextRegion"
            text: qsTr("Focus the next region")
            shortcut: "F6"
            category: root.categoryView
            showInPalette: false
            onTriggered: root.shell.cycleRegion(1)
        },
        OsAction {
            actionId: "focus.previousRegion"
            text: qsTr("Focus the previous region")
            shortcut: "Shift+F6"
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
        for (let n = 1; n <= 9; ++n) {
            const action = gotoComponent.createObject(root, {
                actionId: "tab.goto" + n,
                text: n === 9 ? qsTr("Go to the last tab") : qsTr("Go to tab %1").arg(n),
                shortcut: "Alt+" + n,
                tabNumber: n
            });
            ActionRegistry.register(action);
        }
        if (Platform.debugBuild) {
            for (const action of debugActions)
                ActionRegistry.register(action);
        }
        const conflicts = ActionRegistry.conflicts();
        if (conflicts.length > 0)
            console.warn("AppActions: conflicting shortcuts:", JSON.stringify(conflicts));
    }
}
