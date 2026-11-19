#!/usr/bin/env bash
# Shared plumbing of the M42 live package gates that boot one cell, move the clock, and read the
# ECS through `byro-dbg`: m42-oblivion-packages.sh and m42-skyrim-ambient.sh (PRD
# `ai-packages-completion` §7). `m42-package-procedures.sh` and `m42-eat-sleep.sh` predate it and
# keep their own copies.
#
# The caller sets GATE (the diagnostics tag), PROFILE (the engine `--game` value) and DATA_ENV /
# DATA_DEFAULT / DATA_FILES (a space in a file name is written `@`; the SKIP≠PASS contract, #5427), then sources this file and calls
# `m42_live_init`. After that: `boot_cell <cell> <tag>`, `count_entities <component> <tag>`,
# `entity_list <component> <tag>`, `debug_command <cmd> <outfile>`, `wait_until_count
# <component> <min> <what>`, `fail <msg>`, `stop_engine`.

set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19887}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d "/tmp/byro-${GATE}.XXXXXX")"
engine_pid=""
M42_RUST_LOG="${M42_RUST_LOG:-warn,byroredux::systems=info,byroredux::npc_spawn=info,byroredux::asset_provider=info}"

stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
        engine_pid=""
    fi
}

fail() {
    echo "smoke[$GATE]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

m42_live_init() {
    trap stop_engine EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM

    # The SKIP≠PASS contract (#5427): no game data measures nothing.
    DATA="${!DATA_ENV:-$DATA_DEFAULT}"
    local f
    for f in $DATA_FILES; do
        if [ ! -f "$DATA/${f//@/ }" ]; then
            echo "smoke[$GATE]: SKIP -- missing $DATA/${f//@/ }"
            exit 77
        fi
    done

    if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
        fail "port $PORT already occupied; refusing to attach to another session"
    fi
    cargo build --release --offline --quiet -p byroredux -p byro-dbg || fail "release build failed"
}

debug_command() {
    timeout "$TIMEOUT" env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg \
        >"$2" 2>&1 <<EOC
$1
.quit
EOC
}

wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Fq "$pattern" "$file"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.25
    done
}

# Boot `<cell>` held open for `byro-dbg`. The engine runs from the checkout: `--game` finds its profile
# under `assets/`, relative to the working directory.
boot_cell() {
    local cell="$1" tag="$2"
    env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
        BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings-$tag.toml" RUST_LOG="$M42_RUST_LOG" \
        target/release/byroredux --game "$PROFILE" --cell "$cell" \
        --player --bench-frames 30 --bench-hold \
        >"$LOG_DIR/$tag.stdout" 2>"$LOG_DIR/$tag.stderr" &
    engine_pid=$!
    wait_log "$LOG_DIR/$tag.stderr" 'bench-hold:'
    sleep 4
}

# Capture the frame as `<name>` in the log directory (the debug server writes under `screenshots/`).
capture() {
    debug_command "screenshot $1" "$LOG_DIR/capture-$1.log" || return 0
    sleep 1
    mv "screenshots/$1" "$LOG_DIR/$1" 2>/dev/null || true
}

# Prints the number of `Entity N` rows `entities(<component>)` lists.
count_entities() {
    debug_command "entities($1)" "$LOG_DIR/$2.log" >/dev/null 2>&1 || true
    grep -c 'Entity [0-9]' "$LOG_DIR/$2.log" || true
}

# Polls `entities(<component>)` until it lists at least `<min>` rows; prints the count.
wait_until_count() {
    local component="$1" min="$2" what="$3" deadline=$((SECONDS + TIMEOUT)) n=0
    while (( SECONDS < deadline )); do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $what"
        n="$(count_entities "$component" "wait-$component")"
        (( n >= min )) && { echo "$n"; return 0; }
        sleep 2
    done
    fail "timeout: $what (last count $n of $min)"
}
