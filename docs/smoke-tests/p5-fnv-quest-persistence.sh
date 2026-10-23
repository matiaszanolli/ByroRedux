#!/usr/bin/env bash
# P5 quest persistence, second title — Fallout: New Vegas. The FNV twin of
# `p5-quest-persistence.sh`, on the P4 route's quest (VCG02 "Back in the
# Saddle", fixtures/fnv.env P4_*), whose state lives in ObScript result-script
# effects and quest variables rather than Papyrus fragments:
#
#   1. Sunny Smiles's force-greet opens; `dialogue.select` speaks the opener,
#      whose result scripts start VCG02 at stage 10 (objective 5 displayed)
#      and record the meeting (VFreeformGoodsprings.bMetSunny). F5.
#   2. Stages 45 and 50 run their items (30 displayed, then completed; 40
#      displayed). F9 must REVERT: stage 10, objective 5 displayed, 30 and 40
#      not — and must neither re-run a stage's result script nor let Sunny
#      greet again (the meeting variable is saved quest-script state).
#   3. The restored chain is live: stage 45 displays 30 again. F5, graceful
#      quit, relaunch `--load`: stage 45 and its objective survive the restart,
#      and the force-greet stays satisfied.
#
# Headless (SMOKE_HEADLESS).
#
#   docs/smoke-tests/p5-fnv-quest-persistence.sh

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture p5-fnv-quest-persistence "${1:-fnv}"
smoke_require_fixture_fields P4_CELL P4_CAMERA_POS P4_CAMERA_FORWARD P4_QUEST \
    P4_GREETING P4_OPENER_TOPIC P4_OPENER_INFO P4_MET_VARIABLE
smoke_require_data
# Engine log lines print FormIDs as eight zero-padded hex digits.
hex8() { printf '%08X' "$(( $1 ))"; }

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-19876}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-240}"

LOG_DIR="$(mktemp -d /tmp/byro-p5-fnv-quest.XXXXXX)"
engine_pid=""
keep_artifacts="${BYROREDUX_KEEP_ARTIFACTS:-0}"
kill_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
    local orphan
    orphan=$(ss -tlnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1 || true)
    [[ -z "$orphan" ]] || kill "$orphan" 2>/dev/null || true
    engine_pid=""
}
cleanup() {
    kill_engine
    (( keep_artifacts == 0 )) && rm -rf "$LOG_DIR"
    return 0
}
trap cleanup EXIT INT TERM
fail() {
    keep_artifacts=1
    echo "smoke[p5-fnv-quest-persistence]: FAIL -- $*"
    echo "smoke[p5-fnv-quest-persistence]: artifacts retained at $LOG_DIR"
    exit 1
}
pass() { echo "smoke[p5-fnv-quest-persistence]: PASS -- $*"; }

if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi
if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

# `debug_command <commands> [logfile]` (newline-separated commands).
debug_command() {
    local commands="$1" out="${2:-/dev/stdout}"
    { printf '%s\n' "$commands"; echo .quit; } \
        | timeout "$TIMEOUT" env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$out" 2>&1
}
require() {
    local file="$1" needle="$2" description="$3"
    grep -Fq -- "$needle" "$file" || fail "$description (missing '$needle' in $(basename "$file"))"
    pass "$description"
}
refuse() {
    local file="$1" needle="$2" description="$3"
    grep -Fq -- "$needle" "$file" && fail "$description (found '$needle' in $(basename "$file"))"
    pass "$description"
}
wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Fq "$pattern" "$file" 2>/dev/null; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.25
    done
}
wait_file() {
    local file="$1" deadline=$((SECONDS + TIMEOUT))
    until [[ -s "$file" ]]; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before $file was written"
        (( SECONDS < deadline )) || fail "timeout waiting for $file"
        sleep 0.25
    done
}
launch() { # <phase> [extra engine args...]
    local phase="$1"
    shift
    (
        cd "$SMOKE_DATA"
        exec env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
            BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
            BYROREDUX_SAVE_DIR="$LOG_DIR/saves" \
            RUST_LOG="warn,byroredux::save_io=info,byroredux::systems::npc_dialogue=info,byroredux_scripting::obscript_results=info" \
            "${SMOKE_HEADLESS[@]}" "$ENGINE_BIN" "${SMOKE_ENGINE_ARGS[@]}" \
            --cell "$P4_CELL" --player --radius 1 \
            --camera-pos "$P4_CAMERA_POS" --camera-forward "$P4_CAMERA_FORWARD" \
            --bench-frames 30 --bench-hold "$@" \
            >"$LOG_DIR/$phase.stdout" 2>"$LOG_DIR/$phase.stderr"
    ) &
    engine_pid=$!
    wait_log "$LOG_DIR/$phase.stderr" 'bench-hold:'
}
quest_state() { # <logfile>: the quest + the meeting variable
    debug_command 'player.status' "$LOG_DIR/player.status"
    local player_id
    player_id="$(grep -o 'player=[0-9]*' "$LOG_DIR/player.status" | head -1 | cut -d= -f2)"
    [[ -n "$player_id" ]] || fail "player.status reported no player entity"
    debug_command "quest.show $P4_QUEST
cond $player_id $P4_MET_VARIABLE
dialogue.status" "$1"
}
stage_script_runs() { # <stderr> <stage>
    grep -c "ObScript quest $(hex8 "$P4_QUEST") stage $2 result script ran" "$1" || true
}

echo "smoke[p5-fnv-quest-persistence]: $FIXTURE_LABEL -- VCG02 vs F5/F9/restart"

