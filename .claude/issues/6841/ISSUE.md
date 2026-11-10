# PERF-2026-11-09-D1-02: two small heap allocations per frame on the volumetrics seam (`published` barrier Vec in `dispatch`, `candidates` Vec in `build_composite_params`)

Labels: low,renderer,performance,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: LOW (derived; no quantitative guard exists for either site). **Status**: NEW.

**Location**: (a) `VolumetricsPipeline::dispatch` Stage F in `crates/renderer/src/vulkan/volumetrics.rs`: `let published: Vec<vk::ImageMemoryBarrier> = std::iter::once(post_int).chain(local_published.into_iter().flatten()).collect();`; (b) `build_composite_params` in `crates/renderer/src/vulkan/context/frame_params.rs`: `let mut candidates: Vec<(f32, CompositeSkyAperture)> = Vec::new();` in the interior portal-sky branch.

## Description
(a) The barrier list is always exactly four entries (one plus the grid's `[_; 3]`, or one when the grid is absent) and is built into a fresh `Vec` on every dispatching frame. (b) `candidates` is a new `Vec` per frame in cells with a portal-outdoor sky, grown by doubling to the aperture count (#6753 removed the *second* vector there and left this one).

## Impact
Roughly a handful of allocator calls per frame, well under a microsecond each. It matters only because its neighbours (`sources`, `fog_cluster_refs_scratch`, the moved-surface list) were each given a persistent scratch for exactly this reason (hot-path rule: per-frame allocations that should be persistent).

## Suggested Fix
(a) A `[vk::ImageMemoryBarrier; 4]` with a slice length; (b) a `ScratchBuffers` Vec cleared and refilled, with a `ctx.scratch` row and the existing shrink.

Related: #6831 (REN-D8-09, same seam, which the performance audit measured at ~9 us/call). Report: `docs/audits/AUDIT_PERFORMANCE_2026-11-09.md`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (scratch telemetry row for the new Vec: `fill_scratch_telemetry_covers_every_scratch_field_in_the_crate`)

