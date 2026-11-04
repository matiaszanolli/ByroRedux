# Renderer Evaluation

Renderer changes need more than a clean compile and an attractive screenshot.
This workflow captures the same deterministic scene at known convergence
points, records performance and environment metadata, and produces a
controlled denoiser A/B.

The initial suite deliberately uses the self-contained Cornell scene. It needs
a Vulkan ray-query device, but no Bethesda game files, load order, or external
assets. Real-content cases can be added once this foundation is stable.

## Quick start

From the repository root:

```bash
scripts/renderer-eval.sh
```

Artifacts are written to `target/renderer-eval/` by default:

| Artifact | Purpose |
|---|---|
| `cornell_f1.png` | Cold-history image |
| `cornell_f8.png` | Early temporal convergence |
| `cornell_f32.png` | Mid-convergence image |
| `cornell_f64.png` | Default final reference |
| `cornell_f64_no_atrous.png` | Same frame with `DBG_DISABLE_ATROUS` |
| `cornell_f64_atrous_diff.png` | Optional ImageMagick absolute difference |
| `cornell_f64_restir_temporal_only.png` | Spatial reuse disabled |
| `cornell_f64_restir_spatial_only.png` | Temporal reuse disabled |
| `cornell_f64_restir_no_reuse.png` | Both reuse dimensions disabled |
| `*.log` | Raw engine output, including the machine-readable `bench:` line |
| `manifest.tsv` | Case, frame, flags, PNG SHA-256, and bench summary |
| `run-metadata.txt` | Git revision, dirty state, kernel, timing inputs, Vulkan summary |

ImageMagick's `compare` command is optional. Captures and the manifest are
still produced when it is unavailable.

## Determinism contract

The harness selects `--bench-mode renderer-static` by default. Simulation state
stays frozen while frame-counter-driven TAA and RT sampling advance deterministically.
Each case starts a fresh process, so history begins from the same cold state.
The Cornell camera, geometry, materials, and lights are authored in code and
do not depend on external files.

This controls engine inputs, not every source of GPU variation. Driver,
compiler, device, and floating-point implementation can still produce small
pixel differences, which is why evaluation artifacts record their environment
and the existing golden-frame test uses tolerances rather than byte equality.

## Configuration

| Variable | Default | Meaning |
|---|---|---|
| `BYROREDUX_RENDER_EVAL_OUT` | `target/renderer-eval` | Artifact directory |
| `BYROREDUX_RENDER_EVAL_FRAMES` | `1 8 32 64` | Fresh-process convergence captures |
| `BYROREDUX_RENDER_EVAL_MODE` | `renderer-static` | Named benchmark contract (`renderer-static`, `renderer-stepped`, `system-live`) |
| `BYROREDUX_RENDER_EVAL_CAMERA` | `orbit` | Moving path used only with `renderer-stepped` |
| `BYROREDUX_RENDER_EVAL_LOG` | `warn` | Engine `RUST_LOG` value |
| `BYROREDUX_RENDER_EVAL_RUNNER` | empty | Simple command wrapper, such as `xvfb-run --auto-servernum` |

Example:

```bash
BYROREDUX_RENDER_EVAL_FRAMES="1 4 16 64 256" \
BYROREDUX_RENDER_EVAL_OUT=/tmp/byro-restir-baseline \
scripts/renderer-eval.sh
```

On a headless machine with a working X11 Vulkan path:

```bash
BYROREDUX_RENDER_EVAL_RUNNER="xvfb-run --auto-servernum" \
scripts/renderer-eval.sh
```

The runner value is split on whitespace and executed directly; shell pipelines
and redirections are intentionally unsupported.

The final frame number is reused for all A/B captures. The suite decomposes
ReSTIR into full spatiotemporal, temporal-only, spatial-only, and no-reuse
cases without switching to the compile-time-gated legacy WRS estimator.

## How to evaluate a renderer change

Capture a baseline from a clean revision, make the renderer change, then
capture into another directory with identical frame settings:

```bash
BYROREDUX_RENDER_EVAL_OUT=/tmp/byro-before scripts/renderer-eval.sh
# make the change
BYROREDUX_RENDER_EVAL_OUT=/tmp/byro-after scripts/renderer-eval.sh
```

Review four distinct questions:

1. **Correctness:** missing geometry, NaNs, light leaks, broken material
   response, or history contamination.
2. **Convergence:** whether noise decreases from the cold image to the final
   image without detail collapsing.
3. **Temporal design:** whether the final/no-à-trous pair demonstrates that
   the spatial filter is removing variance rather than hiding an upstream
   estimator defect; and whether each ReSTIR reuse dimension contributes
   stability without history contamination.
4. **Cost:** compare `wall_ms`, `fence`, `gpu_taa`, draw counts, and other
   fields in `manifest.tsv`. Do not compare performance across different
   devices or busy/idle desktop states as if it were controlled data.

