# D6-02: Pose hash is committed at build_render_data time — a draw_frame early return freezes the RT skinned pose while the dirty gate reads "clean"

**Issue**: #1796
**Labels**: medium,renderer,vulkan,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (D6-02)

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D6-02)

## Location
`byroredux/src/render/skinned.rs:152,180` (clear + mark = the hash commit) + `crates/renderer/src/vulkan/context/draw.rs:2118` (early return) + `draw.rs:1711-1715` (consumer skip gate)

## Description
`try_mark_pose_dirty(entity, hash)` records the new pose hash the moment `build_skinned_palettes` runs — CPU-side, in `build_render_data`, before `draw_frame`. The commit is `render/skinned.rs:180` (into `last_pose_hash` on the ECS `SkinSlotPool`, `resources.rs:949-966`); `draw.rs:1711` is the consumer (`pose_dirty.contains(entity)` skip gate), and there is no `last_pose_hash` write anywhere in `crates/renderer/src`. If `draw_frame` early-returns before the skin dispatch (swapchain out-of-date @2118, empty framebuffers), the dispatch never runs but the hash baseline has already advanced. Sequence: frame N-1 dispatches pose P1 (H1); frame N computes P2 → H2 recorded dirty; `draw_frame` N early-returns; frame N+1 the NPC stops → H2 matches stored H2 → gate reads "not dirty" → dispatch + refit skipped with `has_populated_output == true`. The slot output and skinned BLAS stay at P1 while the raster palette (recomputed every frame from `bone_world`) shows P2.

## Evidence
`skinned.rs:180` runs unconditionally in `build_render_data`; grep for `last_pose_hash` in the renderer crate = 0 hits, so nothing rolls it back when `draw_frame` fails to reach the skin section.

## Impact
RT shadows/reflections/GI of the affected NPC freeze at a pose one-plus frames stale relative to the rasterized body, persisting through the idle period after the lost frame. Self-healing on next movement; no crash, no leak.

## Related
D6-01 (same root cause), #1195.

## Suggested Fix
Same transactional shape as D6-01 — stage the frame's pose hashes and fold into `last_pose_hash` only after `draw_frame` confirms the skin section ran, or re-insert the frame's dirty set on early-return paths.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

