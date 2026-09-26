# Manual test matrix

PLAN §10 asks for a manual pass on every Tier 1 environment in each sprint with UI changes. This file records **what was actually run**, when, and on which machine.

**Legend:** ✅ passed · ❌ failed · ⏳ not run yet (needs that environment) · — not applicable

## Sprint 4 (2026-09-26)

### Automated checks

- **Main window (`--smoke-test`, 102 to 105 steps):** as before, plus the workspaces dialog and the tab rename dialog. In real terminals it splits a tab into three panes, moves the focus in every direction, resizes, swaps and maximizes; broadcasts from one pane to a second while the third has left, and checks that the text reaches exactly the two receiving panes, that the third's own input stays there, and that a paste asks first; saves a tab (name, color, ratios, focused pane, profiles) through `Workspaces.roundTrip()`, opens it and compares it; moves the tab to a new window and back and checks that its sessions never ended; then duplicates, pins, closes, reopens and switches tabs (Ctrl+Tab) and closes the others. Nothing is written.
- **Gallery (`--gallery --smoke-test`):** unchanged.
- **Screenshots:** a new series, `terminal-splits-*` and `terminal-broadcast-*`, reviewed on Windows with the real renderer (the panes show the renderer's demo frame; offscreen captures leave terminals blank, as in the gallery).

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) | Native (main) |
|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ | ✅ / ✅ | ✅ `windows` |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ | ✅ / ✅ | ✅ Wayland, ✅ X11 |
| Fedora 43 (WSLg) | 6.10.3 (distro) | ✅ | ✅ / ✅ | ✅ Wayland, ✅ X11 |
| Arch Linux (WSLg) | 6.11.2 (distro) | ✅ (GCC 16 warnings from Qt headers and cxx's generated code, as before) | ✅ / ✅ | ✅ Wayland, ✅ X11 |

### Manual checks

| Check | Windows 10 | Linux |
|---|---|---|
| Alt+Shift+= and Alt+Shift+- split on US, Spanish and German keyboard layouts | ⏳ | ⏳ |
| Alt+arrows reach the shell (word movement) while a tab has one pane, and move the focus with several | ⏳ | ⏳ |
| Dragging a divider, and the pane's terminal size following it (`stty size`) | ⏳ | ⏳ |
| Dragging a tab along the strip, onto another window, and out of every window (X11 places the new window at the pointer; Wayland lets the compositor place it) | ⏳ | ⏳ X11, ⏳ Wayland |
| Ctrl+Tab with Ctrl held shows the list; a quick press switches at once; releasing Ctrl outside the window switches | ⏳ | ⏳ |
| Broadcast to vim in one pane and a shell in another: arrows work in both (application cursor mode) | ⏳ | ⏳ |
| "Restore sessions at startup": quit with split tabs in two windows, start again, same layouts and folders (a shell that reports its directory with OSC 7) | ⏳ | ⏳ |

## Sprint 3 (2026-09-26)

### Automated checks

- **Main window (`--smoke-test`, 84 to 87 steps):** as before, plus Settings > Terminal (with its live preview), Profiles, Themes and Shortcuts, the highlighting rule editor and the theme editor. In a real terminal tab it creates a profile, checks that a profile edit reaches the open terminal (`fontSize`), zooms the tab, toggles highlighting, changes and resets a shortcut, and deletes the profile. Test runs keep these changes in memory: nothing is written.
- **Gallery (`--gallery --smoke-test`):** unchanged.

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery) |
|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ | ✅ / ✅ |
| Debian 13 (WSL) | 6.8.2 (distro) | ✅ | ✅ / ✅ |
| Fedora 43 (WSL) | 6.10.3 (distro) | ✅ | ✅ / ✅ |
| Arch Linux (WSL) | 6.11.2 (distro) | ✅ (GCC 16 warnings from Qt headers, as in Sprint 1) | ✅ / ✅ |

Screenshots (`--screenshots`) of Settings > Appearance, Terminal, Profiles, Themes and Shortcuts were reviewed in dark/light × comfortable/compact on Windows with the real renderer, and the Terminal preview with a profile that turns on ligatures, the Dracula theme, highlighting and a 4.5:1 minimum contrast.

### Manual checks

| Check | Windows 10 | Linux |
|---|---|---|
| Translucent background (`background_opacity` below 1): the window starts with an alpha channel, without warnings | ✅ (started; the desktop showing through was not looked at) | ⏳ |
| The desktop shows through a translucent terminal, and only there (the panels stay opaque); blur from the compositor (KDE, Hyprland) | ⏳ | ⏳ (needs real hardware) |
| Bell "sound" | ⏳ | ⏳ X11 (Wayland has no standard beep: flashes) |
| Bell "notification" flashes the taskbar entry of an inactive window | ⏳ | ⏳ |
| File dialogs (background image, theme import and export): native on Windows, the portal or Qt's own on Linux | ⏳ | ⏳ |
| Fonts from the system font list, fallback fonts (a Nerd Font, a CJK font), hinting and antialiasing off | ⏳ | ⏳ |
| Ligatures with Fira Code and Cascadia Code | ⏳ | ⏳ |
| Legacy encodings against a real device or server (ISO-8859-15, Shift_JIS) | ⏳ (Sprint 7, SSH) | ⏳ |

