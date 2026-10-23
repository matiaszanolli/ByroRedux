#!/usr/bin/env bash
# P4 route smoke, second title — the authored-objective loop on Fallout: New
# Vegas, where it runs on ObScript *result scripts* instead of Papyrus
# fragments: VCG02 "Back in the Saddle" in the Prospector Saloon, opened the
# way the game opens it. Every value is the fixture's (fixtures/fnv.env, P4_*).
#
# Loop under gate:
#   1. Boot the saloon in Character mode. Sunny Smiles's authored force-greet
#      opens her GREETING line with no activation, and the line offers the
#      topics its INFO links (TCLT), "teach me" among them.
#   2. `dialogue.select` picks that topic through the page's own selection.
#      The outgoing greeting's End result script records the meeting (the
#      force-greet gate variable), and the new line's Begin result script
#      starts the quest, completes objective 3, displays 5 and sets stage 10,
#      whose stage item's script then runs. No console stage set is involved.
#   3. The stages the world sets later (the geckos at the well) run their
#      items: 45 displays 30; 50 completes 30 and displays 40 — the
#      completed→next transition, set through `quest.setstage` as the Skyrim
#      route does for its world-action stages.
#   4. Sunny's next authored force-greet finds the player: her line completes
#      objective 40 ("Talk to Sunny Smiles") and offers the follow-up; the
#      reply hands over 5.56mm rounds and displays objective 50.
#   5. Vanilla tutorial boxes the route raises (CGTutorial) are answered, as
#      a player would; each pauses the world until it is.
#
# Runs headless (SMOKE_HEADLESS): the gate's window never reaches the user's
# Wayland session or its input.
#
#   docs/smoke-tests/p4-fnv-quest-route.sh        # needs FNV data on disk

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture p4-fnv-quest-route "${1:-fnv}"
smoke_require_fixture_fields P4_HEADLINE P4_CELL P4_CAMERA_POS P4_CAMERA_FORWARD \
    P4_QUEST P4_GREETING P4_OPENER_TOPIC P4_OPENER_INFO P4_MET_VARIABLE P4_WORLD_STAGES \
    P4_FOLLOWUP_INFO P4_FOLLOWUP_TOPIC P4_FOLLOWUP_REPLY_INFO
smoke_require_data
# Engine log lines print FormIDs as eight zero-padded hex digits.
hex8() { printf '%08X' "$(( $1 ))"; }

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-240}"

LOG_DIR="$(mktemp -d /tmp/byro-p4-fnv.XXXXXX)"
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
    echo "smoke[p4-fnv-quest-route]: FAIL -- $*"
    echo "smoke[p4-fnv-quest-route]: artifacts retained at $LOG_DIR"
    tail -30 "$LOG_DIR/engine.stderr" 2>/dev/null || true
    exit 1
}
pass() { echo "smoke[p4-fnv-quest-route]: PASS -- $*"; }

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[p4-fnv-quest-route]: building release binaries"
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

# `debug_command <commands> [logfile]` (newline-separated commands).
debug_command() {
    local commands="$1" out="${2:-/dev/stdout}"
    { printf '%s\n' "$commands"; echo .quit; } | env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$out" 2>&1
}

require() {
    local file="$1" needle="$2" description="$3"
    grep -Fq -- "$needle" "$file" || fail "$description (missing '$needle')"
    pass "$description"
}

echo "smoke[p4-fnv-quest-route]: $FIXTURE_LABEL -- $P4_HEADLINE"

cd "$SMOKE_DATA"
env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    BYROREDUX_SAVE_DIR="$LOG_DIR/saves" \
    RUST_LOG="warn,byroredux::systems::npc_dialogue=info,byroredux_scripting::obscript_results=info" \
    "${SMOKE_HEADLESS[@]}" "$ENGINE_BIN" "${SMOKE_ENGINE_ARGS[@]}" \
    --cell "$P4_CELL" --player --radius 1 \
    --camera-pos "$P4_CAMERA_POS" --camera-forward "$P4_CAMERA_FORWARD" \
    --bench-frames 30 --bench-hold \
    >"$LOG_DIR/engine.stdout" 2>"$LOG_DIR/engine.stderr" &
engine_pid=$!

deadline=$((SECONDS + TIMEOUT))
until grep -q 'bench-hold:' "$LOG_DIR/engine.stderr" 2>/dev/null; do
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold"
    (( SECONDS < deadline )) || fail "timeout waiting for bench-hold"
    sleep 0.5
done
pass "engine reached the held interactive state"

# ── 1 — the force-greet opens the authored greeting, no activation ──────────
deadline=$((SECONDS + TIMEOUT))
until debug_command 'dialogue.status' "$LOG_DIR/greeting.status" &&
      grep -Fq "$P4_GREETING" "$LOG_DIR/greeting.status"; do
    (( SECONDS < deadline )) || fail "the force-greet never opened $P4_GREETING ($(tail -2 "$LOG_DIR/greeting.status"))"
    sleep 1
done
pass "the force-greet opened Sunny's authored greeting without activation"
require "$LOG_DIR/greeting.status" "offers topic $P4_OPENER_TOPIC" \
    "the greeting's linked topics offer the quest opener"
smoke_answer_message_boxes "$LOG_DIR/answer.log"

# ── 2 — the opener: the greeting ends, the quest's line begins ──────────────
debug_command "dialogue.select $P4_OPENER_TOPIC" "$LOG_DIR/select.log"
require "$LOG_DIR/select.log" "info $P4_OPENER_INFO" \
    "dialogue.select spoke the opener's line through the page's selection"
