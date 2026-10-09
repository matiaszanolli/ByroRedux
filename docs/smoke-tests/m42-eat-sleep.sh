#!/usr/bin/env bash
# M42 gate — Sleep (procedure 4) through ambient selection, live.
#
#   1. Boot the FNV profile into the Prospector Saloon with a player.
#   2. Baseline: at the default boot hour the Goodsprings settler's
#      package is not a Sleep one (`GSSettler04SleepPackage22x8`
#      runs 22:00-08:00; his Flee/Ambush packs above it are
#      GetButtonPressed-gated and evaluate FALSE pre-menu) — he
#      carries no SleepBehavior.
#   3. `time.set 23` drops the clock inside the Sleep window — the
#      game-minute re-evaluation must hand him `SleepBehavior` (the
#      M42 dispatch), and the walk-then-seat runtime must eventually
#      seat him (`[m42] sleep` log; his PLDT bed reference lives in
#      an unloaded cell, so the documented fallback seats him at the
#      nearest sleep/sit marker the saloon offers; Eat shares the
#      same path, unit-gated in `eat_sleep`'s fixture tests).
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19880}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-m42.XXXXXX)"
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
    echo "smoke[m42-eat-sleep]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — the SKIP≠PASS contract every per-game gate honours: a runner
# without the game's data measures nothing (exit 77, never a pass),
# before any port or build work.
DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
for f in "FalloutNV.esm" "Fallout - Meshes.bsa" "Fallout - Textures.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m42-eat-sleep]: SKIP -- missing $DATA/$f"
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
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn,byroredux::systems::eat_sleep=info" \
    target/release/byroredux \
    --game fnv --cell GSProspectorSaloonInterior \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!
wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
sleep 2

# ── Baseline: the settler is awake at the boot hour ─────────────────
debug_command 'find("GSSettlerCM")' "$LOG_DIR/find.log" || fail 'find unavailable'
entity="$(grep -oE 'Entity [0-9]+' "$LOG_DIR/find.log" | head -1 | cut -d' ' -f2 || true)"
[[ -n "$entity" ]] || fail "GSSettlerCM entity not found in the loaded cell"
echo "smoke[m42-eat-sleep]: settler entity=$entity"

debug_command 'entities(SleepBehavior)' "$LOG_DIR/baseline.log" || true
if grep -Fq "Entity $entity" "$LOG_DIR/baseline.log"; then
    fail "the settler must not sleep at the boot hour (his Sleep window is 22:00-08:00): $(sed -n '1,4p' "$LOG_DIR/baseline.log")"
fi
echo "smoke[m42-eat-sleep]: baseline PASS — no SleepBehavior at the boot hour"

# ── The Sleep window: selection hands her the behavior, then the
#    walk-then-seat runtime seats her at the bed ─────────────────────
debug_command "time.set 23" "$LOG_DIR/timeset.log" || fail 'time.set unavailable'
deadline=$((SECONDS + TIMEOUT))
until grep -Fq "Entity $entity" "$LOG_DIR/sleep.log" 2>/dev/null; do
    (( SECONDS < deadline )) || fail "the settler never gained SleepBehavior inside his 22:00-08:00 window"
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for SleepBehavior"
    sleep 1
    debug_command 'entities(SleepBehavior)' "$LOG_DIR/sleep.log" >/dev/null 2>&1 || true
done
echo "smoke[m42-eat-sleep]: selection PASS — time.set 23 handed the settler SleepBehavior"

# #5427 — scope the seat leg to the settler the gate found: the log
# line prints for ANY NPC that sits, and after time.set 23 every
# Sleep-package NPC in the saloon is a candidate, so an unscoped wait
# could pass with the target still standing.
wait_log "$LOG_DIR/session.stderr" "[m42] sleep npc=$entity"
echo "smoke[m42-eat-sleep]: seat PASS — the walk-then-seat runtime seated her at the bed"
echo "smoke[m42-eat-sleep]: PASS — Sleep procedure live; artifacts: $LOG_DIR"
