# PHYS-D3-04

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2867

---

Found by `/audit-physics` Dimension 3 (ECS Sync). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `crates/physics/src/sync.rs:566-683` (leak window `:607-635` vs check at `:670-679`); `collect_newcomers` re-collect at `:523-528`

## Trigger Conditions
`World::register::<RapierHandles>()` was never called (or the storage was dropped) while `PhysicsWorld` is present and >=1 entity carries `CollisionShape` + `RigidBodyData` + `GlobalTransform`. The engine binary pre-registers at `byroredux/src/boot.rs:504`, so this is an embedder/test-fixture misconfiguration path — but the code explicitly handles it, and handles it in the wrong order.

## Description
The function builds and inserts every newcomer's `RigidBody` (`sync.rs:607-608`) and all its colliders (`:631`) into `pw`, calls `update_query_pipeline()` (`:658`), drops the `PhysicsWorld` guard, and only *then* asks for the `RapierHandles` write query (`:670`):

```rust
// sync.rs:607-635 — bodies + colliders committed to the solver
let body_handle = pw.bodies.insert(body);
...
let handle = colliders.insert_with_parent(collider, body_handle, bodies);
...
drop(pw);                                            // :661
// sync.rs:670-679 — the availability check, four dozen lines too late
let mut handles_q = match world.query_mut::<RapierHandles>() {
    Some(q) => q,
    None => { log::error!("RapierHandles storage missing — call World::register..."); return; }
};
```

On `None` it logs and returns — with the Rapier objects already in `RigidBodySet` / `ColliderSet` and no ECS row pointing at them. Nothing can ever free them: `release_victim_rapier_bodies` (`byroredux/src/cell_loader/unload.rs:446-479`) walks `RapierHandles` and `Ragdoll` rows, and neither exists.

It then **repeats**. `collect_newcomers` only skips an entity when the handles query is `Some` **and** `contains(entity)` (`sync.rs:523-528`); with the storage missing, `handles_q` is `None` and the `if let` never runs, so the identical newcomer set is re-collected, re-cloned (including full `CollisionShape::TriMesh` vertex/index data) and re-inserted on the next tick.

## Impact
A per-frame, per-newcomer leak of Rapier bodies + colliders + broad-phase and query-pipeline BVH proxies, with no recovery path and no bound — the #1520 leak shape on a different trigger.

**Severity note**: held at MEDIUM rather than the HIGH the "resource leak per frame" rule implies, because the trigger is a setup error that also emits a `log::error!` every frame and the shipping binary pre-registers the storage. It is a defense-in-depth ordering bug, not a live production leak.

## Suggested Fix
Hoist the `world.query_mut::<RapierHandles>()` availability check to the top of `register_newcomers` (or, cheaper, have `collect_newcomers` return early when `world.query::<RapierHandles>()` is `None`, since that also fixes the re-collect). Keep the `log::error!`, but emit it before anything reaches the solver.

## Related
- #1520 (CLOSED) — same leak class via cell unload
- `crates/save/.../registry_completeness_tests.rs:218` documents `RapierHandles` as *"self-healing ... physics_sync_system does so automatically"*, which is true only when the storage exists
