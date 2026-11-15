#!/usr/bin/env bash
# M65 gate — the detection tick, live on FNV (PRD §7.2 scenario 5, #6911).
#
#   1. Boot the FNV profile into GSProspectorSaloonInterior (the Phase 0 reference, design §13.2)
#      with a player, and set the stance with `stealth.sneak on` (SNK-DX-002).
#   2. `stealth.status` names the FalloutNv rules row and lists the saloon's detectors.
#   3. From the spawn, behind the saloon's walls, a detector reads level Lost with no line of sight.
#   4. The player is placed beside that detector on each of its four sides (a 190° view cone leaves
#      only a rear wedge blind, so at least one side is in front of it): it reads level Seen with a
#      line of sight on one of them, through the same `stealth.status` (SNK-DET-001, SNK-DX-001).
#
# Exits 77 when the FNV data is missing.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19880}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-m65-stealth.XXXXXX)"
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
    echo "smoke[m65-stealth-detection]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
for f in "FalloutNV.esm" "Fallout - Meshes.bsa" "Fallout - Textures.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m65-stealth-detection]: SKIP -- missing $DATA/$f"
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
        >"$2" 2>&1 <<CMDS
$1
.quit
CMDS
    sed -i 's/\\n/\n/g' "$2"
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
    RUST_LOG="warn,byroredux::detection=info" \
    target/release/byroredux \
    --game fnv --cell GSProspectorSaloonInterior \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!
wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
sleep 2
grep -Fq "Detection rules: FalloutNv" "$LOG_DIR/session.stderr" \
    || fail "the install log names no FalloutNv detection row: $(grep -i 'detection rules' "$LOG_DIR/session.stderr" || true)"

status() { debug_command "stealth.status" "$LOG_DIR/status.log" || fail "stealth.status unavailable"; }

# ── 1. the stance ─────────────────────────────────────────────────
debug_command "stealth.sneak on" "$LOG_DIR/sneak.log" || fail "stealth.sneak unavailable"
grep -Fq "stealth: sneaking" "$LOG_DIR/sneak.log" || fail "the stance was refused: $(cat "$LOG_DIR/sneak.log")"
sleep 1

# ── 2–3. behind cover: level Lost ─────────────────────────────────
status
grep -Fq "rules FalloutNv, sneaking" "$LOG_DIR/status.log" || fail "status names no FalloutNv row: $(head -3 "$LOG_DIR/status.log")"
covered="$(grep -E 'los false .* level Lost' "$LOG_DIR/status.log" | head -1 || true)"
[[ -n "$covered" ]] || fail "no detector reads level Lost behind cover: $(cat "$LOG_DIR/status.log")"
entity="$(sed -E 's/.*entity ([0-9]+) .*/\1/' <<<"$covered")"
read -r x y z < <(sed -E 's/.* at \((-?[0-9]+), (-?[0-9]+), (-?[0-9]+)\).*/\1 \2 \3/' <<<"$covered")
echo "smoke[m65-stealth-detection]: Lost PASS — detector $entity behind cover: $covered"

# ── 4. in its sight: level Seen ───────────────────────────────────
seen=""
for offset in "96 0" "-96 0" "0 96" "0 -96"; do
    read -r dx dz <<<"$offset"
    debug_command "cam.pos $((x + dx)) $((y + 60)) $((z + dz))" "$LOG_DIR/teleport.log" || fail "cam.pos unavailable"
    sleep 1
    status
    row="$(grep -E "entity $entity " "$LOG_DIR/status.log" || true)"
    if grep -Eq 'los true .* level Seen' <<<"$row"; then
        seen="$row"
        break
    fi
done
[[ -n "$seen" ]] || fail "detector $entity never read level Seen beside it: $(cat "$LOG_DIR/status.log")"
echo "smoke[m65-stealth-detection]: Seen PASS — detector $entity in sight: $seen"
echo "smoke[m65-stealth-detection]: PASS"
