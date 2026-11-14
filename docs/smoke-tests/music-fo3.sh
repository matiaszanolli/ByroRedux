#!/usr/bin/env bash
# Game Music, PRD §7.2 scenarios 2 to 5 (#6881): the music controller on Fallout 3, headless.
#
#   2. MegatonCratersideSupply (DefaultPublic), RedRacer03 (DefaultDungeon) and
#      MegatonWaterProcessingPlant (1NoMusic) report Location Playing from public\, Playing from
#      dungeon\, and Silent.
#      In RedRacer03, PlayMusic Tension (0x11ff) replaces the dungeon music and PlayMusic 1NoMusic
#      silences it (#6899).
#   3. --grid 0,0 in Wasteland (no cell XCMO, no worldspace ZNAM) reports the Default Music object
#      DefaultExplore Playing from explore\.
#   5. In MegatonCratersideSupply, `obscript.cmd none 1114 t:B01.bik 1 1 1 1` raises the Hold layer and
#      the Location track keeps its position under it; after video.skip it plays on from there.
#   4. StartCombat (0x1016) on the Craterside merc switches to Battle; KillActor (0x108b) on it returns
#      to Location after the recovery time; `levelup xp` and `obscript.cmd 0x14 108b` raise the level-up
#      and death stings.
#
#   docs/smoke-tests/music-fo3.sh

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture music-fo3 fo3
smoke_require_data
[[ -d "$SMOKE_DATA/Music" ]] || { echo "smoke[music-fo3]: SKIP -- no Data/Music"; exit 77; }

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"
# The Craterside merc (MegatonSettlerMercCraterside), the cell's second actor.
MERC=0x0001FF18

LOG_DIR="$(mktemp -d /tmp/byro-music-fo3.XXXXXX)"
engine_pid=""
keep_artifacts="${BYROREDUX_KEEP_ARTIFACTS:-0}"
stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
        engine_pid=""
    fi
    local orphan
    orphan=$(ss -tlnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1 || true)
    [[ -z "$orphan" ]] || kill "$orphan" 2>/dev/null || true
    for _ in $(seq 1 20); do ss -tln 2>/dev/null | grep -q ":$PORT " || break; sleep 0.5; done
}
cleanup() {
    stop_engine
    (( keep_artifacts == 0 )) && rm -rf "$LOG_DIR"
    return 0
}
trap cleanup EXIT INT TERM

fail() {
    keep_artifacts=1
    echo "smoke[music-fo3]: FAIL -- $*"
    echo "smoke[music-fo3]: artifacts retained at $LOG_DIR"
    grep 'music' "$LOG_DIR"/engine.*.stderr 2>/dev/null | tail -10 || true
    exit 1
}
pass() { echo "smoke[music-fo3]: PASS -- $*"; }

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[music-fo3]: building release binaries"
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

ask() {
    { printf '%s\n' "$@"; echo .quit; } | env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$LOG_DIR/ask.tmp" 2>&1
    sed 's/\\n/\n/g; s/\\"/"/g; s/\\\\/\\/g' "$LOG_DIR/ask.tmp"
}
layer() { ask music.status | grep -o "^$1: .*" | head -1; }
position() { grep -oE 'at [0-9]+\.[0-9]s' <<<"$1" | head -1 | grep -oE '[0-9]+\.[0-9]'; }
wait_layer() {
    local layer="$1" seconds="$2" description="$3" pattern="$4" deadline=$((SECONDS + $2))
    until [[ "$(layer "$layer")" =~ $pattern ]]; do
        kill -0 "$engine_pid" 2>/dev/null || fail "the engine exited waiting for: $description"
        (( SECONDS < deadline )) || fail "timeout (${seconds}s) waiting for: $description ($(layer "$layer"))"
        sleep 1
    done
}

# start_engine <label> <engine args...>: launch headless and wait for the bench hold.
start_engine() {
    local label="$1"
    shift
    stop_engine
    (
        cd "$SMOKE_DATA"
        exec env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
            BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
            BYROREDUX_SAVE_DIR="$LOG_DIR/saves" \
            RUST_LOG="warn,byroredux::music=info" \
            "${SMOKE_HEADLESS[@]}" "$ENGINE_BIN" "${SMOKE_ENGINE_ARGS[@]}" "$@" --bench-frames 30 --bench-hold
    ) >"$LOG_DIR/engine.$label.stdout" 2>"$LOG_DIR/engine.$label.stderr" &
    engine_pid=$!
    local deadline=$((SECONDS + 300))
    until grep -q 'bench-hold:' "$LOG_DIR/engine.$label.stderr" 2>/dev/null; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold ($label)"
        (( SECONDS < deadline )) || fail "timeout waiting for bench-hold ($label)"
        sleep 0.5
    done
}

