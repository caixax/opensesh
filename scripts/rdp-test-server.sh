#!/usr/bin/env bash
# The RDP server the helper's real-server test connects to (Sprint 13): xrdp with its Xorg
# session on 127.0.0.1:3389, a test account, and an X session that copies a known text to the
# clipboard once it is up, then runs xterm. Debian and Ubuntu only (CI runs it on Ubuntu 24.04);
# it installs packages and adds a user, so it is meant for throwaway machines.
#
#   sudo scripts/rdp-test-server.sh start   # installs xrdp, adds the user, starts the server
#   sudo scripts/rdp-test-server.sh logs    # xrdp's and the session's logs
#   sudo scripts/rdp-test-server.sh stop    # stops the server
#
# Then:
#
#   OPENSESH_TEST_XRDP=127.0.0.1:3389 OPENSESH_TEST_XRDP_USER=opensesh-rdp \
#   OPENSESH_TEST_XRDP_PASSWORD=opensesh-rdp-test OPENSESH_TEST_XRDP_CLIPBOARD="OpenSesh xrdp clipboard" \
#   cargo test --manifest-path rdp/Cargo.toml --test xrdp
set -euo pipefail

# Test-only account, known to the CI job.
USER_NAME=opensesh-rdp
PASSWORD=opensesh-rdp-test
CLIPBOARD="OpenSesh xrdp clipboard"

die() {
    echo "rdp-test-server: $*" >&2
    exit 1
}

start() {
    [ "$(id -u)" -eq 0 ] || die "run as root"
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -q
    apt-get install -y -q xrdp xorgxrdp xterm xclip dbus-x11 >/dev/null
    if ! id "$USER_NAME" >/dev/null 2>&1; then
        useradd -m -s /bin/bash "$USER_NAME"
    fi
    echo "$USER_NAME:$PASSWORD" | chpasswd
    # The session: the clipboard text once xrdp's clipboard helper runs, then a terminal.
    local home
    home=$(getent passwd "$USER_NAME" | cut -d: -f6)
    cat >"$home/.xsession" <<EOF
#!/bin/sh
( sleep 3; printf '%s' '$CLIPBOARD' | xclip -selection clipboard ) &
exec xterm
EOF
    chmod 755 "$home/.xsession"
    chown "$USER_NAME:" "$home/.xsession"
    # xrdp reads its TLS key through the ssl-cert group.
    adduser xrdp ssl-cert >/dev/null 2>&1 || true
    systemctl restart xrdp-sesman xrdp
    for _ in $(seq 1 30); do
        if (exec 3<>/dev/tcp/127.0.0.1/3389) 2>/dev/null; then
            echo "rdp-test-server: xrdp is listening on 127.0.0.1:3389"
            return 0
        fi
        sleep 1
    done
    die "xrdp didn't start listening"
}

logs() {
    journalctl -u xrdp -u xrdp-sesman --no-pager -n 80 || true
    tail -n 60 /var/log/xrdp.log /var/log/xrdp-sesman.log 2>/dev/null || true
    local home
    home=$(getent passwd "$USER_NAME" | cut -d: -f6 || true)
    if [ -n "$home" ]; then
        tail -n 40 "$home"/.xsession-errors "$home"/.local/share/xorg/*.log 2>/dev/null || true
    fi
}

stop() {
    systemctl stop xrdp xrdp-sesman || true
}

case "${1:-}" in
    start) start ;;
    logs) logs ;;
    stop) stop ;;
    *) die "usage: $0 start|logs|stop" ;;
esac
