# PHYS-D2-2026-09-29-01: #4687(b)'s same-frame query refresh is a no-op — a restored body is invisible to queries for the rest of the frame, and the guard is vacuous (regression of #4687)

**Labels**: low,bug,physics,test-gap,game:fo3

**Source**: `docs/audits/AUDIT_PHYSICS_2026-09-29.md`
**Severity**: LOW
**Dimension**: Step & Sync
**Location**: `crates/physics/src/world.rs` — `refresh_query_geometry_after_restore` and the guard `restored_pose_is_visible_to_ray_queries_same_frame`

Regression of #4687 (closed): part (b) of that fix — the same-frame query refresh after an explosion restore — never took effect.

**Trigger Conditions**: any per-substep explosion recovery (`restored > 0`). The FO3 restored-corpse path (#4772) hits it on every restore.

## Description
The fix propagates the restored poses to the colliders (correct), then calls `query_pipeline.update_incremental(&colliders, &touched, &[], false)`. In rapier 0.22, `refit_and_rebalance = false` runs only `qbvh.pre_update_or_insert` (`rapier3d-0.22.0/src/pipeline/query_pipeline/mod.rs`), which marks the leaf dirty; leaf AABBs are refit only when the flag is `true`. `PhysicsPipeline::step` passes `true` on its final CCD substep, so right after `pipeline.step` the tree holds the EXPLODED AABB. After the restore, BVH culling never reaches the collider at its restored pose.

The guard reproduces the "explosion" with `update_incremental(…, false)` too, so its tree never holds the exploded AABB. It proves only the collider-position propagation and stays green with the production `update_incremental` line deleted.

## Evidence
Scratch probe (real recovery via `step` + `cast_ray` through the public API; three explosion kinds: linvel 3e5, angvel `f32::MAX`, linvel `f32::MAX`):
```
[finite jump] steps=1 recovery=(1, 1, 0) body_y=50 finite_rot=true
[finite jump] same frame: ray@restored body -> Some(1.0); ray@floor tile x=300 -> Some(1.0)
[finite jump] next frame: ray@restored body -> Some(52.0)
```
The ray straight down through the restored body (crown at y=52) returns the floor below it (1.0). The stale active set forces one more step next frame, whose final refit consumes the dirty leaf.

## Impact
For one frame after a recovery, rays, KCC sweeps and LOS pass through the restored body (pre-#4687 they hit it at the exploded pose; now they miss it entirely). Neighbouring static colliders were not poisoned. Small effect, but #4687 is closed as fixed and the audit-physics skill's Dim 2 text asserts the invariant.

## Related
#4687 (closed), #4685 (the incremental design this depends on), #4772.

## Suggested Fix
Pass `true` (refits and rebalances only on a recovery frame), or `mark_colliders_dirty()` and let the post-loop fallback rebuild. Make the guard index the exploded pose with `update_incremental(…, true)` as rapier does, or drive a real recovery through `step`. Then fix the audit-physics skill Dim 2 sentence.

Validated at HEAD 9fcfdc3fc: `refresh_query_geometry_after_restore` calls `update_incremental(&self.colliders, &touched_colliders, &[], false)`; the guard also calls `update_incremental(…, false)` for the exploded pose; rapier 0.22 `update_incremental` refits only when `refit_and_rebalance` is true.

## Completeness Checks
- [ ] **SIBLING**: other post-step `update_incremental(…, false)` call sites that expect same-frame visibility
- [ ] **TESTS**: the guard drives a real recovery (or indexes the exploded pose with `true`) and fails with the fix removed
