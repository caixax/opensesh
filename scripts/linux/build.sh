#!/usr/bin/env bash
# Builds the release binary on this distro, against the distro's own Qt, and packages it the way
# the distro family installs software: a .deb on Debian (13 and later, also Ubuntu 25.04 and
# later: Qt 6.8 or newer), an .rpm on Fedora, a pacman package on Arch.
#
#   wsl -d Debian -- bash /mnt/i/Projects/opensesh/scripts/linux/build.sh
#   wsl -d FedoraLinux-43 -- bash .../build.sh
#   wsl -d archlinux -- bash .../build.sh
#
# The working tree is copied into the Linux filesystem first (building on /mnt is several times
# slower), and the package lands in dist/ of the checkout the script was run from.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
work="${OPENSESH_BUILD_DIR:-$HOME/build/opensesh}"
out="$here/dist"
mkdir -p "$work" "$out"

find "$work" -mindepth 1 -maxdepth 1 ! -name target -exec rm -rf {} +
tar -C "$here" --exclude=./target --exclude=./dist --exclude=./.git --exclude=./.claude \
    --exclude=./CLAUDE.md -cf - . | tar -C "$work" -xf -

# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
export PATH="$HOME/.cargo/bin:$PATH"
export QMAKE="${QMAKE:-$(command -v qmake6 || command -v qmake)}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}"
cd "$work"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
echo "== building OpenSesh $version with $("$QMAKE" -query QT_VERSION) ($QMAKE)"
cargo build --release --locked -p opensesh-app
strip target/release/opensesh-app

# The installed tree, shared by every format.
stage="$work/target/package/root"
rm -rf "$work/target/package"
data=crates/opensesh-app/data
install -Dm755 target/release/opensesh-app "$stage/usr/bin/opensesh-app"
install -Dm644 "$data/cc.caixa.OpenSesh.desktop" "$stage/usr/share/applications/cc.caixa.OpenSesh.desktop"
install -Dm644 "$data/icons/cc.caixa.OpenSesh.svg" "$stage/usr/share/icons/hicolor/scalable/apps/cc.caixa.OpenSesh.svg"
install -Dm644 LICENSE "$stage/usr/share/licenses/opensesh/LICENSE"
for doc in README.md CHANGELOG.md THIRD_PARTY_NOTICES.md; do
    install -Dm644 "$doc" "$stage/usr/share/doc/opensesh/$doc"
done

summary="Remote connections client: terminal, SSH, SFTP, tunnels, RDP and VNC"
description="OpenSesh is an open source, lightweight remote connections client built
 with Rust and Qt 6. This release provides a fast local terminal; SSH, SFTP,
 tunnels and the other protocols follow in later releases."

. /etc/os-release
family=""
for id in ${ID:-} ${ID_LIKE:-}; do
    case "$id" in
        arch | archlinux) family=arch ;;
        debian | ubuntu) family=debian ;;
        fedora | rhel) family=fedora ;;
    esac
    [ -n "$family" ] && break
done

case "$family" in
    debian)
        deb="$work/target/package/deb"
        cp -a "$stage" "$deb"
        # Debian keeps the license under doc.
        mv "$deb/usr/share/licenses/opensesh/LICENSE" "$deb/usr/share/doc/opensesh/copyright"
        rm -rf "$deb/usr/share/licenses"
        # Shared libraries as this distro names them; QML modules and plugins load at run time.
        mkdir -p "$work/target/package/shlibs/debian"
        printf 'Source: opensesh\n\nPackage: opensesh\nArchitecture: any\n' \
            > "$work/target/package/shlibs/debian/control"
        shlibs="$(cd "$work/target/package/shlibs" && dpkg-shlibdeps -O --ignore-missing-info \
            "$deb/usr/bin/opensesh-app" | sed -n 's/^shlibs:Depends=//p')"
        qml="qml6-module-qtquick, qml6-module-qtquick-templates, qml6-module-qtquick-layouts, qml6-module-qtquick-window, qml6-module-qtqml-workerscript, qt6-svg-plugins"
        mkdir -p "$deb/DEBIAN"
        size="$(du -sk "$deb/usr" | cut -f1)"
        cat > "$deb/DEBIAN/control" <<EOF
