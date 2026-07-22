#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
    echo "usage: $0 <unpacked-fidelityfx-sdk-v1.1.4> [output-directory]" >&2
    exit 2
fi

upstream_root=$(realpath "$1")
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
workspace=$(realpath "$script_dir/..")
output=${2:-"$workspace/third_party/fidelityfx-sdk-v1.1.4/generated-vk"}

compiler="$upstream_root/sdk/tools/binary_store/FidelityFX_SC.exe"
gpu="$upstream_root/sdk/include/FidelityFX/gpu"
shader_dir="$upstream_root/sdk/src/backends/vk/shaders/fsr3upscaler"

if [[ ! -f "$compiler" || ! -d "$gpu" || ! -d "$shader_dir" ]]; then
    echo "error: input is not an unpacked FidelityFX SDK v1.1.4 tree" >&2
    exit 1
fi
if ! command -v wine64 >/dev/null 2>&1; then
    echo "error: wine64 is required to run the official FidelityFX shader compiler" >&2
    exit 1
fi

mkdir -p "$output"

base_args=(
    -reflection
    -deps=gcc
    -num-threads=1
    -DFFX_GPU=1
    -DFFX_FSR3UPSCALER_OPTION_UPSAMPLE_SAMPLERS_USE_DATA_HALF=0
    -DFFX_FSR3UPSCALER_OPTION_ACCUMULATE_SAMPLERS_USE_DATA_HALF=0
    -DFFX_FSR3UPSCALER_OPTION_REPROJECT_SAMPLERS_USE_DATA_HALF=1
    -DFFX_FSR3UPSCALER_OPTION_POSTPROCESSLOCKSTATUS_SAMPLERS_USE_DATA_HALF=0
    -DFFX_FSR3UPSCALER_OPTION_UPSAMPLE_USE_LANCZOS_TYPE=2
)
api_args=(
    -compiler=glslang
    -e CS
    --target-env vulkan1.2
    -S comp
    -Os
    -DFFX_GLSL=1
)
permutation_args=(
    '-DFFX_FSR3UPSCALER_OPTION_REPROJECT_USE_LANCZOS_TYPE={0,1}'
    '-DFFX_FSR3UPSCALER_OPTION_HDR_COLOR_INPUT={0,1}'
    '-DFFX_FSR3UPSCALER_OPTION_LOW_RESOLUTION_MOTION_VECTORS={0,1}'
    '-DFFX_FSR3UPSCALER_OPTION_JITTERED_MOTION_VECTORS={0,1}'
    '-DFFX_FSR3UPSCALER_OPTION_INVERTED_DEPTH={0,1}'
    '-DFFX_FSR3UPSCALER_OPTION_APPLY_SHARPENING={0,1}'
)
include_args=("-I$gpu" "-I$gpu/fsr3upscaler")

compile_variant() {
    local shader=$1
    local name=$2
    local half=$3

    WINEDEBUG=${WINEDEBUG:--all} wine64 "$compiler" \
        "${base_args[@]}" \
        "${api_args[@]}" \
        "${permutation_args[@]}" \
        "-name=$name" \
        "-DFFX_HALF=$half" \
        "${include_args[@]}" \
        "-output=$output" \
        "$shader"
}

shopt -s nullglob
shaders=("$shader_dir"/*.glsl)
if [[ ${#shaders[@]} -ne 10 ]]; then
    echo "error: expected 10 FSR 3 upscaler GLSL passes, found ${#shaders[@]}" >&2
    exit 1
fi

for shader in "${shaders[@]}"; do
    filename=$(basename "$shader" .glsl)
    compile_variant "$shader" "$filename" 0
    compile_variant "$shader" "${filename}_wave64" 0
    compile_variant "$shader" "${filename}_16bit" 1
    compile_variant "$shader" "${filename}_wave64_16bit" 1
done

header_count=$(find "$output" -maxdepth 1 -type f -name '*.h' | wc -l)
if [[ $header_count -ne 200 ]]; then
    echo "error: expected 200 generated headers, found $header_count" >&2
    exit 1
fi

echo "generated 200 FSR 3.1.4 Vulkan shader headers in $output"