# ── Session A ────────────────────────────────────────────────────────────────
launch sessionA
deadline=$((SECONDS + TIMEOUT))
until debug_command 'dialogue.status' "$LOG_DIR/a0-greeting.log" &&
      grep -Fq "$P4_GREETING" "$LOG_DIR/a0-greeting.log"; do
    (( SECONDS < deadline )) || fail "the force-greet never opened $P4_GREETING"
    sleep 1
done
debug_command "dialogue.select $P4_OPENER_TOPIC" "$LOG_DIR/a1-select.log"
require "$LOG_DIR/a1-select.log" "info $P4_OPENER_INFO" "the opener's line was spoken"
sleep 2
smoke_answer_message_boxes "$LOG_DIR/answer.log"
debug_command 'dialogue.end' "$LOG_DIR/a1-end.log"
quest_state "$LOG_DIR/a2-opened.log"
require "$LOG_DIR/a2-opened.log" "current-stage: 10" "the opener started VCG02 at stage 10"
require "$LOG_DIR/a2-opened.log" "5: displayed=true completed=false" "objective 5 displayed"
require "$LOG_DIR/a2-opened.log" "= 1" "the meeting was recorded"

debug_command 'input.press quicksave' "$LOG_DIR/a3-quicksave.log"
require "$LOG_DIR/a3-quicksave.log" "action=Quicksave binding=F5" "F5 quicksave delivered through the binding"
wait_file "$LOG_DIR/saves/save_0.ess"

for stage in 45 50; do
    debug_command "quest.setstage $P4_QUEST $stage" "$LOG_DIR/a4-stage-$stage.log"
    require "$LOG_DIR/a4-stage-$stage.log" "result: set stage=$stage" "stage $stage set"
    sleep 2
    smoke_answer_message_boxes "$LOG_DIR/answer.log"
done
quest_state "$LOG_DIR/a5-advanced.log"
require "$LOG_DIR/a5-advanced.log" "30: displayed=true completed=true" "stages 45/50 displayed and completed 30"
require "$LOG_DIR/a5-advanced.log" "40: displayed=true" "stage 50 displayed 40"

stage10_before="$(stage_script_runs "$LOG_DIR/sessionA.stderr" 10)"
debug_command 'input.press quickload' "$LOG_DIR/a6-quickload.log"
require "$LOG_DIR/a6-quickload.log" "action=Quickload binding=F9" "F9 quickload delivered through the binding"
wait_log "$LOG_DIR/sessionA.stderr" 'save load: restored player pose'
sleep 4
smoke_answer_message_boxes "$LOG_DIR/answer.log"
quest_state "$LOG_DIR/a7-restored.log"
require "$LOG_DIR/a7-restored.log" "current-stage: 10" "F9 reverted the quest to the quicksaved stage 10"
require "$LOG_DIR/a7-restored.log" "5: displayed=true completed=false" "the quicksaved objective 5 survived"
require "$LOG_DIR/a7-restored.log" "30: displayed=false completed=false" "the post-save objective 30 was undone"
require "$LOG_DIR/a7-restored.log" "40: displayed=false" "the post-save objective 40 was undone"
require "$LOG_DIR/a7-restored.log" "= 1" "the meeting variable survived the quickload"
refuse "$LOG_DIR/a7-restored.log" "$P4_GREETING" "Sunny did not force-greet again after the quickload"
[[ "$(stage_script_runs "$LOG_DIR/sessionA.stderr" 10)" == "$stage10_before" ]] \
    || fail "the quickload re-ran stage 10's result script"
pass "the quickload re-ran no stage result script"

debug_command "quest.setstage $P4_QUEST 45" "$LOG_DIR/a8-relive.log"
sleep 2
smoke_answer_message_boxes "$LOG_DIR/answer.log"
quest_state "$LOG_DIR/a9-relived.log"
require "$LOG_DIR/a9-relived.log" "30: displayed=true completed=false" "the restored chain advanced again (stage 45 displayed 30)"

debug_command 'input.press quicksave' "$LOG_DIR/a10-quicksave.log"
require "$LOG_DIR/a10-quicksave.log" "action=Quicksave binding=F5" "second F5 quicksave delivered"
wait_file "$LOG_DIR/saves/save_1.ess"
debug_command 'engine.quit' "$LOG_DIR/a11-quit.log"
require "$LOG_DIR/a11-quit.log" "graceful shutdown requested" "graceful shutdown acknowledged"
set +e
wait "$engine_pid"
quit_status=$?
set -e
engine_pid=""
[[ "$quit_status" == 0 ]] || fail "graceful quit exited $quit_status (expected 0)"

# ── Session B — process restart ──────────────────────────────────────────────
launch sessionB --load 1
wait_log "$LOG_DIR/sessionB.stderr" 'save load: restored player pose'
sleep 6
smoke_answer_message_boxes "$LOG_DIR/answer.log"
quest_state "$LOG_DIR/b1-restarted.log"
require "$LOG_DIR/b1-restarted.log" "current-stage: 45" "stage 45 survived the process restart"
require "$LOG_DIR/b1-restarted.log" "30: displayed=true completed=false" "objective 30 survived the process restart"
require "$LOG_DIR/b1-restarted.log" "40: displayed=false" "objective 40 stayed undone across the restart"
require "$LOG_DIR/b1-restarted.log" "= 1" "the meeting variable survived the restart"
refuse "$LOG_DIR/b1-restarted.log" "$P4_GREETING" "Sunny did not force-greet after the restart"
refuse "$LOG_DIR/sessionB.stderr" "ObScript quest $(hex8 "$P4_QUEST") stage" "the restart re-ran no VCG02 stage result script"

kill_engine
pass "VCG02 reverts on F9, re-advances live, and survives process restart at stage 45"
