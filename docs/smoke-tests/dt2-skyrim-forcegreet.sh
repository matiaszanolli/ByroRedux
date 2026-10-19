#!/usr/bin/env bash
# DT2 gate (#5367 Skyrim force-greet) — the procedure-tree PACK dialect,
# live.
#
#   1. Boot the Skyrim SE profile into WhiterunDragonsReach with a
#      player.
#   2. The authored-topic arm: install OrcGuardOutsideForcegreetPackage
#      0x000BBAA4 through `dialogue.forcegreet` — the Skyrim dialect
#      resolves the ForceGreet tree leaf's `Topic` data input
#      (PDTO type 0) and must report topic 000BBA8B.
#   3. The instance arm (#5511): install VigilantofStendarrForcegreet-
#      Daedric 0x000CD613 — a concrete package whose ForceGreet tree
#      lives on its 0x0003C1C4 PKCU template — and must resolve its own
#      PDTO topic 000B30FA through the template's leaf. 334 of the 339
#      Skyrim.esm force-greet packs are this shape.
#   4. The generic arm: install the topic-less ForceGreet package
#      0x0003C1C4 — authored with only the type-1 PDTO constant — and
#      must report the generic greeting.
#   5. Teleport the player beside Irileth (the walk-to-player step is
#      the shared, dt1-gated bridge; Dragonsreach's furniture stalls
#      the KCC walk, so the gate closes the distance directly) and
#      poll `dialogue.status` until the conversation opens on
#      `DialogueGenericHello` — no activation anywhere.
#   6. The #5912 voice leg: the open line resolves its .fuz through the
#      VoiceSoundCache (FUZE parse + WMA payload via the host's optional
#      ffmpeg) and the Phase V log carries a real duration — no ffmpeg
#      on the host degrades the leg to an explicit SKIP of the voice
#      assertions, never a pass.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19879}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-dt2.XXXXXX)"
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
    echo "smoke[dt2-skyrim-forcegreet]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — the SKIP≠PASS contract every per-game gate honours: a runner
# without the game's data measures nothing (exit 77, never a pass),
# before any port or build work.
DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa" "Skyrim - Voices_en0.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[dt2-skyrim-forcegreet]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done
# The voice leg decodes through the host's optional ffmpeg (#5912 —
# parse, don't link). Without it the leg runs but the voice assertions
# are explicitly skipped below; without the archive the whole gate skips.
HAVE_FFMPEG=1
command -v ffmpeg >/dev/null 2>&1 || HAVE_FFMPEG=0

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

wait_status() {
    local pattern="$1" deadline=$((SECONDS + TIMEOUT))
    until grep -Fq "$pattern" "$LOG_DIR/status.log" 2>/dev/null; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 1
        debug_command 'dialogue.status' "$LOG_DIR/status.log" >/dev/null 2>&1 || true
    done
}

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn,byroredux::systems::npc_dialogue=info,byroredux::systems::dialogue_voice=info" \
    target/release/byroredux \
    --game skyrim_se --cell WhiterunDragonsReach \
    --sounds-bsa "$DATA/Skyrim - Voices_en0.bsa" \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!
wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Fq "$pattern" "$file"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.25
    done
}
wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
sleep 2

# Irileth — the deterministic Dragonsreach ACHR the gate drives.
debug_command 'find("Irileth")' "$LOG_DIR/find.log" || fail 'find unavailable'
entity="$(grep -oE 'Entity [0-9]+' "$LOG_DIR/find.log" | head -1 | cut -d' ' -f2 || true)"
[[ -n "$entity" ]] || fail "Irileth entity not found in the loaded cell"
echo "smoke[dt2-skyrim-forcegreet]: Irileth entity=$entity"

# ── The authored-topic arm ─────────────────────────────────────────
debug_command "dialogue.forcegreet $entity 0x000BBAA4" "$LOG_DIR/author.log" \
    || fail 'author-topic install unavailable'
