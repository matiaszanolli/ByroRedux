#!/usr/bin/env bash
#
# FSR 3.1 benchmark matrix — execution phase 7 of
# docs/engine/fsr3-upscaler-integration-plan.md.
#
# Runs every (scene × upscaler) pair N times and emits one TSV row per run,
# plus a median-with-range summary per pair. The report separates two numbers
# the plan is emphatic about not conflating:
#
#   render-work recovered = native(render-resolution passes)
#                         - preset(render-resolution passes)
#   net frame recovery    = native end-to-end frame time
#                         - preset end-to-end frame time
#
# The first is the gross saving from rendering fewer pixels. The second is what
# a player actually gets, after paying for the upscale dispatch and for the
# output-resolution work (presentation, and the swapchain blit) that no preset
# shrinks. Reporting only the first would overstate the win — which is exactly
# the substitution the plan forbids ("do not substitute theoretical pixel-count
# reduction for actual ReSTIR+SVGF recovery").
#
# CWD matters: bare --bsa / --textures-bsa / --materials-ba2 names resolve
# against the current working directory, not the --esm folder. Each scene below
# cd's into its own game Data directory for exactly that reason; running from
# elsewhere makes archives silently fail to open and the scene loads near-empty
# with a spurious FPS figure.
#
# Usage:
#   scripts/fsr-bench-matrix.sh [runs] [frames]
# Defaults: 3 runs of 300 frames, matching the bench-of-record convention.

set -uo pipefail

RUNS="${1:-3}"
FRAMES="${2:-300}"
GAMES_ROOT="${BYROREDUX_GAMES_ROOT:-/mnt/data/SteamLibrary/steamapps/common}"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$REPO/target/release/byroredux"
OUT="${FSR_BENCH_OUT:-$REPO/target/fsr-bench}"

mkdir -p "$OUT"
TSV="$OUT/raw.tsv"

if [[ ! -x "$BIN" ]]; then
  echo "error: $BIN not found — run 'cargo build --release' first" >&2
  exit 1
fi

# Upscaler configurations. TAA at native is the reference every preset is
# measured against.
CONFIGS=(
  "taa:--upscaler taa"
  "fsr-native-aa:--upscaler fsr3 --fsr-quality native-aa"
  "fsr-quality:--upscaler fsr3 --fsr-quality quality"
  "fsr-balanced:--upscaler fsr3 --fsr-quality balanced"
  "fsr-performance:--upscaler fsr3 --fsr-quality performance"
)

# Scene definitions: name, working directory, and load arguments. The three
# game scenes are the existing bench-of-record trio so the numbers stay
# comparable with ROADMAP's history; Cornell is the redistributable control
# that anyone can reproduce without game data.
scene_dir() {
  case "$1" in
    cornell)    echo "$REPO" ;;
    prospector) echo "$GAMES_ROOT/Fallout New Vegas/Data" ;;
    whiterun)   echo "$GAMES_ROOT/Skyrim Special Edition/Data" ;;
    medtek)     echo "$GAMES_ROOT/Fallout 4/Data" ;;
    dugout)     echo "$GAMES_ROOT/Fallout 4/Data" ;;
  esac
}

