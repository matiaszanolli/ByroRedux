#!/usr/bin/env bash
# TSD-1 gate — every terrain layer in the test areas has a height map (#6195).
#
#   For each installed test-area game, boot the exterior grid, wait for the
#   per-layer height generation to settle, and require the `terrain.height
#   all` census to report zero failed and zero not-requested lanes with at
#   least one ready lane. One looking-down screenshot per area lands in the
#   artifact dir as review material (visual judgment stays with Matías).
#
# Areas covered here (the four with harness-documented grids):
#   Skyrim Tamriel 2,-4  — tundra near Whiterun (m34-day-night's grid)
#   FNV WastelandNV 0,0  — Mojave near Goodsprings (m-trees' grid)
#   Oblivion Tamriel 0,0 — Great Forest belt (m-trees' grid)
#   FO4 Commonwealth <grid> — Commonwealth dirt-road area (grid probed live
#   when this gate was written; see the FO4_GRID comment below).
# The Skyrim riverbank (Riverwood) area is a scenic spot for the visual
# review, not the census: the census is area-agnostic in mechanism, so the
# gate covers one exterior per game and the scenic five-spot pass stays with
# Matías's screenshot review.
#
# SKIP≠PASS per game (#5427): a game whose data is missing measures nothing
# and is reported SKIP; a game that runs must pass or the gate fails.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19890}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-tsd1.XXXXXX)"
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
    echo "smoke[tsd1-height-census]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

SKYRIM_DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
FNV_DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
OBL_DATA="${BYROREDUX_OBLIVION_DATA:-/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data}"
FO4_DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"

# FO4 Commonwealth grid, probed live 2026-10-24: 0,0 carries LAND and
# populated splat lanes (the census's own cell report was used to pick it;
# no exterior grid recipe existed in the harness before this gate).
FO4_GRID="${BYROREDUX_TSD1_FO4_GRID:-0,0}"

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

# Poll the census until no lane is still generating, then assert.
# The service generates asynchronously on first sight (the disk cache makes
# later runs instant), so the poll deadline is the honest bound.
census_one_area() {
    local area="$1" deadline=$((SECONDS + TIMEOUT)) line
    local census_log="$LOG_DIR/$area.census"
    : >"$census_log"
    while :; do
        debug_command 'terrain.height all' "$census_log.poll" || true
        line="$(grep -F 'terrain.height all:' "$census_log.poll" || true)"
        [[ -n "$line" ]] || fail "$area: census produced no summary line"
        echo "$line" >>"$census_log"
        if ! grep -qE 'generating=[1-9]' <<<"$line"; then
            break
        fi
        kill -0 "$engine_pid" 2>/dev/null || fail "$area: engine exited mid-generation"
        (( SECONDS < deadline )) || fail "$area: generation did not settle within ${TIMEOUT}s"
        sleep 2
    done
    # The settled line is the last one recorded.
    line="$(tail -1 "$census_log")"
    echo "smoke[tsd1-height-census]: $area — $line"
    # The asserts are `if grep; then fail` rather than `grep && fail`: under
    # `set -e` a not-matching grep in the latter form returns 1 and aborts
    # the gate on its own pass path.
    if grep -qE 'terrain.height all: 0 cells' <<<"$line"; then
        fail "$area: no terrain cells resident — grid carries no LAND"
    fi
    if grep -qE 'ready=0' <<<"$line"; then
        fail "$area: no lane ready"
    fi
    if grep -qE 'failed=[1-9]' <<<"$line"; then
        fail "$area: lanes failed: $(sed -n '/FAILED/p' "$census_log.poll" | head -4)"
    fi
    if grep -qE 'not-requested=[1-9]' <<<"$line"; then
        fail "$area: lanes never requested"
    fi
    if grep -qE 'generating=[1-9]' <<<"$line"; then
        fail "$area: lanes still generating at settle"
    fi
}

screenshot() {
    local area="$1"
    debug_command "screenshot tsd1-$area" "$LOG_DIR/$area.shot.log" || true
    # #5142 — the command takes a bare filename and writes the PNG under the
    # engine cwd's screenshots/ without an extension.
    if [[ -f "screenshots/tsd1-$area" ]]; then
        mv "screenshots/tsd1-$area" "$LOG_DIR/$area.png"
    fi
}

