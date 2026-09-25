# Input methods (IME)

The Sprint 2 checklist asks for basic preedit with ibus and fcitx5 in the terminal. This page records what was checked, how, and what is still open.

## Result (2026-09-25)

Checked with ibus and the Chinese pinyin engine on Debian 13 in WSLg, release build of commit `a47d994`, Qt 6.8.2, `QT_IM_MODULE=ibus`. Typing `nihao`, then Space, then Return in a terminal tab:

| Check | X11 (`xcb`, Xwayland) | Wayland |
|---|---|---|
| The preedit is drawn at the cursor while composing | **Pass.** `你好` is drawn from the cursor cell, underlined, each character two cells wide, with the input method's caret (a beam) at its end; the block cursor is hidden meanwhile. `zhong` shows `中` the same way. | Not captured (see below) |
| Space commits the candidate and the shell receives it | **Pass.** bash runs `你好` (`bash: 你好: command not found`); its history holds the UTF-8 bytes `e4 bd a0 e5 a5 bd` | **Pass**, same history bytes |
| Escape cancels the composition without sending anything | **Pass.** The preedit disappears; the next Return runs an empty line | **Pass**, same history |
| Control: the same keys in the command palette's search field (Qt Quick text input) | Same as the terminal: underlined preedit, then `你好` committed | Not captured |

fcitx5 was not tested. The candidate window's position (the item reports its cursor through `Qt::ImCursorRectangle`) could not be checked in WSLg, see below.

## How to repeat it

As root in the distro (`wsl.exe -d Debian -u root -- ...`):

```sh
apt-get install --no-install-recommends ibus ibus-libpinyin fonts-wqy-microhei   # the font only makes CJK visible
```

As the user, in the session that starts OpenSesh:

```sh
ibus-daemon --daemonize --replace --xim --panel=disable   # no candidate window in WSLg (see below)
ibus engine libpinyin
QT_IM_MODULE=ibus QT_QPA_PLATFORM=xcb ./opensesh-app       # or QT_QPA_PLATFORM=wayland
```

Open a terminal tab, type `nihao` (the preedit shows `你好`), press Space to commit, Return to run it. With the pinyin engine a lone Shift switches between Chinese and English input, so don't tap Shift before the test. Afterwards remove the packages with `apt-get purge ibus ibus-libpinyin fonts-wqy-microhei && apt-get autoremove`.

The keys were real keyboard input: `SendInput` on Windows into the WSLg window, which WSLg forwards over RDP to Weston and from there to the app (through Xwayland for X11). The X11 screenshots are `XGetImage` of the app's window. The shell's `PROMPT_COMMAND` appended `history 1` to a file, which is where the history bytes above come from.

## What WSLg can't show

- **With ibus's candidate window, composing breaks in WSLg, in every Qt text field.** `ibus-ui-gtk3` opens the candidate list as a separate top-level window. WSLg makes it a separate Windows window and Windows gives it the keyboard focus, so the OpenSesh window loses the focus and ibus drops the composition: the preedit vanishes a fraction of a second after the last letter, and Space is no longer taken by the engine (ibus answers "not handled", so the shell gets a plain space). The command palette's text field behaves exactly the same, so this is the environment, not the terminal item. With `--panel=disable` there is no candidate window and composing works; Space takes the first candidate.
- **Wayland windows can't be captured** from the Windows side: a screen grab of the WSLg window is blank and `PrintWindow` returns black, and WSLg's Weston offers clients no screenshot protocol. So on Wayland only what reached the shell was checked.
- **XTest input is not a substitute.** Keys injected with XTest into Xwayland do reach Qt, but the first one after a focus change is lost, and with the candidate window they hit the focus problem above. The checks above use real keyboard input.

## Not checked yet

- fcitx5 (the checklist names it too).
- A native Linux desktop (GNOME, KDE Plasma, Sway), where the candidate window should stay next to the cursor and not take the focus.
- Windows input methods (Microsoft Pinyin, Japanese IME).
- Moving the focus away while composing (the item then drops its preedit; whether the input method commits or discards it was not observed).
