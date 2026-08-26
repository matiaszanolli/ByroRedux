#!/usr/bin/env bash
# Cross-game exterior readiness smoke matrix and traversal gate
# (EX-01 / EX-05 / EX-06 / EX-08 / EX-09 / EX-10 / EX-11 / EX-17).
#
# Each installed profile loads a known populated exterior at radius 1, retains
# a deterministic screenshot plus engine/debug telemetry, and applies hard
# gates to scene population, exterior lighting state, image health, and crash
# diagnostics. Missing game data self-skips; artifacts are intentionally kept.
#
# Modes:
#   static    one settled view; population + image-health gates.
#   boundary  one-way `grid-cross`; each of three crossings must settle (EX-06).
#   soak      repeated out-and-back `grid-soak`; every CPU/GPU/runtime owner
#             must return to baseline and none may grow monotonically (EX-08).
#             The reversal is what exercises worker cancellation, partial-apply
#             cancellation, unload hysteresis, and stale-payload rejection — a
#             one-way traversal never reaches those paths. Ownership is sampled
#             engine-side at each return to origin (see `BenchCameraPath::
#             soak_cycle_completed`), so cycles bind to traversal phase rather
#             than to when this script happens to reconnect.
#   cycle     one settled exterior, then in-session sunrise/noon/night samples.
#             Every phase captures a PNG and gates the live clock, environment,
#             pre-tonemap finite counter, and canonical water ownership without
#             restarting the world or resetting its resources.
#
# Usage:
#   docs/smoke-tests/m-exteriors.sh [fnv|fo3|oblivion|skyrim|fo4|all] [static|boundary|soak|cycle]
#
# Useful overrides:
#   BYROREDUX_SMOKE_FRAMES=10
#   BYROREDUX_BOUNDARY_FRAMES=900
#   BYROREDUX_SOAK_FRAMES=1800
#   BYRO_DEBUG_PORT=9987
#   BYROREDUX_EXTERIOR_ARTIFACT_DIR=/tmp/exterior-smoke

set -euo pipefail

GAME="${1:-all}"
MODE="${2:-static}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ENGINE_BIN="$REPO_ROOT/target/release/byroredux"
DEBUG_BIN="$REPO_ROOT/target/release/byro-dbg"

FNV_DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
FO3_DATA="${BYROREDUX_FO3_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data}"
OBLIVION_DATA="${BYROREDUX_OBLIVION_DATA:-/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data}"
SKYRIM_DATA="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
FO4_DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"

PORT="${BYRO_DEBUG_PORT:-9876}"
case "$MODE" in
    static)   BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}" ;;
    cycle)    BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}" ;;
    boundary) BENCH_FRAMES="${BYROREDUX_BOUNDARY_FRAMES:-900}" ;;
    # Six out-and-back traversals need materially more logical frames than the
    # single one-way pass; the clock also pauses on every boundary.
    soak)     BENCH_FRAMES="${BYROREDUX_SOAK_FRAMES:-1800}" ;;
    *)
        echo "Usage: $0 [fnv|fo3|oblivion|skyrim|fo4|all] [static|boundary|soak|cycle]"
        exit 2
        ;;
esac
TIMEOUT_SECONDS="${BYROREDUX_SMOKE_TIMEOUT:-240}"
ARTIFACT_DIR="${BYROREDUX_EXTERIOR_ARTIFACT_DIR:-$(mktemp -d /tmp/byro-exterior-smoke.XXXXXX)}"
SUMMARY="$ARTIFACT_DIR/summary.tsv"
ACTIVE_PID=""

