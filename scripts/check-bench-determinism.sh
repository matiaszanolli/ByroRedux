#!/usr/bin/env bash
# Run the same deterministic scene three times and assert identical state.
#
# Usage:
#   scripts/check-bench-determinism.sh [engine] [frames] [scene args...]
#
# Defaults to Cornell in renderer-static mode. Override the deterministic
# workload with BYROREDUX_BENCH_MODE and BYROREDUX_BENCH_CAMERA. Set
# BYROREDUX_BENCH_OUT to retain logs and per-run JSON manifests.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
engine="${1:-${repo_root}/target/debug/byroredux}"
frames="${2:-5}"
runner="${BYROREDUX_BENCH_RUNNER:-}"
mode="${BYROREDUX_BENCH_MODE:-renderer-static}"
camera="${BYROREDUX_BENCH_CAMERA:-static}"
artifact_dir="${BYROREDUX_BENCH_OUT:-}"
scene_args=("${@:3}")
if (( ${#scene_args[@]} == 0 )); then
    scene_args=(--cornell)
fi

if [[ ! -x "${engine}" ]]; then
    echo "bench-determinism: engine is not executable: ${engine}" >&2
    exit 2
fi
if [[ ! "${frames}" =~ ^[1-9][0-9]*$ ]]; then
    echo "bench-determinism: frames must be a positive integer, got '${frames}'" >&2
    exit 2
fi
case "${mode}" in
    renderer-static|renderer-stepped) ;;
    *)
        echo "bench-determinism: mode must be renderer-static or renderer-stepped, got '${mode}'" >&2
        exit 2
        ;;
esac
case "${camera}" in
    static|pan|orbit|dolly|cut) ;;
    *)
        echo "bench-determinism: unknown camera path '${camera}'" >&2
        exit 2
        ;;
esac

if [[ -n "${artifact_dir}" ]]; then
    work_dir="${artifact_dir}"
    mkdir -p "${work_dir}"
else
    work_dir="$(mktemp -d "${TMPDIR:-/tmp}/byro-bench-determinism.XXXXXX")"
    trap 'rm -rf -- "${work_dir}"' EXIT
fi
runner_args=()
if [[ -n "${runner}" ]]; then
    # Simple wrappers only (for example `xvfb-run --auto-servernum`).
    read -r -a runner_args <<< "${runner}"
fi

engine_hash="$(sha256sum "${engine}" | awk '{print $1}')"
harness_hash="$(sha256sum "${BASH_SOURCE[0]}" | awk '{print $1}')"
engine_commit="$(git -C "${repo_root}" rev-parse HEAD 2>/dev/null || echo unknown)"
harness_commit="$(git -C "${repo_root}" log -1 --format=%H -- "${BASH_SOURCE[0]}" 2>/dev/null || echo unknown)"
scene_args_text="$(printf '%q ' "${scene_args[@]}")"

