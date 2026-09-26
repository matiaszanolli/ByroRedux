# Performance investigation — MedTek, 2026-09-26

## Executive summary

**Scope:** dense-scene GPU cost, archive/streaming boundaries, and attribution telemetry (performance-audit dimensions 5, 7, 8). This is an investigation, not a shipping optimization or a complete workspace audit.

**Revision:** `e2f99ad55760fd7114ca15d85b3ba068a399edd4`, clean source during captures. RTX 4070 Ti, NVIDIA 580.178.04. Release build with Rust 1.96.0; stale core release artifacts were cleaned before a successful rebuild. No unit tests added or run during this investigation.

MedTekResearch01 reproducibly falls to **2.8 FPS**. Three 300-frame system-live runs averaged **297.5–301.9 ms in the main GPU pass**, versus approximately 7 ms in volumetrics. The dominant measured feature group is direct shadows/ReSTIR, followed by reflection/glass. Disabling these is a diagnostic that changes the image, not a performance fix.

The strongest additional evidence is simulation dependence at matching Native AA resolution: frozen main-pass cost is **60.9 ms**, live cost **297.2 ms**; with all main rays disabled, both cost approximately **35.2 ms**. Investigate ray traversal against animated geometry and the direct visibility estimator before investing in an archive rewrite as an FPS remedy. No individual shader instruction, actor, or commit has yet been established as the root cause.

**Finding status:** one confirmed severe performance hotspot; underlying defect and corrective optimization remain unproven. No new GitHub issues were filed. Existing performance fixes recorded in `PERFORMANCE_FIX_STATUS_2026-09-26.md` were not re-proposed.

**Observed versus ROADMAP:** the historical `4c9a5b36` record uses renderer-stepped/orbit; this reproduction uses system-live/free. The provenance script returned **DIVERGED**, citing `88c23887b` and `8d2a9ebad`. These results do not establish a cross-revision regression percentage. Re-benchmark both revisions in a common configuration before attributing a regression to a commit.

## Measurements and controls

Raw summaries: [BENCH_medtek_2026-09-26.tsv](BENCH_medtek_2026-09-26.tsv). Full local logs and command arrays: `/tmp/medtek-profile/`.

- One engine at a time, Xvfb, 1280×720 output, default starting camera facing `(0,0,-1)` near `(-1536,120,-768)`.
- Three 300-frame system-live baseline runs: wall-time median **357.37 ms**, MAD **0.52 ms**, min/max **355.74/357.89 ms**, peak-to-peak **0.6%**. State hashes differ as expected with wall-clock simulation.
- Baseline selected the saved `render.upscaler = "fsr3/native-aa"`, not the CLI parser's FSR Quality default. `main.rs::install_universal_settings` applies saved settings unless the upscaler is explicitly selected. Native AA render size is 1280×720; froxels are 160×90×64.
- Ablations explicitly selected upscaler, resolution, and ray tier **0**. Tier 0 already disables GI; GI-off would provide no additional isolation.
- All runs count frames from initial rendering; no separate warmup exclusion. The long baseline's slow-frame tail confirms that the collapse persists beyond startup. Short ablation averages include startup/transient frames.
- The live ablations are one 90-frame run per variant. They hold configuration and camera but are **not identical simulation snapshots**: elapsed simulation time and particle draws differ. Treat their large differences as feature-group sensitivity, not additive pass costs or production speedups.
- Frozen FSR Quality comparisons ran twice in opposite order. All frozen captures have the same state hash, `5550b70b11365888`, 39,560 entities, 286 lights, and 13,038 TLAS instances.

### Native AA: live ray-family ablations

| Variant | Main GPU ms | Wall ms | Volumetrics ms |
|---|---:|---:|---:|
| Baseline | 297.152 | 357.94 | 6.303 |
| All main rays disabled, `0x40000000` | 35.173 | 94.53 | 5.045 |
| Reflection/glass disabled, `0x20000000` | 249.996 | 307.22 | 6.058 |
| Direct shadows disabled, `0x08000000` | 110.457 | 171.10 | 5.242 |

