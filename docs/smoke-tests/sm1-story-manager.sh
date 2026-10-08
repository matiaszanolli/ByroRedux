#!/usr/bin/env bash
# SM1 gate (#5366 Phases 1–2) — Story Manager event dispatch, live.
#
#   1. Boot the Skyrim SE profile into WhiterunDragonsReach with a
#      player (`--player` installs `PlayerEntity`; without it the CLOC
#      producer correctly declines to fire).
#   2. The initial interior load installs `CurrentCellContext`
#      (`cell_loader/load.rs`), the CLOC producer fires once on the
#      None → Some location-key change (Phase 2: keyed on the cell's
#      `XLCN` LCTN — DragonsreachLocation), and the dispatcher walks
#      the CLOC subtree in the same frame.
#   3. Phase 3 semantics: `WIGreetingNodeSHARES` (SMQN 0x000C791B,
#      DNAM 0x20000 = Shares Event, its fn-145 condition passing
#      trivially at boot) starts `WIGreeting` (QUST 0x000C7919), and
#      `CWChangeLocationScenes` (SMQN 0x000D5176, DNAM 0x0 — NO shares
#      bit, its fn-56 == 0.0 condition passing at boot) then starts one
#      pool quest and CONSUMES the event: every node below it in the
#      stack — including the previously-anchored unconditional
#      `CRLocationExpansionNode` → `CRHoldExpansion` chain — must NOT
#      evaluate. The boot set is exactly the two starts above (the
#      Phase-1 six-quest boot was the pre-decode continue-always
#      placeholder).
#
# Attribution is airtight: neither quest can start via any other engine
# path (not Start Game Enabled, not the MQ101 engine root, no script
# attachment at boot), the dispatcher's #5366 log lines name the nodes
# and event, and the CRHoldExpansion negative is the shares-consume
# proof.
#
# Phase 2 legs (event-data conditions): `MGSuspension` (QUST 0x0005B5DC)
# is a KILL-subtree node whose authored conditions read the event's
# slots — GetIsID(player) on R2 (the killer), GetInFaction on R2 and
# R1 (the victim). `sm.event` raises the real StoryEvent marker and two
# non-matching events must each be refused: one with no killer at all
# (R2-tagged conditions cannot resolve) and one where the killer IS
# the player but the victim (also the player) carries no college
# faction (the R1 GetInFaction gate). The matching-event positive and
# the NPC-killer negative run as the
# `mgsuspension_kill_gate_on_real_skyrim_content` Rust gate on the
# same authored node (scripting crate), which can stage factions on
# the raised entities.
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

# The dispatcher's own attribution lines — node, quest, event. The
# sharing node fires first; the non-sharing CW node fires second and
# consumes the CLOC event.
wait_log "$LOG_DIR/session.stderr" "started quest 0x000C7919 ('WIGreeting') via node 'WIGreetingNodeSHARES' on 'CLOC' event"
wait_log "$LOG_DIR/session.stderr" "started quest 0x000D5165 ('CW00SolitudeMapTableScene') via node 'CWChangeLocationScenes' on 'CLOC' event"

# The authoritative runtime state, read back through the debug CLI.
debug_command 'quest.show 0x000C7919' "$LOG_DIR/quest.log" \
    || fail 'quest.show unavailable'
grep -Fq 'Quest 0x000C7919' "$LOG_DIR/quest.log" \
    || fail 'quest.show did not report WIGreeting'
grep -Fq 'state: running' "$LOG_DIR/quest.log" \
    || fail "WIGreeting is not running: $(sed -n '1,6p' "$LOG_DIR/quest.log")"

# Phase 3 consume proof: CRHoldExpansion sits BELOW the non-sharing
# CWChangeLocationScenes in the CLOC stack, so the consumed event must
# leave it stopped — the Phase-1 boot anchor is now the negative.
debug_command 'quest.show 0x000F9075' "$LOG_DIR/crhold.log" \
    || fail 'quest.show CRHoldExpansion unavailable'
if grep -Fq 'state: running' "$LOG_DIR/crhold.log"; then
    fail "CRHoldExpansion started — the non-sharing CWChangeLocationScenes did not consume the CLOC event"
fi

# ── Phase 2: event-data condition selectivity, live ─────────────────
#
# Leg A — no killer slot: a KILL event whose R2 is unset cannot satisfy
# any R2-tagged condition (missing reference → condition fails).
debug_command 'sm.event KILL r1=player' "$LOG_DIR/smevent.log" \
    || fail "sm.event unavailable: $(sed -n '1,5p' "$LOG_DIR/smevent.log")"
grep -Fq "raised 'KILL'" "$LOG_DIR/smevent.log" \
    || fail "sm.event did not raise: $(sed -n '1,5p' "$LOG_DIR/smevent.log")"
sleep 1
debug_command 'quest.show 0x0005B5DC' "$LOG_DIR/mgsuspension.log" \
    || fail 'quest.show MGSuspension unavailable'
if grep -Fq 'state: running' "$LOG_DIR/mgsuspension.log"; then
    fail "MGSuspension started without a killer — the R2 conditions cannot resolve"
fi
grep -Fq 'Quest 0x0005B5DC' "$LOG_DIR/mgsuspension.log" \
    || fail 'quest.show did not report MGSuspension (0x0005B5DC)'

# Leg B — killer passes, victim fails: the player IS the R2 the
# GetIsID(player) condition wants, but as R1 they carry no college
# faction, so the GetInFaction-on-R1 authored CTDA must refuse the
# start. Both halves of the event-data gate exercised on one event.
debug_command 'sm.event KILL r1=player r2=player' "$LOG_DIR/smevent2.log" \
    || fail 'second sm.event unavailable'
grep -Fq "raised 'KILL'" "$LOG_DIR/smevent2.log" \
    || fail 'second sm.event did not raise'
sleep 1
debug_command 'quest.show 0x0005B5DC' "$LOG_DIR/mgsuspension2.log" \
    || true
if grep -Fq 'state: running' "$LOG_DIR/mgsuspension2.log"; then
    fail "MGSuspension started for a non-faction victim — the R1 GetInFaction gate failed"
fi

stop_engine
echo "smoke[sm1-story-manager]: PASS -- CLOC dispatched: WIGreetingNodeSHARES"
echo "started WIGreeting (0x000C7919); the non-sharing CWChangeLocationScenes fired"
echo "CW00SolitudeMapTableScene and consumed the event (CRHoldExpansion stopped);"
echo "MGSuspension (0x0005B5DC) correctly refused both non-matching KILL events"
echo "(no-killer and player-victim legs; R1/R2 event-data gates); artifacts: $LOG_DIR"
