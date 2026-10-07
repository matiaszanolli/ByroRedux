#!/usr/bin/env bash
# SM1 gate (#5366 Phase 1) — Story Manager event dispatch, live.
#
#   1. Boot the Skyrim SE profile into WhiterunDragonsreach with a
#      player (`--player` installs `PlayerEntity`; without it the CLOC
#      producer correctly declines to fire).
#   2. The initial interior load installs `CurrentCellContext`
#      (`cell_loader/load.rs`), the CLOC producer fires once on the
#      None → Some location-key change, and the dispatcher walks the
#      CLOC subtree in the same frame.
#   3. `CRLocationExpansionNode` (SMQN 0x000F9076) is the one fully
#      condition-free chain under Skyrim's CLOC event node (corpus
#      census, 2026-10-07), so it must start `CRHoldExpansion`
#      (QUST 0x000F9075, DNAM flags 0x0) through the canonical
#      `QuestStageState` lifecycle.
#
# Attribution is airtight: that quest cannot start via any other engine
# path (not Start Game Enabled, not the MQ101 engine root, no script
# attachment at boot), and the dispatcher's #5366 log line names the
# node and event.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19876}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-sm1.XXXXXX)"
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
    echo "smoke[sm1-story-manager]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

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
    RUST_LOG="warn,byroredux_scripting=info,byroredux::asset_provider=info" \
    target/release/byroredux \
    --game skyrim_se --cell WhiterunDragonsReach \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!

# The install log proves the tree made it into the session (Skyrim.esm
# alone authors 571 nodes; DLC masters are not on this load order).
wait_log "$LOG_DIR/session.stderr" 'Installed Story Manager tree: 571 nodes'

# The dispatcher's own attribution line — node, quest, event.
wait_log "$LOG_DIR/session.stderr" "started quest 0x000F9075 ('CRHoldExpansion') via node 'CRLocationExpansionNode' on 'CLOC' event"

# And the authoritative runtime state, read back through the debug CLI.
debug_command 'quest.show 0x000F9075' "$LOG_DIR/quest.log" \
    || fail 'quest.show unavailable'
grep -Fq 'Quest 0x000F9075' "$LOG_DIR/quest.log" \
    || fail 'quest.show did not report CRHoldExpansion'
grep -Fq 'state: running' "$LOG_DIR/quest.log" \
    || fail "CRHoldExpansion is not running: $(sed -n '1,6p' "$LOG_DIR/quest.log")"

stop_engine
echo "smoke[sm1-story-manager]: PASS -- CLOC dispatched, CRLocationExpansionNode"
echo "started CRHoldExpansion (0x000F9075) through QuestStageState; artifacts: $LOG_DIR"