mkdir -p "$ARTIFACT_DIR"
printf 'profile\tresult\tentities\tdraws\timage_mean\timage_stddev\tenv\tmissing_textures\tfailed_nifs\tcrossings\tfull_samples\tfull_max_ms\tfull_superseded\tlod_samples\tlod_max_ms\tlod_superseded\tframe_p50_ms\tframe_p95_ms\tframe_max_ms\townership\tground_probe\n' > "$SUMMARY"

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
        printf '%s\tSKIP\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\n' "$label" >> "$SUMMARY"
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
    local bench_args=(--bench-frames "$BENCH_FRAMES" --bench-hold --screenshot "$screenshot" --upscaler taa)
    if [[ "$MODE" == boundary ]]; then
        bench_args+=(--bench-mode renderer-stepped --bench-camera grid-cross --fly)
    elif [[ "$MODE" == soak ]]; then
        bench_args+=(--bench-mode renderer-stepped --bench-camera grid-soak --fly)
    else
        bench_args+=(--bench-mode system-live)
    fi
    mkdir -p "$profile_dir"

    printf '%q ' "$ENGINE_BIN" "$@" "${bench_args[@]}" > "$command_file"
    printf '\n' >> "$command_file"

    echo "exterior-smoke[$label]: launching $worldspace $grid (artifacts: $profile_dir)"
    (
        cd "$data_dir"
        env BYRO_DEBUG_PORT="$PORT" \
            RUST_LOG="${BYROREDUX_EXTERIOR_RUST_LOG:-info}" \
            "$ENGINE_BIN" "$@" "${bench_args[@]}"
    ) > "$stdout_log" 2> "$stderr_log" &
    ACTIVE_PID=$!

    local deadline=$(( $(date +%s) + TIMEOUT_SECONDS ))
    while ! grep -q '^bench-hold:' "$stderr_log" 2>/dev/null; do
        if ! kill -0 "$ACTIVE_PID" 2>/dev/null; then
            echo "exterior-smoke[$label]: HARD FAIL - engine exited before bench-hold"
            tail -40 "$stderr_log" || true
            wait "$ACTIVE_PID" 2>/dev/null || true
            ACTIVE_PID=""
            printf '%s\tFAIL\t0\t0\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\n' "$label" >> "$SUMMARY"
            return 1
        fi
        if (( $(date +%s) > deadline )); then
            echo "exterior-smoke[$label]: HARD FAIL - timed out after ${TIMEOUT_SECONDS}s"
            cleanup_active
            printf '%s\tFAIL\t0\t0\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\n' "$label" >> "$SUMMARY"
            return 1
        fi
        sleep 0.5
    done

    if [[ "$MODE" == cycle ]]; then
        env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" > "$debug_log" 2>&1 <<EOF || true
time.pause
time.set 06:00
time.show
env.health
water.dump
r.health
screenshot $profile_dir/sunrise.png
time.set 12:00
time.show
env.health
water.dump
r.health
screenshot $profile_dir/noon.png
time.set 23:00
time.show
env.health
water.dump
r.health
screenshot $profile_dir/night.png
r.health
stats
light.dump
water.contacts
tex.missing
mesh.cache
mesh.cache failed
ctx.scratch
cam.where
lod.coverage
terrain.seams
world.owners
world.owners report
.quit
EOF
    else
        env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" > "$debug_log" 2>&1 <<'EOF' || true
