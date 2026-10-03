# Sprint 18: packaging and the 1.0 release

**Goal:** install OpenSesh anywhere in one command.

**Started:** 2026-10-03

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only (the Spanish user guide waits for the translation, which the owner postponed). The repository is public on GitHub, and the internal planning files stay out of it. The owner asked on 2026-10-02 to keep going from sprint to sprint, and on 2026-10-03 to finish the program.
- **Formats** (the owner's choice, kept since the first releases): the Windows portable zip (with the `portable` marker) and the NSIS installer, `.deb`, `.rpm` and the Arch package, on GitHub Releases. **Not built:** AppImage and Snap (the owner's call). Flatpak and the MSI aren't in the owner's list either, and macOS is best effort in PLAN with no Mac to check it on.
- **What already exists:**
  - `cargo xtask dist windows` (windeployqt, the zip, the NSIS installer);
  - `scripts/linux/build.sh` (the Linux packages);
  - `scripts/release.ps1`, a local release: version, changelog, packages, tag, release;
  - a fallback release workflow run by hand;
  - the update check (opt-in, off by default), from an earlier sprint.
- **The release workflow** runs on a `v*` tag:
  - it builds every package on GitHub's runners;
  - it **installs each one on a clean system and starts it** (the package manager pulls its dependencies; then `--version` and the gallery's offscreen smoke test);
  - it writes `SHA256SUMS.txt` and the winget and Scoop manifests;
  - it publishes the release with the notes of its version from `CHANGELOG.md`.

  A dry run (by hand) tries all of it without publishing, before the first real tag.
- **Code signing:** a step that signs the Windows binaries when a certificate is configured (repository secrets), skipped otherwise. There is no certificate yet.
- **Package managers:**
  - winget and Scoop manifests made by the release (to submit by hand to their repositories);
  - `PKGBUILD`s for the AUR: `opensesh` from the release's source, `opensesh-git` from the repository;
  - Nix: optional in PLAN, left out.
- **Debian and Ubuntu (found at the start):** the app uses Qt's private API, so the `.deb` depends on the exact Qt it was built with (`qt6-declarative-private-abi (= 6.8.2)`). The Debian 13 package can't install on Ubuntu, so Ubuntu 26.04 LTS gets its own `.deb`, built by the workflow, and `install.sh` picks the right one.
- **Versions:** the release script also bumps the RDP helper's workspace and the fuzz workspace (each has its own lock file).
- **Documentation:** a user guide (`docs/user-guide.md`) and the README with screenshots taken from the real app by `--screenshots` (sample data only, nothing personal).
- **The 1.0.0 tag** is made at the end, once CI is green, and its workflow must produce every package.

## Checklist

### Release
- [x] The release workflow on tags: build, install and start every package, checksums, notes from the changelog, publish; a dry run by hand
- [x] An Ubuntu 26.04 `.deb`, and `install.sh` choosing between Debian's and Ubuntu's
- [x] Windows signing step (runs when a certificate is configured)
- [x] winget and Scoop manifests in each release; AUR `PKGBUILD`s (`opensesh`, `opensesh-git`) built and checked by the workflow
- [x] The release script bumps every workspace's version (app, RDP helper, fuzz), and can leave the building to CI (`-InCi`)

### Documentation
- [x] User guide
- [x] README with screenshots of the real app; install instructions per platform

### Quality
- [x] **Done when:** a `v1.0.0` tag produces every package in CI, and they install and start on the Tier 1 systems (Windows, Debian/Ubuntu, Fedora, Arch)

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, deny, audit
- [x] GitHub Actions green; the 1.0.0 release published
- [x] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

**Closed:** 2026-10-03, with **OpenSesh 1.0.0** published.

### What was done

- **The Release workflow** ([ADR 0039](../adr/0039-release-pipeline.md)), on a `v*` tag or by hand:
  - it checks that the tag names `Cargo.toml`'s version and that the changelog has its section;
  - it builds the Linux packages on their distributions (Debian 13 and Ubuntu 26.04 `.deb`s, the Fedora `.rpm`, the Arch package) and the Windows installer and portable zip;
  - it **installs every package on a clean system with its package manager and starts it** (`--version`, the CLI, the RDP helper and the desktop file in place, the gallery's offscreen smoke test), on Debian 13, Ubuntu 26.04, Fedora, Arch and Windows (the silent per-user installer, and the portable zip);
  - it builds the two AUR `PKGBUILD`s with `makepkg`, installs and starts them;
  - it publishes the release with `SHA256SUMS.txt`, the notes from `CHANGELOG.md`, the Scoop manifest and the package manager manifests.

  A dry run (by hand) does all of it without publishing, and every job has a time limit.
- **Ubuntu 26.04 LTS** has its own `.deb`, because the app uses Qt's private API: the Debian 13 package depends on Debian's exact Qt (`qt6-declarative-private-abi (= 6.8.2)`), which the old docs didn't say. `install.sh` picks the package for Debian 13 or Ubuntu 26.04, and sends other releases to build from source.
- **Code signing:** with `OPENSESH_SIGN` set, `cargo xtask dist windows` signs OpenSesh's executables and has makensis sign the installer and its uninstaller (`!finalize`, `!uninstfinalize`). The workflow imports a certificate from two repository secrets when they exist. There is no certificate yet, so the 1.0.0 packages are unsigned.
- **Package managers:**
  - **Scoop:** each release has `opensesh.json`, installable from its URL, that keeps the portable `data` folder and updates itself.
  - **winget:** the three manifests (schema 1.12.0) for `caixax.OpenSesh`, in `OpenSesh-X.Y.Z-package-manifests.zip`.
  - **AUR:** `packaging/aur/opensesh` (the release's source) and `opensesh-git`, with a `.SRCINFO` made by the workflow.
- **The release script** sets the RDP helper's version too, updates the three lock files, and with `-InCi` leaves the building and publishing to the workflow (how 1.0.0 was released).
- **Documentation:** a user guide (`docs/user-guide.md`, checked against the code: shortcuts, settings pages, quick connect forms, file locations); the README with screenshots of the app at 1.0.0 (sample data only, native rendering) and every way to install; `docs/dev-setup.md`; ADR 0039.

### "Done when" (PLAN)

- **A `v1.0.0` tag produces every package in CI, and they install and start on the Tier 1 systems (Windows, Debian/Ubuntu, Fedora, Arch):** **met.** The tag's Release run built every package (the Windows installer and portable zip, the Debian 13 and Ubuntu 26.04 `.deb`s, the Fedora `.rpm`, the Arch package, and the two AUR packages), installed each one on a clean Debian 13, Ubuntu 26.04, Fedora, Arch and Windows system with its package manager, started it, and published [OpenSesh 1.0.0](https://github.com/caixax/opensesh/releases/tag/v1.0.0) with its checksums and manifests. Real desktops (Windows 10/11 machines, KDE, GNOME, Hyprland) are in the manual matrix for the owner.

### How it was verified

- **Windows (Qt 6.10.3):**
  - fmt, clippy `-D warnings`, lint-qml, i18n, `cargo deny`, `cargo audit`;
  - every test (738 in the app's workspace, run again by the release script at 1.0.0);
  - the smoke tests offscreen, native and the gallery;
  - `actionlint` on every workflow;
  - the installer's signing directives with a stand-in signing command (makensis called it for a test installer and its uninstaller);
  - `scripts/release-assets.sh` on stand-in packages;
  - the deployed Windows folder's gallery smoke test, with Qt's offscreen plugin and without Qt on the `PATH`.
- **GitHub Actions:** dry runs of the Release workflow until it was green end to end (below), CI green on every commit, and the 1.0.0 run itself.

### Problems found and fixed

- **The `.deb` installed only on Debian 13** (Qt's private ABI), which the README contradicted for Ubuntu: Ubuntu 26.04 now has its own package.
- **The Windows runner has no NSIS:** the workflow installs NSIS 3.12.0 (the version used here) with Chocolatey.
- **The silent install hung for hours:** Git Bash turned `/S` into `C:/Program Files/Git/S`, so the installer opened its window and waited. Fixed with `MSYS_NO_PATHCONV=1`, and every job and check now has a time limit.
- **The Windows packages had no offscreen platform plugin** (`windeployqt` leaves it out): the smoke test of an installed copy started a Qt that waited on an error dialog. The packages now carry `qoffscreen.dll` (114 KB).
- **SFTP hosts still said "arrives in Sprint 8"** (found while checking the user guide against the code, fixed at the end of Sprint 17).

### Deviations

- **No code signing certificate:** the step exists and is skipped; SmartScreen warns on first start.
- **winget and the AUR need the owner:** the files are in each release, and submitting them is a person's job (a pull request to `microsoft/winget-pkgs`, a push to the AUR).
- **Debian 13 and Ubuntu 26.04 only** for `.deb`s, for the reason above; Flatpak, AppImage, Snap and macOS stay out (the owner's formats).
- **The user guide is in English;** the Spanish one waits for the translation, as the owner decided.
- **The manual matrix** still lists what only the owner can check (desktops, screen readers, real servers, two computers syncing).

### For the owner

- Submit the winget manifests and push the AUR files from `OpenSesh-1.0.0-package-manifests.zip` if you want those channels.
- A code signing certificate, when there is one, goes in the secrets `WINDOWS_SIGNING_CERTIFICATE` (the `.pfx`, base64) and `WINDOWS_SIGNING_PASSWORD`.
- Other clones of the repository (a Forgejo mirror, another computer) need `git fetch` and `git reset --hard origin/main` after the author rewrite of 2026-10-03.
