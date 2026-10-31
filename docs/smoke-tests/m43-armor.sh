#!/usr/bin/env bash
# M43 gate — a worn armor piece gives a Skyrim armor rating and physical damage reduction, live.
#
#   1. Boot the Skyrim SE interior `HaltedStreamCamp01` with a player; `armor.status` reads the starting gear
#      and its reduction must equal `rating x 0.12 + 3 x pieces` (UESP Skyrim:Armor), capped at 80 %.
#   2. Give the player an Iron Armor cuirass (0x00012E49) and equip it.
#   3. `armor.status` must change, stay positive and still follow the formula.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19886}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
CELL="${BYRO_SMOKE_ARMOR_CELL:-HaltedStreamCamp01}"
CUIRASS="${BYRO_SMOKE_CUIRASS:-0x00012E49}"
BASE_RATING="${BYRO_SMOKE_CUIRASS_RATING:-25}"
LOG_DIR="$(mktemp -d /tmp/byro-m43-armor.XXXXXX)"
engine_pid=""

stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
        engine_pid=""
    fi
}
trap stop_engine EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

fail() {
    echo "smoke[m43-armor]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — SKIP != PASS: a runner without the game's data measures nothing.
DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m43-armor]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done

if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

cargo build --release --offline --quiet -p byroredux -p byro-dbg \
    || fail "release build failed"

wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Eq "$pattern" "$file"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.5
    done
}

debug_command() {
    timeout 60 env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg >"$2" 2>&1 <<EOF2
$1
.quit
EOF2
    sed -i 's/\\n/\n/g' "$2"
}

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn" \
    target/release/byroredux \
    --game skyrim_se --cell "$CELL" \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!

wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
sleep 5

parse_status() {  # <log> -> sets rating, pieces, reduction
    local line
    line="$(grep -E 'armor rating' "$1" | head -1 || true)"
    [[ -n "$line" ]] || fail "no status line in $1: $(sed -n '1,6p' "$1")"
    rating="$(sed -E 's/.*armor rating ([0-9]+);.*/\1/' <<<"$line")"
    pieces="$(sed -E 's/.*pieces ([0-9]+);.*/\1/' <<<"$line")"
    reduction="$(sed -E 's/.*reduction ([0-9.]+)%.*/\1/' <<<"$line")"
    status_line="$line"
}

# UESP: reduction % = displayed rating x 0.12 + 3 x pieces worn, capped at 80.
check_formula() {
    python3 -I - "$rating" "$pieces" "$reduction" <<'PY' || fail "the numbers do not follow the formula: $status_line"
import sys
rating, pieces, reduction = float(sys.argv[1]), int(sys.argv[2]), float(sys.argv[3])
assert abs(reduction - min(80.0, rating * 0.12 + 3.0 * pieces)) < 0.01, (rating, pieces, reduction)
PY
}

debug_command "armor.status" "$LOG_DIR/before.log" || fail "armor.status unavailable"
parse_status "$LOG_DIR/before.log"
check_formula
before_rating="$rating"; before_status="$status_line"

debug_command "inv.add $CUIRASS 1" "$LOG_DIR/add.log" || fail "inv.add unavailable"
debug_command "inv.equip $CUIRASS" "$LOG_DIR/equip.log" || fail "inv.equip unavailable"
sleep 2

debug_command "armor.status" "$LOG_DIR/after.log" || fail "armor.status unavailable"
parse_status "$LOG_DIR/after.log"
check_formula
[[ "$rating" != "$before_rating" ]] || fail "equipping the cuirass changed nothing: $before_status -> $status_line"
[[ "$rating" -gt 0 ]] || fail "no armor rating after equipping: $status_line"

if grep -Fq 'panicked' "$LOG_DIR/session.stderr"; then
    fail "the engine panicked: $(grep -F 'panicked' "$LOG_DIR/session.stderr" | head -1)"
fi

stop_engine
echo "smoke[m43-armor]: PASS -- $before_status  ->  $status_line"
echo "artifacts: $LOG_DIR"
