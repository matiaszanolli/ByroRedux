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

The harness sets `BYROREDUX_FIXED_DT=0` by default. Simulation state stays
frozen while frame-counter-driven TAA and RT sampling advance deterministically.
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
| `BYROREDUX_RENDER_EVAL_DT` | `0` | Fixed simulation delta in seconds |
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
emissive contribution, direct shadows, reflections, one-bounce GI, SVGF, and
TAA. It does not yet cover:

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

## Related tools

- [Cornell scene](../../byroredux/src/cornell.rs)
- [Golden-frame integration test](../../byroredux/tests/golden_frames.rs)
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
`screenshot <path>`.
