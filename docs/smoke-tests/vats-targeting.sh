#!/usr/bin/env bash
# vats-targeting.sh — VATS first live gate (#6350, Phases 1–3).
#
# Drives the full targeting loop on FNV through the debug console:
# equip the 10mm, enter VATS (world freezes), price the settler's limb
# table, queue a torso shot, confirm (AP charged, hit rolled through
# the combat funnel), leave.
#
# ⚠ PLUMBING SCOPE — what a green run here does and does NOT mean:
# this gate covers the DATA→MODEL→SCAN→QUEUE→EXECUTION plumbing, not
# limb fidelity. It does not verify per-limb damage routing (BPTD part
# damage multipliers → limb-condition actor values — Phase 4), playback
# cinematics (CAMS), the projected-area screen-percentage integration
# (the scan samples a three-ray occlusion fan), or the in-game VATS
# overlay. A green run with part selection entirely ignored would still
# land a hit — the per-part damage assertion lands with Phase 4.
#
# SKIP≠PASS (#5427): no game data on disk measures nothing — exit 77.

set -u
PORT="${BYROREDUX_VATS_PORT:-9979}"
LOG_DIR="$(mktemp -d /tmp/byro-vats-XXXXXX)"
ENGINE_PID=""
TIMEOUT=90

stop_engine() {
    [ -n "$ENGINE_PID" ] && kill "$ENGINE_PID" 2>/dev/null
    wait "$ENGINE_PID" 2>/dev/null
}
trap stop_engine EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

fail() {
    echo "smoke[vats-targeting]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
for f in "FalloutNV.esm" "Fallout - Meshes.bsa" "Fallout - Textures.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[vats-targeting]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done

if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

cargo build --release --offline --quiet -p byroredux -p byro-dbg \
    || fail "release build failed"

debug_command() {
    timeout "$TIMEOUT" env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg \
        >"$2" 2>&1 <<EOF
$1
.quit
EOF
}

wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Fq "$pattern" "$file"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.25
    done
}

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    RUST_LOG="warn" \
    target/release/byroredux \
    --game fnv --cell GSProspectorSaloonInterior \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/engine.log" 2>&1 &
engine_pid=$!

wait_log "$LOG_DIR/engine.log" "bench-hold:"

# 1 — Baseline: no mode.
debug_command "vats.status" "$LOG_DIR/00-baseline.out"
grep -Fq "mode=none" "$LOG_DIR/00-baseline.out" || fail "baseline is not mode=none"

# 2 — Give + equip the 10mm pistol (FNV 0x0000434F, wiki-pinned AP 17).
debug_command "inv.add 434F 1
inv.equip 434F" "$LOG_DIR/01-equip.out"
grep -Fq "queued equip toggle" "$LOG_DIR/01-equip.out" || fail "inv.equip queued nothing"

# 3 — Enter: the world freezes and the limb tables scan. The spawn
# camera may sit outside the interior (0 % is then the honest price),
# so aim it at a scanned target and re-scan before asserting chances.
debug_command "vats.enter
vats.status" "$LOG_DIR/02-enter.out"
grep -Fq "mode=target-select" "$LOG_DIR/02-enter.out" || fail "vats.enter did not reach target-select"
grep -Fq "world_time_scale=0.00" "$LOG_DIR/02-enter.out" || fail "VATS did not publish the freeze scale"
TARGET_ENTITY=$(grep -oE "entity=[0-9]+" "$LOG_DIR/02-enter.out" | head -1 | cut -d= -f2)
[ -n "$TARGET_ENTITY" ] || fail "scan listed no target entities"
debug_command "cam.tp $TARGET_ENTITY
vats.scan
vats.status" "$LOG_DIR/02b-rescan.out"
grep -Fq "world_time_scale=0.00" "$LOG_DIR/02b-rescan.out" || fail "re-scan lost the freeze"
grep -Eq "part=(Head|Torso|Body) hit=[1-9][0-9]*%" "$LOG_DIR/02b-rescan.out" \
    || fail "no live-priced limb table after aiming the camera (all parts 0 %?)"
grep -Fq "ap=17" "$LOG_DIR/02b-rescan.out" \
    || fail "parts not priced at the 10mm's authored AP (17)"

# 4 — Queue the torso and confirm: AP charged, attack executed.
debug_command "vats.queue Torso
vats.confirm" "$LOG_DIR/03-confirm.out"
grep -Fq "queued Torso" "$LOG_DIR/03-confirm.out" || fail "vats.queue Torso did not queue"
grep -Fq "executed 1 queued attack(s)" "$LOG_DIR/03-confirm.out" || fail "vats.confirm did not execute"

# 5 — Back to the running world.
debug_command "vats.status" "$LOG_DIR/04-after.out"
grep -Fq "mode=none" "$LOG_DIR/04-after.out" || fail "mode did not return to none after confirm"
grep -Fq "world_time_scale=1.00" "$LOG_DIR/04-after.out" || fail "world time scale did not restore"

echo "smoke[vats-targeting]: PASS"
exit 0
