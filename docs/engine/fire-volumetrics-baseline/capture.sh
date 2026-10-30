#!/usr/bin/env bash
# Fire / smoke / explosion baseline capture (tracking #6531, Phase 0 #6532).
#
# Runs the game-data-free `--combustion-lab` scenes in `renderer-stepped` mode
# (fixed dt = 1/60, frame-indexed camera, so simulation time and camera pose are
# a pure function of the frame count) and records, per run, the engine's
# `bench:` line plus a final-frame screenshot. `aggregate.py` then reduces the
# per-run lines to a median / min / max table.
#
# Usage:  docs/engine/fire-volumetrics-baseline/capture.sh OUT_DIR [REPS] [SCENE_PREFIX]
#   REPS          runs per scene, default 5
#   SCENE_PREFIX  only scenes whose name starts with it (e.g. s1)
# Needs a release build (`cargo build --release -p byroredux`), a Vulkan GPU and
# xvfb-run. Do not run it while you have your own engine window open: the debug
# server port is fixed and the two instances compete for the GPU.
#
# Notes that matter when reading the numbers:
#  * Every GPU figure is ONE frame's timestamp bracket (the final frame of the
#    run), not an average. Compare medians across the repeats, not single runs.
#  * `renderer-static` is not used: its clock is frozen, so the transported
#    field never advances (`transport_armed=0`) and the flame is a seed point.
#  * `env -u WAYLAND_DISPLAY` is load-bearing: without it winit opens a window
#    on the live Wayland session and the compositor resizes it mid-run.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
BIN="${BYRO_BIN:-$ROOT/target/release/byroredux}"
OUT="${1:?usage: capture.sh OUT_DIR [REPS] [SCENE_PREFIX]}"
REPS="${2:-5}"
ONLY="${3:-}"

[[ -x "$BIN" ]] || { echo "FAIL: build first: $BIN" >&2; exit 1; }
command -v xvfb-run >/dev/null || { echo "FAIL: xvfb-run not installed" >&2; exit 1; }
mkdir -p "$OUT"

# name | engine args. 60 frames = 1 s of simulation time. The lab's one-shot
# explosion fires at t = 8 s, so 600 frames is 2 s after the blast.
SCENES=(
  "s1_close_flame|--combustion-lab --bench-mode renderer-stepped --bench-camera pan --bench-frames 300 --camera-pos -38,42,110 --camera-forward 0,-0.02,-1"
  "s2a_oil_blast_2s|--combustion-lab --bench-mode renderer-stepped --bench-camera pan --bench-frames 600"
  "s2b_oil_plume_7s|--combustion-lab --bench-mode renderer-stepped --bench-camera pan --bench-frames 900"
  "s3a_nuclear_blast_2s|--combustion-lab --combustion-lab-nuclear --bench-mode renderer-stepped --bench-camera pan --bench-frames 600"
  "s3b_nuclear_cloud_7s|--combustion-lab --combustion-lab-nuclear --bench-mode renderer-stepped --bench-camera pan --bench-frames 900"
  "s4_orbit_oil_lab|--combustion-lab --bench-mode renderer-stepped --bench-camera orbit --bench-frames 900"
)

cd "$ROOT"
for entry in "${SCENES[@]}"; do
  name="${entry%%|*}"
  args="${entry#*|}"
  [[ -z "$ONLY" || "$name" == "$ONLY"* ]] || continue
  for rep in $(seq 1 "$REPS"); do
    # shellcheck disable=SC2086
    env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 BYRO_DEBUG_SERVER=1 \
      timeout 300 xvfb-run -a "$BIN" $args --upscaler taa --window-size 1280x720 \
      --screenshot "$OUT/${name}_r${rep}.png" >"$OUT/${name}_r${rep}.log" 2>&1 \
      || { echo "FAIL: $name run $rep, see $OUT/${name}_r${rep}.log" >&2; exit 1; }
    echo "$name run $rep ok"
  done
done

python3 -I "$(dirname "$0")/aggregate.py" "$OUT" >"$OUT/baseline.tsv"
echo "wrote $OUT/baseline.tsv"
