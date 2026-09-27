#!/usr/bin/env bash
# The SSH servers the real-server tests of opensesh-ssh connect to (Sprint 7). Run as root (CI
# uses sudo; the archlinux WSL distro is root already):
#
#   scripts/ssh-test-servers.sh start   # installs the packages, starts the servers
#   scripts/ssh-test-servers.sh stop    # stops them and undoes every change
#
# Everything listens on 127.0.0.1 only, as the user `opensesh-test`:
#
#   2221  OpenSSH, public key or a user certificate (the first jump host)
#   2222  Dropbear, public key or password (the second jump host)
#   2223  OpenSSH, public key and then a one-time code (TOTP through PAM)
#   2224  Dropbear, password
#   2225  OpenSSH, public key, without the SFTP subsystem (spikes/scp-fallback, the "no SFTP" error)
#
# The servers run from $OPENSESH_SSH_SERVERS (default /tmp/opensesh-ssh-servers) with their own
# host keys and configuration; the system's sshd configuration is not touched. Two system changes
# are undone by `stop`: the user `opensesh-test`, and a marked block at the top of
# /etc/pam.d/sshd that asks that user (and only that user) for the one-time code.
#
# Then, as the user that runs the tests:
#
#   eval "$(ssh-agent -s)" && ssh-add "$OPENSESH_SSH_SERVERS/client_ed25519"
#   OPENSESH_SSH_SERVERS=... cargo test -p opensesh-ssh --test real_servers -- --ignored
set -euo pipefail

STATE=${OPENSESH_SSH_SERVERS:-/tmp/opensesh-ssh-servers}
TEST_USER=opensesh-test
# Test-only secrets, known to the tests (crates/opensesh-ssh/tests/real_servers.rs).
PASSWORD='opensesh test password'
TOTP_SECRET=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ
PAM_FILE=/etc/pam.d/sshd
MARK_BEGIN='# opensesh-test begin (scripts/ssh-test-servers.sh; removed by its stop)'
MARK_END='# opensesh-test end'
export PATH="$PATH:/usr/sbin:/sbin"

die() {
    echo "ssh-test-servers: $*" >&2
    exit 1
}

install_packages() {
    if command -v apt-get >/dev/null; then
        DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
            openssh-server openssh-client dropbear-bin libpam-google-authenticator oathtool >/dev/null
    elif command -v pacman >/dev/null; then
        pacman -S --noconfirm --needed openssh dropbear libpam-google-authenticator oath-toolkit >/dev/null
    elif command -v dnf >/dev/null; then
        dnf install -y -q openssh-server openssh-clients dropbear google-authenticator oathtool >/dev/null
    else
        die "no apt-get, pacman or dnf to install OpenSSH, Dropbear and the PAM TOTP module"
    fi
}

# The PAM block: the test user answers a one-time code, and nothing else; others are unchanged.
pam_block() {
    printf '%s\n' "$MARK_BEGIN" \
        "auth [success=1 default=ignore] pam_succeed_if.so quiet user != $TEST_USER" \
        "auth [success=done default=die] pam_google_authenticator.so" \
        "$MARK_END"
}

remove_pam_block() {
    [ -f "$PAM_FILE" ] || return 0
    if grep -qF "$MARK_BEGIN" "$PAM_FILE"; then
        sed -i "/^# opensesh-test begin/,/^# opensesh-test end/d" "$PAM_FILE"
    fi
}

