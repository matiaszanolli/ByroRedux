#!/usr/bin/env bash
# P4 route smoke — the authored-objective loop on the frozen MS01 fixture
# (docs/engine/p4-quest-fixture.md), in Eltrys's authored cell.
#
# Loop under gate (2026-09-30 state):
#   1. Load MarkarthWarrens with the scripts archive; the M47.2 populate
#      walk registers the TIF_ topic-info fragments (#5152).
#   2. `quest.start MS01` + `quest.setstage` (the canonical-command setup
#      posture; the console-free end-to-end route is the post-P5 goal).
#   3. Activation selects an owned MS01 topic and stamps the response
#      (blocker 1), surfacing through `dialogue.status` and the engine
#      log's selection line (blocker 2's serial bump rides it).
#   4. The objective chain runs its live transitions: stage 15 displays
#      objective 10; stage 35 displays 35; stage 36's conditional pair
#      completes 20 and displays 22 — the completed→next-objective
#      transition through the real fragments (blocker 3; the HUD
#      notifications side is unit-gated).
#
# Known gap, asserted as a WARN (not a pass): MS01's stage-20 blocking
# branch (`MS01EltrysBlockingShrineBranch01`, whose INFO fragments set
# stages 13/82) is entered by Eltrys's FORCE-GREET, which is still
# unmodeled — the player-activation path correctly selects the authored
# top-level topic instead. The dialogue-fragment dispatch mechanism
# itself is bin-test-gated (p4 bin tests drive a synthetic TIF world
# through the same selection path).
#
# `--upscaler taa` + a fresh settings file, per the exterior-smoke
# posture: the FSR reactive masks must not touch live frames, and a
# persisted `render.upscaler` (e.g. native-aa's sharpening on a
# near-black RT interior) must not move the capture.

set -euo pipefail

SKYRIM_DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
QUEST="0x00018B4B"
ELTRYS_REF_NAME="Eltrys"

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

LOG_DIR="$(mktemp -d)"
engine_pid=""
cleanup() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
    [ "${BYRO_SMOKE_KEEP:-0}" = "1" ] || rm -rf "$LOG_DIR"
}
trap cleanup EXIT

for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Meshes1.bsa" \
    "Skyrim - Textures0.bsa" "Skyrim - Misc.bsa" "Skyrim - Interface.bsa"; do
    [ -f "$SKYRIM_DATA/$f" ] || {
        echo "smoke[p4-quest-route]: SKIP 77 — required data not found: $SKYRIM_DATA/$f"
        exit 77
    }
done
for bin in "$REPO/target/release/byroredux" "$REPO/target/release/byro-dbg"; do
    [ -x "$bin" ] || {
        echo "smoke[p4-quest-route]: FAIL — $bin not built (cargo build --release -p byroredux -p byro-dbg)"
        exit 2
    }
done

echo "═══════════════════════════════════════════════════════════════"
echo "  smoke[p4-quest-route]: MS01 in MarkarthWarrens (Eltrys)"
echo "═══════════════════════════════════════════════════════════════"

engine_stdout="$LOG_DIR/engine.stdout"
engine_stderr="$LOG_DIR/engine.stderr"

# Release binaries gate the debug server behind this opt-in (63c0aee3b).
export BYRO_DEBUG_SERVER=1
export BYRO_DEBUG_PORT="$PORT"
# A fresh settings file: a persisted render.upscaler must not move this
# route's frames (#4947 / #5142).
export BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml"
rm -f "$BYROREDUX_SETTINGS_PATH"

(
    cd "$SKYRIM_DATA"
    exec "$REPO/target/release/byroredux" \
        --esm Skyrim.esm --cell MarkarthWarrens \
        --bsa "$SKYRIM_DATA/Skyrim - Meshes0.bsa" \
        --bsa "$SKYRIM_DATA/Skyrim - Meshes1.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures0.bsa" \
        --scripts-bsa "$SKYRIM_DATA/Skyrim - Misc.bsa" \
        --bsa "$SKYRIM_DATA/Skyrim - Interface.bsa" \
        --upscaler taa \
        --bench-frames "$BENCH_FRAMES" \
        --bench-hold
) >"$engine_stdout" 2>"$engine_stderr" &
engine_pid=$!

deadline=$(( $(date +%s) + 180 ))
while ! grep -q '^bench-hold:' "$engine_stderr" 2>/dev/null; do
    if [[ $(date +%s) -gt $deadline ]]; then
        echo "smoke[p4-quest-route]: FAIL — timeout waiting for bench-hold"
        tail -20 "$engine_stderr" || true
        exit 1
    fi
    if ! kill -0 "$engine_pid" 2>/dev/null; then
        echo "smoke[p4-quest-route]: FAIL — engine exited before bench-hold"
        tail -20 "$engine_stderr" || true
        exit 1
    fi
    sleep 0.5
done

hard_fail=0
require_log() {
    local pattern="$1" description="$2" file="$3"
    if grep -Fq "$pattern" "$file"; then
        echo "smoke[p4-quest-route]: PASS — $description"
    else
        echo "smoke[p4-quest-route]: FAIL — $description (missing '$pattern')"
        hard_fail=1
    fi
}

# Gate 1 — the TIF_ populate walk ran against the real archive (#5152).
require_log "INFO response fragments" \
    "the M47.2 populate walk registered INFO response fragments" "$engine_stderr"