PNG hashes establish artifact identity; they are not pass/fail criteria across
different GPUs. Use the ignored golden-frame integration test for a guarded,
tolerance-based regression comparison on a stable GPU runner.

## Current scope and next cases

This first suite evaluates static diffuse, roughness/metalness sweeps, glass,
emissive contribution, direct shadows, reflections, bounded material-aware
path-traced GI, SVGF, and TAA. It does not yet cover:

- Motion-vector and disocclusion behavior
- Skinned BLAS updates
- Alpha-tested foliage and general transparency
- Outdoor sun, sky, terrain, water, or volumetrics
- Dense real-content light clusters
- Legacy WRS comparison; the shipped build compiles that path out, so
  `DBG_DISABLE_RESTIR` is intentionally not presented as a valid toggle

The next renderer-evaluation increment should add one deterministic moving
probe and one real-content dense-light scene. ReSTIR experiments should use
this suite before changing reservoir clamps, history caps, neighbor selection,
or visibility-ray counts.

## M-LIGHT reference gates

The Cornell captures above compare the engine with itself. The M-LIGHT
gates ([reference-lighting PRD](../prd/reference-lighting.md)) compare it
with a converged external render of the same scene, on scene-linear
captures (RLT-REF-007):

```bash
BYROREDUX_MITSUBA_PYTHON=~/mitsuba/bin/python scripts/m-light-gates.sh           # A15 + A16, warm cache
BYROREDUX_MITSUBA_PYTHON=~/mitsuba/bin/python scripts/m-light-gates.sh --cold    # A18 cold timing
cargo test -p byroredux --test m_light_gates -- --ignored --nocapture --test-threads=1
```

