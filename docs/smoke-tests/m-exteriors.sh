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
#   water     fixed real-data waterline poses per profile. Captures one
#             frame above and one below the same authored surface (plus the
#             ungated `water_term` / `water_normal` oracle views at the surface
#             pose), then gates WATR provenance, canonical volume membership, current provenance,
#             image health, and a material above/below visual delta (EX-13/W0).
#
# Usage:
#   docs/smoke-tests/m-exteriors.sh [fnv|fo3|oblivion|skyrim|fo4|all] [static|boundary|soak|cycle|water]
#
# Every profile has a frozen `water` fixture (FNV/Skyrim since W0; FO3,
# Oblivion and FO4 since W2).
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
    water)    BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}" ;;
    boundary) BENCH_FRAMES="${BYROREDUX_BOUNDARY_FRAMES:-900}" ;;
    # Six out-and-back traversals need materially more logical frames than the
    # single one-way pass; the clock also pauses on every boundary.
    soak)     BENCH_FRAMES="${BYROREDUX_SOAK_FRAMES:-1800}" ;;
    *)
        echo "Usage: $0 [fnv|fo3|oblivion|skyrim|fo4|all] [static|boundary|soak|cycle|water]"
        exit 2
        ;;
esac
TIMEOUT_SECONDS="${BYROREDUX_SMOKE_TIMEOUT:-240}"
# #4491 cycle-mode composite_term pixel floors ("sky responds to the sun"
# in pixels). MIN_SUN_DELTA: minimum noon-vs-night pre-tonemap mean luminance
# delta. NOON_MIN_SD: the ~10/255 (0.039) washout line — a noon frame flatter
# than this is the veil class, not a sky. Both are recalibratable via env if
# a fixture legitimately frames low contrast.
CYCLE_MIN_SUN_DELTA="${BYROREDUX_CYCLE_MIN_SUN_DELTA:-0.02}"
CYCLE_NOON_MIN_SD="${BYROREDUX_CYCLE_NOON_MIN_SD:-0.039}"
ARTIFACT_DIR="${BYROREDUX_EXTERIOR_ARTIFACT_DIR:-$(mktemp -d /tmp/byro-exterior-smoke.XXXXXX)}"
SUMMARY="$ARTIFACT_DIR/summary.tsv"
ACTIVE_PID=""
# #4489 — a data-less run must be distinguishable from a green one by exit
# code (README contract: missing game data is SKIP/77, never a pass).
SKIP_COUNT=0
RAN_COUNT=0

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

# Always build (cheap when fresh): a pre-existing binary is not evidence it
# is current, and a stale one invalidates every capture below. Stale-SPIR-V
# trap (SKYAL §4): a recompiled .spv does not reliably trigger a cargo
# rebuild — after any shader edit run
#   touch crates/renderer/src/lib.rs
# before this build. (A renderer build.rs rerun-if-changed on shaders/** is
# the structural fix, tracked separately.)
echo "exterior-smoke: building release engine and debug client"
cargo build --release --quiet -p byroredux -p byro-dbg

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
        SKIP_COUNT=$((SKIP_COUNT + 1))
        return 1
    fi
    return 0
}

# Read one image's RGB mean/standard deviation into FRAME_MEAN/FRAME_STDDEV
# without gating. Shared by image_health's blank/white-out gate and the
# cycle-mode composite_term pixel invariant (#4491), which needs the raw
# stats of captures the blank-frame floors do not apply to.
frame_stats () {
    local image="$1"
    local mean_out stddev_out
    if [[ ! -s "$image" ]]; then
        return 1
    fi
    read -r mean_out stddev_out < <(
        magick "$image" -colorspace RGB \
            -format '%[fx:mean] %[fx:standard_deviation]\n' info:
    )
    FRAME_MEAN="$mean_out"
    FRAME_STDDEV="$stddev_out"
}

image_health () {
    local image="$1"
    if ! frame_stats "$image"; then
        return 1
    fi
    IMAGE_MEAN="$FRAME_MEAN"
    IMAGE_STDDEV="$FRAME_STDDEV"

    # Reject effectively black/white and near-solid frames. These thresholds
    # intentionally leave generous headroom for dark interiors accidentally
    # selected by bad WRLD wiring while catching the historical white-out.
    awk -v mean="$IMAGE_MEAN" -v sd="$IMAGE_STDDEV" \
        'BEGIN { exit !(mean > 0.01 && mean < 0.98 && sd > 0.005) }'
}

