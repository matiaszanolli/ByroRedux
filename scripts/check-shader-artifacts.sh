#!/usr/bin/env bash
# Recompile every first-party GLSL shader and require byte-identical SPIR-V.

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
shader_root="${repo_root}/crates/renderer/shaders"
compiler="${GLSLANG_VALIDATOR:-glslangValidator}"
expected_version="11:16.2.0"

if ! command -v "${compiler}" >/dev/null 2>&1; then
    echo "check-shader-artifacts: ${compiler} not found" >&2
    exit 2
fi

actual_version="$("${compiler}" -dumpfullversion)"
if [[ "${actual_version}" != "${expected_version}" ]]; then
    echo "check-shader-artifacts: expected glslang ${expected_version}, got ${actual_version}" >&2
    echo "exact compiler parity is required because SPIR-V code generation is version-dependent" >&2
    exit 2
fi

scratch="$(mktemp -d)"
trap 'rm -rf -- "${scratch}"' EXIT

mapfile -t shaders < <(
    rg --files "${shader_root}" |
        rg '\.(vert|frag|comp)$' |
        sort
)

if [[ "${#shaders[@]}" -eq 0 ]]; then
    echo "check-shader-artifacts: no shaders found under ${shader_root}" >&2
    exit 2
fi

drift=0
for shader in "${shaders[@]}"; do
    artifact="${shader}.spv"
    relative="${shader#"${repo_root}/"}"
    output="${scratch}/$(basename -- "${shader}").spv"

    if [[ ! -f "${artifact}" ]]; then
        echo "MISSING ${relative}.spv" >&2
        drift=1
        continue
    fi

    "${compiler}" -V -I"${shader_root}" "${shader}" -o "${output}" >/dev/null
    if ! cmp -s "${artifact}" "${output}"; then
        echo "DRIFT ${relative}.spv" >&2
        sha256sum "${artifact}" "${output}" >&2
        drift=1
    fi
done

if [[ "${drift}" -ne 0 ]]; then
    echo "check-shader-artifacts: committed SPIR-V is not reproducible from GLSL" >&2
    exit 1
fi

echo "check-shader-artifacts: ${#shaders[@]} shader artifacts match glslang ${actual_version}"
