#!/usr/bin/env bash
# Measured RT-footprint LOD sweep over Cornell and the four RT recovery scenes.
#
# Each scale gets one instrumented capture and N uninstrumented timing runs.
# Keeping those passes separate is load-bearing: per-fragment diagnostic
# atomics would otherwise contaminate the ray-query GPU-time comparison.
#
# Usage: scripts/rt-lod-sweep.sh [runs] [frames]
# Environment:
#   BYROREDUX_RT_LOD_SCALES="0.000001 6 16 32 64"
#   BYROREDUX_RT_LOD_SCENES="cornell prospector whiterun medtek dugout"
#   BYROREDUX_RT_LOD_OUT=target/rt-lod-sweep
#   BYROREDUX_GAMES_ROOT=/mnt/data/SteamLibrary/steamapps/common
#   BYROREDUX_RT_LOD_XVFB=1
#   BYROREDUX_RT_LOD_PHASES="telemetry timing"

set -euo pipefail

runs="${1:-3}"
frames="${2:-300}"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin="${repo}/target/release/byroredux"
games_root="${BYROREDUX_GAMES_ROOT:-/mnt/data/SteamLibrary/steamapps/common}"
out="${BYROREDUX_RT_LOD_OUT:-${repo}/target/rt-lod-sweep}"
scales_text="${BYROREDUX_RT_LOD_SCALES:-0.000001 6 16 32 64}"
scenes_text="${BYROREDUX_RT_LOD_SCENES:-cornell prospector whiterun medtek dugout}"
read -r -a scales <<< "${scales_text}"
read -r -a scenes <<< "${scenes_text}"
phases_text="${BYROREDUX_RT_LOD_PHASES:-telemetry timing}"
read -r -a phases <<< "${phases_text}"

if [[ ! "${runs}" =~ ^[1-9][0-9]*$ || ! "${frames}" =~ ^[1-9][0-9]*$ ]]; then
    echo "rt-lod-sweep: runs and frames must be positive integers" >&2
    exit 2
fi
if [[ ! -x "${bin}" ]]; then
    echo "rt-lod-sweep: ${bin} missing; run cargo build --release -p byroredux" >&2
    exit 2
fi

mkdir -p "${out}"
out="$(cd "${out}" && pwd)"
mkdir -p "${out}/captures" "${out}/logs"
raw="${out}/raw.tsv"
printf 'scene\tscale\tkind\trun\tgpu_main_ms\twall_ms\tmode\tcamera\tstate_hash\tfragments\tlod0\tlod1\tlod2\tlod3\treflection_traced\treflection_lod_culled\tgi_traced\tgi_lod_culled\tcapture\n' > "${raw}"

scene_dir() {
    case "$1" in
        cornell) echo "${repo}" ;;
        prospector) echo "${games_root}/Fallout New Vegas/Data" ;;
        whiterun) echo "${games_root}/Skyrim Special Edition/Data" ;;
        medtek|dugout) echo "${games_root}/Fallout 4/Data" ;;
        *) return 1 ;;
    esac
}

scene_args() {
    case "$1" in
        cornell)
            args=(--cornell)
            ;;
        prospector)
            args=(--esm FalloutNV.esm --cell GSProspectorSaloonInterior
                --bsa "Fallout - Meshes.bsa"
                --textures-bsa "Fallout - Textures.bsa"
                --textures-bsa "Fallout - Textures2.bsa")
            ;;
        whiterun)
            args=(--esm Skyrim.esm --cell WhiterunBanneredMare
                --bsa "Skyrim - Meshes0.bsa" --bsa "Skyrim - Meshes1.bsa")
            local index
            for index in 0 1 2 3 4 5 6 7 8; do
                args+=(--textures-bsa "Skyrim - Textures${index}.bsa")
            done
            ;;
        medtek|dugout)
            local cell="MedTekResearch01"
            [[ "$1" == dugout ]] && cell="DmndDugoutInn01"
            args=(--esm Fallout4.esm --cell "${cell}"
                --bsa "Fallout4 - Meshes.ba2" --bsa "Fallout4 - MeshesExtra.ba2")
            local index
            for index in 1 2 3 4 5 6 7 8 9; do
                args+=(--textures-bsa "Fallout4 - Textures${index}.ba2")
            done
            args+=(--textures-bsa "Fallout4 - TexturesPatch.ba2"
                --materials-ba2 "Fallout4 - Materials.ba2")
            ;;
        *) return 1 ;;
    esac
}

runner=()
if [[ "${BYROREDUX_RT_LOD_XVFB:-0}" == 1 || -z "${DISPLAY:-}" ]]; then
    runner=(xvfb-run -a env -u WAYLAND_DISPLAY WINIT_UNIX_BACKEND=x11)
