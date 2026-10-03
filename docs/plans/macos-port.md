# Plan: the macOS port

**Status:** planned, not started. Written on 2026-10-03, after 1.0.0, at the owner's request: the sprints below are for later, each one started like any other (its checklist in `docs/sprints/sprint-NN.md` first, decisions in ADRs).

**Goal:** OpenSesh on macOS (Apple Silicon first) as a Mac app, not a Linux app that happens to run there: Mac shortcuts, the menu bar, native window controls, a signed and notarized `.dmg`, Homebrew, and the same checks in CI as the other systems. PLAN lists macOS as **Tier 2** ("as far as possible"); it stays Tier 2 until it has been checked on real Macs.

## Where the code stands

A survey of the code at 1.0.0 (every `cfg` and platform check), so the sprints don't redo what already works:

- **Already works on macOS through `unix`:** the PTY (`portable-pty`), file permissions and atomic writes, the SSH agent (`SSH_AUTH_SOCK`), the single-instance socket, local SFTP, recordings, the remote monitor, X11 forwarding's cookie handling.
- **Already macOS-aware:**
  - `opensesh-core::paths` has a `MacOs` platform: `~/Library/Application Support/OpenSesh` and `~/Library/Caches`;
  - the file watcher uses FSEvents (`notify` with `macos_fsevent`);
  - the keyring crate already brings its macOS Keychain store (`apple-native-keyring-store` is in `Cargo.lock`);
  - `ShortcutHost.qml` shows shortcuts in the Mac's own notation;
  - the About page knows the platform.
- **Linux or Windows only today, to be done for macOS:**
  - the memory figure (`platform::resident_memory`: Windows and `/proc` only);
  - the desktop detection for window decorations (`XDG_CURRENT_DESKTOP`);
  - the RDP keyboard: Windows and XKB scan codes are mapped, while macOS falls back to Qt's key codes, which lose the keyboard layout's physical positions;
  - the custom title bar with its own minimize, maximize and close buttons;
  - the Windows-only parts of the release and the updater;
  - Waypipe (Wayland only: hidden on macOS).
- **Not in the code at all yet:** the macOS keyboard model, the menu bar, the `.app` bundle, signing and notarization, a CI job.

## Before starting (the owner)

- **A Mac to test on.** CI can build, run the tests and the offscreen smoke tests on GitHub's macOS runners, but the keyboard, menus, Retina rendering, input methods and VoiceOver need a person on a real Mac (or a tester). macOS can't run in a virtual machine except on Apple hardware.
- **The Apple Developer Program** (99 USD a year) for a Developer ID certificate and notarization. Without it, the app can only be ad-hoc signed. Then Gatekeeper refuses to open it until the user allows it in System Settings > Privacy & Security, because macOS 15 removed the Control-click "Open" shortcut. The Keychain would also ask again after every update.
- **Decisions** (proposed below, to confirm in ADRs at the start of each sprint):
  - Apple Silicon only, or a universal binary with Intel too;
  - the oldest macOS supported;
  - the default shortcuts on the Mac;
  - whether Homebrew comes through a tap of the owner's or the official cask list.

## Sprint 19: it builds, runs and is tested on macOS

**Goal:** the app, the CLI and the RDP helper build on macOS, the test suite and the smoke tests pass on GitHub's macOS runners, and what a Mac does differently underneath (paths, environment, shells, Keychain) is handled.

### Scope notes

