#!/usr/bin/env bash
# M43.1 quest runtime observability smoke — load real Skyrim QUST data,
# exercise the in-engine quest commands through byro-dbg, and pin lifecycle
# plus alias-diagnostic output. Unit tests own exact alias-fill semantics and
# save/load idempotence; this script proves the production ESM → runtime → TCP
# console path with installed game data.

set -euo pipefail

SKYRIM_DATA="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
CELL="${BYROREDUX_QUEST_CELL:-WhiterunBanneredMare}"
# DA10 is a stable Skyrim.esm quest used by the existing quest-stage/save
# regression fixtures. Override for load-order-specific probes.
QUEST_INPUT="${BYROREDUX_QUEST_FORM_ID:-0x00022F08}"
STAGE="${BYROREDUX_QUEST_STAGE:-37}"
ENTITY_FLOOR="${BYROREDUX_ENTITY_FLOOR:-300}"

if [[ ! "$QUEST_INPUT" =~ ^(0[xX][0-9A-Fa-f]+|[0-9]+)$ ]]; then
    echo "smoke[m43-quest-runtime]: FAIL — invalid quest FormID '$QUEST_INPUT'"
    exit 2
fi
printf -v QUEST '0x%08X' "$QUEST_INPUT"

LOG_DIR="$(mktemp -d)"
engine_pid=""
cleanup() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
    rm -rf "$LOG_DIR"
}
trap cleanup EXIT

if [[ ! -f "$SKYRIM_DATA/Skyrim.esm" ]]; then
    echo "smoke[m43-quest-runtime]: SKIP — Skyrim.esm not at $SKYRIM_DATA"
    exit 0
fi
if [[ ! -f "$SKYRIM_DATA/Skyrim - Misc.bsa" ]]; then
    echo "smoke[m43-quest-runtime]: SKIP — Skyrim - Misc.bsa not at $SKYRIM_DATA"
    exit 0
fi

echo "═══════════════════════════════════════════════════════════════"
echo "  smoke[m43-quest-runtime]: cell '$CELL', quest $QUEST"
echo "═══════════════════════════════════════════════════════════════"

engine_stdout="$LOG_DIR/engine.stdout"
engine_stderr="$LOG_DIR/engine.stderr"
debug_log="$LOG_DIR/debug.log"

cargo run --release --quiet -- \
    --esm "$SKYRIM_DATA/Skyrim.esm" \
    --cell "$CELL" \
    --bsa "$SKYRIM_DATA/Skyrim - Meshes0.bsa" \
    --bsa "$SKYRIM_DATA/Skyrim - Meshes1.bsa" \
    --textures-bsa "$SKYRIM_DATA/Skyrim - Textures0.bsa" \
    --scripts-bsa "$SKYRIM_DATA/Skyrim - Misc.bsa" \
    --bench-frames "$BENCH_FRAMES" \
    --bench-hold \
    >"$engine_stdout" 2>"$engine_stderr" &
engine_pid=$!

deadline=$(( $(date +%s) + 180 ))
while ! grep -q '^bench-hold:' "$engine_stderr" 2>/dev/null; do
    if [[ $(date +%s) -gt $deadline ]]; then
        echo "smoke[m43-quest-runtime]: FAIL — timeout waiting for bench-hold"
        tail -20 "$engine_stderr" || true
        exit 1
    fi
    if ! kill -0 "$engine_pid" 2>/dev/null; then
        echo "smoke[m43-quest-runtime]: FAIL — engine exited before bench-hold"
        tail -20 "$engine_stderr" || true
        exit 1
    fi
    sleep 0.5
done

BYRO_DEBUG_PORT="$PORT" cargo run --release --quiet -p byro-dbg <<EOF >"$debug_log" 2>&1
quest.show $QUEST
quest.aliases $QUEST
quest.start $QUEST
quest.setstage $QUEST $STAGE
quest.show $QUEST
quest.stop $QUEST
quest.show $QUEST
quit
EOF

bench_line="$(grep '^bench:' "$engine_stdout" | tail -1 || true)"
if [[ -z "$bench_line" ]]; then
    echo "smoke[m43-quest-runtime]: FAIL — no bench summary"
    exit 1
fi
echo "$bench_line"

entities="$(echo "$bench_line" | grep -oE 'entities=[0-9]+' | head -1 | cut -d= -f2)"
: "${entities:=0}"
if (( entities < ENTITY_FLOOR )); then
    echo "smoke[m43-quest-runtime]: FAIL — entities=$entities < floor $ENTITY_FLOOR"
    exit 1
fi

hard_fail=0
require_output() {
    local pattern="$1"
    local description="$2"
    if grep -Fq "$pattern" "$debug_log"; then
        echo "smoke[m43-quest-runtime]: PASS — $description"
    else
        echo "smoke[m43-quest-runtime]: FAIL — $description (missing '$pattern')"
        hard_fail=1
    fi
}

require_output "Quest $QUEST" "real QUST definition is inspectable"
require_output "Quest aliases $QUEST" "alias diagnostics are available"
require_output "result: started" "quest.start entered the lifecycle"
require_output "result: set stage=$STAGE" "quest.setstage updated the requested stage"
require_output "result: stopped state=stopped" "quest.stop ran the shutdown lifecycle"
require_output "state: stopped" "final quest.show observes stopped canonical state"

if grep -Fq 'Error:' "$debug_log"; then
    echo "smoke[m43-quest-runtime]: FAIL — command output contained an error"
    hard_fail=1
fi

if (( hard_fail != 0 )); then
    echo "── byro-dbg output ─────────────────────────────────────────────"
    cat "$debug_log"
    exit "$hard_fail"
fi

echo "smoke[m43-quest-runtime]: PASS — production QUST lifecycle and diagnostics are live"
