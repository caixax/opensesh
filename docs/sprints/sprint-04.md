# Sprint 4: Tabs, splits and workspace

**Goal:** window management at the level of a tiling window manager.

**Started:** 2026-09-26

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it.
- **A tab holds a tree of panes; each pane is one terminal session.** Sessions stay in the Rust registry (keyed by pane id), so splitting, closing a neighbour, swapping panes and moving a tab to another window never restart a shell.
- **Sessions are local terminals until SSH arrives (Sprint 7).** A saved workspace remembers each pane's profile and working directory (OSC 7), and restoring it starts a new shell there; the other session kinds join the same format later.
- **Detached windows** are secondary windows with the tab strip, the workspace and the status bar, but no rail or Home views. On Wayland the new window is not positioned (the compositor places it).

## Checklist

### Split tree (`opensesh-core`)
- [x] Binary tree of N levels: split, close, neighbour in a direction, resize, set a divider's ratio, swap, the pane rectangles and dividers, with tests
- [x] Workspace model: windows, tabs (title, color, pinned, focused and zoomed pane, layout) and panes (kind, profile, directory), saved as TOML, with tests

### Tabs
- [x] New, close, rename, reorder by dragging (and with the keyboard), pin, color, duplicate
- [x] Close others, to the left and to the right; reopen closed tabs
- [x] Activity and bell indicators for every pane of a tab; middle click closes
- [x] MRU switching with Ctrl+Tab (a switcher while Ctrl is held)

### Splits
- [x] Split right / down (Alt+Shift+= / Alt+Shift+-), close a pane (Ctrl+Shift+W)
- [x] Move the focus (Alt+arrows), resize with the mouse and the keyboard (Alt+Shift+arrows), swap, maximize (Ctrl+Shift+Z)

### Detached windows
- [x] Dragging a tab out of the window opens it in a new window; dragging it onto another window's tabs moves it there; the same from the tab menu with the keyboard

### Broadcast (MultiExec)
- [x] Ctrl+Shift+B turns broadcast on for a tab; each pane can join or leave
- [x] A colored border on the panes that receive input, and nowhere else
- [x] Pasting while broadcasting asks once
- [x] Optional synchronized scrolling

### Workspaces
- [x] Save the layout with its sessions and restore it identically; open, rename and delete saved workspaces
- [x] "Restore sessions at startup" (Settings > General) brings back the last session

### Quality
- [x] Unit tests for the tree, the workspace format and broadcast input
- [x] Smoke tests: splits, focus, zoom, broadcast reaching only the participants, a workspace saved and restored identically, a tab moved to a new window and back
- [x] Screenshots of split layouts and the broadcast indicators

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [x] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [x] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

### What was done

