#!/usr/bin/env bash
# The S3 server the real-server S3 test of opensesh-ssh uploads to (Sprint 12): RustFS in a
# container (Docker, else Podman), on 127.0.0.1:9000 only, with test keys and its data in
# $OPENSESH_S3_SERVER (default /tmp/opensesh-s3-server).
#
#   scripts/s3-test-server.sh start   # pulls the pinned image and starts the server
#   scripts/s3-test-server.sh stop    # stops it and deletes its data
#
# Then:
#
#   cargo test -p opensesh-ssh --test real_s3 -- --ignored
#
# The image is pinned (RustFS 1.0.0). It runs as UID 10001, which must own the data folder; with
# Docker that takes root (CI uses sudo), rootless Podman maps it by itself.
set -euo pipefail

STATE=${OPENSESH_S3_SERVER:-/tmp/opensesh-s3-server}
IMAGE=docker.io/rustfs/rustfs:1.0.0
NAME=opensesh-rustfs
# Test-only keys, known to the test (crates/opensesh-ssh/tests/real_s3.rs).
ACCESS_KEY=opensesh-test
SECRET_KEY=opensesh-test-secret-key

die() {
    echo "s3-test-server: $*" >&2
    exit 1
}

engine() {
    if command -v docker >/dev/null 2>&1; then
        echo docker
    elif command -v podman >/dev/null 2>&1; then
        echo podman
    else
        die "neither docker nor podman is installed"
    fi
}

start() {
    local run
    run=$(engine)
    mkdir -p "$STATE/data"
    if [ "$run" = docker ]; then
        chown -R 10001:10001 "$STATE/data" || die "the data folder must belong to UID 10001: run as root"
    else
        podman unshare chown -R 10001:10001 "$STATE/data" 2>/dev/null || chown -R 10001:10001 "$STATE/data"
    fi
    "$run" rm -f "$NAME" >/dev/null 2>&1 || true
    "$run" run -d --name "$NAME" \
        -p 127.0.0.1:9000:9000 \
        -v "$STATE/data:/data" \
        -e "RUSTFS_ACCESS_KEY=$ACCESS_KEY" \
        -e "RUSTFS_SECRET_KEY=$SECRET_KEY" \
        "$IMAGE" /data >/dev/null
    # Up when the S3 port answers (any HTTP status).
    for _ in $(seq 1 60); do
        if curl -s -o /dev/null http://127.0.0.1:9000/; then
            echo "s3-test-server: RustFS is listening on 127.0.0.1:9000 ($run)"
            return 0
        fi
        sleep 1
    done
    "$run" logs "$NAME" >&2 || true
    die "RustFS didn't start"
}

stop() {
    local run
    run=$(engine)
    "$run" rm -f "$NAME" >/dev/null 2>&1 || true
    if [ "$run" = podman ]; then
        podman unshare rm -rf "$STATE" 2>/dev/null || rm -rf "$STATE"
    else
        rm -rf "$STATE"
    fi
}

logs() {
    "$(engine)" logs "$NAME"
}

case "${1:-}" in
    start) start ;;
    stop) stop ;;
    logs) logs ;;
    *) die "usage: $0 start|stop|logs" ;;
esac
