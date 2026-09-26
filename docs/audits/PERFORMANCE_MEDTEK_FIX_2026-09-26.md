# MedTek TLAS and streaming fixes — 2026-09-26

## Cause and implementation

The renderer sorted TLAS instances only by BLAS address. Multiple placements
of the same mesh have the same address. Raster/frustum reordering could exchange
distant entities among those equal-address leaves, while the address-only UPDATE
guard continued to allow refits. This produced a valid but expensive traversal
hierarchy. The tiny TLAS build timer did not expose its downstream cost.

Production canonicalization now sorts by `(BLAS address, full EntityId)`.
Shader-visible custom indices still point to the current compacted SSBO entry.
Each frame-in-flight slot retains its entity sequence; changed membership forces
BUILD even when addresses and counts match. Abandoned command recording marks
the slot for BUILD before reuse. Identity storage is reused, shrunk with the
working set, and included in `ctx.scratch` telemetry.

No ray mask, shadow caster, quality tier, shader or skinned-BLAS policy changed.

## Matched diagnostic evidence

RTX 4070 Ti, NVIDIA 580.178.04, Rust 1.96.0 release builds. Baseline renderer
source is `0e2055e45` (same renderer as `e2f99ad55`). This stationary interior
does not exercise the exterior streaming pipeline. One engine at a time, no
concurrent compilation. Explicit 1280×720
Native AA, ray tier 0, renderer-stepped/pan, 120 frames at fixed 1/60 simulation.
These are short captures including startup, not a steady-state percentile study.

| Diagnostic | Main GPU ms | Wall ms | FPS | GPU TLAS ms | GPU BLAS ms |
|---|---:|---:|---:|---:|---:|
| Baseline | 332.913 | 367.08 | 2.7 | 0.030 | 0.445 |
| Exclude actors from first direct shadow sample | 335.339 | 366.95 | 2.7 | 0.030 | 0.437 |
| Fresh skinned BLAS on every dirty pose | 332.144 | 371.45 | 2.7 | 0.030 | 4.638 |
| Stable equal-BLAS entity ordering | 65.926 | 112.80 | 8.9 | 0.030 | 0.436 |
| Fresh TLAS BUILD every frame | 67.333 | 112.84 | 8.9 | 0.161 | 0.441 |

All five finished with state hash `cf6bfdee2e27ebd5`, 39,560 entities, 286 lights,
13,038 TLAS instances, 8,733 raster commands, and simulation time 1.999999 s.
State hashes cover the harness state, not every GPU buffer. The actor diagnostic
had a 31.5 s cold shader-pipeline first frame; it is useful as a negative GPU
workload probe, not as a startup latency comparison. Diagnostic variants were
reverted before the production build.

The fresh-TLAS result corroborates the stable-ordering result. Rebuilding skinned
BLAS adds cost without helping, so changing its rebuild policy is unsupported.

## Production measurements

The final release build includes stable identities, membership/rollback guards,
scratch telemetry and bounded streaming. A fresh serial fixed/baseline/fixed
sequence with the same 120-frame configuration measured:

| Build | Main GPU ms | Wall ms | FPS | GPU TLAS ms |
|---|---:|---:|---:|---:|
| Production, first | 67.075 | 112.86 | 8.9 | 0.030 |
| Baseline | 331.950 | 365.74 | 2.7 | 0.030 |
| Production, second | 65.995 | 113.38 | 8.8 | 0.030 |

All three have state hash `cf6bfdee2e27ebd5` and the same scene counts as the
diagnostics. Main GPU cost fell approximately **80%**; wall time fell **69%**,
or about **3.2×** the FPS calculated from wall milliseconds. These are MedTek
results on this GPU, not a cross-game performance guarantee. CPU TLAS preparation
rose from 0.81 ms to 1.10–1.16 ms; GPU refit cost stayed 0.030 ms.

A separate production capture using FSR Quality (853×480 internal, 1280×720
output) measured **34.438 ms** main GPU, **81.82 ms** wall, **12.2 FPS**, with
the same final state hash. This reduces render resolution and is a separate
quality configuration, not another like-for-like optimization gain. Persisted
user settings were not changed.

Machine-readable summaries: [BENCH_medtek_fix_2026-09-26.tsv](BENCH_medtek_fix_2026-09-26.tsv).
Full local logs and argv arrays: `/tmp/medtek-fix/`.

## Image comparison

A separate baseline/stable-ordering pair used the same settings with screenshot
capture and CPU profiling. Both finished at frame 122, state hash
`4f44c7f913e10dd9`, simulation time 2.033332 s. Capture adds two frames compared
with the timing-only run. Main GPU time was 335.752 versus 64.908 ms.

RGB absolute difference on the 0–255 scale: mean **0.239**, RMSE **0.961**,
99th percentile **4**, maximum **67**; **78.16%** of pixels exactly matched.
Visual inspection found no obvious scene change. This is not pixel equivalence
or exhaustive coverage of all materials and games. Local PNGs and full logs:
`/tmp/medtek-fix/stepped_images/`.

