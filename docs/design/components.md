# OpenSesh component library: contract and conventions

This document is the contract for every QML file under `crates/opensesh-app/qml/`. It covers the `Theme` API (the only source of colors and sizes), how `Os*` components are built, and what each one must support. PLAN §5 is the source of truth for the look. This file is the source of truth for the code.

## 1. `Theme` (QML singleton, implemented in Rust)

`import cc.caixa.opensesh` gives you `Theme`. **Never write a color literal, `Qt.rgba()`, `Qt.darker()`, `Qt.lighter()`, `Qt.alpha()` or a named color in QML.** `cargo xtask lint-qml` (run in CI) rejects them. If you need a color that doesn't exist, add a token in `opensesh-core::theme`.

### Inputs

Bound once per window by `shell/ThemeBinder.qml` from `AppSettings` and the OS color scheme; components never write them:
`requestedMode`, `requestedAccent`, `requestedDensity`, `uiScale`, `reduceMotion`, `uiFontFamily`, `systemDark`.
The gallery and the screenshot runs use `ThemeBinder`'s `override*` properties, so they never write the user's settings.

### Colors (`color`, read-only)

| Token | Use |
|---|---|
| `bg` | Window background |
| `surface` | Cards, panels, popups, dialogs |
| `surface2` | Inputs, rail, alternate/raised areas |
| `border` | Decorative hairlines and separators (low contrast by design) |
| `borderStrong` | Outlines that identify a control: text fields, checkbox boxes, switch tracks (≥ 3:1) |
| `text` | Primary text and icons |
| `textMuted` | Secondary text, placeholders, captions (≥ 4.5:1) |
| `textDisabled` | Disabled text and icons |
| `accent` | Accent fills: primary buttons, selected indicators, checked states, progress |
| `accentText` | Text and icons **on** an `accent` fill (≥ 4.5:1, computed) |
| `accentFg` | Accent used **as** text or icon color on `bg`/`surface`/`surface2` (links, active rail icon) |
| `defaultAccent` | The default ("Sesame") accent of the current scheme, whatever accent is chosen: the first swatch of accent pickers |
| `success`, `warning`, `danger`, `info` | Status fills, borders and icons (≥ 3:1 on `surface`) |
| `focusRing` | Keyboard focus indicator |
| `hover` | Translucent overlay for hovered items (draw on top of the item's own fill) |
| `pressed` | Translucent overlay for pressed items |
| `selection` | Text selection and selected rows (translucent accent) |
| `scrim` | Dimming layer behind modal dialogs and drawers |

**Accent presets:** `accentPresets` (constant list of `"#RRGGBB"` strings: amber, terracotta, rose, lavender, blue, teal, green) is the only source of preset accents; never repeat the hex codes in QML.

**Text on a status fill:** use `Theme.textOn(Theme.danger)`. `textOn(color)` returns the readable ink for any fill.

### Sizes (`real`, logical px; they follow the density and the UI scale)

| Group | Tokens |
|---|---|
| Spacing (4 px scale, §5.1) | `spacingXs` 4, `spacingSm` 8, `spacingMd` 12, `spacingLg` 16, `spacingXl` 24, `spacingXxl` 32. The values shown are at scale 1; they are not affected by density. |
| Controls | `controlHeight`, `controlHeightSmall`, `controlPadding`, `rowHeight` |
| Icons | `iconSize`, `iconSizeSmall` |
| Shell | `titleBarHeight`, `railWidth`, `railWidthLabels`, `statusBarHeight` |
| Shape | `radiusCard` (8), `radiusControl` (6), `radiusSmall` (4), `borderWidth` (1), `focusRingWidth` (2) |
| Type | `fontSizeSmall`, `fontSize`, `fontSizeLarge`, `fontSizeTitle`, `fontFamily` (UI, Inter by default), `monoFontFamily` (JetBrains Mono) |
| Motion | `durationFast` (120 ms), `durationNormal` (180 ms). Both are **0 when "reduce motion" is on**. Always use these, never literal durations. |

### Flags

`dark` (bool), `compact` (bool), `accentLowContrast` (bool: the user's accent is under 3:1 against `bg`, so Settings shows a warning).

## 2. Building a component

- **One component per file:** `qml/components/Os<Name>.qml`. It is exposed by the QML module `cc.caixa.opensesh` because every `.qml` file is listed automatically by `build.rs`.
- **Base on `QtQuick.Templates`** (`import QtQuick.Templates as T`) when a Qt control exists (`T.Button`, `T.TextField`, `T.ComboBox`, `T.Switch`, `T.CheckBox`, `T.Slider`, `T.SpinBox`, `T.TabBar`, `T.TabButton`, `T.Dialog`, `T.Drawer`, `T.Menu`, `T.MenuItem`, `T.ToolTip`, `T.ProgressBar`, `T.SplitView`, `T.ItemDelegate`, `T.ScrollBar`, `T.Popup`). The template supplies behavior, keyboard handling and the right `Accessible.role`; we supply `background`, `contentItem` and `indicator`, painted only with `Theme` tokens.
  - Follow the implicit-size pattern of Qt's Basic style: `implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitContentWidth + leftPadding + rightPadding)`.
- **Fallback style:** the app forces the Qt Quick Controls **Basic** style, so any stock control that slips in (for example a `ScrollBar` inside a `ScrollView`) is light and predictable. Prefer our own `OsScrollBar` where it is visible.
- **Text:** use `OsText` (a `Text` with `Theme.fontFamily`, `Theme.fontSize` and `Theme.text` defaults) for labels. Every user-visible literal goes through `qsTr()`. Identifiers, icon names and object names don't.
- **Icons:** use `OsIcon { name: "search"; color: Theme.text; size: Theme.iconSize }` with a name from `assets/icons/icons.toml`. Never use a file path, never inline SVG.
- **Focus:** every interactive component shows a visible focus ring when it has **keyboard** focus (`visualFocus` on templates; for custom items, `activeFocus` plus the last input being the keyboard, which they pass to `OsFocusRing` as `keyboardFocus`). Use `OsFocusRing { target: control }`: a rounded outline in `Theme.focusRing`, `Theme.focusRingWidth` thick, drawn outside the control. Tab order follows the visual order; `activeFocusOnTab: true` on custom interactive items.
- **Keyboard:** buttons activate with Space/Enter; lists and rails move with the arrow keys and Home/End; popups close with Escape; menus open with the Menu key and Shift+F10 where it applies.
- **Accessibility:** set `Accessible.name` (translated) on every interactive component. Icon-only buttons **must** take a `text`/`toolTip` used as the name. Set `Accessible.role` whenever the template doesn't already set the right one, and `Accessible.description` for extra hints.
- **States:** hover and pressed use the `Theme.hover` / `Theme.pressed` overlays. Disabled uses `Theme.textDisabled` and no hover. Every state change animates with `Theme.durationFast` (a `Behavior on color` is fine).
- **Density:** heights come from `Theme.controlHeight` / `rowHeight`, never literals. Compact mode must never clip text.
- **No heavy shadows (§5.1):** separate things with surfaces and 1 px `Theme.border` lines. Popups may use `Theme.borderStrong` outlines.
- **Minimal public API:** follow the Qt Quick Controls names (`text`, `checked`, `value`, `model`, `currentIndex`, ...), add a few OpenSesh ones (`variant`, `iconName`), and document each extra property in a comment at the top of the file.

## 3. The library (§5.5)

| Component | Base | Notes |
|---|---|---|
| `OsText` | `Text` | Theme font and color defaults; `muted: bool`, `size: "small" \| "normal" \| "large" \| "title"` |
| `OsIcon` | `Image` | `name`, `color`, `size`; loads `image://icon/<name>?color=..&size=..` |
| `OsFocusRing` | `Rectangle` | Visible only on keyboard focus of `target`; for a plain `Item` target (no focus reason), set `keyboardFocus` when its focus came from the keyboard |
| `OsFocusReturn` | `QtObject` | Popup helper: `save()` on `aboutToShow`, `restore()` on `closed` gives the focus back to the opener with its focus reason (so its ring stays) |
| `OsButton` | `T.Button` | `variant: "primary" \| "secondary" \| "ghost" \| "danger"`, optional `iconName` |
| `OsIconButton` | `T.Button` | Square, icon only; `iconName`, `toolTip` (also the accessible name), optional `checkable` |
| `OsTextField` | `T.TextField` | Placeholder in `textMuted`, `borderStrong` outline, accent focus |
| `OsSearchField` | `OsTextField` | Leading search icon, clear button, Escape clears |
| `OsPasswordField` | `OsTextField` | `echoMode: Password` with a reveal toggle |
| `OsComboBox` | `T.ComboBox` | Themed popup and delegates |
| `OsSwitch` | `T.Switch` | |
| `OsCheckBox` | `T.CheckBox` | Also the partially-checked state |
| `OsSlider` | `T.Slider` | |
| `OsSpinBox` | `T.SpinBox` | |
| `OsColorPicker` | `Item` | Preset swatches (`Theme.accentPresets`) plus a `#RRGGBB` field, `value` property, `accepted` signal; optional default swatch (`showDefault`, `defaultSelected`, `defaultPicked` signal) in `Theme.defaultAccent`, so a setting can store `"default"` |
| `OsFontPicker` | `OsComboBox`-like | Families from `Platform.fontFamilies(monospaceOnly)` with a live preview |
| `OsKeybindCapture` | `Item` | Records a key combination and shows it as text (`Platform.keySequenceText`); Escape cancels, Backspace clears |
| `OsTabBar` / `OsTabButton` | `T.TabBar` / `T.TabButton` | Title-bar tabs: icon, title, close button, activity dot |
| `OsRail` / `OsRailItem` | `Item` / `T.AbstractButton` | Vertical icon navigation, optional labels, arrow-key navigation |
| `OsCard` | `Rectangle`/`Item` | `surface`, `radiusCard`, 1 px `border`; optional hover and click |
| `OsListRow` | `T.ItemDelegate` | Icon, title, subtitle, trailing content, selected state |
| `OsTreeView` | `ListView` | Flattened tree (`nodes: [{id, text, iconName, children: [...]}]`); arrows expand and collapse |
| `OsTag` | `Rectangle` | Small pill, optional remove button |
| `OsBadge` | `Rectangle` | Count or dot; `variant` uses the status colors with `Theme.textOn(...)` |
| `OsDialog` | `T.Dialog` | Modal over a `scrim`; title, content, footer buttons; gives the focus back on close (`OsFocusReturn`) |
| `OsDrawer` | `T.Drawer` | Side sheet; gives the focus back on close (`OsFocusReturn`) |
| `OsContextMenu` / `OsMenuItem` | `T.Menu` / `T.MenuItem` | Icons and shortcut text; a top-level menu gives the focus back on close (`OsFocusReturn`) |
| `OsTooltip` | `T.ToolTip` | Short delay, `surface2` background |
| `OsToast` | `Rectangle` | Transient message with `kind` (`info`/`success`/`warning`/`danger`) and an optional action; `focusButton(reason)` |
| `OsEmptyState` | `Item` | Icon, title, body and action buttons |
| `OsSplitter` | `T.SplitView` | Themed handle, keyboard resizable |
| `OsCommandPalette` | `T.Popup` | Search field and fuzzy-filtered action list (see `ActionRegistry`) |
| `OsProgress` | `T.ProgressBar` | Determinate and indeterminate |
| `OsSectionHeader` | `Item` | Section title with an optional trailing action |
| `OsFormRow` | `Item` | Label, control and help/error text, aligned in forms |
| `OsScrollBar` | `T.ScrollBar` | Thin, themed |

Every component appears in the Gallery (`opensesh-app --gallery`) in every state: default, hover (where it can be shown statically), focused, disabled and checked or error.

## 4. Shell singletons and registries (QML)

| Name | Kind | Responsibility |
|---|---|---|
| `Theme`, `AppSettings`, `AppInfo`, `Platform`, `UiState` | Rust | Tokens, persisted settings, startup info, OS helpers, remembered UI state |
| `ActionRegistry` | QML singleton | The single list of user actions. Each is an `OsAction` (`actionId`, `text`, `shortcut`, `category`, `iconName`, `enabled`, `showInPalette`, signal `triggered`) declared next to its handler and added with `register(action)`. `find(id)`, `trigger(id)`, `search(query)` (fuzzy, for the palette) and `conflicts()` (duplicate shortcuts). The shell's `ShortcutHost` creates one `Shortcut` per action. |
| `Toasts` | QML singleton | `show(text, kind, actionText, actionId)` plus the history (`history`, `unread`) used by the notifications panel |

`AppSettings` notes: every setter validates and saves in the background. `readOnly` is true when `config.toml` must not be overwritten, and `readOnlyReason` says why: `"newer"` (written by a newer OpenSesh) or `"unreadable"` (a syntax error; Restore defaults replaces it and keeps a backup). Changes still apply in memory. `reloadedFromDisk` fires after an external edit, and `problem(message)` reports a failed save or a rejected edit.

## 5. Shell helpers (`qml/shell/`)

| File | Responsibility |
|---|---|
| `AppShell.qml` | The main window's content: title bar, rail, views, side panel, status bar, command palette, notifications, toasts |
| `TitleBar.qml`, `SessionTabStrip.qml`, `WindowButtons.qml`, `WindowResizeHandles.qml` | Custom title bar with tabs, window buttons for the `custom` decoration mode, and frameless move/resize through `startSystemMove()` / `startSystemResize()` |
| `StatusBar.qml`, `SidePanel.qml`, `NotificationsPanel.qml` | Bottom bar, collapsible side panel, notification history drawer |
| `AppActions.qml`, `ShortcutHost.qml` | The shell's `OsAction`s (PLAN §6.4 defaults) and one `Shortcut` per action |
| `ThemeBinder.qml` | Feeds `Theme` (see §1) |
| `SmokeTest.qml` | `--smoke-test`: first frame, bridge and encoding checks, then runs `steps` (functions; a step may return more steps) and exits |
| `ScreenshotRunner.qml` | `--screenshots <dir>`: every page in dark/light × comfortable/compact |
