# Dense-scene renderer performance opportunities — 2026-09-26

> **Implementation follow-up:** the narrow opaque early-test prototype measured
> substantial GPU savings but failed the Kendall lighting-history visual gate.
> Production behavior was restored. See [the experiment, captures and archived patch](PERFORMANCE_OPAQUE_EARLY_TESTS_2026-09-26.md).

**HEAD**: `b7491072f47aebf67fc35e76e6cfeffedd5831bf` · **Baseline**: [same-day CPU/streaming/GPU follow-up](PERFORMANCE_CPU_STREAM_GPU_2026-09-26.md), following the [MedTek TLAS fix](PERFORMANCE_MEDTEK_FIX_2026-09-26.md) · **Audited**: dimensions 2, 5, 6, 8, focused on dense-scene GPU cost · **Unchanged since baseline (skimmed)**: existing draw, skinning and TLAS guards. Explicit focus overrides delta-only scoping. This is not a full memory, CPU, streaming or exterior audit.

## Executive summary

The remaining measured hotspot is opaque/alpha-tested rendering. Three fresh MedTek runs at 1280×720 Native AA report **60.402–67.046 ms** in the main GPU pass. Direct timestamp readbacks attribute **94.6–95.2%** of that phase's completion interval to opaque/alpha-tested geometry. Blended geometry, acceleration-structure construction and post-processing are substantially smaller.

FSR Quality reduces median reported main-pass time from **66.214 to 34.171 ms**. Disabling all main-pass ray families at native resolution leaves **39.277 ms**. These controls establish strong resolution and ray-path sensitivity; they do not identify individual shader instructions or establish that the remaining cost is purely rasterization. Both controls change the image, and neither is a newly implemented optimization.

The highest-priority implementation experiment is a **discard-free opaque shader permutation with explicit early fragment tests**, followed by compatible coarse front-to-back ordering. Conservative raster occlusion and material shader specialization are larger follow-ups. No new correctness defect or cross-revision regression was confirmed; the candidates below have no demonstrated shipping speedup and are intentionally not marked NEW audit findings. No GitHub issues were filed.

Low allocated RAM/VRAM is compatible with expensive per-pixel computation, texture access and ray traversal. Capacity usage does not measure bandwidth saturation or shader occupancy. The measurements here locate expensive GPU intervals; they do not establish which hardware execution unit is saturated. Memory ceilings remain owned by [memory-budget.md](../engine/memory-budget.md).

## Reproduction and measurement limits

- RTX 4070 Ti, 12 GB. Release binary rebuilt successfully at the recorded HEAD using the installed Rust 1.96.0 toolchain. Source was clean during captures; no production edits or concurrent compilation.
- Nine serial MedTek captures: three configurations, three repeats each, with configuration order reversed in round two. Each uses renderer-stepped/pan, 120 frames, 1280×720 output, explicitly selected FSR mode and ray tier **0**. GI is already disabled at that tier. No fixed-dt environment override.
- All nine runs end with state hash `43cada0e41e72845`, 39,560 entities, 286 lights, 13,038 TLAS instances, 8,733 raster commands and 1,212 batches. The hash checks harness state, not byte identity of every GPU resource.
- FSR Quality renders at 853×480. It also changes froxel dimensions and resolution-dependent ray LOD; its delta is not a pure pixel-count experiment.
- No separate warmup exclusion was added. These are short controlled captures, not a steady-state frame-rate guarantee. Benchmark summary aggregates and raw per-frame readback means are kept separate below.
- Full commands, logs, runner and issue dedup snapshot are retained locally in `/tmp/audit/performance/dense-2026-09-26/`. Durable per-run measurements: [BENCH_dense_gpu_2026-09-26.tsv](BENCH_dense_gpu_2026-09-26.tsv).
- The existing variability script uses system-live settings. This investigation instead repeats the explicitly pinned renderer-stepped configuration. The observed ranges below are the empirical variability envelope, not confidence intervals. Native main-pass peak-to-peak variation is **6.644 ms**; small future gains require longer matched captures.

