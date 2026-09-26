# Sprint 3: Terminal customization

**Goal:** the most customizable terminal you have seen (PLAN §6.2 to §6.5).

**Started:** 2026-09-26 · **Finished:** 2026-09-26

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it.
- **Hosts and groups arrive in Sprint 5.** The inheritance chain (global, group, host, tab) is implemented and tested in `opensesh-core` now. In the app, the global level (the default profile) and the tab level (the tab's profile and its font zoom) are live. The group and host levels get wired to `hosts.toml` when it exists.
- **What applies live and what doesn't:** every setting reaches open terminals at once, except the ones a running program can't see change: `TERM` and the shell apply to new terminals.
- **Built-in themes come from upstream files** (pinned commits), converted by our own importers, with their licenses in `assets/themes/LICENSES/`. A test checks that each bundled theme still matches its upstream file.
- **Keyword highlighting** lives in `highlights.toml` (a file PLAN §4.2 doesn't list yet; documented), with built-in presets and user rule sets.

## Checklist

### Profiles (`opensesh-core`)
- [x] Terminal settings model: every §6.2 option, with defaults, validation and lenient per-key parsing (warnings name the key)
- [x] Partial overrides and resolution through the chain global, group, host, tab, with tests
- [x] `profiles/*.toml`: the default profile (global level) plus named profiles; atomic writes, hot reload, unknown keys kept

### Fonts
- [x] Font picker with the monospace filter (`isFixedPitch`) and a manual override, fallback families, size, normal and bold weight, italic on or off
- [x] Line height, letter spacing, antialiasing and hinting in the renderer
- [x] Ligatures: spike in `spikes/ligatures/` and an ADR; experimental and off by default
- [x] Font zoom per tab (Ctrl+= / Ctrl+- / Ctrl+0)

### Themes
- [x] Own TOML format (PLAN §4.3), user themes in `themes/*.toml` with hot reload
- [x] Importers with fixtures: iTerm2 (`.itermcolors`), Windows Terminal (JSON), Alacritty (TOML), Kitty (`.conf`), base16 (YAML)
- [x] Export to the own format and to Alacritty
- [x] Built-in themes: OpenSesh Dark and Light, Catppuccin, Dracula, Nord, Gruvbox, Tokyo Night, Solarized, with their licenses
- [x] Visual theme editor with a live preview

### The other §6.2 options
- [x] Colors: theme for the dark and for the light app theme, bold as bright, minimum contrast, selection and cursor colors
- [x] Cursor: shape, blinking, hollow when unfocused
- [x] Window: padding, background opacity, background image with dimming and fit
- [x] Scrolling: scrollback lines, scroll speed, optional smooth scrolling
- [x] Selection and clipboard: word separators, copy on select, right click pastes or opens the menu, primary selection on Linux, opt-in OSC 52
- [x] Behavior: bell (visual, sound, notification, none), `TERM`, Backspace and Delete, Alt as Meta, encodings (UTF-8 and legacy through `encoding_rs`), answerback, paste delay per line

### Keyword highlighting (§6.5)
- [x] Regex rules with a style (foreground, background, bold, underline), rule sets assignable per profile
- [x] Presets: logs, network (IPv4, IPv6, MAC), status, paths and URLs
- [x] Applied at render time, can be turned off per tab, CPU cost measured in `docs/perf.md`
- [x] Rule editor

### Settings pages
- [x] Settings > Terminal: every option, with a live preview of the §6.2 sample (`ls --color`, `git diff`, a powerline prompt, CJK and emoji)
- [x] Settings > Profiles: create, duplicate, rename, delete, pick the default; inherited values shown and resettable
- [x] Settings > Themes: list with swatches, import, export, editor
- [x] Settings > Shortcuts: capture, conflict detection, restore defaults; `keybindings.toml`

### Quality
- [x] Unit tests: profile resolution, settings parsing, theme importers and exporters (fixtures), keybindings, highlighting, encodings, answerback, paste pacing, minimum contrast
- [x] Smoke tests cover the new pages and a live settings change reaching an open terminal
- [x] `docs/perf.md`: highlighting cost, minimum contrast cost

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, shaders, deny, audit
- [x] Build and smoke tests on Windows and in the WSL distros; GitHub Actions green
- [x] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

### What was done

- **Profiles** ([ADR 0016](../adr/0016-terminal-profiles-and-settings-files.md)): `opensesh-core::terminal` declares every §6.2 option once (key, type, default, check). Profiles in `profiles/*.toml` are partial layers over the defaults; `default.toml` is the global level. `ProfileSet::resolve` builds the whole chain (global, then each level's profile and own overrides for group, host and tab), tested for all four levels. In the app, a tab picks a profile (its menu) and adds its font zoom; new tabs use the profile chosen in Settings > Profiles (`[terminal] profile` in `config.toml`).
- **Live changes:** the `TerminalProfiles` and `Keybindings` QML singletons load, watch, validate and save the files; one immutable library (profiles, themes, rules) is swapped on every change, and each `TerminalItem` re-resolves its profile when the revision changes: fonts, padding, colors, engine options, keys, highlighting. Only `TERM` waits for a new terminal. Files edited outside the app apply live, after our own saves settle.
- **Engine (`opensesh-term`):** `Session::set_options` (scrollback, default cursor, hollow cursor, word separators, OSC 52, encoding, answerback) and `set_highlighter` on a running session; legacy encodings through `encoding_rs` (decode before parsing, encode typed input); the answerback reply to ENQ (printable ASCII only, off by default); OSC 52 copying (opt-in, never reading, up to 1 MiB); paced pastes on the engine's timer, cancelled by Escape or Ctrl+C; the minimum contrast; keyword highlighting per redrawn row; Delete can send DEL.
- **Renderer:** font families with fallbacks, weights, italics on or off, line height, letter spacing, antialiasing, hinting, background opacity, smooth wheel scrolling, and experimental ligatures by per-cell glyph substitution ([ADR 0015](../adr/0015-programming-ligatures.md), spike in `spikes/ligatures/`).
- **Themes:** the §4.3 format; 15 built-in themes, 13 of them read from their upstream Alacritty files at pinned commits with their licenses ([`assets/themes/themes.toml`](../../assets/themes/themes.toml), `THIRD_PARTY_NOTICES.md`); importers for iTerm2, Windows Terminal (settings files with comments too), Alacritty, Kitty and base16, tested on real files; export to OpenSesh and Alacritty; missing colors derived.
- **Keyword highlighting:** rule sets in `highlights.toml` and 4 built-in sets (log levels, network addresses, status words, paths and URLs), colors from the theme, per-tab toggle.
- **Settings:** Terminal (every option, own or inherited, reset to inherited or to the default, a live preview of the §6.2 sample drawn by the real engine and renderer), Profiles, Themes (samples, import, export, visual editor), Shortcuts (capture, conflicts, a warning for keys terminal programs need, restore defaults) and the rule set editor.
- **Also:** the bell's four styles (the sound is the system beep: `MessageBeep` on Windows, the X11 bell, none on Wayland, which flashes instead), a translucent window when a profile asks for it at start-up, file dialogs (`QtQuick.Dialogs`, a new Debian dependency), `cargo xtask notices`, and more screenshots (every new Settings page).

### "Done when" (PLAN)

- Any change is visible live in open terminals: yes. The smoke test changes a profile, edits it and deletes it while a real shell runs in a tab, and checks each change on the terminal.
- The per-host override works: the chain is implemented and tested in `opensesh-core` (global, group, host and tab, including a host with its own profile). Hosts themselves arrive in Sprint 5, which wires their `[host.terminal]` tables to it.
- The theme importers pass their fixtures: yes. Dracula reads the same from Alacritty, Windows Terminal and Kitty files; iTerm2 and base16 (flat and tinted-theming layouts) are tested on upstream files, and every built-in theme goes through the Alacritty importer and back through both exporters.

### How it was verified

- fmt, clippy `-D warnings`, lint-qml, i18n, shaders and every test on Windows (Qt 6.10.3) and in the WSL distros Debian 13 (Qt 6.8.2), Fedora 43 (Qt 6.10.3) and Arch (Qt 6.11.2); main and gallery smoke tests on all four.
- Screenshots of every new page in dark and light, comfortable and compact, with the real renderer; the preview with ligatures, Dracula, highlighting and a minimum contrast.
- A translucent profile checked to start cleanly on Windows (the effect on the desktop is left to the manual matrix).
- Performance ([perf.md](../perf.md)): highlighting adds about 0.18 ms to a full frame of 61 busy rows, the minimum contrast about 0.05 ms; the plain snapshot stays within noise of Sprint 2 (a first version was 25 % slower and was fixed: the painter has one copy of its cell function with highlighting and one without).

### Pending

- **Manual matrix on real hardware:** the desktop through a translucent terminal, the sound and notification bells, file dialogs through the portal, fonts and fallbacks, ligatures with other fonts ([manual-matrix.md](../testing/manual-matrix.md)).
- **Hosts and groups** use the profile chain once they exist (Sprint 5).
- Ligatures stay experimental: none across style changes, wide characters or soft wraps.
- The sound bell has no Wayland implementation (no standard protocol in Qt 6.8); it flashes instead.
- Enabling a translucent background for the first time needs a restart (the window's alpha channel is decided at start-up).
- Carried over: the open low-severity review items of Sprint 2, the Windows executable icon, an Ubuntu package.

### Risks

- `encoding_rs` and `quick-xml` are new dependencies (both widely used, licenses allowed).
- A background image or translucency depends on the compositor; X11 without one shows the transparent parts black.
- Theme files from other terminals vary; unknown values are skipped with a note rather than failing the import.
