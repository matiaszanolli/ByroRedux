# CPU, streaming and GPU follow-up — 2026-09-26

**Baseline:** `5eb07a4f3`, following the [TLAS ordering fix](PERFORMANCE_MEDTEK_FIX_2026-09-26.md).
**Scope:** CPU scheduler fixes, cooperative precombine spawning, and finer GPU attribution.
RTX 4070 Ti / NVIDIA 580.178.04, Rust 1.96.0 release builds. Captures run serially,
with no concurrent builds. No shaders, ray masks, quality tiers, physics cadence
or gameplay update frequency changed.

## Implemented

- Package and target registries use `Arc` snapshots with copy-on-write
  installation. Scene execution still releases ECS resource guards before
  evaluating conditions, without cloning whole tables and nested linked-reference
  vectors every frame.
- Trigger detection retains inside pairs plus previous actor/volume sets,
  instead of materializing the complete Cartesian occupancy map. It reuses
  buffers and reads candidate transforms under one storage guard. Every original
  geometric check remains; initial-entry suppression, tethered-horse behavior,
  condition-ready retries and event order are preserved.
- Physics checks use immutable Rapier body access until a write is required.
  Mutable access otherwise marks bodies modified even when the caller only
  reads them. This removes unnecessary dirty entries, but a meaningful physics
  speedup was **not** measured: the remaining step remains around 3 ms.
- CSG precombine placements retain their root and original mesh indices across
  cooperative groups capped at eight submeshes / approximately 4 MiB geometry
  input. Each group completes its BLAS work before yielding. Ordinary NIF
  fallbacks and unlimited bootstrap retain their existing atomic behavior.
- Five GPU timestamp pairs distinguish opaque/alpha-tested, blended, water,
  groundcover-model raster and groundcover-blade raster work. `BYRO_PROFILE=1`
  emits phase timings and active flags. Pools now contain 56 slots / 28 pairs.

## MedTek CPU improvement

Explicit Native AA at 1280×720, ray tier 0, renderer-stepped/pan, fixed simulation
step 1/60. The baseline binary was preserved before source changes. The
120-frame fixed/baseline/fixed sequence measured:

| Build | Scheduler ms | Wall ms | FPS | Main GPU ms |
|---|---:|---:|---:|---:|
| CPU/stream fixes, first | 9.65 | 102.04 | 9.8 | 67.309 |
| Baseline | 21.15 | 113.39 | 8.8 | 65.779 |
| CPU/stream fixes, second | 9.88 | 101.89 | 9.8 | 66.337 |

All three finished with state hash `cf6bfdee2e27ebd5`, 39,560 entities, 286 lights,
13,038 TLAS instances and the same raster counts. Scheduler time fell about
54%; total frame time fell about 10%. Main GPU time did not improve.
Profile samples place trigger detection around **0.46–0.47 ms**, versus
**6.27–6.76 ms** in the baseline; scene packages fell out of the top-eight list
after previously costing approximately 4–5 ms. These are system samples,
not an additive average breakdown.

### Longer run and image comparison

The final build, including the new GPU timers, was compared over 300 requested
frames with screenshot capture (302 actual frames). Both ended at simulation
time 5.033344 s and hash `ffe158a159e1c2e9`, with matching scene/draw counts.

| Build | Scheduler ms | Wall ms | FPS | Main GPU ms |
|---|---:|---:|---:|---:|
| Baseline | 18.58 | 111.62 | 9.0 | 67.384 |
| Final | 8.03 | 101.14 | 9.9 | 66.881 |

RGB difference on a 0–255 scale: mean **0.221**, RMSE **0.958**, 99th percentile
**4**, maximum **67**, **80.04%** exact pixels. This is a small image difference,
not pixel equivalence or exhaustive gameplay coverage. Local images are under
`/tmp/perf-next/stepped_long/`. Captures include startup/transient frames; no
warmup exclusion is applied to the benchmark summaries.

## Remaining GPU cost

Fresh 120-frame diagnostic captures against the saved baseline:

| Diagnostic | Main GPU ms | Wall ms |
|---|---:|---:|
| Normal rays | 60.404 | 106.09 |
| Direct shadows disabled | 45.142 | 88.96 |
| Reflection/glass rays disabled | 55.713 | 104.26 |
| All main rays disabled | 40.129 | 83.39 |

These change the image and were not shipped. Direct-shadow disabling also
bypasses ReSTIR. Results are sensitivities, not additive ray costs. Tier 0
already disables GI. The normal-ray baseline varied from about 60 to 67 ms
across this session, so the small reflection-only delta is not a precise
attribution or a demonstrated optimization opportunity by itself.

A short Nsight Systems Vulkan capture succeeded, but this setup exposed only
queue-submit GPU intervals: 975 workload records, no per-draw shader attribution.
It therefore cannot establish occupancy, ALU, bandwidth or overdraw as the cause.
Local capture: `/tmp/perf-next/medtek-vulkan.nsys-rep` and SQLite export.

The final long run's **300 direct timestamp-readback samples** averaged:

| Phase | Mean ms |
|---|---:|
| Inclusive main render | 63.949 |
| Opaque / alpha-tested geometry | 60.432 |
| Blended geometry | 3.071 |
| Water, groundcover model draws, groundcover blade draws | inactive |

The direct readback sample mean is distinct from the benchmark summary's
aggregation. Approximately 94.5% of the sampled main interval lies in the opaque
phase. Both ends of the internal pairs use `BOTTOM_OF_PIPE`, with no added
barrier or draw reordering. GPU work can overlap phase boundaries: these are
completion intervals, not isolated shader execution times. Main-pass
setup/clear/store work is not fully attributed to the children. See the
[Vulkan timestamp command](https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdWriteTimestamp.html).

