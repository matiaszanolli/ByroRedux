## Issue 2172 [OPEN] PERF-D1-02: collect_lights builds a fresh decorate-sort Vec every frame instead of a caller-owned scratch
labels: bug low performance 

## Severity
LOW

## Dimension
CPU Hot Paths (Dim 1) — `/audit-performance` 2026-07-25

## Location
`byroredux/src/render/lights.rs` (`collect_lights`)

## Description
`collect_lights` allocates a fresh `Vec` each frame for its GI-priority decorate-sort instead of reusing a caller-owned scratch buffer, unlike the other Session-46 scratch-reuse guards in the same file family (`AnimScratch`, `drain_dirty_into`).

## Impact
Redundant allocation in a hot per-frame path. Low absolute cost (light counts are small relative to instance counts) but avoidable.

## Related
Session-46 scratch/gating guards (all re-verified intact this sweep).

## Suggested Fix
Thread a caller-owned scratch `Vec` through `collect_lights` and clear+reuse it, matching the pattern already established for `AnimScratch` and friends.

## Completeness Checks
- [ ] **TESTS**: Existing light-collection tests should still pass unchanged after the scratch-reuse refactor

---
## Issue 2174 [OPEN] D2-03/PERF-D4-03: New rigid motion-history maps use std::collections::HashMap (SipHash) on the per-draw hot path
labels: bug low performance 

## Severity
LOW

## Dimension
Draw-Call & Instancing Efficiency / SSBO Sizing & Per-Frame Upload — `/audit-performance` 2026-07-25 (independently flagged by both Dim 2 and Dim 4, same code, merged into one finding)

## Location
`crates/renderer/src/vulkan/context/mod.rs:38,1079,1081,2745-2746`; hot loop at `crates/renderer/src/vulkan/context/draw.rs:1491-1501`

## Description
`previous_rigid_models` and `current_rigid_models_scratch` (introduced by `33d9a468`) are `std::collections::HashMap<u32, [f32;16]>` — the default SipHash-1-3 hasher — hit once for `.get()` and once for `.insert()` per rigid draw, per frame. The renderer already standardizes on `rustc_hash::FxHashMap` for exactly this shape of per-frame hot map (`material.rs:929`, `context/mod.rs:510`, `scene_buffer/descriptors.rs:303`); `33d9a468` reintroduced SipHash at a new site after #1368 (closed) closed removing it elsewhere.

## Impact
~2 SipHash probes per rigid draw per frame — ~2.4K on Prospector (1224 draws), ~29K on MedTek (14535 draws). Estimated tens of us/frame at MedTek scale; not a bottleneck, purely avoidable CPU. Allocation behaviour is already correct (the maps are `mem::take`n, cleared, and swapped — no per-frame heap churn), so hashing is the only remaining cost.

## Related
#1368 (closed, same anti-pattern, different site); the `FxHashMap` precedent at `material.rs:31,929,971`; the PERF-D4-01 issue (filed separately — same call site, different bug: that one is the entity-ID collision/identity problem, this is the hash-function-choice problem; fixing one doesn't fix the other).

## Suggested Fix
Change both fields to `rustc_hash::FxHashMap<u32, [f32; 16]>` and update the two `HashMap::new()` construction sites — the crate dependency and the in-crate precedent both already exist; one-line-per-site change.

## Completeness Checks
- [ ] **TESTS**: Existing motion-history tests (`current_and_previous_rigid_models_share_current_render_origin`) should pass unchanged after the hasher swap

---
## Issue 2175 [OPEN] D2-04: Water pass rebinds pipeline + 3 descriptor sets once per water plane instead of once per pass
labels: bug low performance 

## Severity
LOW

## Dimension
Draw-Call & Instancing Efficiency (Dim 2) — `/audit-performance` 2026-07-25

## Location
`crates/renderer/src/vulkan/water.rs` (per-plane draw loop)

## Description
The water render pass rebinds its pipeline and 3 descriptor sets on every distinct water plane in the current scene, rather than binding once per pass and iterating planes with only the per-instance push-constant/UBO updated in between.

## Impact
Extra `vkCmdBindPipeline`/`vkCmdBindDescriptorSets` calls per water plane. Cell-dependent blast radius (most interiors have 0-1 water planes; some exteriors have several), so cost is bounded but avoidable.

## Related
None — standalone finding.

## Suggested Fix
Hoist the pipeline/descriptor-set bind out of the per-plane loop; bind once per pass and iterate planes with per-instance data only.

## Completeness Checks
- [ ] **TESTS**: Existing water-pass tests should verify unchanged visual output after hoisting the bind calls

---
## Issue 2178 [OPEN] PERF-D3-03: FrameUpscaler::create_outputs leaks its gpu-allocator sub-allocation if bind_image_memory fails
labels: bug low performance 

## Severity
LOW

## Dimension
GPU Memory Pressure (Dim 3) — `/audit-performance` 2026-07-25

## Location
`crates/renderer/src/vulkan/frame_upscaler.rs` (`create_outputs`)

## Description
`FrameUpscaler::create_outputs` doesn't free its `gpu-allocator` sub-allocation if `bind_image_memory` fails, unlike the established pattern elsewhere in the renderer (e.g. `exposure.rs`) that frees the allocation on a subsequent bind failure.

## Impact
Unreachable except on driver OOM / genuine allocation failure — a narrow error path, but a real leak if it fires (the sub-allocation is never returned to the allocator's free list).

## Related
Same shape exists in `gbuffer.rs` per the audit's own note — fix both together.

## Suggested Fix
Free the `gpu-allocator` sub-allocation on the `bind_image_memory` failure branch in `FrameUpscaler::create_outputs`, mirroring `exposure.rs`'s pattern. Apply the same fix to the equivalent site in `gbuffer.rs`.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked and fixed in `gbuffer.rs`

---
