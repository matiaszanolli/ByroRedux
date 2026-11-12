#!/usr/bin/env bash
# New Vegas, VCG02 "Back in the Saddle": Sunny Smiles takes the player out of the Prospector Saloon
# and teaches them to shoot. The part of the opening that needs an actor to leave an interior.
#
#   1. Boot in the Prospector Saloon with VCG02 at stage 5 (where the new-game route leaves it).
#   2. The player talks to Sunny and asks her to teach them (INFO 0010A1E4, VCG02 stage 10).
#   3. Sunny's Travel package sends her to a marker outside: she walks to a load door of the
#      saloon, passes it (actor.where: routing via door, then not loaded in the exterior).
#   4. The player follows through the same door; Sunny is there, spawned at her ledger pose.
#   5. She walks the navmesh to the shooting range (stage 20), the player steps into its trigger,
#      and her bark runs: objective 10, the bottles come in (a resident Enable, #5065) and the
#      route hands the player a Varmint Rifle.
#   6. Three rifle shots hit three bottles (OnHitWith) and the objective completes.
#
# Headless (Xvfb). FNV only. See docs/prd/cross-cell-actor-travel.md (XCT-G-001) and
# docs/smoke-tests/fnv-new-game.sh for the route that leads here.
#   docs/smoke-tests/fnv-vcg02-route.sh

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture fnv-vcg02-route fnv
smoke_require_fixture_fields VR_CELL VR_QUEST VR_SUNNY VR_SUNNY_TOPIC VR_TRIGGER VR_BOTTLES VR_RIFLE
smoke_require_data

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"

LOG_DIR="$(mktemp -d /tmp/byro-fnv-vcg02-route.XXXXXX)"
engine_pid=""
keep_artifacts="${BYROREDUX_KEEP_ARTIFACTS:-0}"
cleanup() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
    # The pid is the xvfb-run wrapper; the engine is the port's listener.
    local orphan
    orphan=$(ss -tlnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1 || true)
    [[ -z "$orphan" ]] || kill "$orphan" 2>/dev/null || true
    (( keep_artifacts == 0 )) && rm -rf "$LOG_DIR"
    return 0
}
trap cleanup EXIT INT TERM

fail() {
    keep_artifacts=1
    echo "smoke[fnv-vcg02-route]: FAIL -- $*"
    echo "smoke[fnv-vcg02-route]: artifacts retained at $LOG_DIR"
    tail -30 "$LOG_DIR/engine.stderr" 2>/dev/null || true
    exit 1
}
pass() { echo "smoke[fnv-vcg02-route]: PASS -- $*"; }

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[fnv-vcg02-route]: building release binaries"
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

# `debug_command <commands> [logfile]` (newline-separated commands).
debug_command() {
    local commands="$1" out="${2:-/dev/stdout}"
    { printf '%s\n' "$commands"; echo .quit; } | env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$out" 2>&1
}
# `ask <command>` — the command's output, unescaped, on stdout.
ask() {
    debug_command "$1" "$LOG_DIR/ask.tmp"
    sed 's/\\n/\n/g; s/\\"/"/g' "$LOG_DIR/ask.tmp"
}
# `wait_for <seconds> <description> <command> <grep -E pattern>` — poll until the command's output matches.
wait_for() {
    local seconds="$1" description="$2" command="$3" pattern="$4" deadline=$((SECONDS + $1))
    until ask "$command" | grep -Eq -- "$pattern"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "the engine exited waiting for: $description"
        (( SECONDS < deadline )) || fail "timeout (${seconds}s) waiting for: $description ($(ask "$command" | tail -2 | tr '\n' ' ' | cut -c1-300))"
        sleep 2
    done
}
numbers() { grep -o "$1 *([^)]*)" "$2" | head -1 | grep -oE '[-+]?[0-9]+(\.[0-9]+)?' | tr '\n' ' '; }

cd "$SMOKE_DATA"
# A small streamed ring: the gate crosses between the saloon and the town, and a default-radius
# exterior (25 x 25 cells) would spend minutes loading and unloading them.
env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    BYROREDUX_SAVE_DIR="$LOG_DIR/saves" \
    RUST_LOG="warn,byroredux::app_step=info,byroredux::cell_loader::enabled=info,byroredux::cell_loader::created=info" \
    "${SMOKE_HEADLESS[@]}" "$ENGINE_BIN" "${SMOKE_ENGINE_ARGS[@]}" \
    --cell "$VR_CELL" --radius 3 --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/engine.stdout" 2>"$LOG_DIR/engine.stderr" &
engine_pid=$!

deadline=$((SECONDS + 240))
until grep -q 'bench-hold:' "$LOG_DIR/engine.stderr" 2>/dev/null; do
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold"
    (( SECONDS < deadline )) || fail "timeout waiting for bench-hold"
    sleep 0.5
done
pass "engine reached the held saloon"

# ── 1 — VCG02 at stage 5, as the new-game route hands it over ────────────────
debug_command "quest.start $VR_QUEST
quest.setstage $VR_QUEST 5
start.release" /dev/null
sleep 3

# ── 2 — the player asks Sunny to teach them ──────────────────────────────────
debug_command "script.activate $VR_SUNNY" /dev/null
wait_for 60 "Sunny's greeting to open" dialogue.status "npc [0-9]+: topic"
debug_command "dialogue.select $VR_SUNNY_TOPIC" /dev/null
wait_for 60 "VCG02 stage 10" "quest.show $VR_QUEST" "current-stage: (1[0-9]|[2-9][0-9])"
pass "Sunny agreed to teach the player (VCG02 stage 10)"

