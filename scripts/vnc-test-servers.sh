#!/usr/bin/env bash
# The VNC servers the real-server test of opensesh-vnc connects to (Sprint 14): TigerVNC, x11vnc
# and wayvnc, each in its usual setup, on 127.0.0.1 only. Debian and Ubuntu (CI runs it on Ubuntu
# 24.04); it installs packages, so it is meant for throwaway machines. Run with sudo: the servers
# run as the user who called sudo.
#
#   sudo scripts/vnc-test-servers.sh start   # installs the packages, starts the servers
#   sudo scripts/vnc-test-servers.sh logs    # their logs
#   sudo scripts/vnc-test-servers.sh stop    # stops them
#
#   5901  TigerVNC (Xvnc :1), VNC authentication
#   5902  TigerVNC (Xvnc :2), VeNCrypt X509Vnc with a certificate made here
#   5903  x11vnc on Xvfb :3, VNC authentication
#   5904  wayvnc on a headless sway, VeNCrypt with a user name and password (X509Plain)
#
# The X displays run an xterm, and the Wayland one a foot terminal, so there is something to see.
# Then, as the user that runs the tests:
#
#   OPENSESH_VNC_SERVERS=$STATE cargo test -p opensesh-vnc --test real_servers -- --ignored
set -euo pipefail

STATE=${OPENSESH_VNC_SERVERS:-/tmp/opensesh-vnc-servers}
# Test-only secrets, known to the test (crates/opensesh-vnc/tests/real_servers.rs). VNC
# authentication uses 8 characters at most.
PASSWORD=osvnc-ci
USER_NAME=opensesh

die() {
    echo "vnc-test-servers: $*" >&2
    exit 1
}

as_user() {
    sudo -u "${SUDO_USER:?run with sudo}" -H env "$@"
}

wait_port() {
    for _ in $(seq 1 30); do
        if (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null; then
            return 0
        fi
        sleep 1
    done
    die "nothing listens on port $1 (see: $0 logs)"
}

start() {
    [ "$(id -u)" -eq 0 ] || die "run with sudo"
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -q
    apt-get install -y -q tigervnc-standalone-server tigervnc-tools x11vnc xvfb xterm xclip \
        wayvnc sway foot openssl >/dev/null
    rm -rf "$STATE"
    mkdir -p "$STATE"
    chown "$SUDO_USER:" "$STATE"
    as_user bash -s "$STATE" "$PASSWORD" "$USER_NAME" <<'EOF'
set -euo pipefail
STATE=$1 PASSWORD=$2 USER_NAME=$3
cd "$STATE"
printf '%s\n' "$PASSWORD" | vncpasswd -f > passwd
chmod 600 passwd
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 2 \
    -keyout key.pem -out cert.pem -subj "/CN=opensesh-vnc-ci" 2>/dev/null

# TigerVNC: VNC authentication, then VeNCrypt with the certificate.
nohup Xvnc :1 -rfbport 5901 -interface 127.0.0.1 -SecurityTypes VncAuth -PasswordFile passwd \
    -geometry 1024x768 -depth 24 > xvnc1.log 2>&1 &
nohup Xvnc :2 -rfbport 5902 -interface 127.0.0.1 -SecurityTypes X509Vnc -PasswordFile passwd \
    -X509Cert cert.pem -X509Key key.pem -geometry 1024x768 -depth 24 > xvnc2.log 2>&1 &
# x11vnc on Xvfb.
nohup Xvfb :3 -screen 0 1024x768x24 > xvfb3.log 2>&1 &
sleep 2
for display in 1 2 3; do
    DISPLAY=:$display nohup xterm -geometry 80x24+20+20 > /dev/null 2>&1 &
done
nohup x11vnc -display :3 -rfbport 5903 -localhost -passwd "$PASSWORD" -forever -shared \
    -noxdamage > x11vnc.log 2>&1 &

# wayvnc on a headless sway, with a foot terminal.
mkdir -p runtime
chmod 700 runtime
cat > sway.config <<SWAY
xwayland disable
output HEADLESS-1 resolution 1024x768
exec foot
SWAY
cat > wayvnc.config <<WAYVNC
address=127.0.0.1
port=5904
enable_auth=true
username=$USER_NAME
password=$PASSWORD
certificate_file=$STATE/cert.pem
private_key_file=$STATE/key.pem
WAYVNC
export XDG_RUNTIME_DIR=$STATE/runtime WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
    WLR_RENDERER=pixman
nohup sway -c sway.config > sway.log 2>&1 &
for _ in $(seq 1 30); do
    socket=$(ls "$XDG_RUNTIME_DIR"/wayland-* 2>/dev/null | grep -v lock | head -1 || true)
    [ -n "$socket" ] && break
    sleep 1
done
WAYLAND_DISPLAY=$(basename "${socket:?no Wayland socket from sway}") nohup wayvnc -C wayvnc.config \
    > wayvnc.log 2>&1 &
EOF
    for port in 5901 5902 5903 5904; do
        wait_port "$port"
    done
    echo "vnc-test-servers: listening on 127.0.0.1:5901-5904 ($STATE)"
}

logs() {
    tail -n 40 "$STATE"/*.log 2>/dev/null || true
}

stop() {
    pkill -f "Xvnc :1" || true
    pkill -f "Xvnc :2" || true
    pkill -x x11vnc || true
    pkill -f "Xvfb :3" || true
    pkill -x wayvnc || true
    pkill -x sway || true
}

case "${1:-}" in
    start) start ;;
    logs) logs ;;
    stop) stop ;;
    *) die "usage: $0 start|logs|stop" ;;
esac
