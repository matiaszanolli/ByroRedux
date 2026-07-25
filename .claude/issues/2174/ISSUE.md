# 2174: D2-03/PERF-D4-03: New rigid motion-history maps use std::collections::HashMap (SipHash) on the per-draw hot path

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2174
**Labels**: bug, low, performance

---

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
