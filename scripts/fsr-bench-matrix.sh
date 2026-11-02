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
# with a spurious FPS figure. Since #3347 that is checked rather than merely
# documented: every run must clear three gates before its row is written —
# no archive-open error in the log, an entity count above a per-scene floor,
# and a state_hash matching the other runs of its config. Any rejection makes
# the script exit 3, because a matrix with a bad row in it reads as data.
#
# Usage:
#   scripts/fsr-bench-matrix.sh [runs] [frames]
# Defaults: 3 runs of 300 frames, matching the bench-of-record convention.

set -uo pipefail

RUNS="${1:-3}"
FRAMES="${2:-300}"
CAMERA_PATH="${FSR_BENCH_CAMERA:-orbit}"
GAMES_ROOT="${BYROREDUX_GAMES_ROOT:-/mnt/data/SteamLibrary/steamapps/common}"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$REPO/target/release/byroredux"
OUT="${FSR_BENCH_OUT:-$REPO/target/fsr-bench}"

mkdir -p "$OUT"
TSV="$OUT/raw.tsv"
# Rows rejected by the #3347 sanity gates. A non-empty list makes the script
# exit non-zero: a bench-of-record matrix with a silently-bad row in it is worse
# than no matrix, because the row looks like data.
REJECTED=()

if [[ ! -x "$BIN" ]]; then
  echo "error: $BIN not found — run 'cargo build --release' first" >&2
  exit 1
fi
case "$CAMERA_PATH" in
  pan|orbit|dolly|cut) ;;
  *)
    echo "error: FSR_BENCH_CAMERA must be a moving temporal path (pan|orbit|dolly|cut), got '$CAMERA_PATH'" >&2
    exit 2
    ;;
esac

# Upscaler configurations. TAA at native is the reference every preset is
# measured against.
CONFIGS=(
  "taa:--upscaler taa"
  "fsr-native-aa:--upscaler fsr3 --fsr-quality native-aa"
  "fsr-quality:--upscaler fsr3 --fsr-quality quality"
  "fsr-balanced:--upscaler fsr3 --fsr-quality balanced"
  "fsr-performance:--upscaler fsr3 --fsr-quality performance"
)

# Scene definitions (working directory, load arguments, entity floor) live
# in bench-scenes.sh, shared with the M-LIGHT baselines.
# shellcheck source=bench-scenes.sh
source "$REPO/scripts/bench-scenes.sh"

# `gridcross` (#3467) is defined above but intentionally absent here until its
# entity floor is calibrated — run it explicitly with
# `FSR_BENCH_SCENES="gridcross" ./scripts/fsr-bench-matrix.sh`.
SCENES=("${FSR_BENCH_SCENES:-cornell prospector whiterun medtek dugout}")
read -r -a SCENES <<< "${SCENES[0]}"

