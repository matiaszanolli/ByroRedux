# PHYS-D3-02

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2866

---

Found by `/audit-physics` Dimension 3 (ECS Sync). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `crates/physics/src/sync.rs:786-794` (write target); `byroredux/src/scene/nif_loader.rs:493-504` + `:529-537` (the parented producer)

## Trigger Conditions
A NIF imported through `load_nif_scene_hierarchical` (i.e. `load_nif_bytes` / `load_nif_bytes_with_skeleton`) in which a **non-root** `NiNode` carries a `bhkCollisionObject` whose body resolves to `MotionType::Dynamic`, **and** the node's parent chain composes to a non-identity global.

`PhysicsWorld` is present unconditionally in the binary (`byroredux/src/boot.rs:451`), so `cargo run -- mesh.nif` — the headline usage in `CLAUDE.md`/`README.md` — reaches this.

## Description
`register_newcomers` seeds the Rapier body from `GlobalTransform` (`sync.rs:585-587`), so Rapier's pose is world-space by construction. `pull_dynamic` reads that world-space isometry back and assigns it to the **local** `Transform` (`sync.rs:791-793`). For a root entity that is correct (propagation copies root local -> global verbatim). For a parented entity it is not: the next `make_transform_propagation_system` pass composes `parent_global . local` (`crates/core/src/ecs/systems.rs:191-205`), applying the parent chain a second time.

The cell-loading path avoids this deliberately and says so — `spawn_collision_shapes` spawns parentless ghosts with an explicit comment naming the hazard (`byroredux/src/cell_loader/spawn.rs:1099-1104`: *"adding `Parent` would either double-transform it under propagation, or ... orphan it"*). The hierarchical NIF loader has no equivalent guard: it attaches `CollisionShape` + `RigidBodyData` to the node entity at `nif_loader.rs:502-503`, then unconditionally inserts `Parent(parent_entity)` on every non-root node at `:529-537`.

The NPC-spawn consumer of the same loader is protected only by accident: `keyframe_live_ragdoll_bones` (`byroredux/src/npc_spawn.rs:187-198`) flips `Dynamic -> Keyframed` for entries in `skel_map` — **named skeleton bones only**. A dynamic bhk node that is not a named skeleton bone is not covered.

## Impact
The rendered/propagated pose of such a body diverges from its simulated pose by the full parent-chain transform. It is a fixed offset (no runaway — Phase 2 only reads `GlobalTransform` for `Keyframed`, so there is no feedback loop), but the object is drawn in the wrong place for as long as it exists, and the local/world contract on `Transform` is violated. No crash, no leak.

## Suggested Fix
In `pull_dynamic`, resolve the parent's `GlobalTransform` and store `parent_global^-1 . world_pose` into the local `Transform` (falling back to the raw world pose for roots). Or, if parented dynamic bodies are meant to be unsupported, reject them at `register_newcomers` with a one-shot `log::warn!` and a debug assertion, plus a matching guard in `nif_loader.rs` mirroring `keyframe_live_ragdoll_bones`. Either way pin it with a test: parent -> child with a dynamic body, one tick, assert the child's `GlobalTransform` equals the Rapier pose.

## Related
- PHYS-D3-01 (same write site)
- `docs/engine/physics.md:135-141` documents the write target as "the local `Transform`" without stating the parentless precondition — the invariant is real but unwritten and unenforced (see PHYS-D3-05)