- **Split tree** (`opensesh-core::workspace::layout`, [ADR 0017](../adr/0017-tabs-panes-and-windows.md)): a binary tree of any depth with split, close (the focus goes to the nearest pane of the sibling), geometric neighbours, resize of the innermost divider on a side, divider ratios by path, swap, equalize, and the pane rectangles and dividers in the unit square. QML reaches it through the stateless `Layouts` singleton (JSON in and out).
- **Tabs and panes in QML:** `TabWorkspace` (one per tab) keeps the layout and a model of panes keyed by pane id, and lays the `TerminalPane`s out flat from the rectangles, so splitting, closing, swapping and resizing never recreate a terminal. Dividers sit in the gap between panes and keep their items while a drag changes the ratio. Sessions are now by pane id (`TerminalSessions.allocateId()`); a new pane starts with the profile, directory (OSC 7) and zoom of the one it splits (`TerminalItem.startDirectory`).
- **Tab features:** rename, eight theme-aware colors (`Theme.tabColors`, 3:1 on every surface), pin, duplicate, close others / to the left / to the right, reopen closed tabs (the last 20, in any window), reorder by dragging or Ctrl+Shift+PgUp / PgDn, the tab menu, indicators for every pane. Ctrl+Tab opens the most-recently-used switcher; Ctrl is read from the system so a release anywhere switches (`Platform.keyboardModifiers()`).
- **Detached windows:** `DetachedWindow` hosts an `AppShell` with `detached: true` (tabs, panes, status bar, palette; no rail or views). Tabs move out by dragging or from their menu and back the same way, keeping their sessions. `WindowRegistry` tracks the windows and the active one; the app's actions live once in `Main.qml` and act on the window in use; each window has its own shortcuts and only the active one shows new toasts.
- **Broadcast:** per tab, every pane receives until it leaves. Rust copies typed keys to the other receiving panes, encoded with each target's own modes, and pastes bracketed per target; a paste into several panes raises `pasteConfirmationNeeded` before the clipboard is read and asks once per broadcast. Red borders and "Receiving" chips only on receiving panes, and only when at least two receive; the tab and the status bar show it. Synchronized scrolling is optional.
- **Workspaces** ([ADR 0018](../adr/0018-workspace-files.md)): `workspaces/<id>.toml` with windows, tabs (name, color, pin, focused and maximized pane, layout) and panes (kind, profile, directory); pane ids are indexes in the file and new ids on opening. The `Workspaces` singleton saves, opens, renames and deletes them; the dialog is in the palette and the Terminal view. "Restore sessions at startup" saves `last-session.toml` in the data folder when the main window closes and opens it at startup.
- **Also:** Lucide `pin`, `pin-off` and `save` icons; the tab strip scrolls with the wheel instead of flicking; `OsTabButton.markColor`, `OsToastHost.accepting`; screenshots of split tabs with and without broadcast; the Settings text for session restore.

### "Done when" (PLAN)

- A complex layout is saved and restored identically: yes. The smoke test builds a three-pane tab with changed ratios, a name, a color and a focused pane, saves it through the real TOML format (`Workspaces.roundTrip()`), opens it and compares the trees (ratios to 0.001), the focused and maximized panes, name, color, pin and profiles; it also checks that every restored pane has a new id. The Rust tests do the same through the file format, including a broken file.
- Broadcast has clear indicators without false positives: yes. Only receiving panes get the border and chip, and only while two or more receive. The smoke test sends text from one receiving pane and checks it reaches exactly the other receiving pane, that a pane that left gets nothing and its own input goes nowhere else, that a paste asks first, and that turning broadcast off leaves no pane receiving. Target selection is unit-tested (each other pane once, never itself).

### How it was verified

- fmt, lint-qml, i18n, shaders, deny and audit on Windows; clippy `-D warnings` and every test (390) on Windows (Qt 6.10.3) and in Debian 13 (Qt 6.8.2), Fedora 43 (Qt 6.10.3) and Arch (Qt 6.11.2).
- Main and gallery smoke tests offscreen everywhere; the main one also with the native platform: Windows, and Wayland and X11 in each WSL distro (a detached window really opens there).
- Screenshots of the new terminal series in dark and light, comfortable and compact, with the real renderer on Windows.

### Pending

- **Manual matrix** ([manual-matrix.md](../testing/manual-matrix.md)): the split shortcuts on other keyboard layouts, dragging tabs between windows on X11 and Wayland, Ctrl+Tab with Ctrl held, broadcast into full-screen programs, and restoring a real session.
- **Sessions other than local terminals** join the workspace format with SSH (Sprint 7): the pane entry gets their fields.
- Windows don't remember their size or position in workspaces.
- "Confirm before closing with active sessions" (Settings > General) is still not wired; closing a tab or a detached window doesn't ask.
- Carried over: the open low-severity review items of Sprint 2, the Windows executable icon, an Ubuntu package.

### Risks

- Alt+arrows go to the app while a tab has several panes, so programs in split panes don't get them (word movement in some shells); the shortcuts can be changed in Settings > Shortcuts.
- The tab drag is internal: on Wayland, where windows can't be located, a tab dropped outside its window always gets a new window.
- Many panes mean many renderers; one pane costs what one tab did, and no limit is set.
