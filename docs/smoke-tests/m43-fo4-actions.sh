#!/usr/bin/env bash
# M43 gate — Fallout 4's player-action events, live on the real master: HACK, LOCK and IRON.
#
#   1. Boot the FO4 interior `GNN01` with a player. `lock.status` lists the cell's locked references with how
#      the player could bypass each: the locked terminal reads `bypass hack`, the doors `none` (no pick yet).
#   2. `lock.open` on the terminal runs the activation path's own unlock: the engine logs `terminal hacked ...
#      raising HACK`.
#   3. Give the player a BobbyPin (0x0A): a lock of level <= 100 now reads `bypass pick`; `lock.open` logs
#      `lock picked ... raising LOCK`. A lock above Master (level > 100) still reads `none`.
#   4. Equip the 10mm pistol (0x4822) and hold the aim button: `iron sights: the player began aiming; raising
#      IRON`.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19887}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
CELL="${BYRO_SMOKE_FO4_CELL:-GNN01}"
PIN="${BYRO_SMOKE_PIN:-0x0000000A}"
PISTOL="${BYRO_SMOKE_PISTOL:-0x00004822}"
LOG_DIR="$(mktemp -d /tmp/byro-m43-fo4-actions.XXXXXX)"
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
    echo "smoke[m43-fo4-actions]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — SKIP != PASS: a runner without the game's data measures nothing.
DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"
for f in "Fallout4.esm" "Fallout4 - Meshes.ba2" "Fallout4 - Textures1.ba2"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m43-fo4-actions]: SKIP -- missing $DATA/$f"
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
    RUST_LOG="warn,byroredux::lockpick=info,byroredux::systems::iron_sights=info" \
    target/release/byroredux \
    --esm "$DATA/Fallout4.esm" \
    --bsa "$DATA/Fallout4 - Meshes.ba2" --bsa "$DATA/Fallout4 - MeshesExtra.ba2" \
    --textures-bsa "$DATA/Fallout4 - Textures1.ba2" \
    --materials-ba2 "$DATA/Fallout4 - Materials.ba2" \
    --cell "$CELL" --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!

wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
sleep 4

# 1-2. HACK: the locked terminal.
debug_command "lock.status" "$LOG_DIR/locks1.log" || fail "lock.status unavailable"
terminal="$(grep -E '^lock [0-9]+ .* bypass hack ' "$LOG_DIR/locks1.log" | head -1 | awk '{print $2}' || true)"
[[ -n "$terminal" ]] || fail "no hackable terminal is listed in $CELL: $(sed -n '1,12p' "$LOG_DIR/locks1.log")"
grep -Eq '^lock [0-9]+ .* level 50 .* bypass none ' "$LOG_DIR/locks1.log" \
    || fail "expected a level-50 lock that cannot be bypassed yet: $(sed -n '1,12p' "$LOG_DIR/locks1.log")"
debug_command "lock.open $terminal" "$LOG_DIR/hack.log" || fail "lock.open unavailable"
grep -Fq "opened" "$LOG_DIR/hack.log" || fail "the terminal stayed shut: $(sed -n '1,5p' "$LOG_DIR/hack.log")"
wait_log "$LOG_DIR/session.stderr" 'terminal hacked: .*raising HACK'

# 3. LOCK: a bobby pin turns the pickable locks pickable; one above Master stays shut.
debug_command "inv.add $PIN 5" "$LOG_DIR/pin.log" || fail "inv.add unavailable"
debug_command "lock.status" "$LOG_DIR/locks2.log" || fail "lock.status unavailable"
pickable="$(grep -E '^lock [0-9]+ .* bypass pick ' "$LOG_DIR/locks2.log" | head -1 | awk '{print $2}' || true)"
[[ -n "$pickable" ]] || fail "no lock reads bypass pick after giving a bobby pin: $(sed -n '1,12p' "$LOG_DIR/locks2.log")"
debug_command "lock.open $pickable" "$LOG_DIR/pick.log" || fail "lock.open unavailable"
grep -Fq "opened" "$LOG_DIR/pick.log" || fail "the pickable lock stayed shut: $(sed -n '1,5p' "$LOG_DIR/pick.log")"
wait_log "$LOG_DIR/session.stderr" 'lock picked: .*raising LOCK'

# 4. IRON: a gun in hand and the aim button held.
debug_command "inv.add $PISTOL 1" "$LOG_DIR/gun.log" || fail "inv.add unavailable"
debug_command "inv.equip $PISTOL" "$LOG_DIR/equip.log" || fail "inv.equip unavailable"
sleep 2
debug_command "input.hold block 60" "$LOG_DIR/aim.log" || fail "input.hold unavailable"
wait_log "$LOG_DIR/session.stderr" 'iron sights: the player began aiming'

if grep -Fq 'panicked' "$LOG_DIR/session.stderr"; then
    fail "the engine panicked: $(grep -F 'panicked' "$LOG_DIR/session.stderr" | head -1)"
fi

stop_engine
echo "smoke[m43-fo4-actions]: PASS -- HACK (terminal $terminal), LOCK (lock $pickable) and IRON were raised live."
echo "artifacts: $LOG_DIR"
