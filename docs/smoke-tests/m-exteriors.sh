#!/usr/bin/env bash
# Cross-game exterior readiness smoke matrix (EX-01 / EX-05).
#
# Each installed profile loads a known populated exterior at radius 1, retains
# a deterministic screenshot plus engine/debug telemetry, and applies hard
# gates to scene population, exterior lighting state, image health, and crash
# diagnostics. Missing game data self-skips; artifacts are intentionally kept.
#
# Usage:
#   docs/smoke-tests/m-exteriors.sh [fnv|fo3|oblivion|skyrim|fo4|all]
#
# Useful overrides:
#   BYROREDUX_SMOKE_FRAMES=10
#   BYRO_DEBUG_PORT=9987
#   BYROREDUX_EXTERIOR_ARTIFACT_DIR=/tmp/exterior-smoke

set -euo pipefail

GAME="${1:-all}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ENGINE_BIN="$REPO_ROOT/target/release/byroredux"
DEBUG_BIN="$REPO_ROOT/target/release/byro-dbg"

FNV_DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
FO3_DATA="${BYROREDUX_FO3_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data}"
OBLIVION_DATA="${BYROREDUX_OBLIVION_DATA:-/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data}"
SKYRIM_DATA="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
FO4_DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"

PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
TIMEOUT_SECONDS="${BYROREDUX_SMOKE_TIMEOUT:-240}"
ARTIFACT_DIR="${BYROREDUX_EXTERIOR_ARTIFACT_DIR:-$(mktemp -d /tmp/byro-exterior-smoke.XXXXXX)}"
SUMMARY="$ARTIFACT_DIR/summary.tsv"
ACTIVE_PID=""

mkdir -p "$ARTIFACT_DIR"
printf 'profile\tresult\tentities\tdraws\timage_mean\timage_stddev\tmissing_textures\tfailed_nifs\n' > "$SUMMARY"

cleanup_active () {
    if [[ -n "$ACTIVE_PID" ]] && kill -0 "$ACTIVE_PID" 2>/dev/null; then
        kill -TERM "$ACTIVE_PID" 2>/dev/null || true
        wait "$ACTIVE_PID" 2>/dev/null || true
    fi
    ACTIVE_PID=""
}
trap cleanup_active EXIT INT TERM

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "exterior-smoke: building release engine and debug client"
    cargo build --release --quiet -p byroredux -p byro-dbg
fi

if ! command -v magick >/dev/null 2>&1; then
    echo "exterior-smoke: FAIL - ImageMagick 'magick' is required for the blank/white-out gate"
    exit 1
fi

profile_ready () {
    local label="$1"
    shift
    local missing=0
    local path
    for path in "$@"; do
        if [[ ! -f "$path" ]]; then
            echo "exterior-smoke[$label]: missing $path"
            missing=1
        fi
    done
    if (( missing != 0 )); then
        echo "exterior-smoke[$label]: SKIP - required game data is not installed"
        printf '%s\tSKIP\t-\t-\t-\t-\t-\t-\n' "$label" >> "$SUMMARY"
        return 1
    fi
    return 0
}

image_health () {
    local image="$1"
    local mean_out stddev_out
    if [[ ! -s "$image" ]]; then
        return 1
    fi
    read -r mean_out stddev_out < <(
        magick "$image" -colorspace RGB \
            -format '%[fx:mean] %[fx:standard_deviation]\n' info:
    )
    IMAGE_MEAN="$mean_out"
    IMAGE_STDDEV="$stddev_out"

    # Reject effectively black/white and near-solid frames. These thresholds
    # intentionally leave generous headroom for dark interiors accidentally
    # selected by bad WRLD wiring while catching the historical white-out.
    awk -v mean="$IMAGE_MEAN" -v sd="$IMAGE_STDDEV" \
        'BEGIN { exit !(mean > 0.01 && mean < 0.98 && sd > 0.005) }'
}