# Args: label, data_dir, worldspace, grid, entity_floor, draw_floor,
#       missing_tex_baseline, CLI args...
#
# `missing_tex_baseline` is the calibrated steady-state unique-missing-texture
# count for the profile's fixture on a healthy install; the script hard-fails
# at >= 2x it (#4508) because a checkerboard-placeholder × normal-map "chrome"
# frame passes every image/population gate while the texture pipeline is
# broken. Initial values: the legitimate FNV steady state is 1 entry (the
# `<no path, no material>` placeholder, see ROADMAP #chrome) and the
# historical chrome-class failure measured 39 unique on FNV, so the FNV-class
# ceilings land between the two; FO4 carries the largest corpus. Re-measure on
# a clean install before tightening.
run_profile () {
    local label="$1"
    local data_dir="$2"
    local worldspace="$3"
    local grid="$4"
    local entity_floor="$5"
    local draw_floor="$6"
    local missing_tex_baseline="$7"
    shift 7

    RAN_COUNT=$((RAN_COUNT + 1))

    local profile_dir="$ARTIFACT_DIR/$label"
    local stdout_log="$profile_dir/engine.stdout.log"
    local stderr_log="$profile_dir/engine.stderr.log"
    local debug_log="$profile_dir/debug.log"
    local screenshot="$profile_dir/frame.png"
    local command_file="$profile_dir/command.txt"
    local water_surface_pos="" water_submerged_pos="" water_look="" water_under_look=""
    local water_source="" water_requires_flow=0
    if [[ "$MODE" == water ]]; then
        case "$label" in
            fnv)
                # Lake Mead: full-detail WastelandNV CELL water, surface Y=2600.
                # Use the neighbouring open-water tile rather than the
                # crashed-aircraft placement in the fixture's centre.
                water_surface_pos="84000 2700 -55000"
                water_submerged_pos="84000 2450 -55000"
                water_look="45 -12"
                water_source="0x001009CA"
                ;;
            skyrim)
                # White River below Riverwood, authored RiverWaterFlowNE tile at
                # (5,-10), surface Y=-250. The former (3,-11) pose was frozen
                # while the LAND wet mask was mirrored north-south (09432157a);
                # that column is dry rock. Every pose below is chosen with
                # `water_fixture_probe` (plugin example): the whole 3x3 LAND
                # neighbourhood sits at least this far below the surface.
                water_surface_pos="22016 -50 40576"
                water_submerged_pos="22016 -380 40576"
                water_look="0 -15"
                water_source="0x000E717C"
                water_requires_flow=1
                ;;
            fo3)
                # Potomac at Wasteland (10,-9): explicit XCLW 10500, WATR
                # inherited from the WRLD (NAM2), open water >= 644 deep.
                water_surface_pos="44416 10700 34432"
                water_submerged_pos="44416 10250 34432"
                water_look="45 -12"
                # Underwater fog far is 700 BU: a downward view is uniformly
                # fogged, so look up through Snell's window instead (steeper
                # than ~41 degrees; shallower rays totally internally reflect
                # the riverbed back).
                water_under_look="45 70"
                water_source="0x00030009"
                ;;
            oblivion)
                # Tamriel (13,7): NAM2-only worldspace, sea level Z=0 by the
                # Oblivion GameVariant, open water >= 1352 deep.
                water_surface_pos="54400 200 -32384"
                water_submerged_pos="54400 -300 -32384"
                water_look="45 -12"
                water_source="0x00000018"
                ;;
            fo4)
                # Commonwealth (-4,11): explicit CELL XCWT ExtLakeWater, surface
                # Y=750, open water >= 590 deep.
                water_surface_pos="-12416 950 -46592"
                water_submerged_pos="-12416 500 -46592"
                water_look="45 -12"
                # ExtLakeWater authors underwater fog -6400..1700: nearly
                # opaque from the eye outward, so look up through Snell's
                # window at the sky instead.
                water_under_look="45 70"
                water_source="0x000C8633"
                ;;
            *)
                echo "exterior-smoke[$label]: HARD FAIL - no frozen water fixture for this profile"
                return 1
                ;;
        esac
        : "${water_under_look:=$water_look}"
    fi
    # --upscaler taa in every mode (EXT-D7-2026-09-19-06 item 2, MANUAL
    # ACCEPTANCE): the FSR reactive / linear-compression ground-cover masks
    # (#4297) are therefore never exercised against the real upscaler on a
    # live frame here — their only guards are shader-text `contains` scans.
    # The rendered-mask verdict is human-only (EXAL-GC §12.14 upscaler
    # contract).
    local bench_args=(--bench-frames "$BENCH_FRAMES" --bench-hold --screenshot "$screenshot" --upscaler taa)
    if [[ "$MODE" == boundary ]]; then
        bench_args+=(--bench-mode renderer-stepped --bench-camera grid-cross --fly)
    elif [[ "$MODE" == soak ]]; then
        bench_args+=(--bench-mode renderer-stepped --bench-camera grid-soak --fly)
    elif [[ "$MODE" == water ]]; then
        bench_args+=(--bench-mode system-live --fly)
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
        # Per-phase composite_term captures (#4491): the pre-bloom,
        # pre-tonemap, linear view (SKYAL §4) so the pixel invariant below
        # gates sky assembly, not tone mapping or bloom. `render.debug final`
        # restores the ordinary view before the next phase's health sample.
        env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" > "$debug_log" 2>&1 <<EOF || true
