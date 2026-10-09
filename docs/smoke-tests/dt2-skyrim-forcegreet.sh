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
#   3. The generic arm: install the topic-less ForceGreet package
#      0x0003C1C4 — authored with only the type-1 PDTO constant — and
#      must report the generic greeting.
#   4. Teleport the player beside Irileth (the walk-to-player step is
#      the shared, dt1-gated bridge; Dragonsreach's furniture stalls
#      the KCC walk, so the gate closes the distance directly) and
#      poll `dialogue.status` until the conversation opens on
#      `DialogueGenericHello` — no activation anywhere.
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
for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa"; do
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
    RUST_LOG="warn,byroredux::systems::npc_dialogue=info" \
    target/release/byroredux \
    --game skyrim_se --cell WhiterunDragonsReach \
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
echo "smoke[dt2-skyrim-forcegreet]: PASS — Skyrim force-greet live; artifacts: $LOG_DIR"
