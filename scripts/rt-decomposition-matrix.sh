#!/usr/bin/env bash
# Pair runtime-disabled and compile-time-disabled main-pass RT variants.
#
# Runtime variants retain the shipping shader's allocation shape and measure
# avoided execution. Compile-time variants set RT_COMPILE_ABLATION_MASK before
# compiling triangle.frag, permitting driver DCE/register reduction. Their
# difference estimates the value of pipeline specialization.
#
# Usage: scripts/rt-decomposition-matrix.sh [runs] [frames]

set -euo pipefail

runs="${1:-3}"
frames="${2:-1000}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
shader_root="${repo_root}/crates/renderer/shaders"
shader="${shader_root}/triangle.frag"
artifact="${shader}.spv"
output_root="${BYROREDUX_RT_OUT:-${repo_root}/target/rt-decomposition}"
scene="${BYROREDUX_RT_SCENE:-medtek}"
runner="${BYROREDUX_RT_RUNNER:-}"
compiler="${GLSLANG_VALIDATOR:-glslangValidator}"

if [[ -n "$(git -C "${repo_root}" status --porcelain --untracked-files=normal)" ]]; then
    echo "rt-decomposition: working tree must be clean; compile variants are built from HEAD" >&2
    exit 2
fi

if [[ ! "${runs}" =~ ^[1-9][0-9]*$ ]]; then
    echo "rt-decomposition: runs must be a positive integer, got '${runs}'" >&2
    exit 2
fi
if [[ ! "${frames}" =~ ^[1-9][0-9]*$ ]]; then
    echo "rt-decomposition: frames must be a positive integer, got '${frames}'" >&2
    exit 2
fi
if ! command -v "${compiler}" >/dev/null 2>&1; then
    echo "rt-decomposition: shader compiler not found: ${compiler}" >&2
    exit 2
fi
if ! command -v spirv-val >/dev/null 2>&1; then
    echo "rt-decomposition: spirv-val not found" >&2
    exit 2
fi

scene_args() {
    case "$1" in
        cornell)    args=(--cornell) ;;
        prospector) args=(--game fnv --cell GSProspectorSaloonInterior) ;;
        whiterun)   args=(--game skyrim_se --cell WhiterunBanneredMare) ;;
        medtek)     args=(--game fo4 --cell MedTekResearch01) ;;
        dugout)     args=(--game fo4 --cell DmndDugoutInn01) ;;
        *)
            echo "rt-decomposition: unknown scene '${1}'" >&2
            exit 2
            ;;
    esac
}
scene_args "${scene}"

mkdir -p "${output_root}/bin"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/byro-rt-decomposition.XXXXXX")"
trap 'rm -rf -- "${work_dir}"' EXIT

# Compile in an archive copy, never by swapping the tracked artifact. Besides
# keeping the user's tree clean, this prevents an automated commit/watch task
# from capturing a short-lived evaluation module as the shipping SPIR-V.
git archive --format=tar --output="${work_dir}/source.tar" HEAD
mkdir -p "${work_dir}/source"
tar -xf "${work_dir}/source.tar" -C "${work_dir}/source"
variant_artifact="${work_dir}/source/crates/renderer/shaders/triangle.frag.spv"

runner_args=()
launch_prefix=()
if [[ -n "${runner}" ]]; then
    read -r -a runner_args <<< "${runner}"
    launch_prefix=(env -u WAYLAND_DISPLAY -u GDK_BACKEND XDG_SESSION_TYPE=x11 "${runner_args[@]}")
fi

echo "rt-decomposition: building shipping runtime-variant engine" >&2
cargo build --manifest-path "${repo_root}/Cargo.toml" --release -p byroredux --bin byroredux
runtime_engine="${repo_root}/target/release/byroredux"

declare -A compile_masks=(
    [direct-shadow]=1
    [gi]=2
    [reflection-glass]=4
    [all-main-rays]=8
)
compile_order=(direct-shadow gi reflection-glass all-main-rays)

for feature in "${compile_order[@]}"; do
    mask="${compile_masks[${feature}]}"
    variant_spv="${work_dir}/triangle-${feature}.spv"
    echo "rt-decomposition: compiling ${feature} mask=${mask}" >&2
    "${compiler}" -V "-DRT_COMPILE_ABLATION_MASK=${mask}" \
        -I"${shader_root}" "${shader}" -o "${variant_spv}" >/dev/null
    spirv-val "${variant_spv}"
    cp "${variant_spv}" "${variant_artifact}"
    env CARGO_TARGET_DIR="${output_root}/build" \
        cargo build --manifest-path "${work_dir}/source/Cargo.toml" \
        --release -p byroredux --bin byroredux
    cp "${output_root}/build/release/byroredux" \
        "${output_root}/bin/compile-${feature}"
    cp "${variant_spv}" "${output_root}/bin/compile-${feature}.spv"