The runner (`scripts/m-light/gates.py`) builds the engine, runs each
`--cornell-reference` rung with `--linear-capture` (an N-frame f32 OpenEXR of
composite's output before bloom, exposure and tone mapping), exports the
same scene with `--export-mitsuba` (design in [Path Tracing §1](path-tracing.md#1-cornell-scene-exporter-ptr-val-004)),
renders the reference in Mitsuba 3 under the recipes of
[§2.3](path-tracing.md#23-component-recipes-in-mitsuba-terms), and writes
`target/m-light/manifest.tsv`: one row per gate, region and metric with the
engine and reference noise floors (two sampling seeds per side,
`--rt-test-seed-offset` on the engine), their root-sum-square and the pass
state. References are cached by a hash of the exported scene, the Mitsuba
version, the variant, the recipe, the sample count and the seed, so a
shader-only change reuses every one (RLT-REF-015). Regions come from one
camera ray per pixel centre intersected with the exported scene (Mitsuba's
`shape_index` AOV indexes neither the XML order nor `Scene.shapes()`), per
named shape, eroded three pixels so edge pixels — where the engine shades
the pixel centre and Mitsuba's box filter integrates the coverage — never
enter a mean.

The penumbra gates, A2 and A4, run on the `penumbra` and `penumbra-sun`
rungs. They measure visibility on both sides:

- **Engine.** `filtered_direct` divided by the same view captured with
  `DBG_DISABLE_DIRECT_SHADOWS`, at tier 3 with `--rt-test-no-jitter`.
- **Reference.** The direct recipe divided by the direct recipe with the
  rung's casters removed.

For A2, each of the rung's profile lines is averaged across its strip into
one-pixel bins, and the rung's named step (`disk_segment`) is fitted by
least squares. The gate then compares the 10–90 % widths. Below the
10-pixel minimum on the reference's own width the run stops rather than
reporting (RLT-SHD-001).

A4 reports the mean and 95th-percentile visibility error in three places:

- a band one reference width deep on the umbra side of each penumbra;
- the same band on the lit side;
- the rod's shadow region.

The profile and region geometry comes from the rung's `rung.tsv`
([Path Tracing §1.3](path-tracing.md#13-the-sun-and-the-rungs-measurement-geometry)).
Both gates are report-only until the owner sets the L1 budgets.

A5 (RLT-CON-001…008) runs the `contact` rung, which has five casters on a
normal-mapped floor:

- a tall block;
- a cube;
- a plank lying flat;
- an alpha-tested panel;
- a skinned actor proxy.

It reports the same visibility error over each caster's contact band (the
`band` records), at tiers 1–3, with the shadow filter on
(`filtered_direct`) and off (`direct_only`). The proxy is also measured
mid-animation under `renderer-stepped`, with `--bench-camera static` (the
mode needs an explicit path; `static` holds the camera while the clock
steps). That run takes a single-frame
capture at bench frame 45 and exports the scene at the same frame
(`--export-mitsuba-frame`), so the reference is rendered at the drawn pose
(RLT-EXP-001). Drawn frame F is clip time (F + 1) / 60 s, because the
animation steps before frame 0 is drawn. Two such runs must export
byte-identical posed meshes, and the proxy's yaw must be the clip's at
that time (0.46 rad at frame 45). Both held on 2026-11-04, which closed
V9 (#6605): `renderer-stepped` is the RLT-REF-017 mode.

A14 (RLT-SHD-010, `penumbra-moving`) and A3 (RLT-SHD-002, a `pan` of the
`penumbra` rung) are temporal gates. They compare the engine with itself,
taking `final` after TAA (`--linear-capture-tap post-taa`) along a
120-frame path that ends in a hold (`--bench-camera-length`):

- **Samples.** Every 16 frames from frame 32 (`--linear-capture-at`), then
  1, 4 and 16 frames after the stop.
- **Parked reference.** Each sample is compared with the same path frozen
  at that frame (`--bench-camera-freeze-at`) and converged for 256 frames.
- **Floor.** For a sample during the motion: the same single frame of a
  run held at that frame's pose from frame 0, against the same parked
  reference. The run's temporal accumulators are still filling at frame
  32, so this floor carries the cold start as well as sampling noise. The
  excess over it is what the motion adds. After the stop: the post-stop
  frames of a run parked at the stop pose from frame 0.

`SHD-005` publishes `direct_only`'s luminance integral on the `restir-clamp`
rung with the reservoir cap on and off. RLT-SHD-006 re-runs A3 with
`--rt-test-taa-gamma 1.0` beside the shipped 1.5.

Game content is held by declared changes, not a reference (RLT-CNT).
`scripts/m-light-baselines.sh` records, per benchmark scene, the settled
adaptive ray-quality tier (`rt_tier_hist` on the bench line, 60-frame
warm-up), the frame time with the tier pinned there, the material-class
report (`--material-dump`), and two seeded linear captures of `final` at the
pinned pose — the content anchor. `scripts/m-light/content.py compare`
then checks a later capture against the anchor on a 32 × 32 tile grid: a
tile passes inside its two-capture floor plus the scene margin, or inside a
declared region changed the declared way.

Engine switches the rungs use, all diagnostic: `--rt-test-seed-offset N`
(RLT-REF-013), `--rt-test-no-jitter` (TAA only; RLT-REF-020),
`--rt-test-no-dust-floor` (drops the 0.006 m⁻¹ interior dust medium;
RLT-REF-009), `BYROREDUX_RENDER_DEBUG=0x20` (`DBG_DISABLE_SVGF`,
RLT-REF-018), and the `filtered_direct` / `traced_share` views
(RLT-SHD-009, RLT-GI-009). Phase 3 adds:

- **Bias switches.** `--rt-test-no-reservoir-clamp` (RLT-SHD-005) and
  `--rt-test-taa-gamma G` (RLT-SHD-006).
- **Temporal capture.** `--linear-capture-tap post-taa` and
  `--linear-capture-at F1,F2,…`, which needs a `{frame}` in the path.
- **Bench clock.** `--bench-camera-length N` and
  `--bench-camera-freeze-at F`. The camera path and every `BenchMover`
  blocker share this clock.
- **Export frame.** `--export-mitsuba-frame F`.

## Related tools

- [Cornell scene](../../byroredux/src/cornell.rs)
- [Golden-frame integration test](../../byroredux/tests/golden_frames.rs)
- [`rt-decomposition-matrix.sh`](../../scripts/rt-decomposition-matrix.sh) —
  pairs runtime-disabled and compile-time-disabled direct-shadow, GI,
  reflection/glass, and all-main-ray-query variants under `renderer-static`;
  rejects the matrix if any scene-state fingerprint differs.
- [Shader pipeline](shader-pipeline.md)
- [Renderer architecture](renderer.md)
- [Shadow-pipeline trade-offs](shadow-pipeline-tradeoffs.md)
- [Debug CLI](debug-cli.md)

## Fallout: New Vegas real-content probe

The Cornell suite is complemented by a deterministic Prospector Saloon probe
using locally installed FNV assets:

```bash
scripts/renderer-eval-fnv.sh
```

Override the default data directory with `BYROREDUX_FNV_DATA`. The suite
captures all four `--rotation-mode` conventions plus material-state,
pre-SVGF indirect, GI-bounce-only, and no-à-trous views. Its manifest lives in
`target/renderer-eval-fnv/manifest.tsv`.

Material-state colors are grey=opaque, green=alpha-test, red=alpha-blend, and
blue=glass. `DBG_VIZ_RAW_INDIRECT` includes authored ambient; use
`DBG_VIZ_GI_BOUNCE` when the question is specifically whether stochastic
diffuse bounce rays return useful energy.

For interactive follow-up, launch the engine with `--features debug-server`
and `--bench-hold`, then connect with `target/release/byro-dbg`. Useful probes
for this scene include `near 300`, `inspect <entity>`, `stats`, and
`screenshot <name>` (a bare filename, written under the engine cwd's
`screenshots/`).