## Sprint 1 (2026-09-25)

### Automated checks

Each smoke test fails on any warning from our QML (exit code 6), besides the Sprint 0 checks:

- **Main window (`--smoke-test`, 59 steps):** visits every view and every Settings section, opens and closes the command palette, the notifications, the side panel and the Restore defaults dialog, opens and closes tabs, cycles focus with F6, switches the layout, and checks that the keyboard focus never stays on a hidden item. It writes no settings.
- **Gallery (`--gallery --smoke-test`, 26 steps):** visits every section, opens and closes every dialog, drawer and menu, runs a command palette search, shows toasts, and flips theme, density, accent and reduce motion.
- **Crash dialog (`--crash-report <file> --smoke-test`):** unchanged.

**Host:** as in Sprint 0 (Windows 10 22H2; WSL2 with WSLg for Linux).

| Environment | Qt | Build, clippy, tests | `offscreen` (main / gallery / dialog) | Native Wayland (main / gallery / dialog) | X11 `xcb` (main / gallery / dialog) |
|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ | ✅ / ✅ / ✅ | — (native `windows`: ✅ / ✅ / ✅) | — |
| Arch Linux (WSLg) | 6.11.2 (distro) | ✅ build; clippy and tests ✅ before the review fixes (1) (3) | ✅ / ✅ / ✅ | ✅ / ✅ / ✅ | ✅ / ✅ / ✅ |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ build; clippy and tests ✅ before the review fixes, core tests ✅ after (3) | ✅ / ✅ / ✅ | ✅ / ✅ / ✅ | ✅ / ✅ / ✅ |
| Fedora 43 (WSLg) | 6.10.3 (distro) | ✅ build; clippy and tests ✅ before the review fixes (3) | ✅ / ✅ / ✅ | ✅ / ✅ / ✅ (2) | ✅ / ✅ / ✅ |
| Ubuntu 22.04 (WSLg) | 6.10.3 (aqt) | ✅ build and clippy; tests ✅ before the review fixes (3) | ✅ / ✅ / ✅ | ✅ / ✅ / ✅ (2) | ✅ / ✅ / ✅ |

(1) GCC 16 prints a `-Wsfinae-incomplete` warning from Qt's own `qchar.h` while it compiles the cxx-qt generated code. It is not in our code and doesn't fail the build.

(2) With `XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir`, as in Sprint 0.

(3) The smoke tests were run on the final code. The final Linux clippy and test run was stopped because the host ran low on memory (four distros building at once); it is pending, one distro at a time.

Screenshots (`--screenshots`) of the main window, Settings, every gallery page and the crash dialog were reviewed in dark/light × comfortable/compact on Windows (offscreen) and on Debian 13 (native Wayland, Qt 6.8.2). The pseudo-locale was reviewed on Windows.

### Manual checks

| Check | Windows 10 | Linux (WSLg) |
|---|---|---|
| Real key presses (sent with `WScript.Shell.SendKeys` to the running window): Ctrl+Shift+P opens the palette, typing filters it, Enter runs the action (density switched to compact live), Ctrl+Shift+T opens a tab, F6 moves the focus to the rail with a visible focus ring, Ctrl+Shift+Q quits with exit code 0 | ✅ | ⏳ |
| Rail with real key presses: F6 twice reaches the rail (the first stop is the title bar), Down moves, Enter opens SFTP, End then Space opens Settings; the focused item shows its focus ring and label tooltip | ✅ | ⏳ |
| `config.toml` with a TOML syntax error: the app starts on defaults, warns, and leaves the file byte-for-byte unchanged | ✅ | ⏳ |
| An existing `config.toml` doesn't produce a "changed on disk" reload at startup | ✅ | ⏳ |
| Pseudo-locale (`language = "pseudo"`, debug build): every visible string is translated, long strings wrap without clipping | ✅ | ⏳ |
| Release build with `language = "pseudo"` in `config.toml`: the pseudo-locale isn't bundled, so the UI is English | ✅ | ⏳ |
| Real mouse (`SetCursorPos` + `mouse_event`), `custom` decorations: dragging the title bar moves the window, a double-click maximizes and restores it, dragging the bottom-right corner resizes it, the close button quits with exit code 0 | ✅ | ⏳ |
| Frameless maximize fills the work area exactly (1920×1040 on a 1920×1080 screen), so the taskbar stays visible | ✅ | — |
| Tiling compositor: `auto` decorations drop the window buttons (Hyprland, Sway, niri, i3) | — | ⏳ (needs real hardware) |
| Screen reader names (Narrator, Orca) | ⏳ | ⏳ |

## Sprint 0 (2026-09-25)

### Automated checks

**Main window (`opensesh-app --smoke-test`) passes when:**