done

raw_tsv="${output_root}/raw.tsv"
summary="${output_root}/summary.txt"
metadata="${output_root}/metadata.txt"
revision="$(git -C "${repo_root}" rev-parse HEAD)"
dirty="$(git -C "${repo_root}" status --porcelain --untracked-files=normal)"

{
    echo "revision=${revision}"
    echo "tree_dirty=$(if [[ -n "${dirty}" ]]; then echo true; else echo false; fi)"
    echo "timestamp_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "scene=${scene}"
    echo "runs=${runs}"
    echo "frames=${frames}"
    echo "mode=renderer-static"
    echo "compiler=$(${compiler} -dumpfullversion)"
    echo "shipping_spv_sha256=$(sha256sum "${artifact}" | awk '{print $1}')"
    if command -v nvidia-smi >/dev/null 2>&1; then
        nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null || true
    fi
} > "${metadata}"

printf 'scene\tvariant\tform\tfeature\trun\tdebug_flags\tcompile_mask\tspv_bytes\tmode\tcamera\twall_ms\tfence_ms\tgpu_main_ms\tsim_time_s\tentities\tdraws\tlights\ttlas\tstate_hash\n' > "${raw_tsv}"

run_variant() {
    local variant="$1"
    local form="$2"
    local feature="$3"
    local engine="$4"
    local debug_flags="$5"
    local compile_mask="$6"
    local spv_bytes="$7"
    local run log line status

    for run in $(seq 1 "${runs}"); do
        log="${output_root}/${variant}_${run}.log"
        echo "rt-decomposition: ${variant} run ${run}/${runs}" >&2
        set +e
        env -u BYROREDUX_FIXED_DT \
            BYROREDUX_RENDER_DEBUG="${debug_flags}" \
            RUST_LOG="${BYROREDUX_RT_LOG:-warn}" \
            "${launch_prefix[@]}" timeout 900 "${engine}" "${args[@]}" \
            --bench-frames "${frames}" \
            --bench-mode renderer-static > "${log}" 2>&1
        status=$?
        set -e
        if (( status != 0 )); then
            echo "rt-decomposition: ${variant} run ${run} exited ${status} (see ${log})" >&2
            return 1
        fi
        line="$(awk '/^bench:/{line=$0} END{print line}' "${log}")"
        if [[ -z "${line}" ]]; then
            echo "rt-decomposition: ${variant} run ${run} produced no bench line" >&2
            return 1
        fi
        python3 - "${scene}" "${variant}" "${form}" "${feature}" "${run}" \
            "${debug_flags}" "${compile_mask}" "${spv_bytes}" "${line}" \
            >> "${raw_tsv}" <<'PY'
import re
import sys

scene, variant, form, feature, run, flags, mask, size, line = sys.argv[1:]


def token(key, default="-"):
    match = re.search(rf"(?:^|[ \[])({re.escape(key)})=([^ \]]+)", line)
    return match.group(2) if match else default


print("\t".join([
    scene, variant, form, feature, run, flags, mask, size,
    token("mode"), token("camera"), token("wall_ms"), token("fence"),
    token("gpu_main_render"), token("sim_time_s"), token("entities"),
    token("draws"), token("lights"), token("tlas"), token("state_hash"),
]))
PY
    done
}

shipping_spv_bytes="$(wc -c < "${artifact}")"
run_variant baseline runtime baseline "${runtime_engine}" 0x0 0 "${shipping_spv_bytes}"
run_variant runtime-direct-shadow runtime direct-shadow "${runtime_engine}" 0x8000000 0 "${shipping_spv_bytes}"
run_variant runtime-gi runtime gi "${runtime_engine}" 0x10000000 0 "${shipping_spv_bytes}"
run_variant runtime-reflection-glass runtime reflection-glass "${runtime_engine}" 0x20000000 0 "${shipping_spv_bytes}"
run_variant runtime-all-main-rays runtime all-main-rays "${runtime_engine}" 0x40000000 0 "${shipping_spv_bytes}"

for feature in "${compile_order[@]}"; do
    mask="${compile_masks[${feature}]}"
    compile_spv_bytes="$(wc -c < "${work_dir}/triangle-${feature}.spv")"
    run_variant "compile-${feature}" compile "${feature}" \
        "${output_root}/bin/compile-${feature}" 0x0 "${mask}" "${compile_spv_bytes}"
done

python3 "${repo_root}/scripts/rt_decomposition_report.py" "${raw_tsv}" | tee "${summary}"
