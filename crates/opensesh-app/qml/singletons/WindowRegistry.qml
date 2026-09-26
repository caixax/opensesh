pragma Singleton

// The app's windows (PLAN Sprint 4): the main window and the detached ones that tabs move to.
// Every AppShell registers itself; the actions and the command palette act on `activeShell`,
// the shell of the window in use. Also keeps the recently closed tabs for "Reopen closed tab".
//   mainShell: Item      the main window's AppShell
//   activeShell: Item    read-only; the shell of the last active window, else the main one
//   shells: var          read-only; every registered shell, the main one first
// Functions: register(shell), unregister(shell), activated(shell), openWindow(entries, point),
// shellAt(point), windowName(shell), capture(), openWorkspace(workspace, shell),
// closeDetached(), rememberClosed(entry), takeClosed().
import QtQuick
import cc.caixa.opensesh

QtObject {
    id: registry

    property Item mainShell: null
    property Item lastActive: null
    readonly property Item activeShell: lastActive ?? mainShell
    property var shells: []
    // The detached windows (the references also keep them alive).
    property var windows: []
    // Recently closed tabs as workspace entries, the newest last.
    property var closedTabs: []
    readonly property int maxClosedTabs: 20
    property Component windowComponent: null
    // Wayland doesn't tell windows where they are, so drops can't be matched to windows.
    readonly property bool positionsKnown: Qt.platform.pluginName !== "wayland"

    function register(shell) {
        if (shells.indexOf(shell) >= 0)
            return;
        if (shell.detached) {
            shells = shells.concat([shell]);
        } else {
            mainShell = shell;
            shells = [shell].concat(shells);
        }
    }

    function unregister(shell) {
        shells = shells.filter(existing => existing !== shell);
        windows = windows.filter(window => window.shell !== shell);
        if (lastActive === shell)
            lastActive = null;
        if (mainShell === shell)
            mainShell = null;
    }

    // A window became active.
    function activated(shell) {
        lastActive = shell;
    }

    // Opens a detached window with the tabs of `entries` (see AppShell.adoptTab) at `point`
    // (global coordinates; ignored where windows can't be placed). Returns its shell, or null.
    function openWindow(entries, point) {
        if (!windowComponent)
            windowComponent = Qt.createComponent("cc.caixa.opensesh", "DetachedWindow");
        if (windowComponent.status !== Component.Ready) {
            console.warn("WindowRegistry: could not create a window:", windowComponent.errorString());
            return null;
        }
        const main = mainShell ? mainShell.window : null;
        const properties = {
            entries: entries,
            width: main ? Math.round(main.width * 0.8) : 960,
            height: main ? Math.round(main.height * 0.8) : 640
        };
        if (point && positionsKnown) {
            properties.x = Math.round(point.x - Theme.spacingXxl * 2);
            properties.y = Math.round(point.y - Theme.titleBarHeight / 2);
        }
        const window = windowComponent.createObject(null, properties);
        if (!window)
            return null;
        windows = windows.concat([window]);
        return window.shell;
    }

    // The shell whose window contains `point` (global coordinates), the active one first; null
    // when there is none or positions are unknown.
    function shellAt(point) {
        if (!positionsKnown || !point)
            return null;
        const ordered = activeShell ? [activeShell].concat(shells.filter(shell => shell !== activeShell)) : shells;
        for (const shell of ordered) {
            const window = shell.window;
            if (window.visible && point.x >= window.x && point.x < window.x + window.width
                    && point.y >= window.y && point.y < window.y + window.height)
                return shell;
        }
        return null;
    }

    function windowName(shell) {
        if (shell === mainShell)
            return qsTr("Main window");
        return qsTr("Window %1").arg(shells.indexOf(shell) + 1);
    }

    // Every window's tabs, as a workspace (see Workspaces); windows without tabs are left out.
    function capture() {
        return {
            name: "",
            windows: shells.map(shell => shell.captureWindow()).filter(window => window.tabs.length > 0)
        };
    }

    // Opens a workspace from Workspaces.open() or lastSession() (new ids, so new shells): its
    // first window's tabs go to `shell`, the others open as new windows.
    function openWorkspace(workspace, shell) {
        const target = shell ?? activeShell;
        const list = workspace && Array.isArray(workspace.windows) ? workspace.windows : [];
        for (let i = 0; i < list.length; ++i) {
            const tabs = list[i].tabs || [];
            if (tabs.length === 0)
                continue;
            if (i === 0 && target) {
                target.openTabs(tabs, list[i].currentTab);
                continue;
            }
            const other = openWindow(tabs);
            if (other)
                other.selectTab(Math.min((list[i].currentTab || 0) + 1, other.sessionCount));
        }
    }

    // The main window is closing: the detached windows go too (their sessions end).
    function closeDetached() {
        for (const window of windows.slice())
            window.close();
    }

    function rememberClosed(entry) {
        const list = closedTabs.concat([entry]);
        closedTabs = list.length > maxClosedTabs ? list.slice(list.length - maxClosedTabs) : list;
    }

    // The most recently closed tab, removed from the list; null when there is none.
    function takeClosed() {
        if (closedTabs.length === 0)
            return null;
        const entry = closedTabs[closedTabs.length - 1];
        closedTabs = closedTabs.slice(0, closedTabs.length - 1);
        return entry;
    }
}
