#!/usr/bin/env bash
# Writes what a release publishes next to its packages (ADR 0039), from the packages in a folder:
#
#   SHA256SUMS.txt        the packages' checksums (install.sh and the in-app updater check them)
#   release-notes.md      how to install, and the version's section of CHANGELOG.md
#   opensesh.json         the Scoop manifest: `scoop install <its URL in the release>`
#   OpenSesh-X.Y.Z-package-manifests.zip
#                         what to submit to the package managers' repositories: winget/ (the three
#                         winget-pkgs manifests), scoop/ and, when an AUR folder is given, aur/
#                         (the opensesh PKGBUILD and .SRCINFO the release workflow built and checked)
#
#   scripts/release-assets.sh 1.0.0 dist [aur-folder]
#
# The release workflow runs it on the packages it publishes (or on those a local release already
# published); it needs bash, GNU coreutils and findutils, awk and Python 3 (for the zip).
set -euo pipefail

if [ $# -lt 2 ]; then
    echo "usage: $0 <version> <dist folder> [aur folder]" >&2
    exit 2
fi
version="$1"
dist="$2"
aur=""
if [ -n "${3:-}" ]; then aur="$(cd "$3" && pwd)"; fi
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo="caixax/opensesh"
base="https://github.com/$repo/releases/download/v$version"
summary="Remote connections client: terminal, SSH, SFTP, tunnels, RDP and VNC"
setup="OpenSesh-$version-windows-x64-setup.exe"
portable="OpenSesh-$version-windows-x64-portable.zip"
manifests="OpenSesh-$version-package-manifests.zip"

cd "$dist"
for file in "$setup" "$portable"; do
    [ -f "$file" ] || { echo "$dist/$file is missing" >&2; exit 1; }
done
sha() { sha256sum -- "$1" | cut -d' ' -f1; }

# ---- checksums: the packages only (LF line endings, sorted by name)
find . -maxdepth 1 -type f \( -name '*.exe' -o -name '*.zip' -o -name '*.deb' -o -name '*.rpm' -o -name '*.zst' \) \
    ! -name '*-package-manifests.zip' -printf '%f\n' | LC_ALL=C sort | while read -r file; do
    printf '%s  %s\n' "$(sha "$file")" "$file"
done > SHA256SUMS.txt
cat SHA256SUMS.txt

# ---- release notes
section="$(awk -v version="$version" '
    index($0, "## [" version "]") == 1 { found = 1; next }
    found && /^## \[/ { exit }
    found { print }
' "$here/CHANGELOG.md" | sed -e '/./,$!d' -e "s#](docs/#](https://github.com/$repo/blob/v$version/docs/#g")"
if [ -z "$section" ]; then
    echo "CHANGELOG.md has no section for $version" >&2
    exit 1
fi
cat > release-notes.md <<EOF
## Install

- **Windows 10/11:** \`$setup\` (per-user install, no administrator rights, updates itself), or the portable \`$portable\`. With [Scoop](https://scoop.sh):
  \`\`\`powershell
  scoop install $base/opensesh.json
  \`\`\`
- **Linux** (Debian 13, Ubuntu 26.04, Fedora, Arch):
  \`\`\`sh
  curl -fsSL https://raw.githubusercontent.com/$repo/main/install.sh | bash
  \`\`\`
  or install the \`.deb\` (Debian 13, or the \`ubuntu26.04\` one), \`.rpm\` or \`.pkg.tar.zst\` below with your package manager.

Check the downloads against \`SHA256SUMS.txt\`.

## Changes

$section
EOF

# ---- Scoop: the portable zip, its data folder kept between updates
cat > opensesh.json <<EOF
{
    "version": "$version",
    "description": "$summary",
    "homepage": "https://github.com/$repo",
    "license": "GPL-3.0-or-later",
    "architecture": {
        "64bit": {
            "url": "$base/$portable",
            "hash": "$(sha "$portable")",
            "extract_dir": "OpenSesh-$version-windows-x64"
        }
    },
    "bin": "bin\\\\opensesh.exe",
    "shortcuts": [
        [
            "OpenSesh.exe",
            "OpenSesh"
        ]
    ],
    "persist": "data",
    "checkver": "github",
    "autoupdate": {
        "architecture": {
            "64bit": {
                "url": "https://github.com/$repo/releases/download/v\$version/OpenSesh-\$version-windows-x64-portable.zip",
                "extract_dir": "OpenSesh-\$version-windows-x64"
            }
        },
        "hash": {
            "url": "\$baseurl/SHA256SUMS.txt"
        }
    }
}
EOF

# ---- winget: the per-user NSIS installer (winget-pkgs layout: manifests/c/caixax/OpenSesh/<version>)
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
winget="$work/winget/manifests/c/caixax/OpenSesh/$version"
mkdir -p "$winget" "$work/scoop"
schema="https://aka.ms/winget-manifest"
cat > "$winget/caixax.OpenSesh.yaml" <<EOF
# yaml-language-server: \$schema=$schema.version.1.12.0.schema.json

PackageIdentifier: caixax.OpenSesh
PackageVersion: $version
DefaultLocale: en-US
ManifestType: version
ManifestVersion: 1.12.0
EOF
cat > "$winget/caixax.OpenSesh.installer.yaml" <<EOF
# yaml-language-server: \$schema=$schema.installer.1.12.0.schema.json

PackageIdentifier: caixax.OpenSesh
PackageVersion: $version
InstallerType: nullsoft
Scope: user
InstallModes:
- interactive
- silent
- silentWithProgress
UpgradeBehavior: install
ProductCode: OpenSesh
ReleaseDate: $(date -u +%Y-%m-%d)
Installers:
- Architecture: x64
  InstallerUrl: $base/$setup
  InstallerSha256: $(sha "$setup" | tr '[:lower:]' '[:upper:]')
ManifestType: installer
ManifestVersion: 1.12.0
EOF
cat > "$winget/caixax.OpenSesh.locale.en-US.yaml" <<EOF
# yaml-language-server: \$schema=$schema.defaultLocale.1.12.0.schema.json

PackageIdentifier: caixax.OpenSesh
PackageVersion: $version
PackageLocale: en-US
Publisher: OpenSesh contributors
PublisherUrl: https://github.com/$repo
PublisherSupportUrl: https://github.com/$repo/issues
PackageName: OpenSesh
PackageUrl: https://github.com/$repo
License: GPL-3.0-or-later
LicenseUrl: https://github.com/$repo/blob/main/LICENSE
ShortDescription: $summary
Description: |-
  OpenSesh is an open source, lightweight remote connections client built with Rust and Qt 6:
  local terminals, SSH with SFTP and tunnels, telnet, serial ports, mosh, containers, S3 storage
  and remote desktops over RDP and VNC.
Tags:
- rdp
- remote-desktop
- sftp
- ssh
- terminal
- vnc
ReleaseNotesUrl: https://github.com/$repo/releases/tag/v$version
ManifestType: defaultLocale
ManifestVersion: 1.12.0
EOF
cp opensesh.json "$work/scoop/opensesh.json"
if [ -n "$aur" ]; then
    mkdir -p "$work/aur"
    cp -r "$aur"/. "$work/aur/"
fi
rm -f "$manifests"
out="$(pwd)/$manifests"
(cd "$work" && "${PYTHON:-python3}" -m zipfile -c "$out" ./*)
echo "== release assets in $dist"
ls -l SHA256SUMS.txt release-notes.md opensesh.json "$manifests"