stop() {
    local pid
    for pid in "$STATE"/*.pid; do
        [ -f "$pid" ] && kill "$(cat "$pid")" 2>/dev/null || true
    done
    remove_pam_block
    if id "$TEST_USER" >/dev/null 2>&1; then
        # Its sessions, and a user manager that PAM may have started.
        pkill -u "$TEST_USER" 2>/dev/null || true
        sleep 1
        pkill -9 -u "$TEST_USER" 2>/dev/null || true
        sleep 1
        userdel -r "$TEST_USER" 2>/dev/null || userdel -f -r "$TEST_USER" 2>/dev/null || true
    fi
    rm -rf "$STATE"
    echo "ssh-test-servers: stopped"
}

start() {
    [ "$(id -u)" -eq 0 ] || die "run as root (sudo)"
    stop >/dev/null
    install_packages
    local sshd dropbear owner home
    sshd=$(command -v sshd) || die "sshd not found"
    dropbear=$(command -v dropbear) || die "dropbear not found"
    owner=${SUDO_USER:-root}

    mkdir -p "$STATE"
    chmod 755 "$STATE"

    useradd -m -s /bin/sh "$TEST_USER"
    echo "$TEST_USER:$PASSWORD" | chpasswd
    home=$(getent passwd "$TEST_USER" | cut -d: -f6)

    # The tests' key, and the user's authorized_keys and TOTP secret.
    ssh-keygen -q -t ed25519 -N '' -C "opensesh-test-client" -f "$STATE/client_ed25519"
    chown "$owner" "$STATE/client_ed25519" "$STATE/client_ed25519.pub"
    # Another key, not in authorized_keys, with a certificate from a user CA that 2221 trusts.
    ssh-keygen -q -t ed25519 -N '' -C "opensesh-test-ca" -f "$STATE/user_ca"
    ssh-keygen -q -t ed25519 -N '' -C "opensesh-test-certified" -f "$STATE/client_cert_ed25519"
    ssh-keygen -q -s "$STATE/user_ca" -I opensesh-test -n "$TEST_USER" -V -5m:+1d         "$STATE/client_cert_ed25519.pub"
    chown "$owner" "$STATE"/client_cert_ed25519*
    install -d -m 700 -o "$TEST_USER" "$home/.ssh"
    install -m 600 -o "$TEST_USER" "$STATE/client_ed25519.pub" "$home/.ssh/authorized_keys"
    printf '%s\n' "$TOTP_SECRET" '" WINDOW_SIZE 3' '" TOTP_AUTH' >"$home/.google_authenticator"
    chown "$TEST_USER" "$home/.google_authenticator"
    chmod 400 "$home/.google_authenticator"

    [ -f "$PAM_FILE" ] || die "$PAM_FILE is missing"
    { pam_block; cat "$PAM_FILE"; } >"$STATE/pam.sshd"
    cat "$STATE/pam.sshd" >"$PAM_FILE"
    rm -f "$STATE/pam.sshd"

    # Host keys and configuration of our own.
    ssh-keygen -q -t ed25519 -N '' -f "$STATE/ssh_host_ed25519_key"
    dropbearkey -t ed25519 -f "$STATE/dropbear_ed25519_key" >/dev/null 2>&1
    mkdir -p /run/sshd /var/empty
    local port methods ca
    local subsystem
    for port in 2221 2223 2225; do
        methods=publickey
        [ "$port" = 2223 ] && methods=publickey,keyboard-interactive
        ca=none
        [ "$port" = 2221 ] && ca="$STATE/user_ca.pub"
        subsystem="Subsystem sftp internal-sftp"
        [ "$port" = 2225 ] && subsystem="# No SFTP subsystem: scp and shell commands only."
        cat >"$STATE/sshd_$port.conf" <<EOF
Port $port
ListenAddress 127.0.0.1
HostKey $STATE/ssh_host_ed25519_key
PidFile $STATE/sshd_$port.pid
AllowUsers $TEST_USER
AuthenticationMethods $methods
PubkeyAuthentication yes
TrustedUserCAKeys $ca
PasswordAuthentication no
KbdInteractiveAuthentication yes
UsePAM yes
AllowTcpForwarding yes
AllowAgentForwarding yes
AcceptEnv LANG LC_* OPENSESH_*
# For spikes/x11-forwarding (the server also needs xauth to store the cookie).
X11Forwarding yes
X11UseLocalhost yes
PrintMotd no
LogLevel VERBOSE
$subsystem
EOF
        "$sshd" -t -f "$STATE/sshd_$port.conf" || die "the configuration of port $port is wrong"
        "$sshd" -f "$STATE/sshd_$port.conf" -E "$STATE/sshd_$port.log"
    done
    for port in 2222 2224; do
        "$dropbear" -r "$STATE/dropbear_ed25519_key" -p "127.0.0.1:$port" -P "$STATE/dropbear_$port.pid" \
            2>>"$STATE/dropbear_$port.log"
    done
    sleep 1
    for port in 2221 2222 2223 2224 2225; do
        (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null || die "nothing listens on $port (see $STATE/*.log)"
    done
    echo "ssh-test-servers: listening on 127.0.0.1:2221-2225 (state in $STATE)"
}

case "${1:-}" in
    start) start ;;
    stop) stop ;;
    *) die "usage: $0 start|stop" ;;
esac
