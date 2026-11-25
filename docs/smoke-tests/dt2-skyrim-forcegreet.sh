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
#   6. The open line resolves its .fuz through VoiceSoundCache and decodes
#      the embedded WMA v2 stream in process (#7415). The Phase V log
#      carries the decoded duration.
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
    RUST_LOG="warn,byroredux::systems::npc_dialogue=info,byroredux::systems::dialogue_voice=info,byroredux=info" \
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

# ── Phase P (M48 GUI-PAGE-004, #6666): the conversation on the game's own movie ──
# Q1: Skyrim's dialogue menu runs on `interface\dialoguemenu.swf` (S1) once its host calls are
# served; the registry shows the menu open on that movie, and the presented line is the one the
# native page would show — the speaker named, the list mode, the speaking cue (dt1's Phase P).
debug_command 'ui.menus' "$LOG_DIR/menus.log" >/dev/null 2>&1 || fail 'ui.menus unavailable'
grep -Eq 'Dialogue Menu code=- surface=movie interface.+dialoguemenu\.swf' "$LOG_DIR/menus.log" \
    || fail "Phase P: the conversation is not on the dialogue movie: $(sed -n '1,6p' "$LOG_DIR/menus.log")"
# The greeting voices for 0.6 s, already over by the first poll; the speaking cue is read on the
# answer to the first topic the list offers ("What does a Housecarl do?"), selected as a click on
# the list does.
topic="$(sed 's/\\n/\n/g' "$LOG_DIR/status.log" | grep -oE "offers topic 0x[0-9A-Fa-f]+" | head -1 | grep -oE '0x[0-9A-Fa-f]+' || true)"
[[ -n "$topic" ]] || fail "Phase P: the greeting offers no topic: $(sed -n '1,8p' "$LOG_DIR/status.log")"
debug_command "dialogue.select $topic" "$LOG_DIR/select.log" >/dev/null 2>&1 || fail 'dialogue.select unavailable'
debug_command 'dialogue.status' "$LOG_DIR/page.log" >/dev/null 2>&1 || fail 'dialogue.status unavailable'
sed -i 's/\\n/\n/g' "$LOG_DIR/page.log"
page="$(grep -E '^page: ' "$LOG_DIR/page.log" | head -1 || true)"
[[ -n "$page" ]] || fail "Phase P: no page snapshot in dialogue.status: $(sed -n '1,8p' "$LOG_DIR/page.log")"
grep -Fq "speaker 'Irileth'" <<<"$page" || fail "Phase P: the movie's speaker is not named: $page"
grep -Eq 'list=(replies|topics)' <<<"$page" || fail "Phase P: no list mode: $page"
grep -Fq 'speaking=true' <<<"$page" || fail "Phase P: the speaking cue is off right after the topic's answer started: $page"
grep -Fq 'Dialogue Menu opened as movie' "$LOG_DIR/session.stderr" \
    || fail "Phase P: no dialogue movie open line in the log"
echo "smoke[dt2-skyrim-forcegreet]: Phase P PASS — $page, on interface\dialoguemenu.swf"

# ── The #5912 voice leg ────────────────────────────────────────────
# The open line rides play_line_voice like every conversation: the .fuz
# candidate composes from the applied selection's EDIDs, the FUZE
# container parses, and the linked decoder handles the WMA payload.
# The Phase V log line is the observable: N segment(s) with a real
# duration replaces the subtitle estimate.
sleep 3   # the voice resolve logs when the line starts speaking
if grep -Eq '#5367 Phase V: voice line info [0-9A-FXx]+ -> [1-9][0-9]* segment\(s\), [0-9.]+s' \
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
