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
bench_mode="renderer-static"
bench_camera_args=()
if [[ "${bench_camera}" != "static" ]]; then
    bench_mode="renderer-stepped"
    bench_camera_args=(--bench-camera "${bench_camera}")
fi

mkdir -p "${output_root}"
cargo build --manifest-path "${repo_root}/Cargo.toml" --release -p byroredux --bin byroredux
engine="${repo_root}/target/release/byroredux"
manifest="${output_root}/manifest.tsv"
# A full suite starts a fresh manifest. A selected case appends only when a
# manifest already exists, allowing a runner to collect the same four stable
# cases in separate bounded invocations without losing earlier evidence.
if [[ -z "${case_filter}" || ! -s "${manifest}" ]]; then
    printf 'case\tgame\tgrid\thour\tpose\tframes\tpng_sha256\tbench\tgroundcover\n' > "${manifest}"
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
        --bench-groundcover-sampling --screenshot "${png}" >"${log}" 2>&1

    if [[ ! -s "${png}" ]]; then
        echo "renderer-eval-groundcover: ${case_name} produced no screenshot" >&2
        tail -n 80 "${log}" >&2 || true
        exit 1
    fi
    local hash bench groundcover
    hash="$(sha256sum "${png}" | awk '{print $1}')"
    bench="$(awk '/^bench:/{line=$0} END{print line}' "${log}")"
    groundcover="$(awk '/^groundcover:/{line=$0} END{print line}' "${log}")"
    printf '%s\t%s\t%s\t7\t%s|%s\t%s\t%s\t%s\t%s\n' \
        "${case_name}" "${game}" "${grid}" "${pos}" "${forward}" "${frames}" \
        "${hash}" "${bench//$'\t'/ }" "${groundcover//$'\t'/ }" >> "${manifest}"
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
