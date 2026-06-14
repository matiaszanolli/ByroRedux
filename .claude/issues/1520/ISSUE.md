# D7-RAPIER-LEAK: Cell unload leaks Rapier bodies/colliders (despawn frees ECS row, never the physics body)

**Severity**: HIGH
**Dimension**: ECS audit dim 7 — Streaming / component lifecycles
**Source**: docs/audits/AUDIT_ECS_2026-06-14.md
**Status**: NEW (verified end-to-end)

## Description
The streamed-cell path creates physics entities carrying `CollisionShape::TriMesh` + `RigidBodyData` + `GlobalTransform` (the `synthesize_static_trimesh` ghost, used for static architecture meshes without a bhk collider). `physics_sync_system` (registered in `byroredux/src/main.rs:821`, runs every frame) registers each newcomer into `PhysicsWorld.bodies` (`RigidBodySet`) + `PhysicsWorld.colliders` (`ColliderSet`) and attaches `RapierHandles`.

On unload, `unload_cell` despawns the victim entities with `world.despawn(eid)` — which removes only the ECS component rows, including `RapierHandles`, whose handle is dropped **without being used**. Nothing removes the body/collider from the Rapier sets.

## Evidence
- `PhysicsWorld` (`crates/physics/src/world.rs`) has **no** removal method (API: `new`, `body_count`, `awake_counts`, `wake`, `step`, `update_query_pipeline`, `cast_ray_down`, `static_colliders_aabb`, `move_character`).
- Repo-wide grep for `remove_rigid_body` / `bodies.remove` / `colliders.remove` / `remove_collider` → **zero** matches.
- `byroredux/src/cell_loader/unload.rs:179-182`: `for eid in victims { world.despawn(eid); }` — no physics-release pass (unlike the mesh/texture/terrain/skin-slot/item-instance release passes earlier in the same fn).
- Ghost spawn: `byroredux/src/cell_loader/spawn.rs:1062-1081` (+ `synthesize_static_trimesh` at `:93`). Ghosts spawn inside the cell `first..last` span → get `CellRoot`/`CellRootIndex` membership → collected as unload victims.
- Registration: `crates/physics/src/sync.rs:262,285,330-332`.

## Impact
Every cell crossing leaks one Rapier `RigidBody` + colliders per static trimesh ghost (plus bhk/keyframed bodies). Cadence is per-cell-crossing (not per-frame) but monotonic and unbounded over a session — worst for exterior radius streaming, which has no full `PhysicsWorld` reset. Dead fixed bodies also stay in the broad-phase / `QueryPipeline` BVH, so the per-frame physics step + query-pipeline rebuild cost climbs with total cells ever visited (session-length CPU regression).

## Suggested Fix
Add `PhysicsWorld::remove_body(&mut self, handle)` calling `RigidBodySet::remove(body, &mut islands, &mut colliders, &mut impulse_joints, &mut multibody_joints, /*wake_dependents=*/true)` (Rapier cascades attached colliders). In `unload_cell`, before the despawn loop, read each victim's `RapierHandles` and call it, gated on `try_resource_mut::<PhysicsWorld>()` (matches the optional-resource pattern used for the item-instance pool). Add a regression test asserting `pw.body_count()` returns to baseline after a load→unload cycle. The character capsule (`scene.rs`, `CharacterKinematic`) is World-scoped (no `CellRoot`) and correctly excluded.

## Completeness Checks
- [ ] **LOCK_ORDER**: new `PhysicsWorld` write in unload — verify it doesn't overlap a held `RapierHandles`/`CollisionShape` query lock (read victim handles first, then acquire `PhysicsWorld`)
- [ ] **SIBLING**: same release needed for bhk-collider ghosts and keyframed/dynamic bodies, not just static trimesh ghosts
- [ ] **DROP**: confirm `RapierHandles` carries no `Drop` that already frees (it does not today)
- [ ] **TESTS**: regression test mirroring `unload_skin_cleanup_tests` — `body_count()` back to baseline after load→unload
- [ ] **SIBLING**: verify interior full-reset path (if any) and exterior incremental path both release