stats
light.dump
env.health
water.dump
water.contacts
tex.missing
mesh.cache
mesh.cache failed
ctx.scratch
cam.where
r.health
lod.coverage
terrain.seams
world.owners
world.owners report
.quit
EOF
    fi

    cleanup_active

    local bench_line streaming_line entities draws missing_textures failed_nifs
    bench_line="$(grep '^bench:' "$stdout_log" | tail -1 || true)"
    streaming_line="$(grep '^streaming:' "$stdout_log" | tail -1 || true)"
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

    local crossings full_samples full_max_ms full_superseded
    local lod_samples lod_max_ms lod_superseded frame_p50_ms frame_p95_ms frame_max_ms
    crossings="$(grep -oE 'crossings=[0-9]+' <<< "$streaming_line" | cut -d= -f2 || true)"
    full_samples="$(grep -oE 'full_samples=[0-9]+' <<< "$streaming_line" | cut -d= -f2 || true)"
    full_max_ms="$(grep -oE 'full_max_ms=[0-9]+([.][0-9]+)?' <<< "$streaming_line" | cut -d= -f2 || true)"
    full_superseded="$(grep -oE 'full_superseded=[0-9]+' <<< "$streaming_line" | cut -d= -f2 || true)"
    lod_samples="$(grep -oE 'lod_samples=[0-9]+' <<< "$streaming_line" | cut -d= -f2 || true)"
    lod_max_ms="$(grep -oE 'lod_max_ms=[0-9]+([.][0-9]+)?' <<< "$streaming_line" | cut -d= -f2 || true)"
    lod_superseded="$(grep -oE 'lod_superseded=[0-9]+' <<< "$streaming_line" | cut -d= -f2 || true)"
    frame_p50_ms="$(grep -oE 'frame_p50_ms=[0-9]+([.][0-9]+)?' <<< "$bench_line" | cut -d= -f2 || true)"
    frame_p95_ms="$(grep -oE 'frame_p95_ms=[0-9]+([.][0-9]+)?' <<< "$bench_line" | cut -d= -f2 || true)"
    frame_max_ms="$(grep -oE 'frame_max_ms=[0-9]+([.][0-9]+)?' <<< "$bench_line" | cut -d= -f2 || true)"
    : "${crossings:=-}" "${full_samples:=-}" "${full_max_ms:=-}" "${full_superseded:=-}"
    : "${lod_samples:=-}" "${lod_max_ms:=-}" "${lod_superseded:=-}"
    : "${frame_p50_ms:=-}" "${frame_p95_ms:=-}" "${frame_max_ms:=-}"
    IMAGE_MEAN="-"
    IMAGE_STDDEV="-"
    local ownership="-"
    local probe_result="-"
    local env_result="-"

    local hard_fail=0
    if [[ -z "$bench_line" ]]; then
        echo "exterior-smoke[$label]: HARD FAIL - bench summary missing"
        hard_fail=1
    elif [[ "$MODE" == static ]] && (( entities < entity_floor || draws < draw_floor )); then
        echo "exterior-smoke[$label]: HARD FAIL - scene population entities=$entities/$entity_floor draws=$draws/$draw_floor"
        hard_fail=1
    elif [[ "$MODE" == boundary || "$MODE" == soak ]] && (( entities < 10 || draws < 1 )); then
        echo "exterior-smoke[$label]: HARD FAIL - traversal endpoint has no renderable exterior (entities=$entities draws=$draws)"
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
    if ! grep -Fq 'Water dump: planes=' "$debug_log"; then
        echo "exterior-smoke[$label]: HARD FAIL - water.dump did not report canonical water state"
        hard_fail=1
    elif grep -Fq 'volume=missing' "$debug_log"; then
        echo "exterior-smoke[$label]: HARD FAIL - one or more water planes have no canonical WaterVolume"
        hard_fail=1
    elif grep -Eq '3402823[0-9]+|(^|[^0-9])[-]?2147483648([.]0)?([^0-9]|$)' "$debug_log"; then
        echo "exterior-smoke[$label]: HARD FAIL - water.dump exposed an unfiltered no-water sentinel"
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

    if [[ "$MODE" == cycle ]]; then
        local phase phase_image
        for phase in sunrise noon night; do
            phase_image="$profile_dir/$phase.png"
            if image_health "$phase_image"; then
                echo "exterior-smoke[$label]: PASS $phase image mean=$IMAGE_MEAN stddev=$IMAGE_STDDEV"
            else
                echo "exterior-smoke[$label]: HARD FAIL - $phase screenshot missing, blank, white-out, or near-solid"
                hard_fail=1
            fi
        done

        if ! grep -Fq 'clock=06:00 phase=sunrise' "$debug_log" \
                || ! grep -Fq 'clock=12:00 phase=day' "$debug_log" \
                || ! grep -Fq 'clock=23:00 phase=night' "$debug_log"; then
            echo "exterior-smoke[$label]: HARD FAIL - deterministic sunrise/noon/night clock phases were not all observed"
            hard_fail=1
        elif ! grep -Fq 'sun: intensity=4.000' "$debug_log" \
                || ! grep -Fq 'sun: intensity=0.000' "$debug_log"; then
            echo "exterior-smoke[$label]: HARD FAIL - day/night sun intensity endpoints were not observed"
            hard_fail=1
        else
            echo "exterior-smoke[$label]: PASS in-session sunrise/noon/night endpoints"
        fi

        local cycle_water_samples
        cycle_water_samples="$(grep -c 'Water dump: planes=[0-9]*' "$debug_log" || true)"
        if (( cycle_water_samples != 3 )); then
            echo "exterior-smoke[$label]: HARD FAIL - canonical water was not sampled at all three clock phases ($cycle_water_samples/3)"
            hard_fail=1
        elif grep -Eq 'Water dump: planes=0([^0-9]|$)' "$debug_log"; then
            echo "exterior-smoke[$label]: HARD FAIL - cycle profile is not water-adjacent (zero canonical planes)"
            hard_fail=1
        else
            echo "exterior-smoke[$label]: PASS canonical water remains resident across all clock phases"
        fi

        local cycle_health_samples cycle_bad_health
        cycle_health_samples="$(sed 's/\\n/\n/g' "$debug_log" \
            | grep -cE 'since startup: *rgb=[0-9]+ alpha=[0-9]+' || true)"
        cycle_bad_health="$(sed 's/\\n/\n/g' "$debug_log" \
            | grep -E 'since startup: *rgb=[0-9]+ alpha=[0-9]+' \
            | grep -Evc 'rgb=0 alpha=0' || true)"
        if (( cycle_health_samples < 4 )); then
            echo "exterior-smoke[$label]: HARD FAIL - incomplete per-phase pre-tonemap health telemetry ($cycle_health_samples/4)"
            hard_fail=1
        elif (( cycle_bad_health != 0 )); then
            echo "exterior-smoke[$label]: HARD FAIL - non-finite pre-tonemap pixels occurred during the clock cycle"
            hard_fail=1
        else
            echo "exterior-smoke[$label]: PASS pre-tonemap output stayed finite across the clock cycle"
        fi
    fi

    if [[ "$MODE" == boundary ]]; then
        local unsettled_full unsettled_lod
        unsettled_full="$(grep -oE 'unsettled_full=[01]' <<< "$streaming_line" | cut -d= -f2 || true)"
        unsettled_lod="$(grep -oE 'unsettled_lod=[01]' <<< "$streaming_line" | cut -d= -f2 || true)"
        if [[ -z "$streaming_line" ]]; then
            echo "exterior-smoke[$label]: HARD FAIL - streaming summary missing"
            hard_fail=1
        elif [[ ! "$crossings" =~ ^[0-9]+$ || ! "$full_samples" =~ ^[0-9]+$ \
                || ! "$full_superseded" =~ ^[0-9]+$ || ! "$lod_samples" =~ ^[0-9]+$ \
                || ! "$lod_superseded" =~ ^[0-9]+$ || ! "$unsettled_full" =~ ^[01]$ \
                || ! "$unsettled_lod" =~ ^[01]$ ]]; then
            echo "exterior-smoke[$label]: HARD FAIL - incomplete streaming summary"
            hard_fail=1
        elif (( crossings < 3 )); then
            echo "exterior-smoke[$label]: HARD FAIL - grid-cross reported only $crossings/3 boundaries"
            hard_fail=1
        elif (( full_samples != crossings || lod_samples != crossings \
                || full_superseded != 0 || lod_superseded != 0 \
                || unsettled_full != 0 || unsettled_lod != 0 )); then
            echo "exterior-smoke[$label]: HARD FAIL - streaming did not settle each crossing: $streaming_line"
            hard_fail=1
        else
            echo "exterior-smoke[$label]: PASS traversal: $streaming_line"
        fi

        # EX-10/11 / #2371 — live LOD residency coverage: no two resident
        # quads (or a quad and a still-resident full-detail cell, or a quad
        # and a resident VisibleWhenDistant REFR — the EXAL §5.2 culling
        # rule) claim the same ground, and no quad key flapped in and out of
        # residency across the three-crossing traversal. `lod.coverage`'s
        # single-line `machine_line()` has no embedded `\n`, so (unlike
        # `env.health` below) it needs no unescape pass — grep the quoted
        # line directly.
        local coverage_line
        coverage_line="$(grep -oE 'lod-coverage: [^"]*' "$debug_log" | head -1 || true)"
        if [[ -z "$coverage_line" ]]; then
            echo "exterior-smoke[$label]: WARN - lod.coverage reported nothing (pre-#2371 binary, or an interior-only profile)"
        else
            local cov_sampled cov_overlaps cov_full_overlaps cov_vwd_overlaps cov_terrain_churn cov_object_churn
            cov_sampled="$(grep -oE 'sampled=[01]' <<< "$coverage_line" | cut -d= -f2 || true)"
            cov_overlaps="$(grep -oE 'overlaps=[0-9]+' <<< "$coverage_line" | head -1 | cut -d= -f2 || true)"
            cov_full_overlaps="$(grep -oE 'full_detail_overlaps=[0-9]+' <<< "$coverage_line" | cut -d= -f2 || true)"
            # EX-10/11 VWD follow-up — the EXAL §5.2 culling rule checked
            # live: a resident VisibleWhenDistant REFR must never fall
            # inside a resident object-LOD quad's footprint.
            cov_vwd_overlaps="$(grep -oE 'vwd_full_model_overlaps=[0-9]+' <<< "$coverage_line" | cut -d= -f2 || true)"
            cov_terrain_churn="$(grep -oE 'terrain_churn=[0-9]+' <<< "$coverage_line" | cut -d= -f2 || true)"
            cov_object_churn="$(grep -oE 'object_churn=[0-9]+' <<< "$coverage_line" | cut -d= -f2 || true)"
            if [[ "$cov_sampled" != "1" ]]; then
                echo "exterior-smoke[$label]: WARN - lod.coverage never sampled (no LOD reconcile ran this traversal)"
            elif [[ ! "$cov_overlaps" =~ ^[0-9]+$ || ! "$cov_full_overlaps" =~ ^[0-9]+$ \
                    || ! "$cov_vwd_overlaps" =~ ^[0-9]+$ \
                    || ! "$cov_terrain_churn" =~ ^[0-9]+$ || ! "$cov_object_churn" =~ ^[0-9]+$ ]]; then
                echo "exterior-smoke[$label]: HARD FAIL - incomplete lod.coverage line: $coverage_line"
                hard_fail=1
            elif (( cov_overlaps != 0 || cov_full_overlaps != 0 || cov_vwd_overlaps != 0 \
                    || cov_terrain_churn != 0 || cov_object_churn != 0 )); then
                echo "exterior-smoke[$label]: HARD FAIL - LOD coverage violation: $coverage_line"
                hard_fail=1
            else
                echo "exterior-smoke[$label]: PASS lod coverage: $coverage_line"
            fi
        fi

        # EX-10/11 item 7 / #2371 — adjacent-loaded-cell LAND shared-edge
        # agreement. Authored terrain shares byte-identical heightmap/normal
        # payloads at a seam, so `pairs_dirty > 0` is always a real
        # authoring/merge defect, never a magnitude judgement call (zero
        # tolerance by design — see `TerrainSeamStats`'s doc). Same
        # single-line `machine_line()` shape as `lod.coverage` above, so the
        # same direct-grep parse applies.
        local seam_line
        seam_line="$(grep -oE 'terrain-seams: [^"]*' "$debug_log" | head -1 || true)"
        if [[ -z "$seam_line" ]]; then
            echo "exterior-smoke[$label]: WARN - terrain.seams reported nothing (pre-#2371-item-7 binary, or an interior-only profile)"
        else
            local seam_sampled seam_checked seam_dirty seam_height_mismatch seam_normal_mismatch
            seam_sampled="$(grep -oE 'sampled=[01]' <<< "$seam_line" | cut -d= -f2 || true)"
            seam_checked="$(grep -oE 'pairs_checked=[0-9]+' <<< "$seam_line" | cut -d= -f2 || true)"
            seam_dirty="$(grep -oE 'pairs_dirty=[0-9]+' <<< "$seam_line" | cut -d= -f2 || true)"
            seam_height_mismatch="$(grep -oE 'height_mismatch_vertices=[0-9]+' <<< "$seam_line" | cut -d= -f2 || true)"
            seam_normal_mismatch="$(grep -oE 'normal_mismatch_pairs=[0-9]+' <<< "$seam_line" | cut -d= -f2 || true)"
            if [[ "$seam_sampled" != "1" ]]; then
                echo "exterior-smoke[$label]: WARN - terrain.seams never sampled (no adjacent resident-cell pair with LAND on both sides this traversal)"
            elif [[ ! "$seam_checked" =~ ^[0-9]+$ || ! "$seam_dirty" =~ ^[0-9]+$ \
                    || ! "$seam_height_mismatch" =~ ^[0-9]+$ || ! "$seam_normal_mismatch" =~ ^[0-9]+$ ]]; then
                echo "exterior-smoke[$label]: HARD FAIL - incomplete terrain.seams line: $seam_line"
                hard_fail=1
            elif (( seam_dirty != 0 )); then
                echo "exterior-smoke[$label]: HARD FAIL - terrain seam disagreement: $seam_line"
                hard_fail=1
            else
                echo "exterior-smoke[$label]: PASS terrain seams: $seam_line"
            fi
        fi
    fi

    # EX-04 / #2375 — the spawn ground probe. A content-backed cell can still
    # have nothing under the spawn column, and a capsule placed there falls
    # indefinitely. Character mode is now gated on this, so the interesting
    # signals are: did the probe run, and did it find walkable ground.
    local probe_line probe_result
    probe_line="$(grep -oE 'spawn-probe: result=[a-z-]+ colliders=[0-9]+[^"]*' "$stderr_log" \
        | head -1 || true)"
    if [[ -z "$probe_line" ]]; then
        # Absent is not automatically a failure: --fly profiles never probe.
        echo "exterior-smoke[$label]: INFO - no spawn ground probe (FlyCam profile)"
        probe_result="n/a"
    else
        probe_result="$(grep -oE 'result=[a-z-]+' <<< "$probe_line" | cut -d= -f2)"
        local probe_colliders
        probe_colliders="$(grep -oE 'colliders=[0-9]+' <<< "$probe_line" | cut -d= -f2)"
        if [[ "$probe_result" == grounded ]]; then
            echo "exterior-smoke[$label]: PASS ground probe (colliders=$probe_colliders)"
        else
            echo "exterior-smoke[$label]: HARD FAIL - spawn ground probe found no walkable surface: $probe_line"
            hard_fail=1
        fi
    fi

    # EX-05 / #2736 — non-finite pixels in the pre-tonemap HDR scene. The PNG
    # statistics above cannot see these: everything after ACES is clamped to
    # [0,1], so a NaN either reads as white or vanishes. Gate on the running
    # total rather than the last frame, because a NaN is typically transient.
    local health_total
    health_total="$(sed 's/\\n/\n/g' "$debug_log" \
        | grep -oE 'since startup: *rgb=[0-9]+ alpha=[0-9]+' | head -1 || true)"
    if [[ -z "$health_total" ]]; then
        echo "exterior-smoke[$label]: WARN - r.health reported nothing (pre-#2736 binary?)"
    else
        local hrgb halpha
        hrgb="$(grep -oE 'rgb=[0-9]+' <<< "$health_total" | cut -d= -f2)"
        halpha="$(grep -oE 'alpha=[0-9]+' <<< "$health_total" | cut -d= -f2)"
        if (( hrgb != 0 || halpha != 0 )); then
            echo "exterior-smoke[$label]: HARD FAIL - non-finite pre-tonemap pixels (rgb=$hrgb alpha=$halpha)"
            hard_fail=1
        else
            echo "exterior-smoke[$label]: PASS image health (no non-finite pre-tonemap pixels)"
        fi
    fi

    # EX-05 / #2368 — the same question one step upstream: are the environment
    # *inputs* usable? `r.health` above counts pixels, which a NaN only reaches
    # when something multiplies it into the frame; a NaN sun colour behind a
    # zero-intensity sun leaves the image clean and the resource broken. The
    # rules and their justification live in `commands/env_health.rs`; the
    # script only reads the verdict.
    local env_report
    env_report="$profile_dir/env-health.log"
    # `byro-dbg` wraps each command result as one JSON string, so the first
    # and last lines of a multi-line reply carry the `byro> "` prompt and the
    # closing quote. Strip both before anchoring, or the verdict's own header
    # line never matches.
    sed 's/\\n/\n/g' "$debug_log" \
        | sed -e 's/^byro> "//' -e 's/"$//' \
        | grep '^env:' > "$env_report" || true
    if [[ ! -s "$env_report" ]]; then
        echo "exterior-smoke[$label]: WARN - env.health reported nothing (pre-#2368 binary?)"
        env_result=absent
    elif ! grep -Fq 'lighting=present sky=present' "$env_report"; then
        echo "exterior-smoke[$label]: HARD FAIL - environment resources missing after load:"
        sed 's/^/    /' "$env_report"
        env_result=no-resources
        hard_fail=1
    elif grep -q '^env: FAIL' "$env_report"; then
        local env_fail_count
        env_fail_count="$(grep -c '^env: FAIL' "$env_report")"
        echo "exterior-smoke[$label]: HARD FAIL - $env_fail_count unusable environment value(s):"
        grep '^env: FAIL' "$env_report" | sed 's/^/    /'
        env_result="bad=$env_fail_count"
        hard_fail=1
    else
        echo "exterior-smoke[$label]: PASS environment values"
        env_result=ok
    fi

    if [[ "$MODE" == soak ]]; then
        # `byro-dbg` renders a multi-line command result as a single
        # JSON-escaped line, so `^ownership:` never anchors in the raw log.
        # Unescape into a sibling file and gate on that; it also leaves the
        # ownership table readable in the retained artifacts.
        local owners_log="$profile_dir/ownership.log"
        sed 's/\\n/\n/g' "$debug_log" > "$owners_log"

        local owner_cycles owner_fail_count
        owner_cycles="$(grep -oE 'ownership: [0-9]+ cycle\(s\) recorded' "$owners_log" \
            | grep -oE '[0-9]+' | head -1 || true)"
        owner_fail_count="$(grep -c '^ownership: FAIL' "$owners_log" || true)"
        : "${owner_cycles:=0}" "${owner_fail_count:=0}"

        if ! grep -q '^ownership:' "$owners_log"; then
            echo "exterior-smoke[$label]: HARD FAIL - world.owners produced no report"
            ownership=missing
            hard_fail=1
        elif grep -Fq 'no baseline recorded' "$owners_log"; then
            # The baseline is taken engine-side at the first return to origin.
            # Its absence means the traversal never completed one cycle, so the
            # run proves nothing about reclamation and must not read as a pass.
            echo "exterior-smoke[$label]: HARD FAIL - soak never completed a baseline traversal"
            ownership=no-baseline
            hard_fail=1
        elif (( owner_cycles < 4 )); then
            echo "exterior-smoke[$label]: HARD FAIL - only $owner_cycles ownership cycles recorded (need 4 for a growth verdict)"
            ownership="cycles=$owner_cycles"
            hard_fail=1
        elif (( owner_fail_count > 0 )); then
            echo "exterior-smoke[$label]: HARD FAIL - $owner_fail_count leaked/growing owner class(es):"
            grep '^ownership: FAIL' "$owners_log" | sed 's/^/    /'
            ownership="leaks=$owner_fail_count"
            hard_fail=1
        else
            echo "exterior-smoke[$label]: PASS ownership reclaimed over $owner_cycles cycles"
            ownership="ok/$owner_cycles"
        fi
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
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$label" "$result" "$entities" "$draws" "$IMAGE_MEAN" \
        "$IMAGE_STDDEV" "$env_result" "$missing_textures" "$failed_nifs" "$crossings" \
        "$full_samples" "$full_max_ms" "$full_superseded" "$lod_samples" \
        "$lod_max_ms" "$lod_superseded" "$frame_p50_ms" "$frame_p95_ms" \
        "$frame_max_ms" "$ownership" "$probe_result" >> "$SUMMARY"
    return "$hard_fail"
}

