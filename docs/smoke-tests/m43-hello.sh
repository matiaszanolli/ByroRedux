#!/usr/bin/env bash
# M43 gate — a moving actor says hello (AHEL), live (Skyrim SE).
#
#   Whiterun's vanilla NPCs that hold a package allowing random conversations are all idle sandboxers, so no
#   natural mover exists to watch. The gate stages one: `conv.status` lists the real participants the
#   random-conversation pass can use, and `entity.move` pushes four of them back and forth once a second, so the
#   pass sees them travel >= 10 units between its one-second scans. They then run the real attempt — timer,
#   social radius, chance, same-pair cooldown — and the engine logs `actor hello: <initiator> -> <partner>`.
#   (The dice make the wait random: 10-30 s timers and a 10 % roll; the loop gives it several minutes.)
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19898}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-600}"
LOG_DIR="$(mktemp -d /tmp/byro-m43-hello.XXXXXX)"
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
    echo "smoke[m43-hello]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — SKIP != PASS: a runner without the game's data measures nothing.
DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m43-hello]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done

if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

cargo build --release --offline --quiet -p byroredux -p byro-dbg \
    || fail "release build failed"

debug_command() {
    timeout 60 env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg >"$2" 2>&1 <<EOF2
$1
.quit
EOF2
    sed -i 's/\\n/\n/g' "$2"
}

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn,byroredux::systems::random_conversation=debug" \
    target/release/byroredux \
    --game skyrim_se --grid 2,-4 --radius 2 \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!

deadline=$((SECONDS + TIMEOUT))
until grep -Fq 'bench-hold:' "$LOG_DIR/session.stderr"; do
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold"
    (( SECONDS < deadline )) || fail "timeout waiting for bench-hold"
    sleep 0.5
done
sleep 8

debug_command "conv.status" "$LOG_DIR/participants.log" || fail "conv.status unavailable"
movers="$(grep -E '^participant [0-9]+ ' "$LOG_DIR/participants.log" | awk '{print $2}' | head -4 | tr '\n' ' ')"
[[ -n "${movers// /}" ]] || fail "no conversation participant is loaded: $(sed -n '1,5p' "$LOG_DIR/participants.log")"
echo "smoke[m43-hello]: staging movers: $movers"

sign=40
while ! grep -Fq 'actor hello:' "$LOG_DIR/session.stderr"; do
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited while staging movers"
    (( SECONDS < deadline )) || fail "no hello within the timeout (movers: $movers)"
    commands=""
    for m in $movers; do commands+="entity.move $m $sign 0 0"$'\n'; done
    debug_command "$commands" "$LOG_DIR/move.log" || true
    sign=$((-sign))
    sleep 0.5
done

line="$(grep -F 'actor hello:' "$LOG_DIR/session.stderr" | head -1)"
if grep -Fq 'panicked' "$LOG_DIR/session.stderr"; then
    fail "the engine panicked: $(grep -F 'panicked' "$LOG_DIR/session.stderr" | head -1)"
fi

stop_engine
echo "smoke[m43-hello]: PASS -- $(sed 's/^\[[^]]*\] *//' <<<"$line")"
echo "artifacts: $LOG_DIR"