time.pause
time.set 06:00
time.show
env.health
water.dump
r.health
screenshot $profile_dir/sunrise.png
render.debug composite_term
screenshot $profile_dir/sunrise-composite-term.png
render.debug final
time.set 12:00
time.show
env.health
water.dump
r.health
screenshot $profile_dir/noon.png
render.debug composite_term
screenshot $profile_dir/noon-composite-term.png
render.debug final
time.set 23:00
time.show
env.health
water.dump
r.health
screenshot $profile_dir/night.png
render.debug composite_term
screenshot $profile_dir/night-composite-term.png
render.debug final
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
time.set 12:00
time.show
render.debug composite_term
screenshot $profile_dir/noon-2-composite-term.png
render.debug final
.quit
EOF
    elif [[ "$MODE" == water ]]; then
        env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" > "$debug_log" 2>&1 <<EOF || true
time.pause
time.set 12:00
cam.pos $water_surface_pos
input.look $water_look
water.dump
r.health
screenshot $profile_dir/water-surface.png
render.debug water_term
screenshot $profile_dir/water-term.png
render.debug water_normal
screenshot $profile_dir/water-normal.png
render.debug final
cam.pos $water_submerged_pos
input.look $water_under_look
water.dump
r.health
screenshot $profile_dir/water-underwater.png
stats
light.dump
env.health
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
        # MANUAL ACCEPTANCE (EXT-D7-2026-09-19-06 item 3): cloud march
        # rendering (coverage → density, WTHR layer mips) has no pixel
        # invariant in this mode — the gates below see image health and CPU
        # environment state only, never cloud shape. Unit tests pin the
        # source inputs; the rendered cloudscape verdict is human-only
        # (SKYAL §2.3).
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

        # #4491 — "sky responds to the sun" checked in pixels, not in the
        # SkyParamsRes struct the sun-intensity greps above read. The
        # per-phase composite_term captures (pre-bloom, pre-tonemap, linear
        # — SKYAL §4) must show a noon-vs-night mean luminance delta above
        # CYCLE_MIN_SUN_DELTA and above twice the run-to-run noise floor
        # measured by capturing noon twice (SKYAL §4's two-capture rule), and
        # the noon frame must keep scene contrast above the ~10/255 washout
        # line. A sky that ignores the sun, or a repeat of the SKYAL §1.1
        # bloom-gain wash, fails here even with a perfect CPU-side sun value.
        local term_phase term_image
        local noon1_mean="" noon1_sd="" noon2_mean="" night_mean=""
        local term_captures_complete=1
        for term_phase in sunrise noon night; do
            term_image="$profile_dir/$term_phase-composite-term.png"
            if frame_stats "$term_image"; then
                case "$term_phase" in
                    noon)  noon1_mean="$FRAME_MEAN" noon1_sd="$FRAME_STDDEV" ;;
                    night) night_mean="$FRAME_MEAN" ;;
                esac
            else
                echo "exterior-smoke[$label]: HARD FAIL - $term_phase composite_term capture missing or unreadable"
                hard_fail=1
                term_captures_complete=0
            fi
        done
        if frame_stats "$profile_dir/noon-2-composite-term.png"; then
            noon2_mean="$FRAME_MEAN"
        else
            echo "exterior-smoke[$label]: HARD FAIL - noon noise-floor composite_term capture missing or unreadable"
            hard_fail=1
            term_captures_complete=0
        fi
        if (( term_captures_complete != 0 )); then
            if awk -v noon="$noon1_mean" -v night="$night_mean" \
                    -v repeat="$noon2_mean" -v sd="$noon1_sd" \
                    -v min_delta="$CYCLE_MIN_SUN_DELTA" -v min_sd="$CYCLE_NOON_MIN_SD" \
                    'BEGIN {
                        signal = noon - night;
                        noise = repeat - noon; if (noise < 0) noise = -noise;
                        floor = min_delta; if (2 * noise > floor) floor = 2 * noise;
                        exit !(signal > floor && sd > min_sd)
                    }'; then
                echo "exterior-smoke[$label]: PASS sky responds to the sun in pixels (composite_term noon=$noon1_mean night=$night_mean noon-repeat=$noon2_mean sd=$noon1_sd)"
            else
                echo "exterior-smoke[$label]: HARD FAIL - composite_term pixels contradict the sun cycle (noon=$noon1_mean night=$night_mean noon-repeat=$noon2_mean sd=$noon1_sd; need noon-night delta > $CYCLE_MIN_SUN_DELTA and > 2x repeat noise, noon sd > $CYCLE_NOON_MIN_SD)"
                hard_fail=1
            fi
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

    if [[ "$MODE" == water ]]; then
        local water_log="$profile_dir/water-transition.log"
        local surface_image="$profile_dir/water-surface.png"
        local submerged_image="$profile_dir/water-underwater.png"
        sed 's/\\n/\n/g' "$debug_log" > "$water_log"

        if ! grep -Eq 'camera=[0-9]+ pos=\[[^]]+\] submerged=false depth=0[.]00 material=none' "$water_log"; then
            echo "exterior-smoke[$label]: HARD FAIL - above-surface pose did not report dry camera state"
            hard_fail=1
        elif ! grep -Eq "camera=[0-9]+ pos=\\[[^]]+\\] submerged=true depth=[1-9][0-9.]* material=$water_source" "$water_log"; then
            echo "exterior-smoke[$label]: HARD FAIL - below-surface pose did not enter WATR $water_source"
            hard_fail=1
        elif ! grep -E "plane=[0-9]+ camera_inside=true .*source=$water_source" "$water_log" >/dev/null; then
            echo "exterior-smoke[$label]: HARD FAIL - WATR $water_source has no camera-containing canonical volume"
            hard_fail=1
        elif (( water_requires_flow != 0 )) \
                && grep -E "plane=[0-9]+ camera_inside=true .*source=$water_source" "$water_log" \
                    | grep -Fq 'flow=none'; then
            echo "exterior-smoke[$label]: HARD FAIL - authored river WATR $water_source lost its canonical flow"
            hard_fail=1
        else
            echo "exterior-smoke[$label]: PASS waterline transition and WATR $water_source provenance"
        fi

        local water_health_samples
        water_health_samples="$(grep -cE 'since startup: *rgb=0 alpha=0' "$water_log" || true)"
        if (( water_health_samples < 2 )); then
            echo "exterior-smoke[$label]: HARD FAIL - waterline captures did not both report finite pre-tonemap output"
            hard_fail=1
        fi

        if image_health "$surface_image"; then
            echo "exterior-smoke[$label]: PASS water surface image mean=$IMAGE_MEAN stddev=$IMAGE_STDDEV"
        else
            echo "exterior-smoke[$label]: HARD FAIL - water surface screenshot is unhealthy"
            hard_fail=1
        fi
        if image_health "$submerged_image"; then
            echo "exterior-smoke[$label]: PASS underwater image mean=$IMAGE_MEAN stddev=$IMAGE_STDDEV"
        else
            echo "exterior-smoke[$label]: HARD FAIL - underwater screenshot is unhealthy"
            hard_fail=1
        fi

        # MANUAL ACCEPTANCE (EXT-D7-2026-09-19-06 item 5): this oracle is
        # near-vacuous by construction — a >0.01 full-frame mean difference
        # between captures 250-450 units apart with different look angles
        # clears on almost any scene change, so it proves the two captures
        # differ, not that the above/below MATERIAL transition is correct.
        # Material-fidelity acceptance beyond this stays human-only
        # (WATAL §8).
        local waterline_delta
        waterline_delta="$(magick "$surface_image" "$submerged_image" \
            -compose difference -composite -colorspace RGB \
            -format '%[fx:mean]' info: 2>/dev/null || true)"
        if [[ -z "$waterline_delta" ]] \
                || ! awk -v delta="$waterline_delta" 'BEGIN { exit !(delta > 0.01) }'; then
            echo "exterior-smoke[$label]: HARD FAIL - above/below captures have no material visual transition (delta=${waterline_delta:-missing})"
            hard_fail=1
        else
            echo "exterior-smoke[$label]: PASS waterline visual delta=$waterline_delta"
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
    # #4508 — the WARN above is deliberate for content drift, but a texture
    # pipeline this broken produces a "chrome" frame (checker placeholder ×
    # valid normal map) whose sd clears every image gate. At 2x the profile's
    # calibrated baseline it stops being drift and fails the run.
    if [[ "$missing_textures" =~ ^[0-9]+$ ]] \
            && (( missing_textures >= missing_tex_baseline * 2 )); then
        echo "exterior-smoke[$label]: HARD FAIL - $missing_textures unique missing textures reached 2x the calibrated baseline ($missing_tex_baseline)"
        hard_fail=1
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
    local grid="0,0"
    if [[ "$MODE" == water ]]; then
        # Lake Mead: contiguous full-detail CELL water around grid (19,13).
        grid="19,13"
    fi
    run_profile fnv "$FNV_DATA" WastelandNV "$grid" 2500 700 12 \
        --esm "$esm" --grid "$grid" --radius 1 --wrld WastelandNV \
        --bsa "$meshes" --textures-bsa "$textures"
}

