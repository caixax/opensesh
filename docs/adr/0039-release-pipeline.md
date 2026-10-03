# ADR 0039: The release pipeline: packages checked on clean systems, signing, and package managers

- **Status:** accepted
- **Date:** 2026-10-03
- **Sprint:** 18

## Context

PLAN Sprint 18 asks for OpenSesh to install anywhere in one command: a release workflow on tags that builds every package, installs it on a clean system and starts it, code signing, package manager manifests, a user guide, and the 1.0.0 release. "Done when": a `v1.0.0` tag produces every package in CI, and they install and start on the Tier 1 systems (Windows, Debian/Ubuntu, Fedora, Arch).

What existed before:

- **The formats are the owner's choice:** the Windows portable zip and NSIS installer, the `.deb` (Debian 13), the `.rpm` (Fedora) and the Arch package, on GitHub Releases. No AppImage or Snap; Flatpak and MSI aren't in the owner's list; macOS is best effort in PLAN and there is no Mac to check it on.
- **`scripts/release.ps1`** cuts releases on the owner's Windows machine, with the Linux packages built in its WSL distributions: minutes, where GitHub's runners take much longer. The owner wanted it that way.
- **A fallback workflow, run by hand,** built the same packages for an existing tag, but nothing installed them anywhere: a package with a missing dependency would have been found by its first user.

## Options

1. **Keep releasing from the owner's machine only.** Fast, but nothing checks the packages on clean systems, and a release needs that machine.
2. **Release from CI only.** Every package is built and checked before anyone can download it, but every release waits for GitHub's runners.
3. **Both: the tag starts a Release workflow that builds, checks and publishes.** The local script still publishes its own packages right away when the owner wants speed. The workflow then checks the same source on clean systems, and adds only what the release lacks.

## Decision

Option 3.

### The Release workflow (`.github/workflows/release.yml`)

- **Trigger:** a `v*` tag (`scripts/release.ps1` pushes it), or by hand for an existing tag. By hand with `dry_run`, on any branch: everything is built and tested, nothing is published, and the release's files are kept as an artifact. That is how the workflow was tried before the first tag that used it.
- **Version:** the tag must name `Cargo.toml`'s version, and `CHANGELOG.md` must have its section.
- **Build:** the Linux packages, each on its distribution against the distribution's Qt (`scripts/linux/build.sh`): Debian 13 and Ubuntu 26.04 `.deb`s, the Fedora `.rpm`, the Arch package; and the Windows zip and installer (`cargo xtask dist windows`).
- **Install and start:** each package is installed on a clean container or runner by the system's package manager, which has to find every dependency on its own:
  - the Debian 13 `.deb` on Debian 13, and the Ubuntu one on Ubuntu 26.04;
  - the `.rpm` on Fedora;
  - the pacman package on Arch;
  - the installer (silent, per user) and the portable zip on Windows.

  Then each one is started: `--version` must print the release's version, the CLI and the RDP helper must be where the app looks for them, and the component gallery's offscreen smoke test must pass. A missing dependency, a missing file or a library that doesn't load fails the release before it is published. The Windows packages carry Qt's offscreen platform plugin (114 KB) for that: `windeployqt` leaves it out by default, and without it the test would start a Qt that waits on an error dialog. Every command of these checks has a timeout.
- **AUR:** the two `PKGBUILD`s in `packaging/aur/` are built with `makepkg` on Arch, installed and started: `opensesh` from the tag's source archive (the workflow fills in its version and checksum), and `opensesh-git` at the tag's commit. Their `.SRCINFO` is generated there.
- **Publish:** `scripts/release-assets.sh` writes `SHA256SUMS.txt`, the notes (install instructions and the version's changelog section), the Scoop manifest `opensesh.json`, and `OpenSesh-X.Y.Z-package-manifests.zip`. The zip holds what is submitted by hand: the three winget-pkgs manifests (schema 1.12.0, the one `wingetcreate` writes today), the Scoop manifest, and the AUR files. When the local script already published the release, its packages stay, since they are what people download. The manifests are then made from those packages, and only the missing files are added.

### Code signing

`cargo xtask dist windows` signs OpenSesh's own executables (the app, the CLI, the RDP helper) when `OPENSESH_SIGN` names a signing command. It also has makensis sign the installer and its uninstaller (`!finalize`, `!uninstfinalize`). In the workflow, the command is `packaging/windows/sign.cmd`. It is set up only when the secrets `WINDOWS_SIGNING_CERTIFICATE` (a base64 `.pfx`) and `WINDOWS_SIGNING_PASSWORD` exist. The certificate is imported into the runner's user store and signtool signs by thumbprint, with an RFC 3161 timestamp, so the password never reaches a command line. There is no certificate yet: the step is skipped, and the release says the packages are unsigned.

### Package managers

- **Scoop:** each release has `opensesh.json`, so `scoop install <its URL>` works without a bucket. The manifest installs the portable zip, keeps its `data` folder between updates (`persist`) and knows how to update itself (`checkver`, `autoupdate` with `SHA256SUMS.txt`).
- **winget:** the manifests in the zip are for a pull request to `microsoft/winget-pkgs` (the package `caixax.OpenSesh`, the per-user NSIS installer). Submitting them is the owner's call.
- **AUR:** the `PKGBUILD` and `.SRCINFO` in the zip are pushed to the AUR by hand. `opensesh` builds the release's source; `opensesh-git` builds the repository.
- **Not done:** Flatpak, Nix, Homebrew and Chocolatey (not in the owner's list, or optional in PLAN).

### Versions

`scripts/release.ps1` also sets the RDP helper's workspace version and updates its lock file and the fuzz workspace's, so `--locked` builds keep working after a release. With `-InCi` it only sets the version, tests, tags and pushes, and the workflow builds and publishes.

## Consequences

- Every package is installed and started on a clean system before a release is public, even when the owner publishes from their machine (in that case, the same source, built again, is what CI checks).
- A release from CI takes the runners' time: about as long as the slowest package's build plus its install.
- The Windows packages stay unsigned until a certificate is bought or donated (SmartScreen warns on first start). Adding one needs only the two secrets.
- winget and the AUR need a person to submit each version. Scoop works from the release itself.
- **A `.deb` per release of Debian or Ubuntu.** The app uses Qt's private API, so a `.deb` depends on the exact Qt it was built with (`qt6-declarative-private-abi (= 6.8.2)` on Debian 13). The Debian 13 package can't install on Ubuntu, so Ubuntu 26.04, the newest LTS, gets its own (`opensesh_X.Y.Z_ubuntu26.04_amd64.deb`), built by the workflow. `install.sh` picks the right one and sends other releases to build from source. The local script's WSL distributions have no Ubuntu 26.04, so the workflow adds that package to a release the script published.
- **The same private API ties the `.rpm` and the Arch package to their distribution's Qt.** Their dependencies don't name the exact version the way Debian's do, so a distribution's Qt update can need a rebuild. Watching for that is part of each release.