1. The main window renders a first frame.
2. The Knock button is pressed three times and the Rust `SesameDoor` object opens the door.
3. A typed, AOT-compiled QML probe confirms that non-ASCII text survived the build.

**Crash dialog (`--crash-report <file> --smoke-test`) passes when** the dialog renders a first frame.

**Host:** Windows 10 Pro 22H2 (build 19045), 20 threads. Linux runs happen in WSL2 with WSLg, where the Wayland compositor is Weston and X11 goes through XWayland. The Linux builds link with `lld`.

| Environment | Qt | Build | `offscreen` (main / dialog) | Native Wayland (main / dialog) | X11 `xcb` (main / dialog) | App id / WM_CLASS checked |
|---|---|---|---|---|---|---|
| Windows 10 22H2, MSVC 2022 | 6.10.3 (aqt) | ✅ debug + release | ✅ / ✅ | — | — | — |
| Arch Linux (WSLg) | 6.11.2 (distro) | ✅ | ✅ / ✅ | ✅ / ✅ | ✅ / ✅ | ✅ (1) |
| Debian 13 (WSLg) | 6.8.2 (distro) | ✅ | ✅ / ✅ | ✅ / ✅ | ✅ / ✅ | ⏳ |
| Fedora 43 (WSLg) | 6.10.3 (distro) | ✅ | ✅ / ✅ | ✅ / ✅ (2) | ✅ / ✅ | ⏳ |
| Ubuntu 22.04 (WSLg) | 6.10.3 (aqt) | ✅ | ✅ / ✅ | ✅ / ✅ (2) | ✅ / ✅ | ⏳ |

(1) Checked on Arch:
- Wayland: `WAYLAND_DEBUG=1` shows `xdg_toplevel.set_app_id("cc.caixa.OpenSesh")`.
- X11 (`xprop`): `WM_CLASS = "opensesh-app", "OpenSesh"`, and `_GTK_APPLICATION_ID` and `_KDE_NET_WM_DESKTOP_FILE` are both `cc.caixa.OpenSesh`.

(2) These WSL instances couldn't always start the systemd user session. When that happens, `/run/user/1000` has no `wayland-0` socket and Qt fails with "Failed to create wl_display". It's a WSL environment issue, not an app issue: the runs passed with `XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir`.

The debug build also has a regression check for the smoke test itself: with `/utf-8` removed from `build.rs`, `--smoke-test` exits with code 5 ("text encoding BROKEN").

### Manual checks

| Check | Windows 10 | Arch | Debian 13 | Fedora 43 | Ubuntu 22.04 |
|---|---|---|---|---|---|
| Window renders (screenshot), window icon shown | ✅ | ⏳ | ⏳ | ⏳ | ⏳ |
| Keyboard: Space presses the focused Knock button, door opens after 3 knocks | ✅ | ⏳ | ⏳ | ⏳ | ⏳ |
| Panic across FFI (`OPENSESH_DEBUG_PANIC=1`, press Knock) (3) | ✅ | ⏳ | ⏳ | ⏳ | ⏳ |
| Qt fatal error (`QT_QPA_PLATFORM=nosuchplugin`) writes a "Qt fatal error" crash report | ✅ | ⏳ | ⏳ | ⏳ | ⏳ |
| Release build: `--version` printed; startup error shows a message box | ✅ | — | — | — | — |

(3) On Windows, pressing Knock with `OPENSESH_DEBUG_PANIC=1`:
- The process aborted (`0xC0000409`).
- **Exactly one** crash report was written. It starts with the root cause (`sesame.rs`), and cxx's "panic in ffi function ..., aborting" follows as an appended section.
- **Exactly one** crash dialog opened, showing that report.

### Not run yet (needs real hardware)

| Environment | Build | Native Wayland | X11 | App id | Window + keyboard |
|---|---|---|---|---|---|
| Hyprland (`hyprctl clients`), the Sprint 0 "done when" | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| Sway | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| KDE Plasma 6 | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| GNOME | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| X11 session (i3 / XFCE) | ⏳ | — | ⏳ | ⏳ | ⏳ |
| Windows 11 | ⏳ | — | — | — | ⏳ |
| Fractional scaling 125 % / 150 % | ⏳ | ⏳ | ⏳ | — | ⏳ |

The procedure for each is in [`../dev-setup.md`](../dev-setup.md#checking-wayland-and-x11).

## How to run the automated part

```sh
# Linux (repeat with QT_QPA_PLATFORM=wayland and QT_QPA_PLATFORM=xcb)
export OPENSESH_NO_CRASH_DIALOG=1 QT_QPA_PLATFORM=offscreen
cargo run -p opensesh-app -- --smoke-test; echo "exit=$?"
cargo run -p opensesh-app -- --gallery --smoke-test; echo "exit=$?"
printf 'test report\n' > /tmp/report.txt
cargo run -p opensesh-app -- --crash-report /tmp/report.txt --smoke-test; echo "exit=$?"
```

The exit codes are listed in [`../dev-setup.md`](../dev-setup.md#build-run-and-check).
