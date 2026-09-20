#!/usr/bin/env bash
# Deterministic real-content ground-cover reference captures (EXAL §11.5).
#
# The four named cases are intentionally kept in one harness: the two views
# of each worldspace differ only by a 180-degree camera turn, and the game
# clock is frozen at the same low-sun hour for every capture.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fnv_data="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
skyrim_data="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
output_root="${BYROREDUX_GROUNDCOVER_EVAL_OUT:-${repo_root}/target/renderer-eval-groundcover}"
frames="${BYROREDUX_GROUNDCOVER_EVAL_FRAMES:-180}"
runner="${BYROREDUX_RENDER_EVAL_RUNNER:-}"
# Optional deterministic camera path for temporal acceptance captures.  The
# normal reference suite stays static; a non-static path switches to the
# fixed-dt renderer-stepped bench mode required by the engine.
bench_camera="${BYROREDUX_GROUNDCOVER_EVAL_BENCH_CAMERA:-static}"
# Optional comma-separated subset of the four stable case names.  The default
# remains the full review suite; this exists so an external runner with a
# short observation deadline can execute and retain each deterministic case
# independently without copying its pose or invocation.
case_filter="${BYROREDUX_GROUNDCOVER_EVAL_CASES:-}"
# Optional native output resolution for projected-pixel LOD review.  Passed
# directly to the engine so Wayland compositor scaling cannot change it.
window_size="${BYROREDUX_GROUNDCOVER_EVAL_WINDOW_SIZE:-}"

# These are renderer Y-up positions framing the established exterior smoke
# cells. Keep overrides explicit so an audited pose change cannot silently
# replace the reference frame. At 07:00 the default weather's sun is low;
# BYRO_HOUR freezes the game clock for the entire bench/screenshot run.
fnv_pos="${BYROREDUX_GC_FNV_CAMERA_POS:-2048,8556,-1848}"
fnv_backlit="${BYROREDUX_GC_FNV_BACKLIT_FORWARD:-0.707,-0.120,-0.695}"
fnv_frontlit="${BYROREDUX_GC_FNV_FRONTLIT_FORWARD:-0.707,0.120,0.695}"
skyrim_pos="${BYROREDUX_GC_SKYRIM_CAMERA_POS:-10240,-4596,14536}"
skyrim_backlit="${BYROREDUX_GC_SKYRIM_BACKLIT_FORWARD:-0.000,-0.447214,-0.894427}"
skyrim_frontlit="${BYROREDUX_GC_SKYRIM_FRONTLIT_FORWARD:-0.000,0.447214,0.894427}"

for asset in \
    "${fnv_data}/FalloutNV.esm" \
    "${fnv_data}/Fallout - Meshes.bsa" \
    "${fnv_data}/Fallout - Textures.bsa" \
    "${skyrim_data}/Skyrim.esm" \
    "${skyrim_data}/Skyrim - Meshes0.bsa" \
    "${skyrim_data}/Skyrim - Meshes1.bsa" \
    "${skyrim_data}/Skyrim - Textures0.bsa"; do
    if [[ ! -f "${asset}" ]]; then
        echo "renderer-eval-groundcover: required asset missing: ${asset}" >&2
        exit 2
    fi
done
if [[ ! "${frames}" =~ ^[1-9][0-9]*$ ]]; then
    echo "renderer-eval-groundcover: invalid frame count: ${frames}" >&2
    exit 2
fi
case "${bench_camera}" in
    static|pan|orbit|dolly|cut|grid-cross|grid-soak) ;;
    *)
        echo "renderer-eval-groundcover: invalid bench camera '${bench_camera}'" >&2
        exit 2
        ;;
esac
if ! command -v magick >/dev/null 2>&1; then
    echo "renderer-eval-groundcover: ImageMagick 'magick' is required for the luminance-std washout precheck" >&2
    exit 2
