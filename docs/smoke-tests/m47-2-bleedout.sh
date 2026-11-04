#!/usr/bin/env bash
# M47.2 essential actors live on Skyrim (RiverwoodSleepingGiantInn).
#
# Delphine's base is Essential (ACBS 0x2), Orgnar's is not. Real swings
# (combat.approach + input.press attack, the p2 route) take each to zero
# health: Delphine must go into bleedout instead of dying (combat trace
# `killed=false … bleedout`, IsBleedingOut 1, GetDead 0) and get back up after
# Skyrim.esm's 20 s fEssentialDeathTime with fEssentialHealthPercentReGain
# (0.3) of her 100 health; Orgnar must die (killed=true, GetDead 1).
#
#   docs/smoke-tests/m47-2-bleedout.sh
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19881}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-m47-2-bleedout.XXXXXX)"
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
    echo "smoke[m47-2-bleedout]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m47-2-bleedout]: SKIP -- missing $DATA/$f"
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

# `cond <entity> <Func> [p1]` → the printed f32.
cond_value() {
    debug_command "cond $1 $2 ${3:-}" "$LOG_DIR/cond.log" || fail "cond $2 unavailable"
    sed -nE 's/.* on entity [0-9]+ = (-?[0-9.]+).*/\1/p' "$LOG_DIR/cond.log" | head -1
}

find_entity() {
    debug_command "find(\"$1\")" "$LOG_DIR/find.log" || fail "find unavailable"
    grep -oE "Entity [0-9]+ \"$1\"" "$LOG_DIR/find.log" | head -1 | cut -d' ' -f2
}

# Swing at `$1` until the combat trace names it with `$2`; at most 60 swings.
swing_until() {
    local target="$1" pattern="$2"
    for swing in $(seq 1 60); do
        debug_command "combat.approach $target" "$LOG_DIR/approach.log" || fail "approach failed"
        debug_command "input.press attack" "$LOG_DIR/press.log" || fail "attack failed"
        sleep 0.6
        debug_command "combat.status" "$LOG_DIR/combat.log" || fail "combat.status failed"
        if grep -q "last_target=$target .*$pattern" "$LOG_DIR/combat.log"; then
            return 0
        fi
    done
    fail "60 swings at $target never produced '$pattern'"
}

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn,byroredux_scripting::bleedout=info" \
    target/release/byroredux \
    --game skyrim_se --cell RiverwoodSleepingGiantInn \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!
deadline=$((SECONDS + TIMEOUT))
until grep -Fq 'bench-hold:' "$LOG_DIR/session.stderr"; do
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold"
    (( SECONDS < deadline )) || fail "timeout waiting for bench-hold"
    sleep 0.25
done
sleep 2

delphine="$(find_entity Delphine)"
orgnar="$(find_entity Orgnar)"
[[ -n "$delphine" && -n "$orgnar" ]] || fail "Delphine / Orgnar not found"
[[ "$(cond_value "$delphine" IsEssential)" == 1 ]] || fail "Delphine is not essential"
[[ "$(cond_value "$orgnar" IsEssential)" == 0 ]] || fail "Orgnar reads essential"
# #6611 — the recovery target is `fEssentialHealthPercentReGain` of the actor's UNDAMAGED health, and that health
# is composed from the load order (base + class / level terms, #5914/#5915): 190 for Delphine at the time of the
# report, not the 100 the gate once assumed (a literal 30). Read it from the running engine before the fight, when
# nothing has damaged her yet, and derive the expectation from it.
REGAIN="${BYROREDUX_BLEEDOUT_REGAIN:-0.3}"   # vanilla Skyrim.esm fEssentialHealthPercentReGain
undamaged="$(cond_value "$delphine" GetActorValue 0x3E8)"
[[ -n "$undamaged" ]] || fail "could not read Delphine's undamaged health"
expected="$(awk -v u="$undamaged" -v r="$REGAIN" 'BEGIN { printf "%.3f", u * r }')"

swing_until "$delphine" "killed=false.*bleedout"
[[ "$(cond_value "$delphine" IsBleedingOut)" == 1 ]] || fail "Delphine is not bleeding out"
[[ "$(cond_value "$delphine" GetDead)" == 0 ]] || fail "Delphine died"
echo "smoke[m47-2-bleedout]: Delphine is down in bleedout"

sleep 21
[[ "$(cond_value "$delphine" IsBleedingOut)" == 0 ]] || fail "Delphine did not get back up"
health="$(cond_value "$delphine" GetActorValue 0x3E8)"
awk -v h="$health" -v e="$expected" 'BEGIN { d = h - e; exit !(d > -0.01 && d < 0.01) }' \
    || fail "Delphine recovered at $health health, not $expected ($REGAIN of her undamaged $undamaged)"
echo "smoke[m47-2-bleedout]: Delphine got back up at $health health ($REGAIN of $undamaged)"

swing_until "$orgnar" "killed=true"
[[ "$(cond_value "$orgnar" GetDead)" == 1 ]] || fail "Orgnar is not dead"
echo "smoke[m47-2-bleedout]: Orgnar (not essential) died"
echo "smoke[m47-2-bleedout]: PASS"