run_once() {
    local index="$1"
    local log="${work_dir}/run-${index}.log"
    set +e
    if [[ -n "${runner}" ]]; then
        # Keep xvfb-run itself in the foreground. Debian's wrapper waits for
        # Xvfb through SIGUSR1; putting the wrapper in a command substitution,
        # pipeline, or redirected subshell can make that readiness signal race
        # and launch winit before DISPLAY accepts connections. Redirect only
        # the engine child from inside the ready wrapper.
        RUST_LOG="${BYROREDUX_BENCH_LOG:-error,byroredux::boot=info,byroredux_renderer::vulkan::device=info,byroredux_renderer::vulkan::context=info}" \
            "${runner_args[@]}" env \
            -u WAYLAND_DISPLAY -u GDK_BACKEND \
            XDG_SESSION_TYPE=x11 \
            BYROREDUX_BENCH_LOG_FILE="${log}" \
            bash -c 'exec "$@" >"$BYROREDUX_BENCH_LOG_FILE" 2>&1' bash \
            "${engine}" \
            "${scene_args[@]}" \
            --bench-frames "${frames}" \
            --bench-mode "${mode}" \
            --bench-camera "${camera}"
    else
        RUST_LOG="${BYROREDUX_BENCH_LOG:-error,byroredux::boot=info,byroredux_renderer::vulkan::device=info,byroredux_renderer::vulkan::context=info}" \
            "${engine}" \
            "${scene_args[@]}" \
            --bench-frames "${frames}" \
            --bench-mode "${mode}" \
            --bench-camera "${camera}" >"${log}" 2>&1
    fi
    local status=$?
    set -e
    if (( status != 0 )); then
        echo "bench-determinism: run ${index} exited ${status}" >&2
        tail -n 80 "${log}" >&2
        exit 1
    fi
    local line
    line="$(awk '/^bench:/{line=$0} END{print line}' "${log}")"
    if [[ -z "${line}" ]]; then
        echo "bench-determinism: run ${index} produced no bench summary" >&2
        tail -n 80 "${log}" >&2
        exit 1
    fi
    echo "${line}" >&2
    local selected_gpu
    selected_gpu="$(sed -n 's/.*Selected GPU: \("[^"]*"\).*/\1/p' "${log}" | tail -n 1)"
    local selected_upscaler
    selected_upscaler="$(sed -n 's/.*Renderer upscaler selection: //p' "${log}" | tail -n 1)"
    local render_extent
    render_extent="$(sed -n 's/.*Frame extents: render=\([^,]*\), output=.*/\1/p' "${log}" | tail -n 1)"
    local output_extent
    output_extent="$(sed -n 's/.*Frame extents: render=[^,]*, output=\([^ ]*\).*/\1/p' "${log}" | tail -n 1)"
    python3 - \
        "${line}" \
        "${work_dir}/run-${index}.json" \
        "${work_dir}/run-${index}.fingerprint" \
        "${index}" \
        "${engine}" \
        "${engine_hash}" \
        "${engine_commit}" \
        "${harness_hash}" \
        "${harness_commit}" \
        "${frames}" \
        "${mode}" \
        "${camera}" \
        "${scene_args_text}" \
        "${selected_gpu}" \
        "${selected_upscaler}" \
        "${render_extent}" \
        "${output_extent}" <<'PY'
import json
import re
import sys

(
    line,
    manifest_path,
    fingerprint_path,
    run_index,
    engine,
    engine_sha256,
    engine_commit,
    harness_sha256,
    harness_commit,
    frames,
    requested_mode,
    requested_camera,
    scene_args,
    selected_gpu,
    selected_upscaler,
    render_extent,
    output_extent,
) = sys.argv[1:]


def token(key):
    match = re.search(rf"(?:^|[ \[])({re.escape(key)})=([^ \]]+)", line)
    if not match:
        raise SystemExit(f"missing {key} in benchmark summary")
    return match.group(2)


keys = (
    "mode",
    "camera",
    "sim_time_s",
    "entities",
    "draws",
    "lights",
    "tlas",
    "state_hash",
)
values = {key: token(key) for key in keys}
fingerprint = "|".join(values[key] for key in keys)

manifest = {
    "schema": 1,
    "run": int(run_index),
    "engine": engine,
    "engine_sha256": engine_sha256,
    "engine_commit": engine_commit,
    "harness_sha256": harness_sha256,
    "harness_commit": harness_commit,
    "frames": int(frames),
    "requested_mode": requested_mode,
    "requested_camera": requested_camera,
    "scene_args_shell": scene_args.strip(),
    "selected_gpu": selected_gpu.strip('"'),
    "selected_upscaler": selected_upscaler,
    "render_extent": render_extent,
    "output_extent": output_extent,
    "summary": values,
    "fingerprint": fingerprint,
    "verdict": "captured",
}
with open(manifest_path, "w", encoding="utf-8") as output:
    json.dump(manifest, output, indent=2, sort_keys=True)
    output.write("\n")
with open(fingerprint_path, "w", encoding="utf-8") as output:
    output.write(fingerprint)
    output.write("\n")

# Include post-merge batches and actual calls (`draws=N/Mb/Kc`) as well as
# the scene hash. The latter covers renderer-facing draw data; the former
# catches wall-time-driven resource readiness that can change batching while
# leaving the pre-merge draw stream unchanged.
PY
}

run_once 1
run_once 2
run_once 3
first="$(<"${work_dir}/run-1.fingerprint")"
second="$(<"${work_dir}/run-2.fingerprint")"
third="$(<"${work_dir}/run-3.fingerprint")"
if [[ -z "${first}" || -z "${second}" || -z "${third}" ]]; then
    echo "bench-determinism: could not parse the scene-state fingerprint" >&2
    exit 1
fi
if [[ "${first}" != "${second}" || "${first}" != "${third}" ]]; then
    echo "bench-determinism: FAIL — ${mode}/${camera} state drifted" >&2
    echo "  run 1: ${first}" >&2
    echo "  run 2: ${second}" >&2
    echo "  run 3: ${third}" >&2
    exit 1
fi

state_hash="${first##*|}"
for index in 1 2 3; do
    python3 - "${work_dir}/run-${index}.json" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, encoding="utf-8") as source:
    manifest = json.load(source)
manifest["verdict"] = "pass"
with open(path, "w", encoding="utf-8") as output:
    json.dump(manifest, output, indent=2, sort_keys=True)
    output.write("\n")
PY
done

echo "bench-determinism: PASS — ${mode}/${camera} fingerprint and state_hash=${state_hash} match across three runs"
if [[ -n "${artifact_dir}" ]]; then
    echo "bench-determinism: artifacts retained in ${work_dir}"
fi
