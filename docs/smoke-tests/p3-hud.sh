#!/usr/bin/env bash
# Playable-slice P3 HUD gate: prove the native vitals bars and quest
# objective text render in a live Vulkan run, driven through canonical
# state only.
#
#   vitals: the player body's ActorValues, keyed by the PlayerVitals
#           labels resolved from the master's AVIF table (bottom-left bars)
#   objective: a real QUST stage fragment's SetObjectiveDisplayed effect,
#           dispatched by the canonical `quest.setstage` command and drawn
#           as the top-left objective line
#
# The gate is pixel-deterministic: it scans the captures for the exact
# bar-fill and title colors `crates/debug-ui/src/panels.rs` paints, so a
# washed-out or misplaced HUD fails rather than passes. Skyrim-only today:
# the vitals vocabulary and the MS01 stage-15 fragment are Skyrim content,
# and the fixture system carries the data paths.
#
#   docs/smoke-tests/p3-hud.sh            # needs Skyrim SE data on disk

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture p3-hud "$@"

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-360}"

# MS01 "The Forsworn Conspiracy" (0x00018B4B): stage 15's fragment lowers to
# SetObjectiveDisplayed(objective 10) — derived with
# `cargo run -p byroredux-scripting --example dump_stage_fragment_effects`
# against the installed master, not guessed. Stage 10 does NOT display an
# objective (it only pokes the MS01MiscObjectives helper quest), which the
# first capture asserts the hard way.
P3_QUEST=0x18B4B
P3_QUEST_STAGE=15
P3_QUEST_OBJECTIVE_LINE='10: displayed=true'

LOG_DIR="$(mktemp -d /tmp/byro-p3-hud.XXXXXX)"
engine_pid=""
keep_artifacts=0
cleanup() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
    # `kill $engine_pid` hits the xvfb-run wrapper, not the engine child —
    # a leaked engine keeps rendering on the GPU and its frame-time tail
    # then outruns byro-dbg's ~5 s response window for every later smoke
    # on the machine (m48-6's comment documents the same trap). Make sure
    # the listener on $PORT is gone too.
    local orphan
    orphan=$(ss -tlnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1)
    [[ -z "$orphan" ]] || kill "$orphan" 2>/dev/null || true
    if (( keep_artifacts == 0 )); then
        rm -rf "$LOG_DIR"
    fi
}
trap cleanup EXIT INT TERM

fail() {
    keep_artifacts=1
    echo "smoke[p3-hud]: FAIL -- $*"
    echo "smoke[p3-hud]: artifacts retained at $LOG_DIR"
    tail -40 "$LOG_DIR/engine.stderr" 2>/dev/null || true
    exit 1
}

smoke_require_data

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[p3-hud]: building release binaries"
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

debug_command() {
    local command="$1"
    env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" <<EOF
$command
.quit
EOF
}

wait_for_engine_log() {
    local pattern="$1" description="$2"
    local deadline=$(( $(date +%s) + TIMEOUT ))
    while (( $(date +%s) < deadline )); do
        if grep -Fq "$pattern" "$LOG_DIR/engine.stderr" 2>/dev/null; then
            echo "smoke[p3-hud]: PASS -- $description"
            return 0
        fi
        kill -0 "$engine_pid" 2>/dev/null || break
        sleep 2
    done
    fail "$description (engine log never matched '$pattern')"
}

echo "smoke[p3-hud]: $FIXTURE_LABEL -- native vitals + objective HUD"

# The P1 spawn pose: character mode at the Bannered Mare threshold. The
# camera override fixes the capsule's XZ column (see plan_character_spawn).
# Launch from the game data dir (like the m48 HUD smokes) so the engine's
# exit-time default save lands beside the game files instead of in the repo.
cd "$SMOKE_DATA"
env BYRO_DEBUG_PORT="$PORT" \
    RUST_LOG="error" \
    xvfb-run -a "$ENGINE_BIN" \
    "${SMOKE_ENGINE_ARGS[@]}" \
    --cell WhiterunBanneredMare \
    --player \
    --camera-pos -116,226,982 \
    --camera-forward 0,-0.29,0.96 \
    --radius 1 \
    --bench-frames 30 \
    --bench-hold \
    >"$LOG_DIR/engine.stdout" 2>"$LOG_DIR/engine.stderr" &