# #2835 — stamp the harness + engine commits into the artefact itself. Both
# the measurement conditions and the column set have changed once already
# (f19f7f15 switched from a parked camera to `--bench-mode renderer-stepped`
# and added six columns) and the archived TSVs recorded neither, so nothing in
# a committed table said which harness produced it. `fsr_bench_report.py`
# skips `#` lines, so this is metadata to the tool and provenance to a reader.
HARNESS_COMMIT="$(git -C "$REPO" log -1 --format=%h -- scripts/fsr-bench-matrix.sh 2>/dev/null || echo unknown)"
ENGINE_COMMIT="$(git -C "$REPO" rev-parse --short HEAD 2>/dev/null || echo unknown)"
{
  printf '# harness=%s engine=%s mode=renderer-stepped camera=%s runs=%s frames=%s\n' \
    "$HARNESS_COMMIT" "$ENGINE_COMMIT" "$CAMERA_PATH" "$RUNS" "$FRAMES"
  printf 'scene\tconfig\trun\tmode\tcamera\twall_fps\twall_ms\tfence_ms\tbrd_ms\tgpu_main\tgpu_svgf\tgpu_composite\tgpu_ssao\tgpu_volumetrics\tgpu_upscale\tgpu_presentation\tgpu_bloom\tsim_time_s\tentities\tdraws\tlights\ttlas\tstate_hash\tgpu_inactive\traster_cmds\tgpu_sky_cube\tgpu_tlas_build\tgpu_cluster_cull\trt_tier\tgpu_groundcover_models\tgpu_volumetrics_inject\tgpu_volumetrics_integrate\tvolumetric_rt_tier\tvolumetric_light_cap\tfroxel_x\tfroxel_y\tfroxel_z\ttransport_armed\tfog_volume_count\tfog_cluster_max_density\tfog_cluster_max_portal\tcamera_pos\tcamera_forward\tgpu_shadow_filter\tgpu_cloud_shadow\n'
} > "$TSV"

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
    # Reset per config: gate 3 compares runs within one (scene, config) pair.
    config_hash=""

    for run in $(seq 1 "$RUNS"); do
      log="$OUT/${scene}_${name}_${run}.log"
      # Each run is a cold process: pipeline cache is shared on disk (matching
      # the bench-of-record convention) but no GPU state carries over, so one
      # preset cannot warm another. Upscaler comparisons always use the fixed
      # 60 Hz + frame-indexed moving-camera contract: a parked camera would
      # hide disocclusion/reprojection/camera-cut failures by fully converging.
      ( cd "$dir" && RUST_LOG=warn timeout 900 "$BIN" "${ARGS[@]}" "${FLAG_ARR[@]}" \
          --bench-frames "$FRAMES" \
          --bench-mode renderer-stepped \
          --bench-camera "$CAMERA_PATH" ) > "$log" 2>&1

      # A no-screenshot run emits exactly one summary at the target frame.
      line="$(grep '^bench:' "$log" | tail -1)"
      if [[ -z "$line" ]]; then
        echo "warn: $scene/$name run $run produced no bench line (see $log)" >&2
        REJECTED+=("$scene/$name run $run: no bench line")
        continue
      fi

      # Gate 1 (#3347) — the exact failure this file's header describes. The
      # engine logs these at error! (#1776) and the harness runs RUST_LOG=warn,
      # so the message IS in the log; nothing used to read it. Matching the
      # emitted string is exact: no threshold, no false positives.
      if grep -q 'was specified but 0 .* archives opened' "$log"; then
        echo "reject: $scene/$name run $run — archives failed to open, scene is \
near-empty (see $log)" >&2
        REJECTED+=("$scene/$name run $run: archives failed to open")
        continue
      fi

      row="$(python3 - "$scene" "$name" "$run" "$line" <<'PY'
import re, sys
scene, name, run, line = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
def num(key, default="0"):
    m = re.search(rf'{re.escape(key)}=([0-9.]+)', line)
    return m.group(1) if m else default
def token(key, default="-"):
    m = re.search(rf'{re.escape(key)}=([^ ]+)', line)
    return m.group(1) if m else default