The final production build repeated this 122-frame capture with
`BYRO_VALIDATION=1`; synchronization validation was confirmed enabled in the
log. No Vulkan errors or VUID messages were logged; nine warnings reported
vertex outputs unused by the fragment shader. It matched the screenshot pair's
state hash. Production versus baseline image differences were mean **0.239**,
RMSE **0.951**, 99th percentile **4**, maximum **55**, with **78.17%** exact
pixels. Validation-enabled timings are recorded separately and excluded from
the production performance comparison above.

## Bounded streaming pipeline

Exterior cell preparation now overlaps serial archive extraction/inflate with
parallel NIF parse/import on the existing private Rayon pool. It removes the
extract-all barrier. Cells below eight fresh inputs remain serial. Output order,
archive precedence, negative results, cache behavior, and per-NIF panic recovery
are preserved.

Queued/running decoded inputs are limited to 64 MiB and twice the pool's worker
count, capped at 32 tasks. An oversized input runs alone; the coordinator may
hold one additional lookahead buffer. Parsed output, parser scratch and external
Starfield mesh reads are outside this input budget. See
[archive pipeline details](../engine/archives.md#exterior-streaming-overlap).
This targets cell readiness and movement hitches, not stationary MedTek GPU FPS.

Runtime exercise: FO4 Commonwealth `(0,0)`, radius 1, `grid-cross`, 60 logical
frames, 640×360 output, FSR Performance, tier 0. Initial preparation plus three
boundary crossings processed **18 nonempty cells / 2,191 unique-per-request
inputs**, including cells with hundreds of fresh NIFs. Peak queued/running input
capacity was **1,008,878 bytes**, peak tasks **5**, and largest cell pipeline
wall time **62.303 ms**. The coordinator and worker shut down cleanly; no error
or panic appeared in this capture. `unsettled_full=0`, `unsettled_lod=0`,
`full_superseded=0`, and `lod_superseded=0` at exit.

This run demonstrates execution and observed bounds, not a baseline-relative
streaming speedup or saturation/oversized-input coverage. Readiness pauses expand
60 logical frames to 176 actual frames. Full-detail readiness still averaged
9,768 ms per crossing; apply slices averaged 27.98 ms and peaked at 126.42 ms,
and scheduler time averaged 143.46 ms. Significant main-thread work remains.
Local full log: `/tmp/medtek-fix/stepped_exterior/fixed.log`; per-cell data:
[BENCH_stream_pipeline_2026-09-26.tsv](BENCH_stream_pipeline_2026-09-26.tsv).

```sh
env -u WAYLAND_DISPLAY -u GDK_BACKEND -u BYROREDUX_FIXED_DT \
  XDG_SESSION_TYPE=x11 RUST_LOG=info,byroredux::streaming=debug \
  BYROREDUX_RENDER_DEBUG=0 xvfb-run --auto-servernum target/release/byroredux \
  --game fo4 --grid 0,0 --radius 1 --wrld Commonwealth --fly \
  --window-size 640x360 --upscaler fsr3 --fsr-quality performance \
  --rt-test-ray-quality-tier 0 --bench-mode renderer-stepped \
  --bench-camera grid-cross --bench-frames 60
```

## Remaining work

- CPU profile samples place trigger detection around 6–9 ms, scene packages
  around 5 ms and physics sync around 3.5 ms. These are samples, not additive
  averages; total scheduler cost remains about 21 ms on the measured path.
- The main GPU pass still costs roughly 66 ms at Native AA. Secondary-light
  clustering and raster/material work remain candidates for separate profiling.
  Reusing primary clusters safely requires current-frame availability, matching
  radial depth/projection, actual cluster-AABB containment, overflow fallback,
  and deterministic score/index tie-breaking. A blind cluster substitution can
  change lighting, so it is not part of this fix.
- Texture/precombine preparation and upload completion waits remain separate
  streaming work. No general archive-loading speedup is claimed here.

Reproduction after a Rust 1.96.0 release build:

```sh
env -u WAYLAND_DISPLAY -u GDK_BACKEND -u BYROREDUX_FIXED_DT \
  XDG_SESSION_TYPE=x11 RUST_LOG=info BYROREDUX_RENDER_DEBUG=0 \
  xvfb-run --auto-servernum target/release/byroredux \
  --game fo4 --cell MedTekResearch01 --window-size 1280x720 \
  --upscaler fsr3 --fsr-quality native-aa --rt-test-ray-quality-tier 0 \
  --bench-frames 120 --bench-mode renderer-stepped --bench-camera pan
```

No unit tests were added or run. The existing sort-helper fixture was adapted
to the new identity lookup argument. Code graph transport was unavailable;
source discovery used filesystem fallback. All GPU captures ran serially.
