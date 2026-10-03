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
- [ ] The release workflow on tags: build, install and start every package, checksums, notes from the changelog, publish; a dry run by hand
- [ ] An Ubuntu 26.04 `.deb`, and `install.sh` choosing between Debian's and Ubuntu's
- [ ] Windows signing step (runs when a certificate is configured)
- [ ] winget and Scoop manifests in each release; AUR `PKGBUILD`s (`opensesh`, `opensesh-git`) built and checked by the workflow
- [ ] The release script bumps every workspace's version (app, RDP helper, fuzz), and can leave the building to CI (`-InCi`)

### Documentation
- [ ] User guide
- [ ] README with screenshots of the real app; install instructions per platform

### Quality
- [ ] **Done when:** a `v1.0.0` tag produces every package in CI, and they install and start on the Tier 1 systems (Windows, Debian/Ubuntu, Fedora, Arch)

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, deny, audit
- [ ] GitHub Actions green; the 1.0.0 release published
- [ ] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

(Filled in at the end of the sprint.)