This narrows the next renderer investigation to opaque material shading,
depth rejection and primary visibility work. It does not justify a blind
reflection-cluster rewrite or a shadow-quality reduction.

## Exterior application

FO4 Commonwealth `(0,0)`, radius 1, 640×360 output, FSR Performance, tier 0,
renderer-stepped/grid-cross, 60 logical camera frames. Both completed three
crossings with no superseded cells and `unsettled_full=unsettled_lod=0`.

| Metric | Baseline | CPU + grouped spawning |
|---|---:|---:|
| Actual frames, including readiness pauses | 173 | 191 |
| Apply average ms | 25.84 | 22.38 |
| Apply p95 ms | 61.87 | 43.92 |
| Apply maximum ms | 131.94 | 73.78 |
| Full-detail readiness average ms | 9528.93 | 8824.60 |
| Frame median ms | 76.48 | 59.99 |
| Frame p95 ms | 817.70 | 791.70 |
| Frame maximum ms | 1134.61 | 1246.27 |

The worst apply slice fell about 44%, but the **whole-frame maximum did not
improve**. Significant hitches remain outside the newly divided unit, including
physics registration, texture flushes and other indivisible preparation.
Whole-process physics logs also contain approximately 900 ms registration
bursts during bootstrap, before the game loop; these must not be attributed
to the measured mid-traversal stalls.
After entering the game loop, registration peaked at **27.93 / 28.82 ms**;
later Rapier-step spikes reached **99.27 / 106.20 ms** with no new bodies.
Further physics work needs separate shape-conversion/insertion and Rapier-stage
timers. A registration-batch cache for identical immutable triangle meshes is
a candidate, but would not explain or fix the no-newcomer step spike.
For cell `0000E03B`, hash `1959ced1`, the baseline hash took 130.64 ms, including
116.07 ms spawning. The modified cell's largest individual spawn group took
43.91 ms; the hash still took 120.06 ms in total active work. Grouping divides
the stall; it does not remove all of that work or guarantee the 4 ms deadline.

### Capacity and content caveat

This traversal is **not content-identical**. Both hit the same vertex-cap
failure at `3,999,902 → 4,000,443`, but the old transaction rolled back 38 fresh
submeshes while the grouped path rolled back only its current group. The first
16 cells had matching precombine/reference counts. The final two retained 38
additional precombine render entities, and ordinary references reused that
geometry. Smaller admission units therefore preserve useful geometry within
the unchanged cap.

Final counts were 102,805 → 102,926 entities, 4,883 → 4,908 meshes,
2,560 → 2,565 textures and 19,900 → 19,974 TLAS instances. Readiness pauses also
produce different simulation times and hashes. Treat this as a capacity-limited
traversal observation, not a like-for-like exterior FPS percentage. No duplicate
placement retry was found in the source/log review.

## Validation and controls

The final Rust 1.96.0 release build succeeded. A 60-frame MedTek capture
confirmed synchronization validation enabled, with no Vulkan errors or VUID
messages. Nine shader-interface warnings reported unused vertex outputs.
A separate four-frame static exterior capture also completed with synchronization
validation enabled and no Vulkan errors/VUID messages. It exercised active water,
groundcover-model and groundcover-blade timestamp branches. This short capture
checks command recording, not steady-state exterior performance.

Short 60-frame Native AA/tier-0 controls also completed:

| Control | Baseline scheduler ms | Final scheduler ms | Matching final hash |
|---|---:|---:|---|
| FNV `GSProspectorSaloonInterior` | 1.97 | 1.79 | `9c1bcddd51f2ac0f` |
| Skyrim SE `WhiterunBanneredMare` | 2.84 | 1.51 | `b2e52c0f557fe9ba` |

Entity, mesh, texture, light and TLAS counts matched within each pair. These are
smoke/control observations, not a broad gameplay or performance certification.
The first FNV baseline had a 15.6-second cold pipeline compilation spike, so
its first-frame timing is unsuitable for comparison with the warmed final run.

## Reproduction and artifacts

```sh
env -u WAYLAND_DISPLAY -u GDK_BACKEND -u BYROREDUX_FIXED_DT \
  XDG_SESSION_TYPE=x11 RUST_LOG=info BYRO_PROFILE=1 BYROREDUX_RENDER_DEBUG=0 \
  xvfb-run --auto-servernum target/release/byroredux \
  --game fo4 --cell MedTekResearch01 --window-size 1280x720 \
  --upscaler fsr3 --fsr-quality native-aa --rt-test-ray-quality-tier 0 \
  --bench-mode renderer-stepped --bench-camera pan --bench-frames 300
```

For exterior traversal, replace the cell selection with
`--grid 0,0 --radius 1 --wrld Commonwealth --fly`, resolution with `640x360`,
quality with `performance`, camera with `grid-cross`, and frame count with `60`.
Add `BYRO_VALIDATION=1` for synchronization validation and `--screenshot PATH`
for an exit capture; compare validation/capture runs separately from timing-only
runs. Persisted user preferences were not changed.

Raw local logs, argv arrays and images: `/tmp/perf-next/`. Machine-readable
data: [benchmark summaries](BENCH_cpu_stream_gpu_2026-09-26.tsv),
[streaming summaries](BENCH_cpu_stream_gpu_streaming_2026-09-26.tsv), and
[GPU geometry samples](BENCH_gpu_geometry_2026-09-26.tsv). Release compilation and
runtime captures were performed; no unit tests were added or run. Root graph
transport failed; worker graph indexing worked, with filesystem fallback where
needed. No GitHub issues were filed.