sleep 2
smoke_answer_message_boxes "$LOG_DIR/answer.log"
debug_command 'screenshot p4-fnv-line.png' "$LOG_DIR/shot.log" || true
debug_command 'player.status' "$LOG_DIR/player.status"
player_id="$(grep -o 'player=[0-9]*' "$LOG_DIR/player.status" | head -1 | cut -d= -f2)"
[[ -n "$player_id" ]] || fail "player.status reported no player entity"
debug_command "cond $player_id $P4_MET_VARIABLE
quest.show $P4_QUEST" "$LOG_DIR/opened.quest"

require "$LOG_DIR/engine.stderr" "ObScript INFO 00104E7E end result script ran" \
    "the outgoing greeting's End result script ran"
require "$LOG_DIR/opened.quest" "= 1" \
    "the greeting's End script recorded the meeting (the force-greet gate)"
require "$LOG_DIR/engine.stderr" "ObScript INFO $(hex8 "$P4_OPENER_INFO") begin result script ran" \
    "the opener's Begin result script ran"
require "$LOG_DIR/opened.quest" "state: running" "the line's script started the quest"
require "$LOG_DIR/opened.quest" "current-stage: 10" "the line's script set stage 10"
# GECK *SetObjectiveCompleted*: completing does not display — objective 3
# ("Talk to Sunny") was never shown on this route, so it completes hidden.
require "$LOG_DIR/opened.quest" "3: displayed=false completed=true" \
    "the line's script completed objective 3"
require "$LOG_DIR/opened.quest" "5: displayed=true completed=false" \
    "the line's script displayed objective 5"
require "$LOG_DIR/engine.stderr" "ObScript quest $(hex8 "$P4_QUEST") stage 10 result script ran" \
    "stage 10's item result script ran"
debug_command 'dialogue.end' "$LOG_DIR/end.log"

# ── 3 — the world-action stages' items: display, then complete → next ───────
for stage in "${P4_WORLD_STAGES[@]}"; do
    debug_command "quest.setstage $P4_QUEST $stage" "$LOG_DIR/stage-$stage.log"
    require "$LOG_DIR/stage-$stage.log" "result: set stage=$stage" "stage $stage set"
    sleep 2
    smoke_answer_message_boxes "$LOG_DIR/answer.log"
done
debug_command "quest.show $P4_QUEST" "$LOG_DIR/chain.quest"
require "$LOG_DIR/chain.quest" "30: displayed=true completed=true" \
    "stage 45 displayed objective 30 and stage 50 completed it"
require "$LOG_DIR/chain.quest" "40: displayed=true" \
    "stage 50 displayed the follow-up objective 40"

# ── 4 — Sunny's next force-greet: talk to her, take the follow-up ─────────
deadline=$((SECONDS + TIMEOUT))
until debug_command 'dialogue.status' "$LOG_DIR/followup.status" &&
      grep -Fq "info $P4_FOLLOWUP_INFO" "$LOG_DIR/followup.status"; do
    (( SECONDS < deadline )) || fail "Sunny's follow-up force-greet never spoke $P4_FOLLOWUP_INFO"
    sleep 1
    smoke_answer_message_boxes "$LOG_DIR/answer.log"
done
pass "Sunny's authored follow-up force-greet opened her next line"
require "$LOG_DIR/followup.status" "offers topic $P4_FOLLOWUP_TOPIC" \
    "the line offers the follow-up topic"
debug_command "dialogue.select $P4_FOLLOWUP_TOPIC" "$LOG_DIR/followup-select.log"
require "$LOG_DIR/followup-select.log" "info $P4_FOLLOWUP_REPLY_INFO" \
    "the follow-up reply was spoken"
sleep 2
smoke_answer_message_boxes "$LOG_DIR/answer.log"
debug_command "quest.show $P4_QUEST" "$LOG_DIR/followup.quest"
require "$LOG_DIR/followup.quest" "40: displayed=true completed=true" \
    "talking to Sunny completed objective 40"
require "$LOG_DIR/followup.quest" "50: displayed=true completed=false" \
    "the reply's script displayed objective 50"
require "$LOG_DIR/engine.stderr" "ObScript INFO $(hex8 "$P4_FOLLOWUP_REPLY_INFO") begin result script ran" \
    "the reply's Begin result script ran (the ammo hand-over)"

# ── 5 — captures for the record (the presented line; the objective HUD) ───
sleep 1
debug_command 'screenshot p4-fnv-hud.png' "$LOG_DIR/shot.log" || true
sleep 2
for shot in p4-fnv-line.png p4-fnv-hud.png; do
    [[ -s "$SMOKE_DATA/screenshots/$shot" ]] && mv -f "$SMOKE_DATA/screenshots/$shot" "$LOG_DIR/$shot"
done
[[ -s "$LOG_DIR/p4-fnv-line.png" ]] || echo "smoke[p4-fnv-quest-route]: WARN -- no line capture landed"
[[ -s "$LOG_DIR/p4-fnv-hud.png" ]] || echo "smoke[p4-fnv-quest-route]: WARN -- no HUD capture landed"

if grep -qF 'Error:' "$LOG_DIR"/*.log "$LOG_DIR"/*.quest 2>/dev/null; then
    fail "a debug command answered with an error"
fi
pass "force-greet -> linked topic -> result scripts start the quest -> objective chain -> Sunny's follow-up"