fi
# Washout precheck (EXAL §11.5): measure luminance standard deviation before
# trusting any frame. Below ~10/255 (sd 0.039) a frame carries no scene
# contrast; the committed 2026-09-15 backlit baseline measured 0.0092-0.0106 —
# the ground-framing poses WERE the veil. A backlit frame under the line is
# stamped in the manifest and fails the run.
washed_out_line="${BYROREDUX_GC_WASHOUT_SD:-0.039}"
bench_mode="renderer-static"
bench_camera_args=()
if [[ "${bench_camera}" != "static" ]]; then
    bench_mode="renderer-stepped"
    bench_camera_args=(--bench-camera "${bench_camera}")
fi
window_size_args=()
if [[ -n "${window_size}" ]]; then
    if [[ ! "${window_size}" =~ ^[1-9][0-9]*x[1-9][0-9]*$ ]]; then
        echo "renderer-eval-groundcover: invalid window size '${window_size}' (expected WIDTHxHEIGHT)" >&2
        exit 2
    fi
    window_size_args=(--window-size "${window_size}")
fi

mkdir -p "${output_root}"
# Always build (cheap when fresh) so a capture can never be attributed to a
# stale binary. Stale-SPIR-V trap (SKYAL §4): a recompiled .spv does not
# reliably trigger a cargo rebuild — after any shader edit run
#   touch crates/renderer/src/lib.rs
# before this build. (A renderer build.rs rerun-if-changed on shaders/** is
# the structural fix, tracked separately.)
cargo build --manifest-path "${repo_root}/Cargo.toml" --release -p byroredux --bin byroredux
engine="${repo_root}/target/release/byroredux"
manifest="${output_root}/manifest.tsv"
# A full suite starts a fresh manifest. A selected case appends only when a
# manifest already exists, allowing a runner to collect the same four stable
# cases in separate bounded invocations without losing earlier evidence.
# (#4482/#4509: the manifest carries the measured luminance sd and the
# washed_out verdict per row.)
if [[ -z "${case_filter}" || ! -s "${manifest}" ]]; then
    printf 'case\tgame\tgrid\thour\tpose\tframes\tpng_sha256\tbench\tgroundcover\tluma_sd\twashed_out\n' > "${manifest}"
fi

