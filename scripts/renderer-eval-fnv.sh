#!/usr/bin/env bash
# Deterministic real-content renderer diagnostics for the Prospector Saloon.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
data_root="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
output_root="${BYROREDUX_FNV_EVAL_OUT:-${repo_root}/target/renderer-eval-fnv}"
frames="${BYROREDUX_FNV_EVAL_FRAMES:-64}"
cell="${BYROREDUX_FNV_EVAL_CELL:-GSProspectorSaloonInterior}"
runner="${BYROREDUX_RENDER_EVAL_RUNNER:-}"

for asset in \
    "${data_root}/FalloutNV.esm" \
    "${data_root}/Fallout - Meshes.bsa" \
    "${data_root}/Fallout - Textures.bsa" \
    "${data_root}/Fallout - Textures2.bsa"; do
    if [[ ! -f "${asset}" ]]; then
        echo "renderer-eval-fnv: required asset missing: ${asset}" >&2
        exit 2
    fi
done
if [[ ! "${frames}" =~ ^[1-9][0-9]*$ ]]; then
    echo "renderer-eval-fnv: invalid frame count: ${frames}" >&2
    exit 2
fi

mkdir -p "${output_root}"
cargo build --manifest-path "${repo_root}/Cargo.toml" --release -p byroredux --bin byroredux
engine="${repo_root}/target/release/byroredux"
manifest="${output_root}/manifest.tsv"
printf 'case\trotation_mode\tdebug_flags\tframes\tpng_sha256\tbench\n' > "${manifest}"

capture() {
    local case_name="$1"
    local rotation_mode="$2"
    local debug_flags="$3"
    local png="${output_root}/${case_name}.png"
    local log="${output_root}/${case_name}.log"
    local runner_args=()
    if [[ -n "${runner}" ]]; then
        read -r -a runner_args <<< "${runner}"
    fi

    echo "renderer-eval-fnv: ${case_name} (rotation=${rotation_mode}, debug=${debug_flags})"
    BYROREDUX_FIXED_DT=0 \
    BYROREDUX_RENDER_DEBUG="${debug_flags}" \
    RUST_LOG="${BYROREDUX_FNV_EVAL_LOG:-warn}" \
        "${runner_args[@]}" "${engine}" \
        --esm "${data_root}/FalloutNV.esm" \
        --cell "${cell}" \
        --bsa "${data_root}/Fallout - Meshes.bsa" \
        --textures-bsa "${data_root}/Fallout - Textures.bsa" \
        --textures-bsa "${data_root}/Fallout - Textures2.bsa" \
        --rotation-mode "${rotation_mode}" \
        --bench-frames "${frames}" \
        --screenshot "${png}" >"${log}" 2>&1

    if [[ ! -s "${png}" ]]; then
        echo "renderer-eval-fnv: ${case_name} produced no screenshot" >&2
        tail -n 80 "${log}" >&2 || true
        exit 1
    fi
    local hash bench
    hash="$(sha256sum "${png}" | awk '{print $1}')"
    bench="$(awk '/^bench:/{line=$0} END{print line}' "${log}")"
    bench="${bench//$'\t'/ }"
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
        "${case_name}" "${rotation_mode}" "${debug_flags}" "${frames}" "${hash}" "${bench}" \
        >> "${manifest}"
}

# Coordinate convention A/B. Mode 1 is the current shipping path.
for mode in 0 1 2 3; do
    capture "prospector_rotation_${mode}" "${mode}" "0"
done

# Material and lighting decomposition on the shipping coordinate mode.
capture "prospector_material_state" "1" "0x100000"
capture "prospector_raw_indirect" "1" "0x80000"
capture "prospector_gi_bounce" "1" "0x200000"
capture "prospector_no_atrous" "1" "0x4000"

echo "renderer-eval-fnv: artifacts written to ${output_root}"
echo "renderer-eval-fnv: manifest: ${manifest}"