```sh
env -u WAYLAND_DISPLAY -u GDK_BACKEND -u BYROREDUX_FIXED_DT \
  XDG_SESSION_TYPE=x11 RUST_LOG=info BYRO_PROFILE=1 BYROREDUX_RENDER_DEBUG=0 \
  xvfb-run --auto-servernum target/release/byroredux \
  --game fo4 --cell MedTekResearch01 --window-size 1280x720 \
  --upscaler fsr3 --fsr-quality native-aa --rt-test-ray-quality-tier 0 \
  --bench-frames 120 --bench-mode renderer-stepped --bench-camera pan
```

For Quality, replace `native-aa` with `quality`. For the runtime ray ablation, retain Native AA and set `BYROREDUX_RENDER_DEBUG=0x40000000`. This flag changes direct-light/reuse and reflection/glass paths; it retains the same compiled shader and does not disable volumetric rays.

### Reported benchmark summaries

Values are median across three run summaries, with min–max in parentheses. Main-pass and wall-time figures are different measurements and must not be added.

| MedTek configuration | Main GPU ms | Wall ms | Main GPU median absolute deviation |
|---|---:|---:|---:|
| Native AA | 66.214 (60.402–67.046) | 101.43 (95.61–102.81) | 0.832 ms |
| FSR Quality | 34.171 (33.522–34.429) | 70.40 (66.48–70.60) | 0.258 ms |
| Native AA, main rays disabled | 39.277 (38.747–39.650) | 73.93 (71.16–74.73) | 0.373 ms |

### Geometry phase attribution

Each MedTek run logs 118 completed query readbacks for 120 submitted frames, reflecting the two-frame delay. The table reports the median of each run's arithmetic mean. These means are **not** the benchmark-summary aggregation above. Phase timestamps measure GPU completion intervals, not isolated fragment-shader execution.

| Configuration | Inclusive main ms | Opaque/alpha-tested ms | Blended ms |
|---|---:|---:|---:|
| Native AA | 63.789 | 60.380 | 3.016 |
| FSR Quality | 32.268 | 30.523 | 1.452 |
| Native AA, main rays disabled | 36.369 | 34.705 | 1.422 |

Water and ground-cover phases are inactive here. Parent/child intervals overlap; do not sum main rendering with its geometry phases, or volumetrics with inject/integrate.

### Other costs and controls

| Metric | Native MedTek range, ms |
|---|---:|
| GPU TLAS build/refit | 0.030 |
| GPU skinned BLAS refit | 0.434–0.439 |
| GPU skin dispatch / palette | 0.007–0.013 / 0.013 |
| GPU volumetrics | 0.879–1.328 |
| GPU SVGF | 0.476–0.488 |
| GPU cluster cull | 0.178–0.181 |
| GPU sky cube | 0.201–0.210 |
| CPU build-render-data | 4.15–4.35 |
| CPU scheduler | 9.27–10.09 |
| CPU SSBO preparation/upload interval | 2.06–2.25 |
| CPU submit/present interval | 82.13–87.88 |

Submit/present includes host waiting/throttling and is not active CPU computation. Cheap TLAS construction does not imply cheap later traversal. The scheduler remains material to total frame time despite low whole-machine CPU utilization, but it does not explain the 60–67 ms main GPU interval.

Single 60-frame renderer-stepped/pan Native AA controls measured **12.109 ms** main GPU in FNV Prospector and **8.668 ms** in Skyrim SE Bannered Mare. They are contextual controls, not repeated noise estimates. Prospector recorded a **17.25-second maximum frame** and substantial cold pipeline-compilation time; its CPU aggregate is unsuitable as a warm performance baseline. These interior results do not establish exterior terrain/foliage/water costs.

One asynchronous device-wide sample during the matrix reported 3,438 MiB allocated out of 12,282 MiB, 39% GPU utilization and 2% memory-controller utilization. It is not engine-exclusive, sustained utilization, bandwidth throughput or occupancy evidence; no conclusion about hardware saturation follows from that sample.

**Observed versus ROADMAP:** the historical record at `4c9a5b36` uses renderer-stepped/orbit and different capture length/settings. `scripts/check-bench-harness-provenance.sh 4c9a5b36` returned **DIVERGED**, citing `88c23887b` and `8d2a9ebad`. Current pan/Native AA results cannot establish a regression percentage against that record. Re-benchmark both revisions with common settings before making that claim.

## Prioritized optimization candidates

### 1. Avoid shading hidden discard-free opaque fragments

