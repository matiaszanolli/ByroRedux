#!/usr/bin/env bash
# WATAL real-data gate — dynamic bodies in shipped water.
#
# The physics half of WATAL is covered by real-solver unit tests on synthetic
# worlds (`crates/physics/src/water.rs`: rise, settle, sleep, wake on waves,
# downstream drift). This is the missing gate on SHIPPED content: a dynamic
# body that the game's own cell data puts in or over water must reach the
# canonical `WaterContact` sink, float to the buoyancy equilibrium, stay
# there, and let the exterior physics world sleep — "water must never pin the
# exterior physics world awake" (docs/engine/watal.md §7).
#
# No spawning and no teleport: the bodies are authored references (FNV's Lake
# Mead cells drop two clutter bodies into the lake as they load; they fall,
# rise, float at a submerged fraction of ~0.6, sleep, and wake for waves).
# The gate watches `water.contacts` and `phys.stats` over real time.
#
# Game-parameterised like the other slice gates: every coordinate comes from
# `fixtures/<game>.env` (the `W2_*` block). A fixture without one SKIPs (77).
#
#   docs/smoke-tests/w2-dynamic-water.sh fnv

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture w2-dynamic-water "$@"

if [[ -z "${W2_WATER_SOURCE:-}" ]]; then
    echo "smoke[w2-dynamic-water]: SKIP -- missing W2 fixture for $SMOKE_GAME"
    echo "  $SMOKE_GAME.env declares no W2_* route: no authored dynamic body has been"
    echo "  measured in its water."
    exit 77
fi

smoke_require_fixture_fields \
    W2_HEADLINE W2_WORLDSPACE W2_GRID W2_CAMERA_POS W2_CAMERA_FORWARD \
    W2_WATER_SOURCE W2_MIN_BODIES W2_FRACTION_MIN W2_FRACTION_MAX \
    W2_MAX_SURFACE_DEPTH W2_SETTLE_SECONDS W2_OBSERVE_SECONDS

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-360}"
RADIUS="${W2_RADIUS:-1}"

LOG_DIR="$(mktemp -d /tmp/byro-w2-water.XXXXXX)"
engine_pid=""
keep_artifacts=0
cleanup() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
    if (( keep_artifacts == 0 )); then
        rm -rf "$LOG_DIR"
    fi
}
trap cleanup EXIT INT TERM

pass() { echo "smoke[w2-dynamic-water]: PASS -- $*"; }

fail() {
    keep_artifacts=1
    echo "smoke[w2-dynamic-water]: FAIL -- $*"
    echo "smoke[w2-dynamic-water]: artifacts retained at $LOG_DIR"
    tail -40 "$LOG_DIR/engine.stderr" 2>/dev/null || true
    exit 1
}

smoke_require_data

echo "smoke[w2-dynamic-water]: checking shader artifacts are up to date"
"$ROOT_DIR/scripts/check-shader-artifacts.sh"
echo "smoke[w2-dynamic-water]: building release binaries"
(cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)

engine_stdout="$LOG_DIR/engine.stdout"
engine_stderr="$LOG_DIR/engine.stderr"
samples_log="$LOG_DIR/samples.log"

debug_command() {
    local command="$1" output="$2"
    env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$output" 2>&1 <<EOF
$command
.quit
EOF
}

# One sample of the physics sink: the wet non-player contacts of the
# fixture's water, one `entity depth fraction` per line, plus the awake
# dynamic body count. Everything is appended to the retained artifact.
sample() {
    local contacts="$LOG_DIR/contacts.tmp" stats="$LOG_DIR/stats.tmp"
    debug_command "water.contacts" "$contacts" || return 1
    debug_command "phys.stats" "$stats" || return 1
    {
        echo "── t=$(( $(date +%s) - started_at ))s ──"
        sed 's/\\n/\n/g' "$contacts"
        sed 's/\\n/\n/g' "$stats"
    } >>"$samples_log"
    SAMPLE_BODIES="$(sed 's/\\n/\n/g' "$contacts" \
        | sed -nE "s/.*entity=([0-9]+) depth=([-0-9.]+) fraction=([-0-9.]+) .*material=${W2_WATER_SOURCE}.*/\1 \2 \3/p")"
    SAMPLE_AWAKE="$(sed 's/\\n/\n/g' "$stats" | sed -nE 's/.*awake dynamic=([0-9]+).*/\1/p' | head -1)"
}

numeric() { [[ "$1" =~ ^-?[0-9]+([.][0-9]+)?$ ]]; }