fi

append_row() {
    local scene="$1" scale="$2" kind="$3" run="$4" log="$5" capture="$6"
    python3 - "${scene}" "${scale}" "${kind}" "${run}" "${log}" "${capture}" >> "${raw}" <<'PY'
import re
import sys
from pathlib import Path

scene, scale, kind, run, log_path, capture = sys.argv[1:]
text = Path(log_path).read_text(errors="replace")
benches = [line for line in text.splitlines() if line.startswith("bench:")]
if not benches:
    raise SystemExit(f"{log_path}: no bench line")
bench = benches[-1]

def token(key, default="-"):
    match = re.search(rf"(?:^|[ \[]){re.escape(key)}=([^ \]]+)", bench)
    return match.group(1) if match else default

values = ["0"] * 9
samples = [line for line in text.splitlines() if "rt-lod-telemetry:" in line]
if samples:
    sample = samples[-1]
    patterns = [
        r"fragments=(\d+)",
        r"bins=\[(\d+), (\d+), (\d+), (\d+)\]",
        r"reflection_traced=(\d+)",
        r"reflection_lod_culled=(\d+)",
        r"gi_traced=(\d+)",
        r"gi_lod_culled=(\d+)",
    ]
    matches = [re.search(pattern, sample) for pattern in patterns]
    if not all(matches):
        raise SystemExit(f"{log_path}: malformed telemetry line: {sample}")
    values = [
        matches[0].group(1),
        *matches[1].groups(),
        matches[2].group(1),
        matches[3].group(1),
        matches[4].group(1),
        matches[5].group(1),
    ]

print("\t".join([
    scene, scale, kind, run, token("gpu_main_render"), token("wall_ms"),
    token("mode"), token("camera"), token("state_hash"), *values, capture,
]))
PY
}

run_one() {
    local scene="$1" scale="$2" kind="$3" run="$4" dir="$5"
    local log="${out}/logs/${scene}_${scale}_${kind}_${run}.log"
    local capture="-"
    local extra=(--rt-test-lod-scale "${scale}" --rt-test-ray-quality-tier 3)
    local rust_log="warn"
    if [[ "${kind}" == telemetry ]]; then
        capture="${out}/captures/${scene}_${scale}.png"
        extra+=(--rt-test-lod-telemetry --screenshot "${capture}")
        rust_log="warn,byroredux_renderer::vulkan::context::draw=info"
    fi

    echo "rt-lod-sweep: ${scene} scale=${scale} ${kind} ${run}" >&2
    set +e
    (
        cd "${dir}"
        env RUST_LOG="${rust_log}" "${runner[@]}" timeout 900 "${bin}" \
            "${args[@]}" --upscaler taa --bench-frames "${frames}" \
            --bench-mode renderer-static "${extra[@]}"
    ) > "${log}" 2>&1
    status=$?
    set -e
    if (( status != 0 )); then
        echo "rt-lod-sweep: failed (${status}); see ${log}" >&2
        return 1
    fi
    if ! rg -q '^rt-integrity:.*verdict=PASS' "${log}"; then
        echo "rt-lod-sweep: integrity gate failed; see ${log}" >&2
        return 1
    fi
    if [[ "${kind}" == telemetry && ! -s "${capture}" ]]; then
        echo "rt-lod-sweep: capture missing after successful run; see ${log}" >&2
        return 1
    fi
    append_row "${scene}" "${scale}" "${kind}" "${run}" "${log}" "${capture}"
}

for scene in "${scenes[@]}"; do
    dir="$(scene_dir "${scene}")" || {
        echo "rt-lod-sweep: unknown scene ${scene}" >&2
        exit 2
    }
    if [[ ! -d "${dir}" ]]; then
        echo "rt-lod-sweep: skip ${scene}; ${dir} is absent" >&2
        continue
    fi
    scene_args "${scene}"
    for scale in "${scales[@]}"; do
        for phase in "${phases[@]}"; do
            case "${phase}" in
                telemetry)
                    run_one "${scene}" "${scale}" telemetry 1 "${dir}"
                    ;;
                timing)
                    for run in $(seq 1 "${runs}"); do
                        run_one "${scene}" "${scale}" timing "${run}" "${dir}"
                    done
                    ;;
                *)
                    echo "rt-lod-sweep: unknown phase ${phase}; expected telemetry or timing" >&2
                    exit 2
                    ;;
            esac
        done
    done
done

if [[ " ${phases_text} " == *" telemetry "* && " ${phases_text} " == *" timing "* ]]; then
    python3 "${repo}/scripts/rt_lod_report.py" "${raw}" | tee "${out}/summary.txt"
else
    echo "rt-lod-sweep: partial phase complete; raw rows: ${raw}" >&2
fi
