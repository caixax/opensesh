# vttest

[vttest](https://invisible-island.net/vttest/) is Thomas Dickey's VT100/VT102 conformance program. The Sprint 2 definition of done says "vttest passes its basic sections". This page says what that means for OpenSesh, how the check runs, what passes, and which deviations remain.

## What "basic sections" means (decision D8)

The lead took decision D8 of the Sprint 2 review: the basic sections are these main menu items, at 80 columns and 24 lines:

| Item | Name | What is checked |
|---|---|---|
| 1 | Test of cursor movements | every screen |
| 2 | Test of screen features | every screen |
| 3 | Test of character sets | US ASCII and DEC Special Graphics |
| 6 | Test of terminal reports | 6.2 LineFeed/NewLine mode (LNM), 6.3 device status (DSR) and cursor position (CPR) reports, 6.4 primary device attributes (DA1) |
| 8 | Test of VT102 features (Insert/Delete Char/Line) | every screen |

Items 4 (double-size characters), 5 (keyboard), 7 (VT52 mode) and 9 to 12 are outside the basic sections. The deviations below are shared with Alacritty, whose engine (`alacritty_terminal`) OpenSesh uses; the owner accepts them with D8.

## How the check runs

The tests live in [`crates/opensesh-term/tests/vttest.rs`](../../crates/opensesh-term/tests/vttest.rs). They are Linux tests (vttest needs a Unix terminal), `#[ignore]`d by default, and they skip with a message on Windows or when vttest is missing.

```sh
cargo xtask vttest                                                    # once: builds the pinned vttest 20251205
cargo test -p opensesh-term --test vttest -- --include-ignored        # compare every screen with its golden
OPENSESH_BLESS=1 cargo test -p opensesh-term --test vttest -- --include-ignored   # rewrite the goldens
```

From PowerShell, in a WSL distro (use the same `CARGO_TARGET_DIR` for both commands, since the tests look for `<target>/vttest/vttest`):

```powershell
wsl.exe -d Debian -- bash -lc 'cd /mnt/i/Projects/opensesh && export CARGO_TARGET_DIR=~/.cache/opensesh-target && cargo xtask vttest && cargo test -p opensesh-term --test vttest -- --include-ignored'
```

- **Program.** `<target>/vttest/vttest` from `cargo xtask vttest`, or the program named by `OPENSESH_VTTEST`. It must be release 20251205 (the tests run `vttest -V` and fail on another release): distributions ship 20210210 to 20241208, and their screens differ.
- **Pipeline.** vttest runs on the real PTY (`pty::spawn`) behind the production `Session`, at 80 x 24, with a clean home directory and `LC_ALL=C.UTF-8`.
- **Keys.** Every key goes through the production key encoder (`KeyInput::from_qt` then `encode_key`) with the program's current modes, as the terminal item does. Test 6.2 depends on it: while vttest sets LNM, Enter must send CR LF.
- **Stepping.** After each key the harness waits until the engine reports new output (`Notice::Dirty`), then until no new output comes for 500 ms (vttest pauses about 200 ms while it waits for a terminal report). `OPENSESH_TEST_TIMEOUT_SCALE` multiplies that quiet time on slow machines.
- **Goldens.** Each screen's text dump (`Session::text_dump`: one line per row, trailing spaces trimmed) is compared with `crates/opensesh-term/tests/vttest/<item>-<nn>.txt`. When a screen has bold, underlined or inverse cells, a second file `<item>-<nn>.attrs.txt` holds one character per cell: `.` for none, otherwise a hex digit adding 1 (bold), 2 (underline) and 4 (inverse, that is a background other than the default). A failure lists every differing row, and the actual screens are written to `<target>/tmp/vttest-actual/`.
- **Other items.** `vttest_other_items_run_to_the_end` walks items 4 and 7 and the reports 6.1, 6.5, 6.6 and 6.7 without goldens: it checks that vttest gets through them and back to its menu, and leaves their screens in `<target>/tmp/vttest-actual/other/` for review.

The goldens were generated once and then reviewed one by one against vttest's source (`main.c`, `reports.c`, `charsets.c` of 20251205) and against an independent emulator: vttest driven in a detached tmux 3.5a at 80 x 24 (`tmux send-keys`, `tmux capture-pane -p`). Every difference with tmux is explained below.

## Results (2026-09-25, vttest 20251205)

All six tests pass, with identical goldens, on every WSL distro: Debian 13 (3 runs), Fedora 43, Ubuntu 22.04 and Arch (2 runs each). A run takes about 13 s. On Windows the tests print `skipped: vttest is a Unix program` and pass.

| Item | Screens | As a VT100 shows them | Known deviation | Wrong (new finding) |
|---|---|---|---|---|
| 1 Cursor movements | 6 | 4 | 2 (132 columns) | 0 |
| 2 Screen features | 15 | 9 (2 of them without blink) | 4 (132 columns, light background) | 2 |
| 3 Character sets | 1 | US ASCII, DEC Special Graphics | British, DEC alternate ROM | 0 |
| 6 Terminal reports (6.2, 6.3, 6.4) | 5 + menu | all but one report | CPR in origin mode | 0 |
| 8 VT102 insert/delete | 14 | 7 (80 columns) + 8-14 | 6 (132 columns) | 0 |

In short: apart from the deviations D8 accepts, every screen of the basic sections is what a VT100 shows, except 2-07 and 2-09, which hit a bug in `alacritty_terminal` ([below](#new-finding-cursor-up-in-origin-mode)).

### Item 1: cursor movements

| Screen | What vttest draws | Result |
|---|---|---|
| 1-01 | A border of `*` and `+`, and a frame of `E`s around the text with one free position, at 80 columns | ✅ |
| 1-02 | The same at 132 columns | ⚠️ 132 columns: the grid stays 80 wide, so the picture wraps |
| 1-03 | Autowrap mixed with control characters: `I`..`Z` on the left margin, `i`..`z` on the right, in order | ✅ (tmux 3.5a gets this one wrong: it breaks the `N`/`n`, `R`/`r`, `V`/`v` and `Z`/`z` pairs over two lines) |
| 1-04 | The same at 132 columns | ⚠️ 132 columns: vttest addresses column 132, the cursor stops at column 80, so the picture is the 80-column one |
| 1-05 | Control characters inside escape sequences: four identical lines | ✅ |
| 1-06 | Leading zeros in escape sequences: "This is a correct sentence" | ✅ |

### Item 2: screen features

Attributes are checked too (`.attrs.txt` goldens) for 2-13, 2-14 and 2-15.

| Screen | What vttest draws | Result |
|---|---|---|
| 2-01 | Wrap around: three lines of `*` | ✅ |
| 2-02 | Tab set and reset: two identical lines | ✅ |
| 2-03 | 132 columns, light background | ⚠️ 132 columns (the ruler wraps at 80) and DECSCNM (the background stays dark) |
| 2-04 | 80 columns, light background | ⚠️ DECSCNM: the text is right, the background stays dark |
| 2-05 | 132 columns, dark background | ⚠️ 132 columns |
| 2-06 | 80 columns, dark background | ✅ |
| 2-07 | Soft scroll up and down in a 2-line region (rows 12 and 13) | ❌ Expected (and what tmux shows): row 12 `Push <RETURN>`, row 13 `Soft scroll down region [12..13] size 2 Line 29`, nothing else. OpenSesh runs the scroll-down half on row 1, outside the region. See [the cursor bug](#new-finding-cursor-up-in-origin-mode) |
| 2-08 | Soft scroll in the whole screen | ✅ |
| 2-09 | Jump scroll in a 2-line region | ❌ as 2-07 |
| 2-10 | Jump scroll in the whole screen | ✅ |
| 2-11 | Origin mode with a region at rows 23 and 24 | ✅ (the text on the screen says which line goes where. tmux 3.5a is wrong here: on DECSTBM it homes the cursor to row 1 instead of the region's top) |
| 2-12 | Origin mode off: first and last row | ✅ |
| 2-13 | Graphic rendition pattern, dark background | ✅ text, bold, underline, inverse and their combinations; ⚠️ blink: the `blink` cells carry no attribute |
| 2-14 | The same with a light background (DECSCNM) | ⚠️ DECSCNM: identical to 2-13 |
| 2-15 | Save and restore cursor with character sets and renditions: ten of each (`*`, `─`, `x`, `◆`) per rendition, and a 5 x 4 rectangle of `A`s | ✅; ⚠️ blink: the "blinking" column carries no attribute |

### Item 3: character sets (screen 3-01)

At the VT102 level vttest shows one screen, "These are the installed character sets", with each set selected as G0 (SI) and G1 (SO). The set names are bold (checked).

| Set | Result |
|---|---|
| B, US ASCII | ✅ |
| A, British | ⚠️ UK character set: `#` stays `#` (it should be `£`) |
| 0, DEC Special Graphics | ✅ `◆▒␉␌␍␊°±␤␋┘┐┌└┼⎺⎻─⎼⎽├┤┴┬│≤≥π≠£·`, and `_` is blank |
| 1, DEC Alternate ROM standard characters | shown as US ASCII (there is no alternate character ROM; outside D8) |
| 2, DEC Alternate ROM special graphics | shown as US ASCII (same) |

### Item 6: terminal reports

| Screen | Test | Result |
|---|---|---|
| 6-00-menu | The reports menu | ✅ |
| 6-2-01 to 6-2-03 | LineFeed/NewLine mode: Enter while LNM is set, then after it is reset | ✅ ` <13> <10>  -- OK` and ` <13>  -- OK`: the key encoder sends CR LF under LNM. (tmux's `send-keys Enter` sends CR only, so tmux shows "Not expected") |
| 6-3-01 | DSR 5 (status), DSR 6 (cursor position) without and with origin mode | ✅ `ESC [ 0 n -- means "TERMINAL OK"`, ✅ `ESC [ 5 ; 1 R -- OK`; ⚠️ with origin mode: `ESC [ 8 ; 1 R -- Ignores origin mode` |
| 6-4-01 | DA1 | ✅ `ESC [ ? 6 c -- means VT102` |

### Item 8: VT102 insert and delete

| Screen | What vttest draws | Result |
|---|---|---|
| 8-01 | Screen accordion (insert and delete line): `A`..`X` lines | ✅ |
| 8-02 | "Top line: A's, bottom line: X's, this line, nothing more" | ✅ |
| 8-03 | Insert mode: `A***...***B` | ✅ |
| 8-04 | Delete character: `AB` | ✅ |
| 8-05, 8-06 | Right column staggered by one (delete character) | ✅ |
| 8-07 | ANSI insert character: two identical `A B C ... Z` lines | ✅ |
| 8-08 to 8-13 | The same tests at 132 columns | ⚠️ 132 columns: 132-column lines written into 80 columns (tmux gets different wrong pictures for 8-11 and 8-12) |
| 8-14 | Insert character at 132 columns | the 80-column picture (the test fits in 80 columns) |

## Known deviations

Each item of the D8 list was checked against what OpenSesh shows (screens above, and `vttest_other_items_run_to_the_end` for items outside the basic sections). All of them come from `alacritty_terminal` 0.26.0 and happen in Alacritty too.

| Deviation | Seen on | What happens |
|---|---|---|
| DECCOLM, 132 columns (`CSI ? 3 h`) | 1-02, 1-04, 2-03, 2-05, 8-08 to 8-14, 4-03 and 4-04 | Ignored apart from clearing the screen and resetting the margins (`Term::deccolm`): the grid keeps its size. vttest's 132-column pictures wrap or stop at column 80. Confirmed. |
| Double-width and double-height lines (DECDWL, DECDHL, `ESC # 3/4/6`) | item 4 (6 screens) | Ignored: "This is a Double-width line" and the double-height pairs render at normal size; the framed "mad programmer" pattern is single size. Confirmed. |
| VT52 mode (`CSI ? 2 l`) | item 7 (3 screens) | Not supported: the VT52 cursor addressing shows up as text (`:5*`, `7)!*6d!r"f#q$...`), and the VT52 identify request gets the ANSI answer `ESC [ ? 6 c`. Confirmed. |
| UK character set (`ESC ( A`) | 3-01 | Not supported: shown as US ASCII (`#` instead of `£`). Confirmed. |
| Blink (SGR 5) | 2-13, 2-15 | The engine has no blink attribute: blinking text is drawn steady (the attribute maps show no attribute on it). Confirmed. |
| DECSCNM, light background (`CSI ? 5 h`) | 2-03, 2-04, 2-14 | Ignored: the screen isn't reversed; 2-14 is identical to 2-13, text and attributes. Confirmed. |
| DECREQTPARM (`CSI x`) | 6.7 | No reply: vttest shows `Report is:  -- Bad format` for both arguments. Confirmed. |
| CPR in origin mode | 6-3-01 | The cursor position report is absolute: `ESC [ 8 ; 1 R -- Ignores origin mode` (a VT100 reports `5;1`, relative to the region). Confirmed. |
| ENQ answerback | 6.1 | ENQ is ignored, so nothing is sent, which is what a terminal with an empty answerback message does. An answerback message can't be configured (the Sprint 3 option would need to catch ENQ before the parser). Confirmed. |

Also observed outside the basic sections, and not deviations: DA2 answers `ESC [ > 0 ; 2600 ; 1 c` (vttest reads "firmware version 260.0": it is the `alacritty_terminal` version, so it changes when the crate is updated); DA3 is "not supported" (a VT420 feature).

### New finding: cursor up in origin mode

Screens 2-07 and 2-09 fail because of a bug in `alacritty_terminal` 0.26.0 that is also on its master branch (checked on 2026-09-25): in origin mode, CUU and CPL (and CUD and CNL, where clamping usually hides it) add the region's top to a position that already includes it. `Term::move_up` computes the absolute line `cursor - n` and hands it to `Term::goto`, which adds `scroll_region.start` again and clamps. Reproduced with the replay backend, region at rows 12 and 13, origin mode on:

| Sequence | VT100 and xterm | OpenSesh |
|---|---|---|
| `CSI 2;1 H` then `CSI A` (up 1 from the region's last row) | row 12 | row 13: the cursor doesn't move |
| `CSI 2;1 H` then `CSI 24 A` | row 12 (stops at the top margin) | row 1, outside the region |
| `CSI 1;1 H` then `CSI B` | row 13 | row 13 |

Without origin mode, a large CUU from inside the region also goes past the top margin to row 1 (a VT100 stops at the margin). Programs rarely combine origin mode with relative cursor moves (vttest does; tmux, vim, less and htop use absolute positioning), so the impact is small. It is the only difference in the basic sections that isn't on the D8 list: the lead or the owner decides whether to accept it, report it upstream, or work around it.

## Not covered

- **Item 5 (keyboard)** needs a person reading the screen; the key encoder has its own unit tests. The harness could drive 5.4 (cursor keys) and 5.5 (numeric keypad) later.
- **Items 9 to 12** (known bugs, reset, non-VT100 terminals, setup) are outside D8.
- **Colors and pixels.** The goldens check text and bold, underline and inverse. Colors, cursor shape and rendering are the GUI's job (Sprint 2 render tests and the manual matrix).