**Dimension:** GPU Pipeline / Draw & Instancing. **Status:** optimization experiment; benefit unmeasured.

`crates/renderer/shaders/triangle.frag` combines ordinary opaque materials, alpha tests, effects and fire refraction. It writes reservoir storage near line 3833 and contains atomic diagnostics, but does not declare early fragment tests. Shader storage side effects constrain automatic rejection before shader execution. Explicit early fragment tests can skip hidden invocations, but depth writes then precede shader discard. See the [Vulkan fragment-operations specification](https://docs.vulkan.org/spec/latest/chapters/fragops.html).

The shared shader has discard sites at lines 448, 462, 1193, 1234, 1255 and 1275: authored alpha comparison, nearly transparent blended texels, effect opacity/fades, and fire-refraction ray/coverage decisions. Globally enabling early tests can leave depth behind for pixels that should be holes. [Issue #779](https://github.com/matiaszanolli/ByroRedux/issues/779) was closed as unsafe as filed; its narrow opaque-permutation follow-up remains relevant, not a regression of a landed fix.

**Concrete experiment:** compile a separate SPIR-V module with `layout(early_fragment_tests) in;`, selected only for a conservative allowlist of canonical opaque materials with ordinary depth behavior, no alpha blending, no alpha threshold and no effect/fire-refraction discard path. Eligibility must reflect current material state. Carry the distinction through `PipelineKey::Opaque` (`crates/renderer/src/vulkan/pipeline.rs:112`), pipeline lifetime/recreation, sorting and batching (`context/build_and_upload_instances.rs:527`) so safe and unsafe instances cannot share the wrong variant. This execution mode requires a separate module, not a runtime toggle or specialization constant.

**Acceptance:** matched GPU A/B and fragment-invocation counts if instrumentation is added; image/temporal-history comparisons across MedTek and small controls; explicit cutout, effect, glass, animated-material and custom-depth coverage; Vulkan validation and rollback. Reduced hidden reservoir writes also require temporal-lighting review. Source alone does not prove the driver currently executes all hidden fragments or quantify the attainable saving.

### 2. Put useful opaque occluders earlier

**Dimension:** Draw & Instancing. **Status:** optimization experiment; benefit unmeasured.

`byroredux/src/render/mod.rs:868–888` orders the opaque key by state and mesh before coarse depth. Near geometry therefore only sorts ahead within a mesh/state group. `opaque_depth_bucket` retains ten bits to limit camera-driven upload churn. Current depth comes from model-pivot clip W (`render/static_meshes.rs:742–748`), which can be shared by precombined submeshes.

**Concrete experiment:** after establishing safe early rejection, compare coarse depth-before-mesh ordering within compatible ordinary opaque depth/layer classes. Preserve decals, authored depth/cull state, true alpha-over order and no-sorter semantics. Measure opaque GPU time against batch count, indirect calls, CPU sorting and instance/indirect upload hash hits. Evaluate bounds-based near depth separately; a negative pivot-based test would not disprove its value.

This trades some batching/cache stability for less hidden shading. It is not the previously falsified decorate-sort-undecorate proposal from #4194. A depth prepass is a separate, larger experiment requiring exact coverage/position parity; prior prepass artifacts are a reason to start with the narrow permutation.

### 3. Add conservative raster occlusion for detailed interiors

**Dimension:** Draw & Instancing. **Status:** architectural candidate; benefit unmeasured.

Static raster admission uses frustum bounds (`byroredux/src/render/static_meshes.rs:461–505`). The geometry pass directly submits shaded batches (`crates/renderer/src/vulkan/context/geometry_pass.rs:33–111`); it has no scene Hi-Z visibility stage. Frustum inclusion does not eliminate objects behind walls.

**Concrete experiment:** prototype raster-only conservative Hi-Z occlusion after suitable opaque depth is available, or room/portal visibility with a conservative fallback. Measure rejected raster work and total cost, including depth generation, synchronization and culling. Camera movement, disocclusion, doors, incomplete bounds, transparent geometry and skinned objects need explicit handling.

Room/portal metadata exists in `crates/plugin/src/esm/cell/mod.rs:475–483,655–660` and is parsed in `cell/walkers.rs:1003–1020`, but has no live visibility consumer. FO4 UVD support is header-only (`crates/bsa/src/uvd.rs:4–14,71–77`): [#3810](https://github.com/matiaszanolli/ByroRedux/issues/3810) closed the header/corpus research spike, not payload decoding or renderer integration.

Retain offscreen TLAS shadow casters and reflectors. Camera raster visibility is not a valid global RT-admission test; secondary rays have different visibility requirements.

### 4. Separate shader footprint from executed ray cost

**Dimension:** GPU Pipeline. **Status:** diagnostic prerequisite to material specialization.

Runtime all-main-rays-off still uses the same compiled shader. Its remaining ~39 ms cannot distinguish ordinary material work from resource allocation imposed by dormant shader paths. Neither shader length nor low VRAM usage proves register pressure, spilling or poor occupancy.

Use the existing isolated-copy workflow in `scripts/rt-decomposition-matrix.sh` to compare runtime all-off against **compile-time all-off mask 8**, with the same pinned resolution, tier, camera and mode used here. The script's defaults need adjustment for this comparison. It was inspected but not run in this investigation. Then collect shader register/local-memory/occupancy counters if supported. A material-specialized common opaque path is justified if that experiment shows a material benefit; ablated lighting itself is not a shipping fix.

### 5. Reduce repeated light-candidate work where compiled evidence supports it

**Dimension:** GPU Pipeline. **Status:** smaller code experiment; benefit unmeasured.

The primary cluster loop (`triangle.frag:3185–3300`) computes direction, attenuation and contribution gates before `shadowableLightRadiance` (`include/lighting.glsl:174–344`) repeats related work. Surface-only energy compensation and anisotropic tangent-frame construction also occur in helper evaluations reused by temporal/spatial reservoirs (`triangle.frag:3518,3640,3779`).

Pass a precomputed light sample into the helper and evaluate invariant surface terms once after final normal/roughness, retaining an index-based wrapper for reuse callers. Preserve back/rim/translucency contributions, weather-adjusted roughness and the NaN-safe tangent guard. Inlining/common-subexpression elimination may already remove some duplication, while hoisting can increase register lifetimes. Inspect compiled results and A/B each change before retaining it.

Secondary priorities need more scene evidence: reflection-hit light scans (`include/lighting.glsl:565`), conservative spotlight-cone cluster tests instead of sphere-only inclusion (`cluster_cull.comp:257–280`), and a certified solid-occluder shadow fast path. Do not truncate light populations or blindly use first-hit termination: cutout/emitter skipping and glass transport require existing nearest-hit semantics.

## Guards, telemetry and completion

- Stable TLAS ordering by BLAS address **and entity identity**, membership-based rebuild and abandoned-recording invalidation remain present in `acceleration/predicates.rs` and `acceleration/tlas.rs`. The prior large TLAS-identity defect is fixed; rebuilding animated BLAS more frequently was already falsified by measurement.
- Dirty palette/skin/refit gates, first-sight initialization, jittered rebuild cadence and submit rollback guards remain present. Current skin/BLAS timings do not justify prioritizing a rewrite.
- Specular-AA work is already hoisted out of the per-light loop; legacy WRS is compile-disabled. These are not new opportunities.
- Current `gpu_timers.rs` has **56 query slots / 28 pairs**, including five geometry phase pairs. Readback respects frame completion and active flags. The skill's 20-bracket inventory is stale, and its claim that opaque draws always write FSR mask attachments does not match current shader/pipeline policy. Neither is evidence of the measured bottleneck.
- No renderer fragment-invocation pipeline-statistics implementation was found. Adding it requires device feature support/enablement and a query lifecycle; timestamps alone cannot prove overdraw, occupancy or ALU/bandwidth saturation.
- Validation performed: successful release build; eleven completed GPU captures; pinned settings and matched MedTek state/counts; source guard review; live issue dedup; harness-provenance check. No production changes, unit-test run, RenderDoc capture or new optimization A/B in this investigation.

Start with candidate 1, measure candidate 2 independently, then use those results to decide between architectural occlusion and shader specialization. Each experiment must retain image correctness and improve matched GPU time beyond observed run variation. There are **zero publishable NEW defects** in this report; `/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-09-26b.md` should not turn these unmeasured candidates into confirmed bug reports.
