# CONC-SD-01: `subtree_has_skinned_mesh` takes `SkinnedMesh` before `Children`, against the canonical order, and the lane cannot see it

Labels: medium,concurrency,bug

**Severity**: MEDIUM (per the report). `_audit-severity` lists "ECS deadlock potential" as HIGH minimum, but the report's premise is that the only caller owns `&mut World`, so no second thread can hold the opposite order and there is no live deadlock. Rated like the 10-17 C-1 (same `&mut World`, same class). The table's letter would say HIGH; the report's stated rating is kept.
**Dimension**: ECS Lock Ordering & Deadlock (Dim 3), reached from the NPC spawn tail of `cell_loader` streaming
**Location**: `byroredux/src/npc_spawn/resumable/runtime.rs:1277-1284` (`subtree_has_skinned_mesh`); sole caller `runtime.rs:1164-1166` inside `spawn_shared_skeleton_part` (`:1122`, takes `world: &mut World`)

## Description
`docs/engine/ecs.md` section "Canonical acquisition order" (`:595-606`) fixes `... Parent -> Children -> GlobalTransform -> SkinnedMesh -> MeshHandle ...`, and `systems/bounds.rs:133-148` follows it (`Children`, `GlobalTransform`, then `SkinnedMesh`). The function does the reverse:
```rust
let skinned = world.query::<byroredux_core::ecs::SkinnedMesh>();     // runtime.rs:1280, guard live
byroredux_core::ecs::collect_subtree(world, root)                    // acquires Children inside, :1281
    .into_iter()
    .any(|entity| skinned.as_ref().is_some_and(|q| q.get(entity).is_some()))
```
This is the 10-17 C-1 / #5954 shape (`common_shape_rotation` held `MeshHandle` across `collect_subtree`), left behind in the function directly above it. #5954 fixed one of the two #5713 call sites. The same SkinnedMesh-then-Children order existed in the pre-#5713 body (`cfabdfd6a^`), so #5713 kept it rather than introducing it.

Flow: NPC spawn job (`NpcSpawnJob::advance`) -> `spawn_shared_skeleton_part` (FO3/FNV only: `head_parts_use_head_bone_mount(GameKind::Fallout3NV)`, labels `hair` / `eyebrow HDPT` / `eye mesh` / `head sub-part`) -> `subtree_has_skinned_mesh`.

## Evidence
No test reaches it: the function has one non-doc reference (the call at `:1166`); `spawn_shared_skeleton_part` needs real NIFs, and no test in `npc_spawn/tests.rs` or `resumable/mod.rs` calls it. That is why the green 3035-test lane (`BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux`) does not contradict this. A scratch probe crate with the exact body reproduces the detector verdict:
```
canonical order recorded                      # Children -> SkinnedMesh, as bounds.rs does
ECS cross-thread deadlock risk (lock-order cycle): attempted acquisition of `...hierarchy::Children`
while holding `...skinned_mesh::SkinnedMesh` ... `Children` -> `SkinnedMesh` -> `Children`
```
Trigger: debug build, `BYRO_LOCK_ORDER_CHECK=1`, an FO3/FNV cell with an NPC whose head parts take the head-bone mount path, with bounds having already recorded `Children -> SkinnedMesh` in an earlier frame. Release builds and runs without the env var are unaffected. A real cross-thread ABBA would need this function to run on a worker thread while `bounds` runs in the parallel batch; today it cannot (it needs `&mut World`).

## Impact
A developer or CI lane that runs a real FNV/FO3 session under the detector aborts at NPC head spawn (a panic, not a warning); and it is one more site that makes the sink-and-order rules unreliable, the pattern behind #4982, #5535, #5954, #6062. Latent hang risk only if the spawn tail is ever moved off `&mut World`.

## Related
#5954 (closed, same shape, sibling function), #5535 (closed), #5713 (closed, the refactor), #6062 (open, same fix idiom).

## Suggested Fix
Collect first, then take the guard, exactly as `common_shape_rotation` (`runtime.rs:1338-1345`) and the #6062 fix do: `let subtree = collect_subtree(world, root); let Some(skinned) = world.query::<SkinnedMesh>() else { return false }; subtree.into_iter().any(|e| skinned.get(e).is_some())`. Add a test with `Children` and `SkinnedMesh` registered and the canonical order recorded first, because a fixture without the `SkinnedMesh` storage records no edge (`query` returns `None`), the #5372 blind-spot class.

Source: docs/audits/AUDIT_CONCURRENCY_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SIBLING**: Same pattern checked in related files (other per-placement components absent from the eviction row)
- [ ] **TESTS**: A regression test pins this specific fix