# ── 3 — Sunny walks to a load door of the saloon and passes it ───────────────
wait_for 90 "Sunny routing through a load door" "actor.where $VR_SUNNY" "routing via door entity [0-9]+"
door=$(ask "actor.where $VR_SUNNY" | grep -o 'routing via door entity [0-9]*' | grep -o '[0-9]*$' | head -1)
pass "Sunny is routing to load door $door (a destination in another space)"
wait_for 120 "Sunny to pass the door" "actor.where $VR_SUNNY" "not loaded, in Exterior.*moved from its home cell"
pass "Sunny passed the door: the ledger puts her in the exterior, not the saloon"

# ── 4 — the player follows; Sunny is spawned there ───────────────────────────
debug_command "script.activate $door" /dev/null
wait_for 420 "the player outside" player.status "grid=\(-1[0-9],[0-9]+\) grounded=true"
wait_for 120 "Sunny spawned outside" "actor.where $VR_SUNNY" "actor [0-9A-F]+: in Exterior"
pass "Sunny is outside, spawned at her ledger pose"

# ── 5 — Sunny walks to the shooting range; her bark runs ─────────────────────
wait_for 240 "Sunny to reach the shooting range (VCG02 stage 20)" "quest.show $VR_QUEST" "current-stage: (2[0-9]|[3-9][0-9])"
pass "Sunny walked the navmesh to her place (stage 20)"
debug_command "mesh.info $VR_TRIGGER" "$LOG_DIR/trigger.info"
read -r tx ty tz <<<"$(numbers 'Transform.global: *pos' "$LOG_DIR/trigger.info")"
[[ -n "${tz:-}" ]] || fail "the shooting-range trigger has no placement ($(head -3 "$LOG_DIR/trigger.info"))"
debug_command "cam.pos $(awk -v x="$tx" -v y="$ty" -v z="$tz" 'BEGIN { printf "%.1f %.1f %.1f", x + 120, y + 40, z }')" /dev/null
wait_for 120 "Sunny's bark (objective 10 displayed)" "quest.show $VR_QUEST" " 10: displayed=true"
pass "Sunny's bark ran: objective 10 is displayed"
first_bottle=${VR_BOTTLES%% *}
deadline=$((SECONDS + 60))
until grep -q "Enabled reference $first_bottle: its content spawned" "$LOG_DIR/engine.stderr"; do
    (( SECONDS < deadline )) || fail "the bark never brought the bottles in"
    sleep 1
done
pass "the bottles were enabled with the cell loaded (#5065)"
wait_for 30 "the rifle in the player's hands" inventory.status "equipped_weapon=0x$VR_RIFLE"
pass "the route armed the player with the Varmint Rifle"

# ── 6 — three shots, three bottles ───────────────────────────────────────────
for bottle in $VR_BOTTLES; do
    debug_command "mesh.info 0x$bottle" "$LOG_DIR/bottle.$bottle"
    read -r bx by bz <<<"$(numbers 'Transform.global: *pos' "$LOG_DIR/bottle.$bottle")"
    [[ -n "${bz:-}" ]] || fail "bottle $bottle has no placement ($(head -3 "$LOG_DIR/bottle.$bottle"))"
    debug_command "phys.census $bx $bz 40" "$LOG_DIR/census.$bottle"
    grep -q "form=0x${bottle#00}" "$LOG_DIR/census.$bottle" || fail "bottle $bottle has no collision body"
    # Stand 400 units west-north-west of the bottle (clear of the fence it sits on), aim, fire.
    debug_command "cam.pos $(awk -v x="$bx" -v y="$by" -v z="$bz" 'BEGIN { printf "%.1f %.1f %.1f", x - 382, y + 60, z + 118 }')" /dev/null
    sleep 4
    debug_command cam.where "$LOG_DIR/cam.where"
    read -r cx cy cz <<<"$(numbers 'position:' "$LOG_DIR/cam.where")"
    debug_command "input.look $(awk -v bx="$bx" -v by="$by" -v bz="$bz" -v cx="$cx" -v cy="$cy" -v cz="$cz" 'BEGIN {
        dx = bx - cx; dy = by + 6 - cy; dz = bz - cz; deg = 180 / atan2(0, -1)
        printf "%.2f %.2f", atan2(-dx, -dz) * deg, atan2(dy, sqrt(dx * dx + dz * dz)) * deg }')" /dev/null
    sleep 1
    debug_command "input.press attack" /dev/null
    sleep 3
done
debug_command "quest.show $VR_QUEST" "$LOG_DIR/vcg02.bottles"
require() {
    grep -Fq -- "$2" "$1" || fail "$3 (missing '$2' in: $(grep -E 'script-vars| 10:' "$1" | cut -c1-200))"
    pass "$3"
}
require "$LOG_DIR/vcg02.bottles" "nTargetCount=3" "three rifle shots hit three bottles (OnHitWith)"
grep -Eq ' 10: displayed=(true|false) completed=true' "$LOG_DIR/vcg02.bottles" ||
    fail "the shooting objective (10) did not complete ($(grep ' 10:' "$LOG_DIR/vcg02.bottles"))"
pass "the shooting objective completes"
echo "smoke[fnv-vcg02-route]: PASS -- the saloon -> Sunny leaves through the door -> the shooting range -> three bottles"