# Args: label, data_dir, worldspace, grid, entity_floor, draw_floor, CLI args...
run_profile () {
    local label="$1"
    local data_dir="$2"
    local worldspace="$3"
    local grid="$4"
    local entity_floor="$5"
    local draw_floor="$6"
    shift 6

    local profile_dir="$ARTIFACT_DIR/$label"
    local stdout_log="$profile_dir/engine.stdout.log"
    local stderr_log="$profile_dir/engine.stderr.log"
    local debug_log="$profile_dir/debug.log"
    local screenshot="$profile_dir/frame.png"
    local command_file="$profile_dir/command.txt"
    mkdir -p "$profile_dir"

    printf '%q ' "$ENGINE_BIN" "$@" --bench-frames "$BENCH_FRAMES" \
        --bench-hold --screenshot "$screenshot" --upscaler taa > "$command_file"
    printf '\n' >> "$command_file"

    echo "exterior-smoke[$label]: launching $worldspace $grid (artifacts: $profile_dir)"
    (
        cd "$data_dir"
        env BYRO_DEBUG_PORT="$PORT" \
            RUST_LOG="${BYROREDUX_EXTERIOR_RUST_LOG:-info}" \
            "$ENGINE_BIN" "$@" \
            --bench-frames "$BENCH_FRAMES" \
            --bench-hold \
            --screenshot "$screenshot" \
            --upscaler taa
    ) > "$stdout_log" 2> "$stderr_log" &
    ACTIVE_PID=$!

    local deadline=$(( $(date +%s) + TIMEOUT_SECONDS ))
    while ! grep -q '^bench-hold:' "$stderr_log" 2>/dev/null; do
        if ! kill -0 "$ACTIVE_PID" 2>/dev/null; then
            echo "exterior-smoke[$label]: HARD FAIL - engine exited before bench-hold"
            tail -40 "$stderr_log" || true
            wait "$ACTIVE_PID" 2>/dev/null || true
            ACTIVE_PID=""
            printf '%s\tFAIL\t0\t0\t-\t-\t-\t-\n' "$label" >> "$SUMMARY"
            return 1
        fi
        if (( $(date +%s) > deadline )); then
            echo "exterior-smoke[$label]: HARD FAIL - timed out after ${TIMEOUT_SECONDS}s"
            cleanup_active
            printf '%s\tFAIL\t0\t0\t-\t-\t-\t-\n' "$label" >> "$SUMMARY"
            return 1
        fi
        sleep 0.5
    done

    env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" > "$debug_log" 2>&1 <<'EOF' || true
stats
light.dump
tex.missing
mesh.cache
mesh.cache failed
ctx.scratch
cam.where
.quit
EOF

    cleanup_active

    local bench_line entities draws missing_textures failed_nifs
    bench_line="$(grep '^bench:' "$stdout_log" | tail -1 || true)"
    entities="$(grep -oE 'entities=[0-9]+' <<< "$bench_line" | head -1 | cut -d= -f2 || true)"
    draws="$(grep -oE 'draws=[0-9]+' <<< "$bench_line" | head -1 | cut -d= -f2 || true)"
    missing_textures="$(grep -oE '[0-9]+ unique missing textures' "$debug_log" | head -1 | grep -oE '^[0-9]+' || true)"
    failed_nifs="$(grep -oE '[0-9]+ failed' "$debug_log" | head -1 | grep -oE '^[0-9]+' || true)"
    : "${entities:=0}"
    : "${draws:=0}"
    if grep -Fq 'No missing textures' "$debug_log"; then
        missing_textures=0
    fi
    : "${missing_textures:=unknown}"
    : "${failed_nifs:=unknown}"
    IMAGE_MEAN="-"
    IMAGE_STDDEV="-"

    local hard_fail=0
    if [[ -z "$bench_line" ]]; then
        echo "exterior-smoke[$label]: HARD FAIL - bench summary missing"
        hard_fail=1
    elif (( entities < entity_floor || draws < draw_floor )); then
        echo "exterior-smoke[$label]: HARD FAIL - scene population entities=$entities/$entity_floor draws=$draws/$draw_floor"
        hard_fail=1
    else
        echo "exterior-smoke[$label]: PASS population entities=$entities draws=$draws"
    fi

    # WRLD EDIDs are normalized by the record index, so the runtime may log
    # `megatonworld` for the user-facing `MegatonWorld` override.
    if ! grep -Fiq "Exterior world context built: worldspace '$worldspace'" "$stderr_log"; then
        echo "exterior-smoke[$label]: HARD FAIL - requested worldspace '$worldspace' was not confirmed"
        hard_fail=1
    fi
    if ! grep -Eq 'is_interior[[:space:]]*=[[:space:]]*false' "$debug_log"; then
        echo "exterior-smoke[$label]: HARD FAIL - light.dump did not report exterior state"
        hard_fail=1
    fi
    if grep -Eiq 'panicked at|VUID-|validation error|ERROR.*Vulkan|Vulkan.*ERROR' "$stdout_log" "$stderr_log"; then
        echo "exterior-smoke[$label]: HARD FAIL - panic or Vulkan validation error in engine logs"
        hard_fail=1
    fi
    if image_health "$screenshot"; then
        echo "exterior-smoke[$label]: PASS image mean=$IMAGE_MEAN stddev=$IMAGE_STDDEV"
    else
        echo "exterior-smoke[$label]: HARD FAIL - screenshot missing, blank, white-out, or near-solid"
        hard_fail=1
    fi

    if [[ "$missing_textures" != "unknown" && "$missing_textures" != "0" ]]; then
        echo "exterior-smoke[$label]: WARN - $missing_textures unique missing textures"
    fi
    if [[ "$failed_nifs" != "unknown" && "$failed_nifs" != "0" ]]; then
        echo "exterior-smoke[$label]: WARN - $failed_nifs failed NIF cache entries"
    fi

    local result=PASS
    if (( hard_fail != 0 )); then
        result=FAIL
    fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$label" "$result" "$entities" "$draws" "$IMAGE_MEAN" \
        "$IMAGE_STDDEV" "$missing_textures" "$failed_nifs" >> "$SUMMARY"
    return "$hard_fail"
}

