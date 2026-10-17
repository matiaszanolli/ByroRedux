# CONC-2026-10-17-C1: Regression of #5535 — `common_shape_rotation` holds a `MeshHandle` read guard across `collect_subtree`'s `Children` acquisition

Labels: medium,concurrency,ecs,bug
Source: docs/audits/AUDIT_CONCURRENCY_2026-10-17.md

- **Severity**: MEDIUM (REGRESSION of #5535; closed issue, fix undone by #5713's refactor `cfabdfd6a`, 2026-10-15)
- **Dimension**: ECS Lock Ordering (CPU half; out of volumetrics scope, found by the lock-order lane)
- **Location**: `byroredux/src/npc_spawn/resumable/runtime.rs:1320-1342` (`common_shape_rotation`)
- **Status**: Regression of #5535 (open-issue snapshot has no tracker; #5713 is the closed refactor that caused it)
- **Description**: #5535 reordered this function so the subtree's shapes were gathered first and every hierarchy guard dropped before composing bind transforms. #5713 then routed the walk through the shared `collect_subtree`, but wrote:
  ```rust
  let meshes = world.query::<MeshHandle>()?;          // read guard alive for the whole block
  collect_subtree(world, part_root)                   // acquires Children inside
      .into_iter().filter(|e| meshes.get(*e).is_some()).collect::<Vec<_>>()
  ```
  That records `MeshHandle -> Children`, the reverse of `tag_descendants_as_actor`'s `Children -> MeshHandle` — exactly the edge #5535's comment says this function must never record. The detector reports `Children -> MeshHandle -> Children`; with the full test list the victims were `female_hair_with_cancelling_root_and_shape_rotations_keeps_actor_axes`, `an_intentional_shape_tilt_survives_the_head_mount`, `male_hair_with_identity_root_and_shape_keeps_actor_axes` (a subset rerun hit `parent_part_only_tags_the_newly_attached_subtree` instead).
- **Trigger Conditions**: `BYRO_LOCK_ORDER_CHECK=1` (CI `lock-order-check`) over the whole bin test set. Not a live deadlock: the caller `align_part_root_to_actor_axes` takes `&mut World`, so no other thread can hold these locks (the skill's "graph cannot tell `&mut World` from `&World`" case). It does turn the hard-gate lane red, which is why it is not LOW.
- **Fix shape**: call `collect_subtree` first (it drops its `Children` guard on return), then take `world.query::<MeshHandle>()` and filter. Pin with the existing lock-order tests; no new test needed.
- **Related**: #5535, #5713, #5032.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other init/teardown sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_CONCURRENCY_2026-10-17.md` (area-scoped `volumetrics-deep` suite run, HEAD e16a624e9). Fix with `/fix-issue <number>`._