engine_pid=$!

wait_for_engine_log "bench-hold:" "engine reached the held interactive state"

status="$(debug_command "player.status")"
grep -Fq "mode=Character" <<<"$status" || fail "Character mode is not active"
player_id="$(grep -o 'player=[0-9]*' <<<"$status" | head -1 | cut -d= -f2)"
[[ -n "$player_id" ]] || fail "player.status did not report a player entity"
# The vitals bars are unreachable without ActorValues on the body; modav is
# also the probe that proved the pre-fix HUD gap was a probe artifact.
debug_command "modav $player_id 0x3E8 5" >"$LOG_DIR/modav.txt"
grep -Fq "modav" "$LOG_DIR/modav.txt" || fail "modav probe did not answer"

# Capture 1 — bars present, no objective yet (the quest is untouched).
# The screenshot round-trip (request → present → PNG encode → response) can
# miss byro-dbg's per-command response window while the engine is settling
# streaming/texture work, so retry until the file lands — the same loop
# shape p1's wait_for_debug_pattern uses.
sleep 3
capture_screenshot() {
    local path="$1" attempt
    for attempt in 1 2 3 4 5; do
        debug_command "screenshot $path" >/dev/null 2>&1 || true
        sleep 2
        if [[ -s "$path" ]]; then
            return 0
        fi
    done
    return 1
}
capture_screenshot "$LOG_DIR/hud-vitals.png" \
    || fail "vitals capture was not written"

# Advance MS01 through the canonical fragment path; its stage-15 fragment
# displays objective 10.
setstage="$(debug_command "quest.setstage $P3_QUEST $P3_QUEST_STAGE")"
grep -Fq "result: set stage=$P3_QUEST_STAGE" <<<"$setstage" \
    || fail "quest.setstage did not advance MS01 (got: $setstage)"
sleep 2
quest="$(debug_command "quest.show $P3_QUEST")"
grep -Fq "$P3_QUEST_OBJECTIVE_LINE" <<<"$quest" \
    || fail "stage $P3_QUEST_STAGE fragment did not display objective 10"

# Capture 2 — bars + the new objective line.
capture_screenshot "$LOG_DIR/hud-full.png" \
    || fail "full HUD capture was not written"

# Pixel verification — the exact colors panels.rs paints. python3 + PIL are
# also used by the shader-parity tooling; absence is a smoke-environment
# failure, not a SKIP.
python3 - "$LOG_DIR" <<'EOF' || fail "HUD pixel verification did not pass"
import collections
import sys
from PIL import Image

log = sys.argv[1]

def hud_counts(path):
    img = Image.open(path).convert("RGB")
    w, h = img.size
    # vitals anchor bottom-left (18 px margin, ~240x72 block); objective
    # titles anchor top-left.
    bottom_left = collections.Counter(img.crop((0, h - 140, 420, h)).getdata())
    top_left = collections.Counter(img.crop((0, 0, 700, 220)).getdata())
    def near(counter, rgb, tol=4):
        return sum(
            count
            for (r, g, b), count in counter.items()
            if abs(r - rgb[0]) <= tol and abs(g - rgb[1]) <= tol and abs(b - rgb[2]) <= tol
        )
    return {
        "health": near(bottom_left, (198, 58, 48)),
        "magicka": near(bottom_left, (72, 110, 220)),
        "stamina": near(bottom_left, (74, 168, 88)),
        "objective": near(top_left, (235, 220, 160)),
    }

bars = hud_counts(f"{log}/hud-vitals.png")
full = hud_counts(f"{log}/hud-full.png")

for key in ("health", "magicka", "stamina"):
    assert bars[key] > 200, f"vitals capture is missing the {key} bar fill: {bars}"
assert bars["objective"] == 0, f"objective line appeared before the quest advanced: {bars}"
assert full["objective"] > 0, f"objective line did not appear after quest.setstage: {full}"
for key in ("health", "magicka", "stamina"):
    assert full[key] > 200, f"full capture lost the {key} bar: {full}"
print("hud pixel verification: bars present; objective appears only after the stage fragment")
EOF

echo "smoke[p3-hud]: PASS -- vitals bars live from ActorValues; objective text live from the MS01 stage-15 fragment"