fnv_run () {
    local esm="$FNV_DATA/FalloutNV.esm"
    local meshes="$FNV_DATA/Fallout - Meshes.bsa"
    local textures="$FNV_DATA/Fallout - Textures.bsa"
    profile_ready fnv "$esm" "$meshes" "$textures" || return 0
    run_profile fnv "$FNV_DATA" WastelandNV 0,0 250 100 \
        --esm "$esm" --grid 0,0 --radius 1 --wrld WastelandNV \
        --bsa "$meshes" --textures-bsa "$textures"
}

fo3_run () {
    local esm="$FO3_DATA/Fallout3.esm"
    local meshes="$FO3_DATA/Fallout - Meshes.bsa"
    local textures="$FO3_DATA/Fallout - Textures.bsa"
    profile_ready fo3 "$esm" "$meshes" "$textures" || return 0
    # MegatonWorld (0,0) is a valid empty dummy CELL. (-1,-7) is the
    # populated MegatonPlaza foreground and is intentionally the smoke gate.
    run_profile fo3 "$FO3_DATA" MegatonWorld -1,-7 2000 700 \
        --esm "$esm" --grid -1,-7 --radius 1 --wrld MegatonWorld \
        --bsa "$meshes" --textures-bsa "$textures"
}

oblivion_run () {
    local esm="$OBLIVION_DATA/Oblivion.esm"
    local meshes="$OBLIVION_DATA/Oblivion - Meshes.bsa"
    local textures="$OBLIVION_DATA/Oblivion - Textures - Compressed.bsa"
    profile_ready oblivion "$esm" "$meshes" "$textures" || return 0
    run_profile oblivion "$OBLIVION_DATA" Tamriel 0,0 2500 500 \
        --esm "$esm" --grid 0,0 --radius 1 --wrld Tamriel \
        --bsa "$meshes" --textures-bsa "$textures"
}