fnv_run () {
    local esm="$FNV_DATA/FalloutNV.esm"
    local meshes="$FNV_DATA/Fallout - Meshes.bsa"
    local textures="$FNV_DATA/Fallout - Textures.bsa"
    profile_ready fnv "$esm" "$meshes" "$textures" || return 0
    run_profile fnv "$FNV_DATA" WastelandNV 0,0 2500 700 \
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
    run_profile oblivion "$OBLIVION_DATA" Tamriel 0,0 3500 1300 \
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

    local grid="2,-4"
    if [[ "$MODE" == cycle ]]; then
        # BleakfallsBarrowPath: the established WATAL water-adjacent streaming
        # fixture. Static/population baselines retain their historical grid.
        grid="2,-10"
    fi
    local args=(--esm "$esm" --grid "$grid" --radius 1 --wrld Tamriel)
    args+=(--bsa "$SKYRIM_DATA/Skyrim - Meshes0.bsa")
    args+=(--bsa "$SKYRIM_DATA/Skyrim - Meshes1.bsa")
    for archive in "$SKYRIM_DATA"/Skyrim\ -\ Textures{0..8}.bsa; do
        args+=(--textures-bsa "$archive")
    done
    run_profile skyrim "$SKYRIM_DATA" Tamriel "$grid" 3500 500 "${args[@]}"
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
    run_profile fo4 "$FO4_DATA" Commonwealth 0,0 30000 12000 "${args[@]}"
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
        echo "Usage: $0 [fnv|fo3|oblivion|skyrim|fo4|all] [static|boundary|soak|cycle]"
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
