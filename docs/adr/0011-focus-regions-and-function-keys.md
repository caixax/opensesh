# ADR 0011: F6 moves the focus between window regions, and the terminal keeps function keys

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 1

## Context

The main window has five focus regions: title bar, rail, content, side panel and status bar. Tab follows them in order, and Sprint 1 added F6 / Shift+F6 to jump from one region to the next or previous one, as in browsers and most desktop apps.

PLAN §6.4 lists the default shortcuts and adds a rule: app shortcuts **never** take combinations that terminal programs need, which is why they use Shift and Alt. F6 and Shift+F6 are not in that table, and they break the rule:

- Midnight Commander uses F6 (move) and Shift+F6 (rename), and htop uses F6 (sort by).
- ShortcutHost binds every action as a window shortcut, so it fires whatever item has the focus, unless that item accepts the `ShortcutOverride` event for the key.
- If the terminal accepts `ShortcutOverride` for F6, F6 reaches the program, but it can no longer take the focus out of the terminal. That is exactly where a keyboard user needs it most.

There is no terminal yet (it arrives in Sprint 2), so nothing is broken today, but the choice has to be made before the terminal item exists.

## Options

1. **Keep F6 / Shift+F6 as window shortcuts everywhere.** It's simple and matches other apps, but terminal programs never get those keys.
2. **Use only a modified combination (Ctrl+F6 / Ctrl+Shift+F6).** Terminal programs keep the function keys, but keyboard users lose the key they know from other apps in every region.
3. **Keep F6 / Shift+F6, let the terminal take function keys, and add Ctrl+F6 / Ctrl+Shift+F6 as alternatives that always work.**

## Decision

Option 3:

- **F6 / Shift+F6** stay the default keys to move between regions (`focus.nextRegion`, `focus.previousRegion`).
- **Ctrl+F6 / Ctrl+Shift+F6** are registered as hidden alternatives (`focus.nextRegionAlt`, `focus.previousRegionAlt`, not listed in the command palette). The terminal never takes them, so they are the keyboard way out of the terminal.
- **From Sprint 2**, the terminal item accepts `ShortcutOverride` for function keys without Ctrl or Alt, so F6, Shift+F6 and the other function keys reach the program. F11 stays with the app (full screen, PLAN §6.4), as in most terminal emulators.

## Consequences

- Outside the terminal, F6 behaves as in other desktop apps. Inside it, programs such as mc and htop get their function keys.
- Leaving the terminal from the keyboard takes Ctrl+F6. The shortcuts help and the keyboard settings (a later sprint) must show it.
- A terminal program can't receive Ctrl+F6 or Ctrl+Shift+F6. Programs rarely use them, and all shortcuts will be editable.
- This is the one documented exception to the PLAN §6.4 rule, and the CHANGELOG says so.
- Sprint 2 must add the `ShortcutOverride` handling to the terminal item and test it with mc and htop.