skyrim_run () {
    local esm="$SKYRIM_DATA/Skyrim.esm"
    local required=(
        "$esm"
        "$SKYRIM_DATA/Skyrim - Meshes0.bsa"
        "$SKYRIM_DATA/Skyrim - Meshes1.bsa"
    )
    local archive
    for archive in "$SKYRIM_DATA"/Skyrim\ -\ Textures{0..8}.bsa; do
        required+=("$archive")
    done
    profile_ready skyrim "${required[@]}" || return 0

    local args=(--esm "$esm" --grid 2,-4 --radius 1 --wrld Tamriel)
    args+=(--bsa "$SKYRIM_DATA/Skyrim - Meshes0.bsa")
    args+=(--bsa "$SKYRIM_DATA/Skyrim - Meshes1.bsa")
    for archive in "$SKYRIM_DATA"/Skyrim\ -\ Textures{0..8}.bsa; do
        args+=(--textures-bsa "$archive")
    done
    run_profile skyrim "$SKYRIM_DATA" Tamriel 2,-4 500 150 "${args[@]}"
}

fo4_run () {
    local esm="$FO4_DATA/Fallout4.esm"
    local required=(
        "$esm"
        "$FO4_DATA/Fallout4 - Meshes.ba2"
        "$FO4_DATA/Fallout4 - MeshesExtra.ba2"
        "$FO4_DATA/Fallout4 - TexturesPatch.ba2"
        "$FO4_DATA/Fallout4 - Materials.ba2"
    )
    local archive
    for archive in "$FO4_DATA"/Fallout4\ -\ Textures{1..9}.ba2; do
        required+=("$archive")
    done
    profile_ready fo4 "${required[@]}" || return 0

    local args=(--esm "$esm" --grid 0,0 --radius 1 --wrld Commonwealth)
    args+=(--bsa "$FO4_DATA/Fallout4 - Meshes.ba2")
    args+=(--bsa "$FO4_DATA/Fallout4 - MeshesExtra.ba2")
    for archive in "$FO4_DATA"/Fallout4\ -\ Textures{1..9}.ba2; do
        args+=(--textures-bsa "$archive")
    done
    args+=(--textures-bsa "$FO4_DATA/Fallout4 - TexturesPatch.ba2")
    args+=(--materials-ba2 "$FO4_DATA/Fallout4 - Materials.ba2")
    run_profile fo4 "$FO4_DATA" Commonwealth 0,0 500 100 "${args[@]}"
}

total_rc=0
run_selected () {
    "$1" || total_rc=$(( total_rc | $? ))
}

case "$GAME" in
    fnv)       run_selected fnv_run ;;
    fo3)       run_selected fo3_run ;;
    oblivion)  run_selected oblivion_run ;;
    skyrim)    run_selected skyrim_run ;;
    fo4)       run_selected fo4_run ;;
    all)
        run_selected fnv_run
        run_selected fo3_run
        run_selected oblivion_run
        run_selected skyrim_run
        run_selected fo4_run
        ;;
    *)
        echo "Usage: $0 [fnv|fo3|oblivion|skyrim|fo4|all]"
        exit 2
        ;;
esac

echo
column -t -s $'\t' "$SUMMARY" 2>/dev/null || cat "$SUMMARY"
echo "exterior-smoke: artifacts retained at $ARTIFACT_DIR"

if (( total_rc != 0 )); then
    echo "exterior-smoke: FAIL - one or more installed profiles hit a hard gate"
    exit "$total_rc"
fi
echo "exterior-smoke: PASS - every installed selected profile passed"
