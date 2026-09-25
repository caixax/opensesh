#!/usr/bin/env bash
# OpenSesh installer for Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/caixax/opensesh/main/install.sh | bash
#
# Finds out which distribution this machine runs, downloads the matching package of the latest
# release (a .deb for Debian 13, an .rpm for Fedora, a pacman package for Arch), checks it against
# the release's SHA256SUMS.txt and installs it with the system's package manager, which pulls in
# Qt. On a Wayland session it also adds Qt's Wayland plugin. Running it again updates OpenSesh.
#
# Options (after `bash -s --` when piping):
#   --version X.Y.Z   install that release instead of the latest
#   --yes             do not ask before installing packages
#   --uninstall       remove OpenSesh (settings stay)
#   --dry-run         show what would be done and do nothing
set -euo pipefail

REPO="caixax/opensesh"
version=""
assume_yes=0
uninstall=0
dry_run=0
while [ $# -gt 0 ]; do
    case "$1" in
        --version) version="${2:-}"; shift ;;
        --yes | -y) assume_yes=1 ;;
        --uninstall) uninstall=1 ;;
        --dry-run) dry_run=1 ;;
        -h | --help) sed -n '2,16p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

if [ -t 1 ]; then
    bold=$'\033[1m'; dim=$'\033[2m'; red=$'\033[31m'; green=$'\033[32m'; yellow=$'\033[33m'; reset=$'\033[0m'
else
    bold=""; dim=""; red=""; green=""; yellow=""; reset=""
fi
step() { printf '%s==>%s %s\n' "$bold" "$reset" "$*"; }
note() { printf '    %s%s%s\n' "$dim" "$*" "$reset"; }
warn() { printf '%swarning:%s %s\n' "$yellow" "$reset" "$*" >&2; }
die() { printf '%serror:%s %s\n' "$red" "$reset" "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# ------------------------------------------------------------------ machine

[ "$(uname -s)" = "Linux" ] || die "this installer is for Linux; Windows has its own setup on the releases page"
[ "$(uname -m)" = "x86_64" ] || die "only x86_64 builds exist for now (this is $(uname -m))"
have curl || die "curl is needed to download the release"

# shellcheck disable=SC1091
. /etc/os-release 2>/dev/null || die "cannot read /etc/os-release"
family=""
for id in ${ID:-} ${ID_LIKE:-}; do
    case "$id" in
        arch | archlinux | manjaro | endeavouros | cachyos) family=arch ;;
        debian | ubuntu | linuxmint | pop) family=debian ;;
        fedora | nobara) family=fedora ;;
    esac
    [ -n "$family" ] && break
done
[ -n "$family" ] || die "no OpenSesh package for ${PRETTY_NAME:-this distribution} yet (Debian 13, Fedora and Arch are packaged); build it from source: https://github.com/$REPO"

# The .deb is built against Debian 13's Qt 6.8.2 and asks for exactly that Qt.
if [ "$family" = "debian" ] && [ "${ID:-}" != "debian" ]; then
    die "${PRETTY_NAME:-this distribution} is not packaged yet: the .deb needs Debian 13's Qt; build from source: https://github.com/$REPO"
fi
if [ "$family" = "debian" ] && [ "${VERSION_ID:-0}" -lt 13 ] 2>/dev/null; then
    die "Debian ${VERSION_ID:-?} is too old: OpenSesh needs Qt 6.8 (Debian 13 or later)"
fi

session="${XDG_SESSION_TYPE:-}"
if [ -z "$session" ]; then
    if [ -n "${WAYLAND_DISPLAY:-}" ]; then session=wayland; elif [ -n "${DISPLAY:-}" ]; then session=x11; else session=unknown; fi
fi

step "This machine"
note "distribution: ${PRETTY_NAME:-$ID} (family: $family)"
note "session:      $session"

sudo_cmd=""
if [ "$(id -u)" -ne 0 ]; then
    have sudo || die "sudo is needed to install packages (or run this as root)"
    sudo_cmd="sudo"
