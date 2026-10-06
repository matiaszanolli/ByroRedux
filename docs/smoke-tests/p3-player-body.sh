#!/usr/bin/env bash
# Playable-slice P3 player-body gate: prove the player is no longer a bare
# capsule — the NPC-assembly machinery (player-body variant) attaches a race
# skeleton + skin/outfit meshes under the Character-mode player entity, the
# equip-appearance system owns them, and the first/third-person view toggle
# shows and hides them in a live Vulkan run.
#
#   body:      player.body reports an assembled root with meshes, a skeleton
#              target, NpcEquipmentPart ownership retargeted to the
#              player entity (what equipment_appearance_system matches),
#              and the capsule's walk-clip wiring (anim=walk)
#   view:      player.view third reveals the meshes (hidden_first_person=0)
#              and player.view first hides them again; a third-person
#              screenshot is retained as visual evidence
#   rays:      interaction.status after the attach must still resolve to a
#              world target or none — never the player itself — because the
#              body registers no bone colliders beside the capsule
#
# Skyrim-only today: the reference slice route and its race-skin content.
#
#   docs/smoke-tests/p3-player-body.sh    # needs Skyrim SE data on disk

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture p3-player-body "$@"

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-360}"

LOG_DIR="$(mktemp -d /tmp/byro-p3-player-body.XXXXXX)"
engine_pid=""
keep_artifacts=0
cleanup() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
    # Same orphan-listener trap p3-hud documents: a leaked engine keeps
    # rendering and outruns byro-dbg's response window for later smokes.
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
    echo "smoke[p3-player-body]: FAIL -- $*"
    echo "smoke[p3-player-body]: artifacts retained at $LOG_DIR"
    tail -40 "$LOG_DIR/engine.stderr" 2>/dev/null || true
    exit 1
}

smoke_require_data

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[p3-player-body]: building release binaries"
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
            echo "smoke[p3-player-body]: PASS -- $description"
            return 0
        fi
        kill -0 "$engine_pid" 2>/dev/null || break
        sleep 2
    done
    fail "$description (engine log never matched '$pattern')"
}

echo "smoke[p3-player-body]: $FIXTURE_LABEL -- player body attach + view toggle"

# P1 spawn pose: Character mode at the Bannered Mare threshold (the p3-hud
# gate's pose). --player forces the capsule; the body attach hangs off it.
# BYRO_DEBUG_SERVER=1 is the release binary's explicit debug-server opt-in
# (63c0aee3b): without it the held engine binds no port and byro-dbg can
# never attach. RUST_LOG must keep byroredux at info — the attach-log wait
# below greps a log::info! line, which a bare "error" filter would hide.
cd "$SMOKE_DATA"
env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    RUST_LOG="error,byroredux=info" \
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
# The attach log line is the first structural witness: the job assembled the
# player's body off NPC_ 0x7 and the post-pass parented it.
wait_for_engine_log "Player body: assembled from NPC_ 00000007" \
    "player body assembly completed at spawn"

# Body: assembled, meshed, skeleton-targeted, parts owned by the player.
body="$(debug_command "player.body")"
echo "$body" >"$LOG_DIR/player-body.txt"
grep -Fq "not attached" <<<"$body" && fail "player.body reports no attached body"
mesh_count="$(grep -oP 'meshes=\K[0-9]+' <<<"$body" | head -1)"
[[ -n "$mesh_count" && "$mesh_count" -gt 0 ]] \
    || fail "player.body reports no body meshes (got: $body)"
grep -Fq "skeleton=none" <<<"$body" \
    && fail "player.body reports no skeleton target (got: $body)"
grep -Eq "parts=\[.+[0-9A-F]{8}" <<<"$body" \
    || fail "player.body reports no NpcEquipmentPart ownership stamps (got: $body)"
grep -Fq "hidden_first_person=$mesh_count" <<<"$body" \
    || fail "the body must start hidden in first person (got: $body)"
# #5095 — vanilla ships no player facegeom, so the head comes from the
# pre-baked PNAM head-part fallback. A headless third-person body must
# fail the gate, not pass it.
head_count="$(grep -oP 'head_parts=\K[0-9]+' <<<"$body" | head -1)"
[[ -n "$head_count" && "$head_count" -gt 0 ]] \
    || fail "player.body reports no head-part meshes — the pre-baked FaceGen \
miss left the player headless (got: $body)"
# Third-person locomotion: the capsule must carry the walk clip (idle too on
# KF games — Skyrim's standing shape is walk-only), or the body moves rigid.
grep -Fq "anim=walk(" <<<"$body" \
    || fail "player.body reports no walk clip on the capsule — third-person \
locomotion would be rigid (got: $body)"

status="$(debug_command "player.status")"
player_id="$(grep -o 'player=[0-9]*' <<<"$status" | head -1 | cut -d= -f2)"
[[ -n "$player_id" ]] || fail "player.status did not report a player entity"

# Rays: the attach must not have given the player extra physics presence.
# Whatever the interaction ray resolves in the mare interior (an NPC, a
# door, or nothing), it must never be the player's own entity.
interaction="$(debug_command "interaction.status")"
echo "$interaction" >"$LOG_DIR/interaction.txt"
if grep -Eq "target=$player_id\b" "$LOG_DIR/interaction.txt"; then
    fail "interaction target resolved to the player itself — the body registered colliders"
fi

capture_screenshot() {
    # The debug screenshot command rejects paths outside the engine's
    # `screenshots/` dir ("screenshot path must be a filename inside
    # screenshots/"), so capture by bare filename in the engine's cwd
    # (SMOKE_DATA) and move the file into the retained log dir.
    local name="$1" attempt
    local shot_dir="$SMOKE_DATA/screenshots"
    mkdir -p "$shot_dir"
    rm -f "$shot_dir/$name"
    for attempt in 1 2 3 4 5; do
        debug_command "screenshot $name" >/dev/null 2>&1 || true
        sleep 2
        if [[ -s "$shot_dir/$name" ]]; then
            mv "$shot_dir/$name" "$LOG_DIR/$name"
            return 0
        fi
    done
    return 1
}

# Third person: reveal + capture.
debug_command "player.view third" >"$LOG_DIR/view-third.txt"
grep -Fq "ThirdPerson" "$LOG_DIR/view-third.txt" \
    || fail "player.view third did not switch (got: $(cat "$LOG_DIR/view-third.txt"))"
body="$(debug_command "player.body")"
grep -Fq "hidden_first_person=0 view=ThirdPerson" <<<"$body" \
    || fail "third person did not reveal the body meshes (got: $body)"
sleep 3
capture_screenshot "player-third-person.png" \
    || fail "third-person capture was not written"

# First person: hide again.
debug_command "player.view first" >"$LOG_DIR/view-first.txt"
grep -Fq "FirstPerson" "$LOG_DIR/view-first.txt" \
    || fail "player.view first did not switch back (got: $(cat "$LOG_DIR/view-first.txt"))"
body="$(debug_command "player.body")"
grep -Fq "hidden_first_person=$mesh_count view=FirstPerson" <<<"$body" \
    || fail "first person did not re-hide the body meshes (got: $body)"

echo "smoke[p3-player-body]: PASS -- body attached, view toggle verified, no self-targeting"