# run_area <area> <engine args…>
run_area() {
    local area="$1"; shift
    if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
        fail "port $PORT already occupied; refusing to attach to another session"
    fi
    env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
        BYROREDUX_SETTINGS_PATH="$LOG_DIR/$area.settings.toml" \
        RUST_LOG="warn" \
        target/release/byroredux \
        --bench-frames 30 --bench-hold \
        "$@" \
        >"$LOG_DIR/$area.stdout" 2>"$LOG_DIR/$area.stderr" &
    engine_pid=$!
    wait_log "$LOG_DIR/$area.stderr" 'bench-hold:'
    sleep 3
    census_one_area "$area"
    screenshot "$area"
    stop_engine
    sleep 1
}

any_skip=0
any_ran=0

skyrim_run() {
    for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa"; do
        if [ ! -f "$SKYRIM_DATA/$f" ]; then
            echo "smoke[tsd1-height-census]: Skyrim SKIP — missing $SKYRIM_DATA/$f"
            any_skip=1
            return 0
        fi
    done
    any_ran=1
    run_area skyrim-tundra \
        --esm "$SKYRIM_DATA/Skyrim.esm" \
        --grid 2,-4 --radius 1 --wrld Tamriel \
        --bsa "$SKYRIM_DATA/Skyrim - Meshes0.bsa" \
        --bsa "$SKYRIM_DATA/Skyrim - Meshes1.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures0.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures1.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures2.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures3.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures4.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures5.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures6.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures7.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures8.bsa"
}

fnv_run() {
    for f in "FalloutNV.esm" "Fallout - Meshes.bsa" "Fallout - Textures.bsa"; do
        if [ ! -f "$FNV_DATA/$f" ]; then
            echo "smoke[tsd1-height-census]: FNV SKIP — missing $FNV_DATA/$f"
            any_skip=1
            return 0
        fi
    done
    any_ran=1
    run_area fnv-mojave \
        --esm "$FNV_DATA/FalloutNV.esm" \
        --grid 0,0 --radius 1 --wrld WastelandNV \
        --bsa "$FNV_DATA/Fallout - Meshes.bsa" \
        --textures-bsa "$FNV_DATA/Fallout - Textures.bsa"
}

oblivion_run() {
    for f in "Oblivion.esm" "Oblivion - Meshes.bsa" "Oblivion - Textures - Compressed.bsa"; do
        if [ ! -f "$OBL_DATA/$f" ]; then
            echo "smoke[tsd1-height-census]: Oblivion SKIP — missing $OBL_DATA/$f"
            any_skip=1
            return 0
        fi
    done
    any_ran=1
    run_area oblivion-forest \
        --esm "$OBL_DATA/Oblivion.esm" \
        --grid 0,0 --radius 1 --wrld Tamriel \
        --bsa "$OBL_DATA/Oblivion - Meshes.bsa" \
        --textures-bsa "$OBL_DATA/Oblivion - Textures - Compressed.bsa"
}

fo4_run() {
    for f in "Fallout4.esm" "Fallout4 - Meshes.ba2" "Fallout4 - Textures1.ba2"; do
        if [ ! -f "$FO4_DATA/$f" ]; then
            echo "smoke[tsd1-height-census]: FO4 SKIP — missing $FO4_DATA/$f"
            any_skip=1
            return 0
        fi
    done
    any_ran=1
    run_area fo4-commonwealth \
        --esm "$FO4_DATA/Fallout4.esm" \
        --grid "$FO4_GRID" --radius 1 --wrld Commonwealth \
        --bsa "$FO4_DATA/Fallout4 - Meshes.ba2" \
        --bsa "$FO4_DATA/Fallout4 - MeshesExtra.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures1.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures2.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures3.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures4.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures5.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures6.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures7.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures8.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures9.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - TexturesPatch.ba2" \
        --materials-ba2 "$FO4_DATA/Fallout4 - Materials.ba2"
}

skyrim_run
fnv_run
oblivion_run
fo4_run

if [[ "$any_ran" -eq 0 ]]; then
    echo "smoke[tsd1-height-census]: SKIP — no test-area game data present"
    exit 77
fi
echo "smoke[tsd1-height-census]: PASS — every resident terrain lane has a height map; artifacts: $LOG_DIR"