Direct-shadow disabling also bypasses the ReSTIR path; the difference does not isolate traversal alone. Main-ray flags do not disable volumetric rays. Savings are not additive because control flow, visibility sampling, GPU scheduling, and live simulation state differ.

Reproduction command (change the debug flag for each diagnostic variant):

```sh
env -u WAYLAND_DISPLAY -u GDK_BACKEND -u BYROREDUX_FIXED_DT \
  XDG_SESSION_TYPE=x11 RUST_LOG=info BYROREDUX_RENDER_DEBUG=0 \
  xvfb-run --auto-servernum target/release/byroredux \
  --game fo4 --cell MedTekResearch01 --window-size 1280x720 \
  --upscaler fsr3 --fsr-quality native-aa --rt-test-ray-quality-tier 0 \
  --bench-frames 90 --bench-mode system-live
```

Frozen comparisons replace `system-live` with `renderer-static`; the reduced-resolution captures also replace `native-aa` with `quality`.

### Frozen simulation: resolution and ray controls

| Render configuration | Main GPU ms, baseline | Main GPU ms, all main rays disabled |
|---|---:|---:|
| Native AA, 1280×720, one run each | 60.904 | 35.266 |
| FSR Quality, 853×480, two runs each | 29.980 / 27.827 | 20.034 / 17.895 |

FSR Quality frozen direct-off measured 22.173/21.202 ms; reflection-off measured 29.083/29.287 ms. Those results must not be substituted for the live Native AA measurements.

Native AA has approximately 2.25× as many render pixels as FSR Quality. The shader's derivative-based RT LOD also changes with resolution, so ray cost need not scale linearly with pixel count.

### Other hot paths

The first long live capture averaged: build_render_data 4.09 ms, scheduler 28.88 ms, instance/scene work 2.24 ms, CPU TLAS preparation 0.74 ms, GPU TLAS build 0.029 ms, SVGF 0.512 ms, cluster cull 0.184 ms, sky cube 0.205 ms, and volumetrics 7.132 ms. Main rendering was 301.896 ms.

CPU submit/present was 320.35 ms; this is a host interval that includes waiting/throttling and must not be added to GPU time. Large main-pass timestamps independently establish substantial GPU work. Cheap TLAS construction does not imply cheap subsequent ray traversal.

A device-wide sample showed 65% GPU utilization, 3,125 MiB resident memory, 44°C and 2,850 MHz graphics clock. Desktop idle memory was 1,137 MiB. This is not engine-only VRAM or an occupancy measurement. Low allocated RAM/VRAM does not establish spare shader, traversal, cache, or bandwidth capacity.

## Source-backed candidates, not proven causes

1. **Live geometry and shadow traversal.** `shaders/include/shadow_transport.glsl::traceShadowTransmittanceDetailed` performs closest-hit opaque/alpha traversal with up to eight continuation layers, followed by up to four glass interfaces. Current masks include animated actors. Frozen and live no-ray costs being nearly equal makes live ray work the highest-priority discriminator. There is no current actor-only exclusion flag or alpha-continuation counter. `rt.masks` gives a population census; `render.debug probe x y` gives one selected ray's first hit and mask.
2. **Skinned BLAS quality under refits.** `acceleration/constants.rs` selects FAST_BUILD plus ALLOW_UPDATE. Dirty poses refit; rebuilding is count-based at 600–660 refits. At three FPS this can span minutes. Measure an actor-mask diagnostic, then fresh rebuild versus refit before changing policy. Historical comments record regressions from blindly selecting FAST_TRACE, including on MedTek.
3. **Visibility policy broadened since the historical record.** `3ce970a5a`, `d54382415`, and `b9e961eeb` broaden imported-light masks to actors, props, foliage and glass. `f97775ca8` restores ordinary alpha-blended shadow casters. These are correctness changes; reverting them would reintroduce lighting leaks. The shadow-transport loop and primary clustered-light algorithm themselves are unchanged across that comparison range. No commit regression is established here.
4. **Secondary lighting scans.** `include/lighting.glsl::reflectionHitIrradiance` scans all submitted lights, selects four candidates, and can perform multiple visibility queries. `pathHitRadiance` also scans all lights, but its GI path is disabled at measured tier 0. These scans predate the historical record. Secondary candidate acceleration is a separate optimization candidate, not the primary measured feature group.
5. **Residual raster/shader cost.** All-main-rays-off still costs approximately 35 ms at Native AA. This retains the compiled shader and its register footprint. A compile-time ray-free variant and a GPU capture would be needed to separate shader specialization, material ALU, overdraw, and bandwidth. No occupancy claim is made from shader size alone.