cd "$ROOT_DIR"
echo "================================================================"
echo "  smoke[w2-dynamic-water]: $FIXTURE_LABEL -- $W2_HEADLINE"
echo "================================================================"

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    RUST_LOG="error" \
    "$ENGINE_BIN" \
    "${SMOKE_ENGINE_ARGS[@]}" \
    --grid "$W2_GRID" \
    --wrld "$W2_WORLDSPACE" \
    --radius "$RADIUS" \
    --fly \
    --camera-pos "$W2_CAMERA_POS" \
    --camera-forward "$W2_CAMERA_FORWARD" \
    --bench-frames "$BENCH_FRAMES" \
    --bench-hold \
    >"$engine_stdout" 2>"$engine_stderr" &
engine_pid=$!

deadline=$(( $(date +%s) + TIMEOUT ))
until grep -Fq "bench-hold:" "$engine_stderr" 2>/dev/null; do
    (( $(date +%s) < deadline )) || fail "timeout waiting for the engine to reach the held state"
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before reaching the held state"
    sleep 0.5
done
pass "engine reached the held interactive state"
started_at="$(date +%s)"

# ── contact: the authored bodies reach the canonical sink ────────────────
deadline=$(( $(date +%s) + W2_SETTLE_SECONDS ))
while true; do
    sample || fail "the physics sink could not be read"
    count="$(printf '%s\n' "$SAMPLE_BODIES" | grep -c . || true)"
    (( count >= W2_MIN_BODIES )) && break
    (( $(date +%s) < deadline )) \
        || fail "only $count dynamic bodies reached water $W2_WATER_SOURCE in ${W2_SETTLE_SECONDS}s (need $W2_MIN_BODIES)"
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited while waiting for the water contacts"
    sleep 1
done
pass "$count authored dynamic bodies are in water $W2_WATER_SOURCE (WaterContact published)"

# ── float: every body reaches the buoyancy equilibrium band ──────────────
in_band() {
    local line entity depth fraction ok=1
    while read -r entity depth fraction; do
        [[ -n "$entity" ]] || continue
        numeric "$depth" && numeric "$fraction" || return 1
        awk -v f="$fraction" -v d="$depth" -v lo="$W2_FRACTION_MIN" -v hi="$W2_FRACTION_MAX" -v md="$W2_MAX_SURFACE_DEPTH" \
            'BEGIN { exit !(f >= lo && f <= hi && d >= 0 && d <= md) }' || ok=0
    done <<<"$SAMPLE_BODIES"
    (( ok == 1 ))
}
deadline=$(( $(date +%s) + W2_SETTLE_SECONDS ))
until sample && (( $(printf '%s\n' "$SAMPLE_BODIES" | grep -c .) >= W2_MIN_BODIES )) && in_band; do
    (( $(date +%s) < deadline )) || {
        printf '%s\n' "$SAMPLE_BODIES"
        fail "the bodies did not settle into submerged fraction [$W2_FRACTION_MIN, $W2_FRACTION_MAX] within depth $W2_MAX_SURFACE_DEPTH in ${W2_SETTLE_SECONDS}s"
    }
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited while the bodies were settling"
    sleep 1
done
pass "every body floats at the equilibrium (fraction in [$W2_FRACTION_MIN, $W2_FRACTION_MAX], depth <= $W2_MAX_SURFACE_DEPTH)"

# ── stay: no sinking, no launch, no pinned-awake world ───────────────────
observe_until=$(( $(date +%s) + W2_OBSERVE_SECONDS ))
samples=0
saw_asleep=0
while (( $(date +%s) < observe_until )); do
    sample || fail "the physics sink could not be read while observing"
    in_band || {
        printf '%s\n' "$SAMPLE_BODIES"
        fail "a floating body left the equilibrium band while observed"
    }
    numeric "$SAMPLE_AWAKE" || fail "phys.stats did not report an awake dynamic count"
    (( SAMPLE_AWAKE == 0 )) && saw_asleep=1
    samples=$(( samples + 1 ))
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited while observing"
    sleep 2
done
pass "the bodies held the equilibrium across $samples samples over ${W2_OBSERVE_SECONDS}s"
(( saw_asleep == 1 )) \
    || fail "the exterior physics world never went to sleep with its floats settled (awake dynamic stayed > 0): water is pinning it awake"
pass "the settled floats let the physics world sleep (awake dynamic reached 0)"

[[ -s "$engine_stderr" ]] && grep -qiE 'panic|device lost' "$engine_stderr" \
    && fail "the engine logged a panic or a device loss"
pass "authored dynamic bodies in water: contact -> float -> hold -> sleep"