fi
run() {
    if [ "$dry_run" -eq 1 ]; then
        note "would run: $*"
    else
        "$@"
    fi
}
confirm() {
    [ "$assume_yes" -eq 1 ] && return 0
    [ "$dry_run" -eq 1 ] && return 0
    # Piped from curl, stdin is the script; the terminal is the way to ask.
    if [ -r /dev/tty ]; then
        printf '%s [Y/n] ' "$1" > /dev/tty
        read -r answer < /dev/tty || answer=""
    else
        return 0
    fi
    case "$answer" in n | N | no | NO) return 1 ;; *) return 0 ;; esac
}

# ---------------------------------------------------------------- uninstall

if [ "$uninstall" -eq 1 ]; then
    step "Removing OpenSesh"
    case "$family" in
        debian) run $sudo_cmd apt-get remove -y opensesh ;;
        fedora) run $sudo_cmd dnf remove -y opensesh ;;
        arch) run $sudo_cmd pacman -R --noconfirm opensesh ;;
    esac
    note "settings (~/.config/opensesh) and data (~/.local/share/opensesh) were left alone"
    exit 0
fi

# ----------------------------------------------------------------- download

step "Finding the release"
if [ -z "$version" ]; then
    tag="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)"
    [ -n "$tag" ] || die "could not read the latest release from GitHub"
    version="${tag#v}"
else
    tag="v${version#v}"
    version="${version#v}"
fi
case "$family" in
    debian) asset="opensesh_${version}_amd64.deb" ;;
    fedora) asset="opensesh-${version}.x86_64.rpm" ;;
    arch) asset="opensesh-${version}-x86_64.pkg.tar.zst" ;;
esac
base="https://github.com/$REPO/releases/download/$tag"
note "OpenSesh $version: $asset"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
if [ "$dry_run" -eq 0 ]; then
    curl -fL --progress-bar -o "$tmp/$asset" "$base/$asset" || die "release $tag has no $asset"
    curl -fsSL -o "$tmp/SHA256SUMS.txt" "$base/SHA256SUMS.txt" || die "release $tag has no SHA256SUMS.txt"
    # The file may come from a Windows machine with CRLF line ends.
    expected="$(tr -d '\r' < "$tmp/SHA256SUMS.txt" | awk -v f="$asset" '$2 == f { print $1 }')"
    [ -n "$expected" ] || die "SHA256SUMS.txt does not list $asset"
    actual="$(sha256sum "$tmp/$asset" | cut -d' ' -f1)"
    [ "$expected" = "$actual" ] || die "checksum mismatch for $asset, nothing was installed"
    note "checksum verified"
    # pacman and dnf run the package manager as root: let it read the file.
    chmod 755 "$tmp"
    chmod 644 "$tmp/$asset"
fi

# ------------------------------------------------------------------ install

confirm "Install OpenSesh $version with the package manager?" || die "cancelled"
step "Installing"
case "$family" in
    debian)
        run $sudo_cmd apt-get update -qq || true
        run $sudo_cmd apt-get install -y "$tmp/$asset"
        [ "$session" = "wayland" ] && { run $sudo_cmd apt-get install -y qt6-wayland || warn "could not install qt6-wayland"; }
        ;;
    fedora)
        run $sudo_cmd dnf install -y "$tmp/$asset"
        [ "$session" = "wayland" ] && { run $sudo_cmd dnf install -y qt6-qtwayland || warn "could not install qt6-qtwayland"; }
        ;;
    arch)
        run $sudo_cmd pacman -U --noconfirm "$tmp/$asset"
        [ "$session" = "wayland" ] && { run $sudo_cmd pacman -S --noconfirm --needed qt6-wayland || warn "could not install qt6-wayland"; }
        ;;
esac

if [ "$dry_run" -eq 1 ]; then
    printf '\nDry run: nothing was downloaded or installed.\n'
    exit 0
fi
printf '\n%sOpenSesh %s is installed.%s Start it from your applications menu or run: opensesh-app\n' "$green" "$version" "$reset"
