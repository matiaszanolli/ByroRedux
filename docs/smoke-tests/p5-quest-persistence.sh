#!/usr/bin/env bash
# P5 hardening gate 2 — quest/objective state survives quicksave → quickload
# and quicksave → process restart, and the restored chain still advances.
#
# Companion to `p4-quest-route.sh` (which proves the authored objective loop
# live on the frozen MS01 fixture): this gate takes the same loop through the
# P5 persistence requirements —
#
#   1. Drive MS01 to stage 15 (objective 10 displayed), F5 quicksave.
#   2. Advance to stage 36 (35 displayed, 20 completed, 22 displayed).
#   3. F9 quickload: quest state must REVERT to the quicksaved point —
#      current-stage 15, objective 10 still displayed, objective 22 no
#      longer displayed (the post-save transitions were undone, not merged).
#   4. The restored chain is live: setstage 35 again displays objective 35.
#   5. F5 again, engine.quit, relaunch with --load: the stage-35 state
#      survives the process restart.
#
# `quest.show`/`quest.setstage` are the canonical-command setup posture (the
# console-free route is the post-P5 goal); `input.press quicksave`/`quickload`
# deliver the same F5/F9 binding edges the window path uses.
set -euo pipefail

SKYRIM_DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
PORT="${BYROREDUX_SMOKE_PORT:-19875}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-240}"
QUEST="0x00018B4B"

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

LOG_DIR="$(mktemp -d /tmp/byro-p5-quest-persistence.XXXXXX)"
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
    echo "smoke[p5-quest-persistence]: FAIL — $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Meshes1.bsa" \
    "Skyrim - Textures0.bsa" "Skyrim - Misc.bsa" "Skyrim - Interface.bsa"; do
    [ -f "$SKYRIM_DATA/$f" ] || {
        echo "smoke[p5-quest-persistence]: SKIP 77 — required data not found: $SKYRIM_DATA/$f"
        exit 77
    }
done
for bin in "$REPO/target/release/byroredux" "$REPO/target/release/byro-dbg"; do
    [ -x "$bin" ] || {
        echo "smoke[p5-quest-persistence]: FAIL — $bin not built (cargo build --release -p byroredux -p byro-dbg)"
        exit 2
    }
done
if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

# Release binaries gate the debug server behind this opt-in (63c0aee3b).
export BYRO_DEBUG_SERVER=1
export BYRO_DEBUG_PORT="$PORT"
export BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml"
export BYROREDUX_SAVE_DIR="$LOG_DIR/saves"
rm -f "$BYROREDUX_SETTINGS_PATH"

engine_stderr="$LOG_DIR/engine.stderr"
launch_engine() { # <phase> [--load <slot>]
    local phase="$1"
    shift
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
            --bench-frames 30 \
            --bench-hold "$@" \
            >"$LOG_DIR/$phase.stdout" 2>"$LOG_DIR/$phase.stderr"
    ) &
    engine_pid=$!
    local deadline=$((SECONDS + TIMEOUT))
    until grep -q '^bench-hold:' "$LOG_DIR/$phase.stderr" 2>/dev/null; do
        kill -0 "$engine_pid" 2>/dev/null \
            || fail "engine ($phase) exited before bench-hold"
        (( SECONDS < deadline )) || fail "timeout waiting for bench-hold ($phase)"
        sleep 0.5
    done
}
wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Fq "$pattern" "$file"; do
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
dbg() { # <output-file> <command lines...>
    local out="$1"
    shift
    {
        printf '%s\n' "$@"
        printf '%s\n' ".quit"
    } | timeout "$TIMEOUT" "$REPO/target/release/byro-dbg" >"$out" 2>&1
}
require_in() {
    local pattern="$1" description="$2" file="$3"
    grep -Fq "$pattern" "$file" \
        || fail "$description (missing '$pattern' in $(basename "$file"))"
}

echo "═══════════════════════════════════════════════════════════════"
echo "  smoke[p5-quest-persistence]: MS01 objective state vs F5/F9/restart"
echo "═══════════════════════════════════════════════════════════════"

# ── Session A — drive the chain to stage 15, quicksave, advance to 36 ──────
launch_engine sessionA

dbg "$LOG_DIR/a1-setup.log" "quest.start $QUEST" "quest.setstage $QUEST 10" "quest.setstage $QUEST 15" "quest.show $QUEST"
require_in "result: set stage=15" "stage 15 advanced through the canonical command" "$LOG_DIR/a1-setup.log"
require_in "10: displayed=true" "stage 15's fragment displayed objective 10" "$LOG_DIR/a1-setup.log"

