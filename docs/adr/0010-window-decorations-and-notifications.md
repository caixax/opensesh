# ADR 0010: Window decoration modes, tiling detection and in-app notifications

- **Status:** accepted
- **Date:** 2026-09-25
- **Sprint:** 1

## Context

PLAN §5.3 describes the window chrome:

- a custom title row that holds the tabs, a command palette hint and the window buttons;
- decoration modes `auto | custom | native | none`;
- in `auto` mode, tiling compositors (Hyprland, Sway, river, niri, i3) are detected and only the window buttons are hidden;
- dragging uses `startSystemMove()`.

Sprint 1 also asks for "toasts and notifications".

Facts verified in Sprint 1 research:

- `Qt.FramelessWindowHint` disables both client-side and server-side decorations on Wayland.
- On Windows, a frameless window has no native hit-testing, so it needs our own resize edges calling `startSystemResize(edges)`.
- `startSystemMove()` and `startSystemResize()` are callable from QML in Qt 6.8. On Wayland they must be called from a press or drag.
- Hyprland, niri and sway set socket variables, but those can be stale in the systemd or D-Bus environment. river sets no reliable variable.

## Decision

**Detection (`opensesh-core::desktop`):**
- **Hyprland:** tiling when `HYPRLAND_INSTANCE_SIGNATURE` is set and its socket exists (when `XDG_RUNTIME_DIR` is known).
- **sway, niri, i3:** tiling when `SWAYSOCK`, `NIRI_SOCKET` or `I3SOCK` points to an existing socket.
- **Everything else:** tiling when any entry of `XDG_CURRENT_DESKTOP` (colon-separated, case-insensitive) is a known tiling WM. river is detected only when the session sets that variable.

**Modes:**

| Mode | Behavior |
|---|---|
| `auto` | `none` on tiling compositors, `custom` elsewhere |
| `custom` | Frameless window with our title row: logo, tabs, palette hint and window buttons. Dragging uses `startSystemMove()`; edge handlers use `startSystemResize()`; double-click toggles maximize. |
| `native` | The system title bar; our tab row sits below it, with no window buttons |
| `none` | Frameless, with no window buttons (for tiling compositors). The tab row stays and remains draggable. |

**Window state:** size, maximized state and (off Wayland) position persist in `state.toml`.

**Notifications:** in-app **toasts** (`Toasts` singleton plus `OsToastHost`) and a notification history panel in the status bar. Native OS notifications (freedesktop or Windows toasts) are postponed until the first feature that needs them, the terminal bell's "notification" mode in Sprint 3, where they'll get their own ADR.

## Consequences

- Tiling users get a clean tab row without useless buttons, and floating desktops get the designed title bar.
- Frameless mode loses some native niceties: Windows 11 snap layouts on the maximize button, and the native shadow. `native` stays available for anyone who prefers the system frame.
- Detection is heuristic. The user can always force a mode in Settings > Appearance.