capture() {
    local case_name="$1"
    local game="$2"
    local grid="$3"
    local pos="$4"
    local forward="$5"
    shift 5
    if [[ -n "${case_filter}" && ",${case_filter}," != *",${case_name},"* ]]; then
        return
    fi
    local png="${output_root}/${case_name}.png"
    local log="${output_root}/${case_name}.log"
    local runner_args=()
    if [[ -n "${runner}" ]]; then
        read -r -a runner_args <<< "${runner}"
    fi

    echo "renderer-eval-groundcover: ${case_name} (${game} ${grid})"
    BYRO_HOUR=7 \
    RUST_LOG="${BYROREDUX_GROUNDCOVER_EVAL_LOG:-warn}" \
        "${runner_args[@]}" "${engine}" \
        "$@" \
        --grid "${grid}" --radius 1 --wrld "${game}" \
        --fly --camera-pos "${pos}" --camera-forward "${forward}" \
        --bench-frames "${frames}" --bench-mode "${bench_mode}" \
        "${bench_camera_args[@]}" \
        "${window_size_args[@]}" \
        --bench-groundcover-sampling --screenshot "${png}" >"${log}" 2>&1

    if [[ ! -s "${png}" ]]; then
        echo "renderer-eval-groundcover: ${case_name} produced no screenshot" >&2
        tail -n 80 "${log}" >&2 || true
        exit 1
    fi
    local hash bench groundcover luma_sd washed_out
    hash="$(sha256sum "${png}" | awk '{print $1}')"
    bench="$(awk '/^bench:/{line=$0} END{print line}' "${log}")"
    groundcover="$(awk '/^groundcover:/{line=$0} END{print line}' "${log}")"
    if ! luma_sd="$(magick "${png}" -colorspace RGB -format '%[fx:standard_deviation]' info:)"; then
        echo "renderer-eval-groundcover: ${case_name}: luminance-std precheck could not read ${png}" >&2
        exit 1
    fi
    washed_out="no"
    if awk -v sd="${luma_sd}" -v line="${washed_out_line}" 'BEGIN { exit !(sd < line) }'; then
        washed_out="yes"
    fi
    printf '%s\t%s\t%s\t7\t%s|%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "${case_name}" "${game}" "${grid}" "${pos}" "${forward}" "${frames}" \
        "${hash}" "${bench//$'\t'/ }" "${groundcover//$'\t'/ }" \
        "${luma_sd}" "${washed_out}" >> "${manifest}"

    # A missing telemetry row must never mint a well-formed reference row
    # (#4509): an empty bench:/groundcover: column is the engine scattering
    # nothing, or the bench summary never printing, recorded as if measured.
    if [[ -z "${bench}" ]]; then
        echo "renderer-eval-groundcover: ${case_name}: FAIL - no bench: telemetry row in ${log}" >&2
        exit 1
    fi
    if [[ -z "${groundcover}" ]]; then
        echo "renderer-eval-groundcover: ${case_name}: FAIL - no groundcover: telemetry row in ${log}" >&2
        exit 1
    fi
    if [[ "${case_name}" == gc-backlit-* ]]; then
        local chunks blades
        chunks="$(grep -oE 'chunks=[0-9]+' <<< "${groundcover}" | head -1 | cut -d= -f2 || true)"
        blades="$(grep -oE 'blades=[0-9]+' <<< "${groundcover}" | head -1 | cut -d= -f2 || true)"
        if [[ "${chunks}" == "0" && "${blades}" == "0" ]]; then
            echo "renderer-eval-groundcover: ${case_name}: FAIL - backlit case reports annihilated ground cover (${groundcover})" >&2
            exit 1
        fi
        if [[ "${washed_out}" == "yes" ]]; then
            echo "renderer-eval-groundcover: ${case_name}: FAIL - backlit frame is washed out (luminance sd ${luma_sd} < ${washed_out_line}); row stamped washed_out=yes. This baseline IS the veil (#4482): fix the veil, then re-mint with sd recorded." >&2
            exit 1
        fi
    elif [[ "${washed_out}" == "yes" ]]; then
        echo "renderer-eval-groundcover: ${case_name}: WARN - frame under the washout line (sd ${luma_sd} < ${washed_out_line}), stamped washed_out=yes; only gc-backlit-* poses are ground-framing" >&2
    fi
}

# Goodsprings outskirts: WastelandNV 0,0.  The named pose identifiers are
# stable review anchors; do not rename them without updating EXAL §11.5.
capture gc-backlit-fnv WastelandNV 0,0 "${fnv_pos}" "${fnv_backlit}" \
    --esm "${fnv_data}/FalloutNV.esm" \
    --bsa "${fnv_data}/Fallout - Meshes.bsa" \
    --textures-bsa "${fnv_data}/Fallout - Textures.bsa" \
    --textures-bsa "${fnv_data}/Fallout - Textures2.bsa"
capture gc-frontlit-fnv WastelandNV 0,0 "${fnv_pos}" "${fnv_frontlit}" \
    --esm "${fnv_data}/FalloutNV.esm" \
    --bsa "${fnv_data}/Fallout - Meshes.bsa" \
    --textures-bsa "${fnv_data}/Fallout - Textures.bsa" \
    --textures-bsa "${fnv_data}/Fallout - Textures2.bsa"

# Whiterun tundra: Tamriel 2,-4, the established Skyrim exterior fixture.
skyrim_args=(
    --esm "${skyrim_data}/Skyrim.esm"
    --bsa "${skyrim_data}/Skyrim - Meshes0.bsa"
    --bsa "${skyrim_data}/Skyrim - Meshes1.bsa"
)
for archive in "${skyrim_data}"/Skyrim\ -\ Textures{0..8}.bsa; do
    skyrim_args+=(--textures-bsa "${archive}")
done
capture gc-backlit-skyrim Tamriel 2,-4 "${skyrim_pos}" "${skyrim_backlit}" "${skyrim_args[@]}"
capture gc-frontlit-skyrim Tamriel 2,-4 "${skyrim_pos}" "${skyrim_frontlit}" "${skyrim_args[@]}"

echo "renderer-eval-groundcover: artifacts written to ${output_root}"
echo "renderer-eval-groundcover: manifest: ${manifest}"