# ── 2 — cell music types ─────────────────────────────────────────────────────
start_engine dungeon --cell RedRacer03
wait_layer Location 20 "RedRacer03's music" '^Location: Playing — DefaultDungeon .*cell XCMO.* music\\dungeon\\'
pass "RedRacer03: $(layer Location)"

# ── PlayMusic replaces the music (MUS-SCR-005, #6899) ────────────────────────
ask "obscript.cmd none 11ff t:Tension" >/dev/null
wait_layer Script 10 "PlayMusic Tension" '^Script: Playing — Tension .*PlayMusic.* music\\tension\\'
[[ "$(layer Location)" == *"paused under Script"* ]] || fail "PlayMusic did not replace the cell's music ($(layer Location))"
pass "PlayMusic Tension replaces the cell's music: $(layer Script)"
ask "obscript.cmd none 11ff t:1NoMusic" >/dev/null
wait_layer Script 10 "PlayMusic 1NoMusic" '^Script: Silent — 1NoMusic'
[[ "$(layer Location)" == *"paused under Script"* ]] || fail "PlayMusic 1NoMusic did not silence the cell's music ($(layer Location))"
pass "PlayMusic 1NoMusic silences it"

start_engine silent --cell MegatonWaterProcessingPlant
wait_layer Location 20 "MegatonWaterProcessingPlant's music" '^Location: Silent — 1NoMusic .*cell XCMO'
pass "MegatonWaterProcessingPlant: $(layer Location)"

# ── 3 — the Default Music object outdoors ────────────────────────────────────
start_engine exterior --wrld Wasteland --grid 0,0 --radius 1
wait_layer Location 20 "the Wasteland's default music" '^Location: Playing — DefaultExplore .*DOBJ Default Music.* music\\explore\\'
pass "Wasteland (0,0): $(layer Location)"

start_engine public --cell MegatonCratersideSupply
wait_layer Location 20 "MegatonCratersideSupply's music" '^Location: Playing — DefaultPublic .*cell XCMO.* music\\public\\'
pass "MegatonCratersideSupply: $(layer Location)"

# ── 5 — the movie hold ───────────────────────────────────────────────────────
ask "obscript.cmd none 1114 t:B01.bik 1 1 1 1" >/dev/null
wait_layer Location 30 "the Location layer paused under Hold" '^Location: Playing \(paused under Hold\)'
ask music.status | grep -q "Hold: on" || fail "the Hold layer is not reported"
sleep 1
held=$(position "$(layer Location)")
sleep 2
again=$(position "$(layer Location)")
[[ "$held" == "$again" ]] || fail "the Location track moved under the Hold layer ($held -> $again)"
pass "B01.bik holds the music: Location paused at ${held}s"
ask video.skip >/dev/null
wait_layer Location 15 "the Location layer resuming" '^Location: Playing — '
resumed=$(position "$(layer Location)")
awk -v a="$resumed" -v b="$held" 'BEGIN { exit !(a >= b && a < b + 5) }' \
    || fail "the Location track did not resume where it was (${held}s -> ${resumed}s)"
pass "after video.skip the Location track plays on from ${held}s (${resumed}s)"

# ── 4 — battle and stings ────────────────────────────────────────────────────
ask "obscript.cmd $MERC 1016 r:00000014" >/dev/null
wait_layer Battle 15 "battle music" '^Battle: Playing — DefaultBattle .*DOBJ Battle Music.* music\\battle\\'
[[ "$(layer Location)" == *"paused under Battle"* ]] || fail "Location is not paused under Battle ($(layer Location))"
pass "StartCombat: $(layer Battle)"
ask "obscript.cmd $MERC 108b" >/dev/null
wait_layer Battle 20 "the recovery time to pass" '^Battle: none'
wait_layer Location 5 "Location to resume" '^Location: Playing — DefaultPublic'
pass "KillActor: Battle held for its recovery, then Location resumed"
ask "levelup xp 2000" >/dev/null
wait_layer Sting 10 "the level-up sting" '^Sting: Playing — DefaultSuccess .*DOBJ Level Up Music.* music\\special\\success\.mp3'
pass "levelup: $(layer Sting)"
ask "obscript.cmd 0x14 108b" >/dev/null
wait_layer Sting 10 "the death sting" '^Sting: Playing — DefaultDeath .*DOBJ Death Music.* music\\special\\death\.mp3'
pass "the player's death: $(layer Sting)"

echo "smoke[music-fo3]: PASS -- Fallout 3 is scored: cell, default, battle, sting and movie hold"