draws = re.search(r'draws=(\S+)', line)
print("\t".join([
    scene, name, run,
    token("mode"), token("camera"),
    num("wall_fps"), num("wall_ms"), num("fence"), num("brd_ms"),
    num("gpu_main_render"), num("gpu_svgf"), num("gpu_composite"),
    num("gpu_ssao"), num("gpu_volumetrics"), num("gpu_upscale"),
    num("gpu_presentation"), num("gpu_bloom"),
    num("sim_time_s"), num("entities"), draws.group(1) if draws else "-",
    num("lights"), num("tlas"), token("state_hash"),
    # #2821 — which `gpu_*` zeros above mean "bracket did not run" rather than
    # "measured zero". Appended LAST on purpose: the entity-floor and
    # state-hash gates below `cut` fixed field numbers out of this row.
    token("gpu_inactive"),
    # #4800 — the count `sort_draw_commands` gates its serial/parallel branch
    # on. `draws` above is the complete stream, RT-only occluders included, so
    # it cannot say which side of DRAW_SORT_PARALLEL_THRESHOLD a scene lands
    # on. `-` = an engine build that predates the bench-line token. Appended
    # after gpu_inactive for the same field-number reason.
    token("bench_draws_raster_cmds"),
    # #4808 — include the bake timer in the archived frame sample so sky-cube
    # work is not silently absorbed into gpu_main_render.
    num("gpu_sky_cube"),
    num("gpu_tlas_build"),
    num("gpu_cluster_cull"),
    num("rt_tier"),
    num("gpu_groundcover_models"),
    num("gpu_volumetrics_inject"),
    num("gpu_volumetrics_integrate"),
    num("volumetric_rt_tier"),
    num("volumetric_light_cap"),
    num("froxel_x"),
    num("froxel_y"),
    num("froxel_z"),
    num("transport_armed"),
    num("fog_volume_count"),
    num("fog_cluster_max_density"),
    num("fog_cluster_max_portal"),
    # #5128 — the camera pose at the measured frame. The stepped path is
    # frame-indexed from the spawn pose, so a moved spawn moves this: the
    # R6a-regress-22 "FO4 frame time doubled at flat content" was
    # `b9e961eeb`'s interior spawn ladder moving the path's origin into the
    # room, which no count column could show. `-` = an engine build that
    # predates the bench-line token. Appended last (field-number `cut`s).
    token("camera_pos"),
    token("camera_forward"),
    # #5959 — the direct-shadow filter, nested inside gpu_svgf (never add
    # the two). Appended last (field-number `cut`s).
    num("gpu_shadow_filter"),
    # #6316 — the cloud-shadow bake, nested inside gpu_sky_cube (never add
    # the two). Appended last (field-number `cut`s).
    num("gpu_cloud_shadow"),
]))
PY
)"

      # Gate 2 (#3347) — entity floor. Defence in depth behind gate 1: catches a
      # near-empty load that arrived some other way (a wrong --cell, a missing
      # master) and never printed the archive error.
      entities="$(printf '%s' "$row" | cut -f19)"
      floor="$(scene_entity_floor "$scene")"
      if [[ "${entities%%.*}" -lt "$floor" ]]; then
        echo "reject: $scene/$name run $run — $entities entities is below the \
$floor floor for this scene; the run did not load real content (see $log)" >&2
        REJECTED+=("$scene/$name run $run: $entities entities < $floor floor")
        continue
      fi

      # Gate 3 (#3347) — state_hash must be identical across the runs of one
      # config. The three runs are the same scene at the same frame; a differing
      # hash means they did not converge on the same world state, so a median
      # over them is a median over different scenes. The archived TSV is already
      # constant here, so this pins an invariant that holds today.
      hash="$(printf '%s' "$row" | cut -f23)"
      if [[ -z "$config_hash" ]]; then
        config_hash="$hash"
      elif [[ "$hash" != "$config_hash" ]]; then
        echo "reject: $scene/$name run $run — state_hash $hash differs from \
$config_hash on earlier runs of this config; the runs are not the same scene" >&2
        REJECTED+=("$scene/$name run $run: state_hash $hash != $config_hash")
        continue
      fi

      printf '%s\n' "$row" >> "$TSV"
      printf '.' >&2
    done
    echo " $scene/$name done" >&2
  done
done

echo >&2
echo "raw rows: $TSV" >&2
python3 "$REPO/scripts/fsr_bench_report.py" "$TSV"

# #3347 — a matrix with silently-bad rows in it is worse than no matrix, so
# surface every rejection and exit non-zero. The report above still prints:
# the operator needs to see what *did* land alongside what was thrown out.
if (( ${#REJECTED[@]} > 0 )); then
  echo >&2
  echo "error: ${#REJECTED[@]} run(s) rejected by the sanity gates — this \
matrix is NOT a valid bench-of-record:" >&2
  for r in "${REJECTED[@]}"; do
    echo "  - $r" >&2
  done
  exit 3
fi
