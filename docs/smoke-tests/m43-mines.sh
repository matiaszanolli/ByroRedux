#!/usr/bin/env bash
# M43 gate — a placed proximity mine trips, burns its fuse and blasts, live (Fallout 4).
#
#   1. Boot the FO4 interior `AndrewStation01` (nine frag mines in Fallout4.esm) with a player.
#   2. `mine.status` lists the placed mines: the PGRE placements of mine projectiles
#      (flags 0x66, Alt. Trigger, proximity 100, timer 1.5 — read from the PROJ DNAM).
#   3. `cam.pos` carries the player body onto one mine; the engine logs `mine ... tripped`
#      then `mine ... detonated` after the fuse, and `mine.status` reads the mine `spent`.
#
# Attribution is airtight: nothing but the mine system writes those log lines.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19881}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
CELL="${BYRO_SMOKE_MINE_CELL:-AndrewStation01}"
LOG_DIR="$(mktemp -d /tmp/byro-m43-mines.XXXXXX)"
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
    echo "smoke[m43-mines]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — SKIP != PASS: a runner without the game's data measures nothing.
DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"
for f in "Fallout4.esm" "Fallout4 - Meshes.ba2" "Fallout4 - Textures1.ba2"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m43-mines]: SKIP -- missing $DATA/$f"
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
    # byro-dbg prints a multi-line reply as one quoted string with literal \n: split it back into lines.
    sed -i 's/\\n/\n/g' "$2"
}

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn,byroredux::systems::mines=info" \
    target/release/byroredux \
    --esm "$DATA/Fallout4.esm" \
    --bsa "$DATA/Fallout4 - Meshes.ba2" --bsa "$DATA/Fallout4 - MeshesExtra.ba2" \
    --textures-bsa "$DATA/Fallout4 - Textures1.ba2" \
    --materials-ba2 "$DATA/Fallout4 - Materials.ba2" \
    --cell "$CELL" --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!

wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
sleep 5

debug_command "mine.status" "$LOG_DIR/before.log" || fail "mine.status unavailable"
grep -Eq 'placed mines: [1-9]' "$LOG_DIR/before.log" || fail "no placed mine is tracked: $(sed -n '1,6p' "$LOG_DIR/before.log")"
line="$(grep -E '^mine .* waiting' "$LOG_DIR/before.log" | head -1 || true)"
[[ -n "$line" ]] || fail "no waiting mine in: $(sed -n '1,8p' "$LOG_DIR/before.log")"
coords="$(sed -E 's/.* at \(([-0-9.]+), ([-0-9.]+), ([-0-9.]+)\).*/\1 \2 \3/' <<<"$line")"
read -r mx my mz <<<"$coords"

debug_command "cam.pos $mx $my $mz" "$LOG_DIR/move.log" || fail "cam.pos unavailable"
wait_log "$LOG_DIR/session.stderr" 'mine .* detonated'
grep -Eq 'mine .* tripped by' "$LOG_DIR/session.stderr" || fail "the mine detonated without tripping first"

debug_command "mine.status" "$LOG_DIR/after.log" || fail "mine.status unavailable"
grep -Eq '^mine .* spent' "$LOG_DIR/after.log" || fail "no mine reads spent: $(sed -n '1,8p' "$LOG_DIR/after.log")"

if grep -Fq 'panicked' "$LOG_DIR/session.stderr"; then
    fail "the engine panicked: $(grep -F 'panicked' "$LOG_DIR/session.stderr" | head -1)"
fi

stop_engine
echo "smoke[m43-mines]: PASS -- a placed mine tripped on the player, burned its fuse,"
echo "detonated, and reads spent."
echo "artifacts: $LOG_DIR"