## Archive and streaming assessment

FO4+ uses **BA2**, through the shared BSA/BA2 provider. Existing concurrency already includes a background cell coordinator, a dedicated Rayon NIF parse/import pool, and decompression outside both archive file locks. The completed BA2 lock fix (#3659) is present.

Remaining opportunities verified in source:

- `streaming.rs::pre_parse_cell` serially extracts/inflates all uncached NIFs before parallel parse/import begins. Extraction and parsing do not overlap across that barrier.
- `asset_provider/texture.rs::resolve_texture_view_with_clamp` still extracts new texture data on the caller thread before DDS upload enqueueing.
- `cell_loader/precombined.rs::PrecombinedSpawnJob::advance` can extract `_oc.nif`, open CSG data, and prepare materials on the main thread. One indivisible unit can exceed the cooperative apply budget.
- `texture_registry/upload.rs::flush_upload_batch` uses a synchronous completion fence before reclaiming staging and publishing descriptors. The recent dynamic RGBA/HUD change does not make fresh DDS uploads asynchronous.

A useful archive upgrade would overlap packed reads, inflate, and parsing using **byte-bounded queues**, preserve archive precedence/canonical dedup, cancel obsolete cell generations, prefetch texture and CSG payloads, and retain GPU staging until upload completion. Merely adding an async facade does not remove synchronous work. Measure queue wait, packed reads, inflate, parse/import, apply, and upload waits separately. The current worker_parse aggregate includes extraction/decompression.

These changes target loading and movement hitches. They cannot by themselves remove the measured live main-pass GPU workload.

## Prioritized next work

1. Capture live Native AA at fixed simulation snapshots and collect visibility-mask populations. Add a diagnostic that excludes actors only from **direct** shadow queries, preserving raster actors and TLAS content; never ship that as the fix.
2. If actor traversal dominates, compare fresh BLAS builds against refits and inspect posed geometry/bounds. Otherwise instrument opaque/alpha continuation and glass traversal separately from ReSTIR reuse.
3. Implement and compare a correctness-preserving optimization at fixed tier/resolution/state, with image comparison and GPU validation. No shipping speedup is claimed by this report.
4. Investigate the remaining approximately 35 ms ray-free main-pass cost and the approximately 29 ms live CPU scheduler cost.
5. Upgrade archive/texture/precombine streaming against measured movement-hitch evidence with bounded memory and correct upload lifetimes.

## Stale audit premises

- Current GPU timers allocate 46 query slots, **23 brackets**; the audit skill and module prose still mention 20. The current table includes groundcover models and separate volumetric inject/integrate brackets. Parent volumetrics and its children overlap and must not be summed.
- `atw_pre`, `atw_scheduler`, and `atw_post` are sequential siblings; `atw_post` contains the render_one_frame phases. The skill's older nesting statement is wrong.
- The skill's blanket assertion that static BLAS recovery skips every set that cannot fit its budget is stale relative to existing recorded design changes.
- Benchmark tools that omit explicit upscaler settings inherit persisted preferences. Record or override those settings before comparing captures.

The codebase graph service returned `Transport closed`; source discovery used filesystem fallback. Two read-only dimension agents assisted; all GPU captures ran serially under the primary agent.
