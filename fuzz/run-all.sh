#!/bin/sh
# Runs every fuzz target for SECONDS_EACH seconds (60 by default) after seed.sh, and says which
# ones failed. CI runs each one for an hour (.github/workflows/fuzz.yml).
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
cd "$(dirname "$0")"
[ -d corpus ] || sh seed.sh
mkdir -p target
for t in quick_connect paste themes monitor ssh_config mobaxterm putty remmina csv bundle sync_merge; do
    if cargo +nightly fuzz run "$t" -- -max_total_time=${SECONDS_EACH:-60} -rss_limit_mb=2048 -timeout=10 > "target/fuzz-$t.log" 2>&1; then
        echo "$t: ok $(grep -o 'Done [0-9]* runs' target/fuzz-$t.log | tail -1)"
    else
        echo "$t: FAILED"
        grep -E "panicked|ERROR|SUMMARY|Test unit written|timeout|out-of-memory" "target/fuzz-$t.log" | head -8
    fi
done
