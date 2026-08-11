#!/usr/bin/env bash
# Run the same renderer-static scene twice and assert identical scene state.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
engine="${1:-${repo_root}/target/debug/byroredux}"
frames="${2:-5}"
runner="${BYROREDUX_BENCH_RUNNER:-}"

if [[ ! -x "${engine}" ]]; then
    echo "bench-determinism: engine is not executable: ${engine}" >&2
    exit 2
fi
if [[ ! "${frames}" =~ ^[1-9][0-9]*$ ]]; then
    echo "bench-determinism: frames must be a positive integer, got '${frames}'" >&2
    exit 2
fi

work_dir="$(mktemp -d "${TMPDIR:-/tmp}/byro-bench-determinism.XXXXXX")"
trap 'rm -rf -- "${work_dir}"' EXIT
runner_args=()
launch_prefix=()
if [[ -n "${runner}" ]]; then
    # Simple wrappers only (for example `xvfb-run --auto-servernum`).
    read -r -a runner_args <<< "${runner}"
    # A self-hosted shell may export Wayland variables even though the wrapper
    # provides only X11. Force winit onto the display owned by the wrapper.
    launch_prefix=(env -u WAYLAND_DISPLAY -u GDK_BACKEND XDG_SESSION_TYPE=x11 "${runner_args[@]}")
fi

run_once() {
    local index="$1"
    local log="${work_dir}/run-${index}.log"
    set +e
    RUST_LOG="${BYROREDUX_BENCH_LOG:-error}" \
        "${launch_prefix[@]}" "${engine}" \
        --cornell \
        --bench-frames "${frames}" \
        --bench-mode renderer-static >"${log}" 2>&1
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
    sed -n 's/.* state_hash=\([0-9a-fA-F]\{16\}\).*/\1/p' <<< "${line}"
}

first="$(run_once 1)"
second="$(run_once 2)"
if [[ -z "${first}" || -z "${second}" ]]; then
    echo "bench-determinism: could not parse a 16-digit state_hash" >&2
    exit 1
fi
if [[ "${first}" != "${second}" ]]; then
    echo "bench-determinism: FAIL — renderer-static state drifted (${first} != ${second})" >&2
    exit 1
fi

echo "bench-determinism: PASS — renderer-static state_hash=${first} across two runs"
