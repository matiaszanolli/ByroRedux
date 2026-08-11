#!/usr/bin/env bash
# Measure the system-live variability envelope at one revision.
#
# Every run is a fresh process and emits both performance values and the final
# scene-state fingerprint. The fingerprint is forensic in system-live mode: it
# is expected to move when wall-clock dt changes and must never be used as a
# regression assertion in this mode.
#
# Usage:
#   scripts/bench-variability-envelope.sh [runs] [frames]
#
# Environment:
#   BYROREDUX_ENVELOPE_SCENES="prospector medtek dugout"
#   BYROREDUX_ENVELOPE_OUT=target/bench-variability-envelope
#   BYROREDUX_ENVELOPE_RUNNER="xvfb-run --auto-servernum"
#   BYROREDUX_GAMES_ROOT=/path/to/steamapps/common

set -uo pipefail

runs="${1:-5}"
frames="${2:-300}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
engine="${BYROREDUX_ENVELOPE_ENGINE:-${repo_root}/target/release/byroredux}"
output_root="${BYROREDUX_ENVELOPE_OUT:-${repo_root}/target/bench-variability-envelope}"
runner="${BYROREDUX_ENVELOPE_RUNNER:-}"
scenes_raw="${BYROREDUX_ENVELOPE_SCENES:-prospector medtek dugout}"
revision="$(git -C "${repo_root}" rev-parse HEAD)"
if [[ -z "$(git -C "${repo_root}" status --porcelain --untracked-files=normal)" ]]; then
    revision_label="${revision}"
    tree_dirty=false
else
    revision_label="${revision}+dirty"
    tree_dirty=true
fi

if [[ ! "${runs}" =~ ^[1-9][0-9]*$ ]]; then
    echo "bench-envelope: runs must be a positive integer, got '${runs}'" >&2
    exit 2
fi
if [[ ! "${frames}" =~ ^[1-9][0-9]*$ ]]; then
    echo "bench-envelope: frames must be a positive integer, got '${frames}'" >&2
    exit 2
fi
if [[ ! -x "${engine}" ]]; then
    echo "bench-envelope: engine is not executable: ${engine}" >&2
    echo "bench-envelope: run 'cargo build --release -p byroredux' first" >&2
    exit 2
fi

read -r -a scenes <<< "${scenes_raw}"
runner_args=()
launch_prefix=()
if [[ -n "${runner}" ]]; then
    # Simple wrappers only, for example `xvfb-run --auto-servernum`.
    read -r -a runner_args <<< "${runner}"
    launch_prefix=(env -u WAYLAND_DISPLAY -u GDK_BACKEND XDG_SESSION_TYPE=x11 "${runner_args[@]}")
fi

scene_args() {
    case "$1" in
        cornell)    args=(--cornell) ;;
        prospector) args=(--game fnv --cell GSProspectorSaloonInterior) ;;
        whiterun)   args=(--game skyrim_se --cell WhiterunBanneredMare) ;;
        medtek)     args=(--game fo4 --cell MedTekResearch01) ;;
        dugout)     args=(--game fo4 --cell DmndDugoutInn01) ;;
        *)
            echo "bench-envelope: unknown scene '$1'" >&2
            return 1
            ;;
    esac
}

mkdir -p "${output_root}"
raw_tsv="${output_root}/raw.tsv"
summary="${output_root}/summary.txt"
metadata="${output_root}/metadata.txt"

{
    echo "revision=${revision}"
    echo "tree_dirty=${tree_dirty}"
    echo "timestamp_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "engine=${engine}"
    echo "runs=${runs}"
    echo "frames=${frames}"
    echo "scenes=${scenes_raw}"
    echo "mode=system-live"
    echo "dt=wall-clock"
    echo "camera=free"
    echo "kernel=$(uname -srmo)"
    if command -v nvidia-smi >/dev/null 2>&1; then
        nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null || true
    fi
} > "${metadata}"

printf 'revision\tscene\trun\tmode\tcamera\twall_fps\twall_ms\tfence_ms\tbrd_ms\tgpu_main_ms\tframe_p50_ms\tframe_p95_ms\tframe_max_ms\tsim_time_s\tcamera_pos\tcamera_forward\tentities\tdraws\tlights\ttlas\tstate_hash\n' > "${raw_tsv}"

failures=0
for scene in "${scenes[@]}"; do
    if ! scene_args "${scene}"; then
        failures=$((failures + 1))
        continue
    fi
    for run in $(seq 1 "${runs}"); do
        log="${output_root}/${scene}_${run}.log"
        echo "bench-envelope: ${scene} run ${run}/${runs}" >&2
        env -u BYROREDUX_FIXED_DT \
            RUST_LOG="${BYROREDUX_ENVELOPE_LOG:-warn}" \
            "${launch_prefix[@]}" timeout 900 "${engine}" "${args[@]}" \
            --bench-frames "${frames}" \
            --bench-mode system-live > "${log}" 2>&1
        status=$?
        if (( status != 0 )); then
            echo "bench-envelope: ${scene} run ${run} exited ${status} (see ${log})" >&2
            failures=$((failures + 1))
            continue
        fi
        line="$(awk '/^bench:/{line=$0} END{print line}' "${log}")"
        if [[ -z "${line}" ]]; then
            echo "bench-envelope: ${scene} run ${run} produced no bench line (see ${log})" >&2
            failures=$((failures + 1))
            continue
        fi
        python3 - "${revision_label}" "${scene}" "${run}" "${line}" >> "${raw_tsv}" <<'PY'
import re
import sys

revision, scene, run, line = sys.argv[1:]


def token(key, default="-"):
    match = re.search(rf"(?:^|[ \[])({re.escape(key)})=([^ \]]+)", line)
    return match.group(2) if match else default


def number(key, default="0"):
    value = token(key, default)
    return value if re.fullmatch(r"-?[0-9]+(?:\.[0-9]+)?", value) else default


print("\t".join([
    revision,
    scene,
    run,
    token("mode"),
    token("camera"),
    number("wall_fps"),
    number("wall_ms"),
    number("fence"),
    number("brd_ms"),
    number("gpu_main_render"),
    number("frame_p50_ms"),
    number("frame_p95_ms"),
    number("frame_max_ms"),
    number("sim_time_s"),
    token("camera_pos"),
    token("camera_forward"),
    number("entities"),
    token("draws"),
    number("lights"),
    number("tlas"),
    token("state_hash"),
]))
PY
    done
done

python3 "${repo_root}/scripts/bench_variability_report.py" "${raw_tsv}" | tee "${summary}"
report_status=${PIPESTATUS[0]}
if (( failures != 0 )); then
    echo "bench-envelope: ${failures} run(s) failed; raw table is incomplete" >&2
    exit 1
fi
exit "${report_status}"
