# ADR 0017: Tabs as split trees of panes, broadcast input and detached windows

- **Status:** accepted
- **Date:** 2026-09-26
- **Sprint:** 4

## Context

PLAN Sprint 4 asks for window management at the level of a tiling window manager: tabs that can be renamed, colored, pinned, duplicated, reordered and reopened; splits as a binary tree of any depth with keyboard focus movement, resizing, swapping and zoom; tabs dragged out into new windows and back; broadcast input (MultiExec) with clear indicators "and no false positives"; saved workspaces; and a most-recently-used Ctrl+Tab switcher. Until Sprint 3 a tab was one `TerminalItem` whose tab id was also its session id, in one window. The layout must survive saving identically (the sprint's "done when"), and moving panes or tabs must never restart a shell.

## Options

1. **Nested QML split views** (`SplitView` inside `SplitView`), the tree living in the item hierarchy. Natural to draw, but splitting or closing re-parents the terminal items, keyboard neighbours and saving need a walk over items, and the geometry is hard to test.
2. **A tree model in Rust, panes drawn flat.** The tree is data with its operations in `opensesh-core`; QML keeps one item per pane, keyed by pane id, and places it at the rectangle the tree gives.
3. **One window per tab group, tabs managed by the window manager** (as some tiling terminals do). Portable only where the desktop cooperates, and no control over the tab strip.

## Decision

Option 2.

- **The tree** (`opensesh-core::workspace::layout`): a node is a pane (`{ pane = id }`) or a split (`{ split = "horizontal" | "vertical", ratio, first, second }`; horizontal puts the children side by side). `Layout` checks that ids are unique and clamps ratios to 0.05..0.95, and does `split`, `close` (the focus goes to the nearest pane of the sibling), `neighbor` in a direction (geometric: most shared edge, then the closest, then reading order), `resize` (the innermost divider on that side), `set_ratio` by path, `swap`, `equalize`, and gives the pane rectangles and the dividers in the unit square. Everything is unit-tested without Qt.
- **To QML** it goes through the stateless `Layouts` singleton, JSON in and JSON out. Each tab is a `TabWorkspace` that owns its layout string and a list model of panes; a `Repeater` keyed by pane id creates one `TerminalPane` per pane and binds its box to the geometry, so splitting, closing a neighbour, swapping or resizing only moves items. Dividers are separate items in the gap between panes, kept as the same items while only ratios change, so a drag is never interrupted. A maximized pane fills the tab and the others keep running underneath.
- **Sessions by pane id.** `TerminalSessions.allocateId()` hands out tab and pane ids from one counter for the whole process. A session lives in the registry until its pane or tab closes; the `TerminalItem` only attaches and detaches, and a session has one attachment at a time.
- **Detached windows** are a second `AppShell` with `detached: true` inside a `DetachedWindow`: tab strip, tabs and status bar, no rail, Home, views or side panel (those open in the main window). Moving a tab takes its live entry (layout, pane ids, broadcast state) out of one shell without ending its sessions and inserts it in another, whose panes attach to the same sessions. Dragging a tab reorders it inside the strip; dropped outside its window it goes to the window under the pointer, or to a new one. Wayland doesn't tell clients where windows are, so there the drop always opens a new window (placed by the compositor) and the tab menu's "Move to window" moves tabs back. Closing a detached window ends its tabs' sessions (they can be reopened); closing the main window closes the others.
- **One set of actions.** `AppActions` lives in the main window and acts on `WindowRegistry.activeShell`, the shell of the window in use; each window has its own `ShortcutHost`, whose window shortcuts only fire in their window. Pane actions on keys terminal programs also use (Alt+arrows, Alt+Shift+arrows) are enabled only while the tab has more than one pane, so with one pane the keys reach the program. Ctrl+Shift+W closes the focused pane (the tab with its last pane), as PLAN §6.4 says.
- **Broadcast** is per tab: every pane receives until it leaves. The receiving panes, and only those, get a border in `Theme.danger`, a "Receiving" chip, and the tab and status bar show a radio tower; a pane that left shows "Not receiving". Nothing is broadcast unless at least two panes receive. The copy happens in Rust: `TerminalItem.broadcastTargets` lists the other receiving panes, and each key is encoded again with each target's own terminal modes (a program in application cursor mode gets its own arrow keys); pastes are bracketed per target. A paste into several panes raises `pasteConfirmationNeeded` before the clipboard is read; once confirmed, it doesn't ask again until broadcast is turned off. Synchronized scrolling uses `scrollSyncTargets` the same way.
- **Ctrl+Tab** opens a most-recently-used switcher (`TabSwitcher`): a quick Ctrl+Tab switches to the previous tab at once, and holding Ctrl shows the list. Ctrl is read from the system (`Platform.keyboardModifiers()`, `QGuiApplication::queryKeyboardModifiers`), so releasing it outside the window still switches. Ctrl+PgUp/PgDn keep the strip order.
- **Tab colors** are names (red, orange, yellow, green, teal, blue, purple, pink) resolved by the theme for each scheme with at least 3:1 contrast on every surface, so a saved tab keeps a readable color in dark and light.

## Consequences

- The tree and its geometry have unit tests; the smoke test splits a tab, moves the focus and resizes, swaps, maximizes, broadcasts to two of three panes and checks the third never gets the text, restores a saved workspace and compares it, and moves a tab to a new window and back without ending its sessions.
- Terminal items are never re-parented, so a split or a move between windows keeps scrollback, selection state in the session and the running program.
- The layout is plain data, which the workspace files (ADR 0018) store as they are.
- Dragging tabs uses an internal drag, not the platform's drag and drop: nothing can be dropped from or to other applications, and the tab strip no longer flicks (the wheel scrolls it).
- Pinned tabs stay first and have no close button; "close others", "to the left" and "to the right" skip them.
