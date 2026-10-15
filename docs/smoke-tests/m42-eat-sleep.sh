#!/usr/bin/env bash
# M42 gate — Sleep (procedure 4) through ambient selection, live.
#
#   1. Boot the FNV profile into the Prospector Saloon with a player.
#   2. Baseline: at the default boot hour the Goodsprings settler's
#      package is not a Sleep one (`GSSettler04SleepPackage22x8`
#      runs 22:00-08:00; his Flee/Ambush packs above it are
#      gated on `GetQuestVariable` (fn 79), which reads the quest
#      script's locals, 0 until a run assigns them, so they evaluate
#      FALSE) — he carries no SleepBehavior.
#   3. `time.set 23` drops the clock inside the Sleep window — the
#      game-minute re-evaluation must hand him `SleepBehavior` (the
#      M42 dispatch). His PLDT is In-Cell for his own house (not
#      the loaded saloon), so he idles rather than walking or seating
#      (#5391); the seat runtime is unit-gated in `eat_sleep`'s tests.
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

# ── The Sleep window: selection hands him the behavior; a non-resident
#    In-Cell package then idles (#5391) instead of walking or seating ──
debug_command "time.set 23" "$LOG_DIR/timeset.log" || fail 'time.set unavailable'
deadline=$((SECONDS + TIMEOUT))
until grep -Fq "Entity $entity" "$LOG_DIR/sleep.log" 2>/dev/null; do
    (( SECONDS < deadline )) || fail "the settler never gained SleepBehavior inside his 22:00-08:00 window"
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for SleepBehavior"
    sleep 1
    debug_command 'entities(SleepBehavior)' "$LOG_DIR/sleep.log" >/dev/null 2>&1 || true
done
echo "smoke[m42-eat-sleep]: selection PASS — time.set 23 handed the settler SleepBehavior"

# The seat leg. The settler's Sleep package (`GSSettler04SleepPackage22x8`)
# is an In-Cell package for `GSHouseInterior02` (0x001055E1) — his own house,
# not the saloon this fixture loads. Per #5391 an In-Cell package whose cell
# is not the resident interior idles: no walk and no seat. (An earlier
# revision of this gate expected him to take the nearest saloon marker; that
# only held while an unresolvable anchor froze the actor's own position as the
# destination, which #5500 removed.) So the gate asserts the idle: after the
# window has had time to act he carries SleepBehavior and no Seated. The
# walk-then-seat runtime itself is unit-gated in `eat_sleep`'s fixture tests;
# no NPC loaded in this cell has an In-Cell Eat/Sleep package for it
# (`GSSettlerCFEatPackage12x1` belongs to `GSSettlerCF`, who is not spawned
# here), so there is nothing live in this cell to seat.
sleep 10
debug_command 'entities(Seated)' "$LOG_DIR/seated.log" || true
if grep -Fq "Entity $entity" "$LOG_DIR/seated.log"; then
    fail "the settler's Sleep package is In-Cell for another cell, so he must idle (#5391), yet he was seated: $(sed -n '1,4p' "$LOG_DIR/seated.log")"
fi
echo "smoke[m42-eat-sleep]: idle PASS — the In-Cell package for a non-resident cell neither walks nor seats (#5391)"
echo "smoke[m42-eat-sleep]: PASS — Sleep selection live; artifacts: $LOG_DIR"
