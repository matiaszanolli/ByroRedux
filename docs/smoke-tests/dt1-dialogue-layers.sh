#!/usr/bin/env bash
# DT1 gate (#5367 Phases G + F) — dialogue greeting + force-greet, live.
#
#   1. Boot the FNV profile into Doc Mitchell's house with a player.
#   2. Phase G: activate Doc Mitchell (bound to no *running* quest at
#      boot) — pre-#5367 this selected nothing; now the master's
#      generic `GREETING` topic must open the conversation.
#   3. Phase F: install the authored Dialogue-procedure PACK
#      0x000613BB (`CitGunnyGreetPlayer`, PKDD topic 0x000000C8) via
#      `dialogue.forcegreet` — the NPC walks to the player and opens
#      the conversation with the package's topic WITHOUT any
#      activation, through the same selection path.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19878}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-dt1.XXXXXX)"
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
    echo "smoke[dt1-dialogue-layers]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5429 — the layers are documented "FO3/FNV" but only FNV has a measured live
# route (Doc Mitchell's house + PACK 0x000613BB). FO3 is a declared SKIP, never
# a pass: its data premises (GREETING/HELLO topics, Dialogue PACKs, per-INFO
# QSTI) are pinned by the ignored `dialogue_greeting_and_forcegreet_fo3_floor`
# real-data test in crates/plugin/tests/parse_real_esm.rs, and its voice archive
# is not mounted by any profile (#5433), so Phase V cannot run live either.
# A measured FO3 cell/NPC/PACK pin is the open work.
case "${1:-fnv}" in
    fnv) ;;
    fo3)
        echo "smoke[dt1-dialogue-layers]: SKIP -- no measured FO3 live route (#5429); \
data premises are gated by dialogue_greeting_and_forcegreet_fo3_floor"
        exit 77
        ;;
    *)
        echo "smoke[dt1-dialogue-layers]: FAIL -- unknown game '$1' (fnv|fo3)" >&2
        exit 2
        ;;
esac

# #5427 — the SKIP≠PASS contract every per-game gate honours: a runner
# without the game's data measures nothing (exit 77, never a pass),
# before any port or build work.
DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
for f in "FalloutNV.esm" "Fallout - Meshes.bsa" "Fallout - Textures.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[dt1-dialogue-layers]: SKIP -- missing $DATA/$f"
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
    RUST_LOG="warn,byroredux::asset_provider=info,byroredux::systems::dialogue_voice=info" \
    target/release/byroredux \
    --game fnv --cell GSDocMitchellHouse \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!
wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
# Let the quest/dialogue systems take their first ticks (Start Game
# Enabled quests install, conditions evaluate against the started
# clock) before activating — activating on the load frame races the
# first selection pass.
sleep 2

# ── Phase G: the quest-less activation greets ─────────────────────
debug_command 'find("DocMitchell")' "$LOG_DIR/find.log" || fail 'find unavailable'
entity="$(grep -oE 'Entity [0-9]+' "$LOG_DIR/find.log" | head -1 | cut -d' ' -f2 || true)"
[[ -n "$entity" ]] || fail "Doc Mitchell entity not found in the loaded cell"
echo "smoke[dt1-dialogue-layers]: DocMitchell entity=$entity"

debug_command "script.activate $entity" "$LOG_DIR/activate.log" || fail 'activation unavailable'
# Doc Mitchell's authored `onActivate` ObScript consumes the activation
# and re-queues a native one for the dialogue consumer, so the greeting
# lands a few frames after `script.activate` returns (#6055). A
# `dialogue.status` read in the same debug drain races it; poll instead.
deadline=$((SECONDS + TIMEOUT))
: > "$LOG_DIR/greet.log"
until grep -Fq "topic 0x0000C8" "$LOG_DIR/greet.log" 2>/dev/null; do
    (( SECONDS < deadline )) || break
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited during activation"
    sleep 0.5
    debug_command 'dialogue.status' "$LOG_DIR/greet.log" >/dev/null 2>&1 || true
done
grep -Fq "topic 0x0000C8" "$LOG_DIR/greet.log" \
    || fail "Phase G: activation did not open the generic GREETING topic: $(sed -n '1,8p' "$LOG_DIR/greet.log")"
grep -Fq 'npc '"$entity" "$LOG_DIR/greet.log" \
    || fail "Phase G: no selection stamped on Doc Mitchell"
echo "smoke[dt1-dialogue-layers]: Phase G PASS — quest-less activation greets"

# ── Phase V: the greeted line carries its authored voice ──────────
# The FNV profile opens `Fallout - Voices1.bsa` through the sounds
# list (the archive this phase made a consumer for); the presented
# greeting resolves `sound\voice\falloutnv.esm\maleuniquedocmitchell\
# vcg01_greeting_00107222_1.ogg` (quest `VCG01` + topic `GREETING`
# EDIDs — INFO records carry none), decodes, and schedules at the
# NPC. The resolution+decode+request path is device-independent (the
# audio queue counts a request even without an output device).
wait_log "$LOG_DIR/session.stderr" "voice type 'maleuniquedocmitchell', plugin falloutnv.esm"
echo "smoke[dt1-dialogue-layers]: Phase V PASS — greeting voice resolved and scheduled"

# ── Phase F: the force-greet package opens without activation ─────
# End the G conversation first so the F assertion reads a fresh serial.
sleep 2
debug_command "dialogue.forcegreet $entity 0x000613BB" "$LOG_DIR/install.log" \
    || fail 'force-greet install unavailable'
grep -Fq 'force-greet installed' "$LOG_DIR/install.log" \
    || fail "force-greet install rejected: $(cat "$LOG_DIR/install.log")"

# The NPC walks to the player (KCC-stepped, walk speed) and the
# conversation opens with the package's topic. Poll for it.
deadline=$((SECONDS + TIMEOUT))
until grep -Fq "topic 0x0000C8" "$LOG_DIR/fg.status" 2>/dev/null; do
    (( SECONDS < deadline )) || fail "Phase F: the force-greet never opened the conversation"
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited during force-greet"
    sleep 1
    debug_command 'dialogue.status' "$LOG_DIR/fg.status" >/dev/null 2>&1 || true
done
echo "smoke[dt1-dialogue-layers]: Phase F PASS — PACK-driven conversation opened without activation"

stop_engine
echo "smoke[dt1-dialogue-layers]: PASS — G + F live; artifacts: $LOG_DIR"