dbg "$LOG_DIR/a2-quicksave.log" "input.press quicksave"
require_in "action=Quicksave binding=F5" "F5 quicksave delivered through the binding" "$LOG_DIR/a2-quicksave.log"
wait_file "$LOG_DIR/saves/save_0.ess"

dbg "$LOG_DIR/a3-advance.log" "quest.setstage $QUEST 35" "quest.setstage $QUEST 36" "quest.show $QUEST"
require_in "result: set stage=36" "stage 36 advanced" "$LOG_DIR/a3-advance.log"
require_in "35: displayed=true" "objective 35 displayed at stage 35" "$LOG_DIR/a3-advance.log"
require_in "20: displayed=false completed=true" "objective 20 completed by the stage-36 pair" "$LOG_DIR/a3-advance.log"
require_in "22: displayed=true" "objective 22 displayed by the stage-36 pair" "$LOG_DIR/a3-advance.log"

# ── F9 quickload must REVERT the chain to the quicksaved point ──────────────
dbg "$LOG_DIR/a4-quickload.log" "input.press quickload"
require_in "action=Quickload binding=F9" "F9 quickload delivered through the binding" "$LOG_DIR/a4-quickload.log"
wait_log "$LOG_DIR/sessionA.stderr" 'save load: restored player pose'
sleep 2  # the restored world needs a frame to re-derive quest bindings

dbg "$LOG_DIR/a5-restored.log" "quest.show $QUEST"
require_in "current-stage: 15" "quest stage reverted to the quicksaved stage 15" "$LOG_DIR/a5-restored.log"
require_in "10: displayed=true" "quicksaved objective 10 display survived the restore" "$LOG_DIR/a5-restored.log"
if grep -Fq "22: displayed=true" "$LOG_DIR/a5-restored.log"; then
    fail "objective 22 is still displayed after quickload — the post-save transitions were merged, not reverted"
fi
require_in "22: displayed=false" "objective 22's post-save display was undone by the restore" "$LOG_DIR/a5-restored.log"

# ── The restored chain is live: it advances again ───────────────────────────
dbg "$LOG_DIR/a6-relive.log" "quest.setstage $QUEST 35" "quest.show $QUEST"
require_in "result: set stage=35" "the restored chain advanced again after quickload" "$LOG_DIR/a6-relive.log"
require_in "35: displayed=true" "objective 35 re-displayed after restore + advance" "$LOG_DIR/a6-relive.log"

# ── Session B — the same state survives a process restart ───────────────────
dbg "$LOG_DIR/a7-quicksave2.log" "input.press quicksave"
require_in "action=Quicksave binding=F5" "second F5 quicksave delivered" "$LOG_DIR/a7-quicksave2.log"
wait_file "$LOG_DIR/saves/save_1.ess"

dbg "$LOG_DIR/a8-quit.log" "engine.quit"
require_in "graceful shutdown requested" "graceful shutdown acknowledged" "$LOG_DIR/a8-quit.log"
set +e
wait "$engine_pid"
quit_status=$?
set -e
engine_pid=""
[[ "$quit_status" == 0 ]] || fail "graceful quit exited $quit_status (expected 0)"

launch_engine sessionB --load 1
wait_log "$LOG_DIR/sessionB.stderr" 'save load: restored player pose'
sleep 2

dbg "$LOG_DIR/b1-restarted.log" "quest.show $QUEST"
require_in "current-stage: 35" "quest stage 35 survived the process restart" "$LOG_DIR/b1-restarted.log"
require_in "done-stages: 10,15,35" "the done-stage history survived the process restart" "$LOG_DIR/b1-restarted.log"
require_in "35: displayed=true" "objective 35 display survived the process restart" "$LOG_DIR/b1-restarted.log"
# Objective 20 completes at stage 36 (the conditional pair); the re-advanced
# timeline after quickload stopped at 35, so it must be present-but-unkept —
# asserting exactly this catches a restore that silently re-ran fragments.
require_in "20: displayed=false completed=false" \
    "objective 20 untouched by the restore (stage 36 was never re-run)" "$LOG_DIR/b1-restarted.log"

stop_engine
echo "smoke[p5-quest-persistence]: PASS — MS01 objective chain reverts on F9 (stage 15), re-advances live, and survives process restart at stage 35"
echo "artifacts: $LOG_DIR"