fo3_run () {
    local esm="$FO3_DATA/Fallout3.esm"
    local meshes="$FO3_DATA/Fallout - Meshes.bsa"
    local textures="$FO3_DATA/Fallout - Textures.bsa"
    profile_ready fo3 "$esm" "$meshes" "$textures" || return 0
    # MegatonWorld (0,0) is a valid empty dummy CELL. (-1,-7) is the
    # populated MegatonPlaza foreground and is intentionally the smoke gate.
    local world="MegatonWorld" grid="-1,-7"
    if [[ "$MODE" == water ]]; then
        world="Wasteland"
        grid="10,-9"
    fi
    run_profile fo3 "$FO3_DATA" "$world" "$grid" 2000 700 12 \
        --esm "$esm" --grid "$grid" --radius 1 --wrld "$world" \
        --bsa "$meshes" --textures-bsa "$textures"
}

oblivion_run () {
    local esm="$OBLIVION_DATA/Oblivion.esm"
    local meshes="$OBLIVION_DATA/Oblivion - Meshes.bsa"
    local textures="$OBLIVION_DATA/Oblivion - Textures - Compressed.bsa"
    profile_ready oblivion "$esm" "$meshes" "$textures" || return 0
    local grid="0,0"
    if [[ "$MODE" == water ]]; then
        grid="13,7"
    fi
    run_profile oblivion "$OBLIVION_DATA" Tamriel "$grid" 3500 1300 12 \
        --esm "$esm" --grid "$grid" --radius 1 --wrld Tamriel \
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
    elif [[ "$MODE" == water ]]; then
        # Riverwood / White River, shared with the W1 traversal fixture.
        grid="4,-11"
    fi
    local args=(--esm "$esm" --grid "$grid" --radius 1 --wrld Tamriel)
    args+=(--bsa "$SKYRIM_DATA/Skyrim - Meshes0.bsa")
    args+=(--bsa "$SKYRIM_DATA/Skyrim - Meshes1.bsa")
    for archive in "$SKYRIM_DATA"/Skyrim\ -\ Textures{0..8}.bsa; do
        args+=(--textures-bsa "$archive")
    done
    run_profile skyrim "$SKYRIM_DATA" Tamriel "$grid" 3500 500 12 "${args[@]}"
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

    local grid="0,0"
    if [[ "$MODE" == water ]]; then
        grid="-4,11"
    fi
    local args=(--esm "$esm" --grid "$grid" --radius 1 --wrld Commonwealth)
    args+=(--bsa "$FO4_DATA/Fallout4 - Meshes.ba2")
    args+=(--bsa "$FO4_DATA/Fallout4 - MeshesExtra.ba2")
    for archive in "$FO4_DATA"/Fallout4\ -\ Textures{1..9}.ba2; do
        args+=(--textures-bsa "$archive")
    done
    args+=(--textures-bsa "$FO4_DATA/Fallout4 - TexturesPatch.ba2")
    args+=(--materials-ba2 "$FO4_DATA/Fallout4 - Materials.ba2")
    run_profile fo4 "$FO4_DATA" Commonwealth "$grid" 30000 12000 24 "${args[@]}"
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
        echo "Usage: $0 [fnv|fo3|oblivion|skyrim|fo4|all] [static|boundary|soak|cycle|water]"
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
# #4489 — README contract (lines 7-8): missing game data is an explicit SKIP
# with exit code 77, never a pass. When nothing actually ran, the SKIP rows
# above stay visible but the exit code says "not a green run", the same
# contract w1-water-traversal.sh honours and playable-smoke.yml promotes to a
# CI error.
if (( RAN_COUNT == 0 )); then
    echo "exterior-smoke: SKIP - no selected profile ran (${SKIP_COUNT} skipped: missing game data)"
    exit 77
fi
echo "exterior-smoke: PASS - every installed selected profile passed"