Package: opensesh
Version: $version
Section: net
Priority: optional
Architecture: amd64
Maintainer: OpenSesh contributors <noreply@github.com>
Homepage: https://github.com/caixax/opensesh
Installed-Size: $size
Depends: $shlibs, $qml
Recommends: qt6-wayland
Description: $summary
 $description
EOF
        dpkg-deb --root-owner-group --build "$deb" "$out/opensesh_${version}_amd64.deb"
        ;;
    fedora)
        top="$work/target/package/rpm"
        mkdir -p "$top"/{BUILD,RPMS,SPECS,SOURCES}
        cat > "$top/SPECS/opensesh.spec" <<EOF
Name:           opensesh
Version:        $version
Release:        1
Summary:        $summary
License:        GPL-3.0-or-later
URL:            https://github.com/caixax/opensesh
# Shared libraries are found by rpm's dependency generator; QML modules and plugins load at run time.
Requires:       qt6-qtdeclarative qt6-qtsvg
Recommends:     qt6-qtwayland
# The binary is built outside of rpmbuild: nothing to strip again, no debug package.
%global debug_package %{nil}
%global __strip /bin/true

%description
$description

%install
cp -a "$stage/." %{buildroot}/

%files
/usr/bin/opensesh-app
/usr/share/applications/cc.caixa.OpenSesh.desktop
/usr/share/icons/hicolor/scalable/apps/cc.caixa.OpenSesh.svg
%license /usr/share/licenses/opensesh/LICENSE
%doc /usr/share/doc/opensesh
EOF
        rpmbuild --define "_topdir $top" -bb "$top/SPECS/opensesh.spec"
        cp "$top"/RPMS/x86_64/opensesh-"$version"-1*.x86_64.rpm "$out/opensesh-${version}.x86_64.rpm"
        ;;
    arch)
        # Outside of the home directory: makepkg may run as another user (see below).
        pkg="$(mktemp -d /tmp/opensesh-pkg.XXXXXX)"
        chmod 755 "$pkg"
        tar -C "$stage" -czf "$pkg/root.tar.gz" .
        cat > "$pkg/PKGBUILD" <<EOF
pkgname=opensesh
pkgver=$version
pkgrel=1
pkgdesc="$summary"
arch=('x86_64')
url="https://github.com/caixax/opensesh"
license=('GPL-3.0-or-later')
depends=('qt6-base' 'qt6-declarative' 'qt6-svg' 'gcc-libs' 'glibc')
optdepends=('qt6-wayland: native Wayland windows')
options=('!strip' '!debug')
source=('root.tar.gz')
sha256sums=('SKIP')
noextract=('root.tar.gz')
package() {
    tar -C "\$pkgdir" -xzf "\$srcdir/root.tar.gz"
}
EOF
        # makepkg refuses to run as root, which is what a WSL Arch is.
        if [ "$(id -u)" -eq 0 ]; then
            id builder >/dev/null 2>&1 || useradd -m builder
            chown -R builder "$pkg"
            su builder -c "cd '$pkg' && PKGDEST='$pkg' makepkg -f --nodeps"
        else
            (cd "$pkg" && PKGDEST="$pkg" makepkg -f --nodeps)
        fi
        cp "$pkg"/opensesh-"$version"-1-x86_64.pkg.tar.zst "$out/opensesh-${version}-x86_64.pkg.tar.zst"
        rm -rf "$pkg"
        ;;
    *)
        echo "no package format for ${PRETTY_NAME:-this distro}" >&2
        exit 1
        ;;
esac

echo "== packages in $out"
ls -l "$out" | grep -i "opensesh" || true