# Argument arrays are built per scene rather than by word-splitting a string,
# because every vanilla archive name contains spaces.
run_scene_args() {
  local scene="$1"
  case "$scene" in
    cornell)
      ARGS=(--cornell)
      ;;
    prospector)
      ARGS=(--esm FalloutNV.esm --cell GSProspectorSaloonInterior
            --bsa "Fallout - Meshes.bsa"
            --textures-bsa "Fallout - Textures.bsa"
            --textures-bsa "Fallout - Textures2.bsa")
      ;;
    whiterun)
      ARGS=(--esm Skyrim.esm --cell WhiterunBanneredMare
            --bsa "Skyrim - Meshes0.bsa" --bsa "Skyrim - Meshes1.bsa")
      local i
      for i in 0 1 2 3 4 5 6 7 8; do
        ARGS+=(--textures-bsa "Skyrim - Textures$i.bsa")
      done
      ;;
    medtek)
      ARGS=(--esm Fallout4.esm --cell MedTekResearch01
            --bsa "Fallout4 - Meshes.ba2" --bsa "Fallout4 - MeshesExtra.ba2")
      local i
      for i in 1 2 3 4 5 6 7 8 9; do
        ARGS+=(--textures-bsa "Fallout4 - Textures$i.ba2")
      done
      ARGS+=(--textures-bsa "Fallout4 - TexturesPatch.ba2"
             --materials-ba2 "Fallout4 - Materials.ba2")
      ;;
    dugout)
      ARGS=(--esm Fallout4.esm --cell DmndDugoutInn01
            --bsa "Fallout4 - Meshes.ba2" --bsa "Fallout4 - MeshesExtra.ba2")
      local i
      for i in 1 2 3 4 5 6 7 8 9; do
        ARGS+=(--textures-bsa "Fallout4 - Textures$i.ba2")
      done
      ARGS+=(--textures-bsa "Fallout4 - TexturesPatch.ba2"
             --materials-ba2 "Fallout4 - Materials.ba2")
      ;;
    *)
      echo "unknown scene '$scene'" >&2
      return 1
      ;;
  esac
}

SCENES=("${FSR_BENCH_SCENES:-cornell prospector whiterun medtek dugout}")
read -r -a SCENES <<< "${SCENES[0]}"

printf 'scene\tconfig\trun\twall_fps\twall_ms\tfence_ms\tbrd_ms\tgpu_main\tgpu_svgf\tgpu_composite\tgpu_ssao\tgpu_volumetrics\tgpu_upscale\tgpu_presentation\tgpu_bloom\tentities\tdraws\n' > "$TSV"

for scene in "${SCENES[@]}"; do
  dir="$(scene_dir "$scene")"
  if [[ ! -d "$dir" ]]; then
    echo "skip $scene — $dir not present" >&2
    continue
  fi
  run_scene_args "$scene" || continue

  for config in "${CONFIGS[@]}"; do
    name="${config%%:*}"
    flags="${config#*:}"
    read -r -a FLAG_ARR <<< "$flags"

    for run in $(seq 1 "$RUNS"); do
      log="$OUT/${scene}_${name}_${run}.log"
      # Each run is a cold process: pipeline cache is shared on disk (matching
      # the bench-of-record convention) but no GPU state carries over, so one
      # preset cannot warm another.
      ( cd "$dir" && RUST_LOG=warn timeout 900 "$BIN" "${ARGS[@]}" "${FLAG_ARR[@]}" \
          --bench-frames "$FRAMES" ) > "$log" 2>&1

      # The engine prints a bench line per frame once past the target; the
      # last one is the fullest sample.
      line="$(grep '^bench:' "$log" | tail -1)"
      if [[ -z "$line" ]]; then
        echo "warn: $scene/$name run $run produced no bench line (see $log)" >&2
        continue
      fi
      python3 - "$scene" "$name" "$run" "$line" >> "$TSV" <<'PY'
import re, sys
scene, name, run, line = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
def num(key, default="0"):
    m = re.search(rf'{re.escape(key)}=([0-9.]+)', line)
    return m.group(1) if m else default
draws = re.search(r'draws=(\S+)', line)
print("\t".join([
    scene, name, run,
    num("wall_fps"), num("wall_ms"), num("fence"), num("brd_ms"),
    num("gpu_main_render"), num("gpu_svgf"), num("gpu_composite"),
    num("gpu_ssao"), num("gpu_volumetrics"), num("gpu_upscale"),
    num("gpu_presentation"), num("gpu_bloom"),
    num("entities"), draws.group(1) if draws else "-",
]))
PY
      printf '.' >&2
    done
    echo " $scene/$name done" >&2
  done
done

echo >&2
echo "raw rows: $TSV" >&2
python3 "$REPO/scripts/fsr_bench_report.py" "$TSV"