- **Toolchain:**
  - Qt 6.10.3 for macOS from aqtinstall (the `clang_64` build; Qt's macOS binaries are universal);
  - Rust's `aarch64-apple-darwin` target, plus `x86_64-apple-darwin` if Intel is kept;
  - the Xcode command line tools, and cxx-qt pointed at qmake through `QMAKE` as elsewhere.

  Check at the sprint's start:
  - the oldest macOS that Qt 6.10 supports (that becomes OpenSesh's minimum, written in the ADR);
  - which macOS runner images GitHub offers then (`macos-15` is Apple Silicon; whether an Intel image still exists).
- **A GUI app's environment:** an app started from the Finder or the Dock gets launchd's minimal `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`), no `LANG`, and not the user's shell setup. That means no Homebrew (`/opt/homebrew/bin`, `/usr/local/bin`), so no `mosh-client`, `docker`, `kubectl`, `git` from Homebrew, or the user's editor.
  - OpenSesh reads the login shell's environment once, in the background (`$SHELL -l -c` printing `PATH` and the locale), with a timeout. It falls back to the Homebrew folders.
  - It uses that environment for local shells and for every program it starts.
  - An ADR records how.
- **Local shells:**
  - the default is the user's login shell (zsh on current macOS);
  - the shell starts as a login shell, as Terminal.app does, so `path_helper` and `/etc/zprofile` run;
  - `/etc/shells` lists the others;
  - `LANG`/`LC_CTYPE` get a UTF-8 locale when the environment has none;
  - `TERM_PROGRAM` names OpenSesh.
- **The vault's key in the Keychain:** the keyring crate's macOS store. The sprint checks:
  - a missing or locked Keychain;
  - the "allow access" prompt, which an unsigned build gets after every rebuild;
  - the vault's existing fallback, the master password.
- **The platform layer, item by item:**
  - the memory figure through Mach (`task_info`), in a small module with its `unsafe` documented as elsewhere;
  - "auto" window decorations mean native on macOS;
  - the single-instance socket path stays under macOS's 104-byte `sun_path` limit (`~/Library/Application Support/OpenSesh/…` with a long user name; a shorter fallback if needed);
  - `/dev/cu.*` serial ports are listed;
  - `open` replaces `xdg-open` wherever a file or link is opened.
- **CI:** a macOS job (Apple Silicon) with clippy, the tests, the build, the RDP helper and its test server, the offscreen smoke tests (main window and gallery), and the screenshots as an artifact. The jobs that need Linux containers (the real SSH, S3, RDP and VNC servers) stay on Linux.
- **Not in this sprint:** the Mac shortcuts, menus and window chrome (Sprint 20), packaging (Sprint 21).

### Checklist

- [ ] ADR: architectures, minimum macOS, toolchain
- [ ] The workspace and the RDP helper build on macOS; `cargo xtask` tasks that matter there work (icons, i18n, lint-qml, rdp)
- [ ] The login shell's environment for a GUI app (PATH, locale), with a timeout and fallbacks; used by local shells and every program started
- [ ] Local shells as login shells; `/etc/shells`; UTF-8 locale; `TERM_PROGRAM`
- [ ] The Keychain store for the vault's key; prompts and a locked Keychain handled
- [ ] Platform layer: memory through Mach, native decorations, the socket path length, `/dev/cu.*` serial ports, opening files and links with `open`
- [ ] A macOS CI job: clippy, tests, build, offscreen smoke tests, screenshots
- [ ] **Done when:** CI on macOS builds everything and passes the tests and both offscreen smoke tests, and a local shell tab works there

## Sprint 20: a Mac app (keyboard, menus, window)

**Goal:** OpenSesh behaves the way Mac users expect: Command shortcuts, a menu bar, the native window controls, Retina rendering, input methods and VoiceOver.

### Scope notes

- **The keyboard** (an ADR):
  - **Command for the app.** Qt already maps `Ctrl` in shortcuts to Command on macOS: Qt's ControlModifier is the Command key there, and MetaModifier is the Control key. PLAN §6.4 asks for "Ctrl becomes Cmd". On the Mac, Command doesn't collide with terminal programs, so the Mac defaults follow Mac terminals rather than translating the Linux ones key for key: Cmd+T new tab, Cmd+W close, Cmd+C and Cmd+V copy and paste, Cmd+D and Cmd+Shift+D split, Cmd+1 to 9, Cmd+F find, Cmd+K clear, Cmd+, settings, Cmd+Q quit, Cmd+Shift+P the palette. The Mac keybindings get their own defaults table, still editable, with conflict detection.
  - **Control for the terminal.** The physical Control key reaches the terminal as Control: the input encoder (`opensesh-term`) reads Qt's Meta as Control on macOS.
  - **Option:** a setting per side, left and right, as iTerm2 has: Option types accented characters (the Mac default), or sends Alt/Meta (Esc+key).
  - **System shortcuts stay the system's:** Cmd+Tab, Cmd+Space, Cmd+H, Cmd+M, Cmd+\`, Cmd+Ctrl+Space (emoji).
  - Cmd+click opens links.
- **The menu bar:** the app menu (About OpenSesh, Settings…, Services, Hide, Quit) and File, Edit, View, Window and Help menus, built from the action registry, so the palette, the shortcuts and the menus stay one list. At the start, check which Qt API gives a native macOS menu bar from QML in Qt 6.10 (Qt Quick Controls' `MenuBar` or `Qt.labs.platform`).
- **The window:**
  - the native "traffic light" buttons instead of the custom ones, with the content under a transparent title bar if Qt 6.10 allows it (check the window flags at the start) or a native title bar otherwise;
  - native full screen;
  - reopening a window when the Dock icon is clicked with none open;
  - a Dock menu (new window, recent hosts).
- **Rendering:** the terminal renderer on Metal (Qt's RHI picks it), the glyph atlas at the Retina scale, the bundled fonts with macOS's fallback for emoji and CJK. The same budgets as elsewhere (PLAN §9), measured on a real Mac when there is one.
- **Text input:** the input methods (Japanese, Chinese, Korean), dead keys and Option accents, the emoji picker and dictation into the terminal (the IME notes of `docs/testing/ime.md`, for macOS).
- **Remote desktops:** the RDP keyboard from macOS's virtual key codes (by position, like the Windows and XKB paths); Command maps to the Windows key, or to Control, as a setting. The VNC keysyms are checked.
- **Protocols and importers, Mac specifics:**
  - X11 forwarding to XQuartz: launchd's `DISPLAY` is a socket path such as `/private/tmp/com.apple.launchd.…/org.xquartz:0`, which the `DISPLAY` parser must accept;
  - Waypipe is hidden;
  - containers through Docker Desktop, OrbStack or colima, found with the login shell's `PATH`;
  - Apple's OpenSSH options in `~/.ssh/config` (`UseKeychain`) are accepted and passed over by the importer;
  - the PuTTY importer reads `~/.putty`.
- **Accessibility:** VoiceOver reads the rail, the tabs and the dialogs (the names exist; check them with VoiceOver); "Increase contrast" and "Reduce motion" are followed where Qt reports them on macOS (check at the start).

### Checklist

- [ ] ADR: the Mac keyboard (Command, Control, Option), with the Mac defaults table
- [ ] Control reaches the terminal; Option per side; Cmd+click on links
- [ ] The menu bar from the action registry
- [ ] Native window controls, full screen, Dock behavior
- [ ] Metal rendering and Retina glyphs checked; fonts and fallback
- [ ] Input methods, dead keys, emoji picker
- [ ] RDP keys from macOS key codes; Command's mapping; VNC checked
- [ ] XQuartz `DISPLAY`, Waypipe hidden, containers, `UseKeychain` in the importer
- [ ] VoiceOver, Increase contrast, Reduce motion
- [ ] Screenshots of the Mac app (CI) for the docs
- [ ] **Done when:** on a real Mac, the shortcuts, menus, window controls, an input method and VoiceOver work (the manual matrix), and CI stays green

## Sprint 21: the `.dmg`, signing, notarization and the release

**Goal:** a tag also produces a signed, notarized macOS `.dmg` that installs and starts on a clean runner, and Homebrew can install it.

### Scope notes

- **The `.app` bundle:**
  - `macdeployqt` puts Qt in the bundle. It also needs the offscreen platform plugin for the install check, since it leaves that plugin out just as `windeployqt` does (Sprint 18's lesson).
  - `Info.plist` holds the identifier `cc.caixa.OpenSesh`, the version, the minimum macOS, `NSHighResolutionCapable` and the application category.
  - The icon is an `.icns` made by `cargo xtask icons` from the generated logo (icons are never drawn by hand).
  - The RDP helper and the CLI go inside the bundle (`Contents/MacOS`), with the helper lookup taught where to find them. The CLI is linked into `PATH` by Homebrew, or by hand as the guide explains.
  - Optionally, handlers for the `ssh://`, `sftp://`, `telnet://`, `rdp://` and `vnc://` URL schemes, which quick connect already parses.
- **Architectures:** as decided in Sprint 19's ADR. Either Apple Silicon only, or a universal binary (two Rust targets, `lipo`; Qt is already universal). Apple has said macOS 26 is the last release for Intel Macs, which weighs toward Apple Silicon only, or Intel for a limited time.
- **Signing and notarization** (an ADR):
  - every Mach-O file is signed from the inside out with the Developer ID certificate and the hardened runtime: the Qt frameworks and plugins, the helper, the CLI, then the app;
  - the `.dmg` is signed;
  - the app is notarized with `notarytool` (an App Store Connect API key in the repository's secrets) and the ticket stapled with `stapler`.

  Without the secrets the build is ad-hoc signed, as Sprint 18 does for Windows without a certificate, and the release notes say so.
- **The `.dmg`:** the app and a link to Applications, made with `hdiutil` (or macdeployqt's own `-dmg`).
- **Homebrew:** the release workflow writes a cask, as it writes the Scoop and winget manifests: for a tap of the owner's (`caixax/homebrew-tap`), and later for Homebrew's own cask list, if the project meets its notability rules.
- **Updates:** the in-app check opens the download page on macOS, as the portable app does; Homebrew updates its installs. An in-place updater for the `.app` is left for later (its own ADR if wanted).
- **The Release workflow:** a macOS job builds, signs and notarizes (when configured), then installs the `.dmg` on a clean runner: it mounts it, copies the app to `/Applications`, and runs `--version`, the CLI and the gallery's offscreen smoke test, with timeouts. The `.dmg` joins `SHA256SUMS.txt`, and `scripts/release-assets.sh` adds the cask. The local release script leaves macOS to CI (there is no Mac here).
- **Documentation:**
  - README: installing on macOS (the `.dmg`, Homebrew, Gatekeeper when the build isn't notarized);
  - the user guide: the Mac shortcuts and the menu bar;
  - `docs/dev-setup.md`: building on a Mac;
  - the manual matrix: a macOS column.

### Checklist

- [ ] The `.app` bundle: `macdeployqt`, `Info.plist`, `.icns`, the helper and the CLI inside, the offscreen plugin
- [ ] ADR: signing and notarization; the job signs and notarizes when the secrets exist, ad-hoc otherwise
- [ ] The `.dmg`, with its checksum in `SHA256SUMS.txt`
- [ ] The Homebrew cask made by the release
- [ ] The Release workflow installs and starts the `.dmg` on a clean macOS runner
- [ ] README, user guide, dev setup and manual matrix for macOS
- [ ] **Done when:** a tag produces the `.dmg` in CI, it installs and starts on a clean macOS runner (notarized when the secrets are set), and Homebrew installs it from the tap

## Risks and open questions

- **No Mac at hand:** everything visual or about input needs a person on a Mac. Until then, CI proves only that it builds, that the tests pass and that it starts offscreen.
- **Notarization needs a paid Apple account;** without it, the first start on each Mac needs the Privacy & Security dance, and the Keychain asks again after every update.
- **Qt and macOS versions:** the minimum macOS follows Qt 6.10's supported platforms. A future Qt upgrade can raise it.
- **The login shell's environment** can be slow or print noise (a heavy `.zshrc`). Hence the timeout, the fallback folders, and a setting to override the `PATH`.
- **App Store:** out of scope. Its sandbox doesn't allow starting local shells and other programs the way a terminal must.
- **Not planned:** an in-place updater for the `.app` (Sparkle and similar), iOS or iPadOS.