# Gate 2 — the dialogue loop: start, stage to the authored pre-conversation
# state, activate Eltrys, read the selection.
dbg_out="$LOG_DIR/dbg.log"
BYRO_DEBUG_PORT="$PORT" "$REPO/target/release/byro-dbg" <<EOF >"$dbg_out" 2>&1 || true
find("$ELTRYS_REF_NAME")
quest.start $QUEST
quest.setstage $QUEST 10
quit
EOF
cat "$dbg_out"

# The expression `find("Eltrys")` answers `Entity <id> "Eltrys"`; the
# activation needs the id, so resolve it in bash rather than guessing one.
entity="$(grep -oE 'Entity [0-9]+' "$dbg_out" | head -1 | cut -d' ' -f2 || true)"
if [[ -z "$entity" ]]; then
    echo "smoke[p4-quest-route]: FAIL — Eltrys entity not found in the loaded cell"
    exit 1
fi
echo "smoke[p4-quest-route]: Eltrys entity=$entity"

dbg_out2="$LOG_DIR/dbg2.log"
BYRO_DEBUG_PORT="$PORT" "$REPO/target/release/byro-dbg" <<EOF >"$dbg_out2" 2>&1 || true
script.activate $entity
dialogue.status
quit
EOF
cat "$dbg_out2"

require_log "topic 0x0D1949" \
    "activation selected Eltrys's authored top-level topic" "$dbg_out2"
require_log "quest 0x018B4B" "the selection carries the MS01 ownership edge" "$dbg_out2"
# The companion string tables must resolve (Skyrim - Interface.bsa carries
# them; a bare relative --esm launch silently missed discovery until the
# empty-parent fix) — a placeholder here means the whole session lost
# localization, the #4073 class.
if grep -q "lines=<lstring" "$dbg_out2"; then
    echo "smoke[p4-quest-route]: FAIL — the selected response is an unresolved <lstring> placeholder (string tables did not load)"
    hard_fail=1
else
    echo "smoke[p4-quest-route]: PASS — the selected response resolves from the string tables"
fi
require_log "npc dialogue: selected topic" \
    "the selection stamped the response surface (engine log)" "$engine_stderr"

# Gate 3 — the objective chain's live transitions (blocker 3): stage 15
# displays objective 10; 35 displays 35; the stage-36 conditional pair
# completes 20 and displays 22.
dbg_out3="$LOG_DIR/dbg3.log"
BYRO_DEBUG_PORT="$PORT" "$REPO/target/release/byro-dbg" <<EOF >"$dbg_out3" 2>&1 || true
quest.setstage $QUEST 15
quest.show $QUEST
quest.setstage $QUEST 35
quest.setstage $QUEST 36
quest.show $QUEST
screenshot p4-route.png
quit
EOF
cat "$dbg_out3"

require_log "result: set stage=15" "stage 15 advanced through the canonical command" "$dbg_out3"
require_log "result: set stage=35" "stage 35 advanced" "$dbg_out3"
require_log "result: set stage=36" "stage 36 advanced" "$dbg_out3"

# objective 10 displayed after stage 15's fragment (the p3-hud gate, in state form)
assert_state() {
    local needle="$1" description="$2"
    if grep -Fq "$needle" "$dbg_out3"; then
        echo "smoke[p4-quest-route]: PASS — $description"
    else
        echo "smoke[p4-quest-route]: FAIL — $description (missing '$needle')"
        hard_fail=1
    fi
}
assert_state "10: displayed=true" "stage 15's fragment displayed objective 10"
assert_state "35: displayed=true" "stage 35's fragment displayed objective 35"
# The completed→next transition: the stage-36 pair completes 20 and the
# next objective (22) displays — never a bare completion without a follow-up.
assert_state "20: displayed=false completed=true" \
    "stage 36's conditional pair completed objective 20"
assert_state "22: displayed=true" \
    "stage 36's pair displayed the follow-up objective 22"

# Gate 4 — a capture for the record: the objective line + scene through
# TAA. Bare filename under the engine cwd (63c0aee3b), moved out (#5142).
shot="$SKYRIM_DATA/screenshots/p4-route.png"
if [[ -s "$shot" ]]; then
    mv -f "$shot" "$LOG_DIR/p4-route.png"
    echo "smoke[p4-quest-route]: PASS — capture retained: $LOG_DIR/p4-route.png"
else
    echo "smoke[p4-quest-route]: WARN — no capture landed (screenshot command timing)"
fi

# The documented gap, as a WARN: the force-greet-entered blocking branch
# (whose INFO fragments set stages 13/82) is unmodeled, so the live stage
# advance comes from the QUST stage fragments only. The dialogue-fragment
# dispatch mechanism is bin-test-gated.
echo "smoke[p4-quest-route]: WARN — MS01's force-greet blocking branch is unmodeled; the dialogue-fragment stage advance is gated by the p4 bin tests, not this route"

if grep -Fq 'Error:' "$dbg_out2" "$dbg_out3"; then
    echo "smoke[p4-quest-route]: FAIL — command output contained an error"
    hard_fail=1
fi

if (( hard_fail != 0 )); then
    echo "── byro-dbg output ─────────────────────────────────────────────"
    cat "$dbg_out" "$dbg_out2" "$dbg_out3"
    exit "$hard_fail"
fi

echo "smoke[p4-quest-route]: PASS — activation → selection → response, and the objective chain's live displayed/completed transitions"
