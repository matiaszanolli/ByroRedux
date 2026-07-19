#!/usr/bin/env bash
# Deterministic renderer evaluation capture.
#
# Produces Cornell-box convergence frames plus a final-frame SVGF spatial
# filter A/B. The output directory contains PNGs, raw engine logs, and a TSV
# manifest with the exact revision, environment, flags, hashes, and bench line
# for every capture.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_root="${BYROREDUX_RENDER_EVAL_OUT:-${repo_root}/target/renderer-eval}"
fixed_dt="${BYROREDUX_RENDER_EVAL_DT:-0}"
capture_frames="${BYROREDUX_RENDER_EVAL_FRAMES:-1 8 32 64}"
runner="${BYROREDUX_RENDER_EVAL_RUNNER:-}"

if [[ -e "${output_root}" && ! -d "${output_root}" ]]; then
    echo "renderer-eval: output path exists and is not a directory: ${output_root}" >&2
    exit 2
fi

mkdir -p "${output_root}"

echo "renderer-eval: building release engine"
cargo build --manifest-path "${repo_root}/Cargo.toml" --release -p byroredux --bin byroredux

engine="${repo_root}/target/release/byroredux"
if [[ ! -x "${engine}" ]]; then
    echo "renderer-eval: engine binary missing after build: ${engine}" >&2
    exit 2
fi

manifest="${output_root}/manifest.tsv"
metadata="${output_root}/run-metadata.txt"

{
    echo "revision=$(git -C "${repo_root}" rev-parse HEAD)"
    echo "revision_short=$(git -C "${repo_root}" rev-parse --short=12 HEAD)"
    echo "dirty=$(if [[ -z "$(git -C "${repo_root}" status --porcelain)" ]]; then echo false; else echo true; fi)"
    echo "timestamp_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "kernel=$(uname -srmo)"
    echo "fixed_dt=${fixed_dt}"
    echo "capture_frames=${capture_frames}"
    echo "render_debug_default=${BYROREDUX_RENDER_DEBUG:-0}"
    if command -v vulkaninfo >/dev/null 2>&1; then
        vulkaninfo --summary 2>/dev/null || true
    else
        echo "vulkaninfo=unavailable"
    fi
} > "${metadata}"

printf 'case\tframes\tdebug_flags\tpng_sha256\tbench\n' > "${manifest}"

capture() {
    local case_name="$1"
    local frames="$2"
    local debug_flags="$3"
    local png="${output_root}/${case_name}.png"
    local log="${output_root}/${case_name}.log"

    rm -f "${png}" "${log}"
    echo "renderer-eval: ${case_name} (${frames} frames, debug=${debug_flags})"

    set +e
    if [[ -n "${runner}" ]]; then
        # The runner is intentionally split on shell whitespace. It is meant
        # for simple wrappers such as `xvfb-run --auto-servernum`, not an
        # arbitrary shell pipeline.
        read -r -a runner_args <<< "${runner}"
        BYROREDUX_FIXED_DT="${fixed_dt}" \
        BYROREDUX_RENDER_DEBUG="${debug_flags}" \
        RUST_LOG="${BYROREDUX_RENDER_EVAL_LOG:-warn}" \
            "${runner_args[@]}" "${engine}" --cornell --bench-frames "${frames}" \
            --screenshot "${png}" >"${log}" 2>&1
    else
        BYROREDUX_FIXED_DT="${fixed_dt}" \
        BYROREDUX_RENDER_DEBUG="${debug_flags}" \
        RUST_LOG="${BYROREDUX_RENDER_EVAL_LOG:-warn}" \
            "${engine}" --cornell --bench-frames "${frames}" --screenshot "${png}" \
            >"${log}" 2>&1
    fi
    local status=$?
    set -e

    # Some drivers have historically faulted during teardown after the PNG was
    # written. Match the golden-frame test's contract: the capture is the
    # success signal, while the exit status remains visible in the log/console.
    if [[ ! -s "${png}" ]]; then
        echo "renderer-eval: ${case_name} produced no screenshot (exit ${status})" >&2
        tail -n 80 "${log}" >&2 || true
        if [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" && -z "${runner}" ]]; then
            echo "renderer-eval: no display detected; set BYROREDUX_RENDER_EVAL_RUNNER='xvfb-run --auto-servernum' on a compatible X11/Vulkan setup" >&2
        fi
        exit 1
    fi

    local bytes
    bytes="$(wc -c < "${png}")"
    if (( bytes <= 1024 )); then
        echo "renderer-eval: ${case_name} screenshot is too small (${bytes} bytes)" >&2
        exit 1
    fi

    local hash
    local bench
    hash="$(sha256sum "${png}" | awk '{print $1}')"
    bench="$(awk '/^bench:/{line=$0} END{print line}' "${log}")"
    bench="${bench//$'\t'/ }"
    printf '%s\t%s\t%s\t%s\t%s\n' \
        "${case_name}" "${frames}" "${debug_flags}" "${hash}" "${bench}" \
        >> "${manifest}"

    if (( status != 0 )); then
        echo "renderer-eval: note: ${case_name} exited ${status} after capture" >&2
    fi
}

for frames in ${capture_frames}; do
    if [[ ! "${frames}" =~ ^[1-9][0-9]*$ ]]; then
        echo "renderer-eval: invalid frame count in BYROREDUX_RENDER_EVAL_FRAMES: ${frames}" >&2
        exit 2
    fi
    capture "cornell_f${frames}" "${frames}" "0"
done

# DBG_DISABLE_ATROUS = 0x4000. Keep the final frame count aligned with the
# last convergence capture so these two images form a controlled A/B.
final_frames="${capture_frames##* }"
capture "cornell_f${final_frames}_no_atrous" "${final_frames}" "0x4000"

# ReSTIR reuse decomposition at the same final frame:
#   0x10000 = no spatial (temporal-only)
#   0x40000 = no temporal (spatial-only)
#   0x50000 = neither reuse dimension (current-frame reservoir only)
capture "cornell_f${final_frames}_restir_temporal_only" "${final_frames}" "0x10000"
capture "cornell_f${final_frames}_restir_spatial_only" "${final_frames}" "0x40000"
capture "cornell_f${final_frames}_restir_no_reuse" "${final_frames}" "0x50000"

if command -v compare >/dev/null 2>&1; then
    compare "${output_root}/cornell_f${final_frames}.png" \
        "${output_root}/cornell_f${final_frames}_no_atrous.png" \
        "${output_root}/cornell_f${final_frames}_atrous_diff.png" || true
fi

echo "renderer-eval: artifacts written to ${output_root}"
echo "renderer-eval: manifest: ${manifest}"