grep -Fq 'topic 000BBA8B' "$LOG_DIR/author.log" \
    || fail "the ForceGreet leaf's Topic input must resolve 000BBA8B: $(sed -n '1,5p' "$LOG_DIR/author.log")"
echo "smoke[dt2-skyrim-forcegreet]: authored-topic arm PASS — PDTO topic resolved"

# ── The template-instance arm (#5511) ──────────────────────────────
# A concrete package: no tree of its own, PKCU 0x0003C1C4 carrying the
# ForceGreet leaf, and its own PDTO topic at the leaf's input index.
debug_command "dialogue.forcegreet $entity 0x000CD613" "$LOG_DIR/instance.log" \
    || fail 'instance install unavailable'
grep -Fq 'topic 000B30FA' "$LOG_DIR/instance.log" \
    || fail "the instance's own PDTO topic must resolve through the template leaf (000B30FA): $(sed -n '1,5p' "$LOG_DIR/instance.log")"
echo "smoke[dt2-skyrim-forcegreet]: instance arm PASS — template leaf + own PDTO resolved"

# ── The generic arm + the open ─────────────────────────────────────
# Irileth does not own the Orc-guard topic's lines (its INFO conditions
# are authored for the guard), so the open leg installs the topic-less
# package: its line is the master's generic hello, the same authored
# remainder shape as FNV's PKDD-less packages.
debug_command "dialogue.forcegreet $entity 0x0003C1C4" "$LOG_DIR/generic.log" \
    || fail 'generic install unavailable'
grep -Fq 'topic generic greeting' "$LOG_DIR/generic.log" \
    || fail "the topic-less ForceGreet package must greet generically: $(sed -n '1,5p' "$LOG_DIR/generic.log")"
echo "smoke[dt2-skyrim-forcegreet]: generic arm PASS — topic-less package resolves the greeting"

debug_command 'cam.pos -250 40 -2840' "$LOG_DIR/tp.log" >/dev/null 2>&1 || true
wait_status 'DialogueGenericHello'
echo "smoke[dt2-skyrim-forcegreet]: open PASS — conversation opened with no activation"

# ── The #5912 voice leg ────────────────────────────────────────────
# The open line rides play_line_voice like every conversation: the .fuz
# candidate composes from the applied selection's EDIDs, the FUZE
# container parses, and (with a host ffmpeg) the WMA payload decodes.
# The Phase V log line is the observable: N segment(s) with a real
# duration replaces the subtitle estimate.
sleep 3   # the voice resolve logs when the line starts speaking
if [[ "$HAVE_FFMPEG" -ne 1 ]]; then
    echo "smoke[dt2-skyrim-forcegreet]: SKIP voice leg -- no host ffmpeg (line stays on the subtitle estimate, #5912 degradation)"
elif grep -Eq '#5367 Phase V: voice line info [0-9A-FXx]+ -> [1-9][0-9]* segment\(s\), [0-9.]+s' \
        "$LOG_DIR/session.stderr"; then
    grep -E '#5367 Phase V: voice line info' "$LOG_DIR/session.stderr" | sed 's/^/smoke[dt2-skyrim-forcegreet]: voice /'
    echo "smoke[dt2-skyrim-forcegreet]: voice PASS — .fuz resolved and decoded"
else
    # A silent line is only legal when the log says WHY (no audio device
    # or no voice key) — a quiet nothing is a failure.
    if grep -q 'no audio device\|is_active' "$LOG_DIR/session.stderr"; then
        echo "smoke[dt2-skyrim-forcegreet]: SKIP voice leg -- no audio device on this host"
    else
        fail "the open line voiced no segment and logged no degradation: $(grep -c 'Phase V\|5912' "$LOG_DIR/session.stderr" || true) matching lines"
    fi
fi
echo "smoke[dt2-skyrim-forcegreet]: PASS — Skyrim force-greet live; artifacts: $LOG_DIR"
