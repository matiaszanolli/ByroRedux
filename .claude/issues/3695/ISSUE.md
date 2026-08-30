# #3695 — ECS-2026-08-30-D1-01: scene_centroid_distance inverts the canonical GlobalTransform → MeshHandle order, closing a live 2-cycle against build_render_data

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: Lock Ordering & Deadlock
**Location**: `byroredux/src/app_step.rs` (`App::scene_centroid_distance`, ~:406-411); opposing edge at `byroredux/src/render/static_meshes.rs` (~:99-100)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D1-01)

## Description

`docs/engine/ecs.md` fixes one process-wide acquisition order for the hierarchy/skinning/bounds cluster:

```
CharacterController -> RapierHandles -> Transform -> Parent -> Children ->
GlobalTransform -> SkinnedMesh -> MeshHandle -> LocalBound -> WorldBound ->
Name -> StringPool
```

("skipping types is fine, reordering them is not").

`App::scene_centroid_distance` acquires `MeshHandle` first and then `GlobalTransform`, holding both across the centroid loop — the inverse of that order, and the inverse of the edge `build_render_data`'s static-mesh pass establishes on the same two types. Neither guard is scoped or dropped between acquisition and use, so both edges are real observations for the `BYRO_LOCK_ORDER_CHECK` graph.

## Evidence

```rust
// byroredux/src/app_step.rs:410-411 — MeshHandle -> GlobalTransform (inverted)
let meshes = self.world.query::<MeshHandle>()?;
let globals = self.world.query::<GlobalTransform>()?;
```

```rust
// byroredux/src/render/static_meshes.rs:99-100 — GlobalTransform -> MeshHandle (canonical)
let tq = world.query::<GlobalTransform>();
let mq = world.query::<MeshHandle>();
```

Both sites are reachable in one process: `scene_centroid_distance` is called from `measure_bench_subject_distance` (`byroredux/src/app_step.rs:390`) and the bench-camera orbit setup, and a `--bench-frames` / `--bench-hold` run also renders frames through `build_render_data`.

## Impact

No live deadlock — both sites run on the main thread and cannot overlap. The concrete cost is the one `docs/engine/ecs.md` warns about directly: "an inverted pair that is *safe* still aborts a debug build once both sites run." A debug build with `BYRO_LOCK_ORDER_CHECK=1` (the CI lock-order job, #1410 / #2137) panics on whichever observation lands second, taking down a bench run for a non-bug. It also erodes the one invariant that makes future promotion of either site to a parallel system safe by construction.

## Related

- #3445 (`studio_host` `Name -> StringPool` inversion, OPEN, MEDIUM — same class, same non-scheduler blast radius), #3446, #2388
- #3260 / #3303 are the scheduler-side HIGH precedents
- **#3580** (`combat_approach_line_of_sight_reaches`, `PhysicsWorld -> RapierHandles`) is a **different** cycle in the same detector — filed separately by the concurrency audit. Fixing one does not fix the other.

## Suggested Fix

Swap the two acquisitions in `byroredux/src/app_step.rs` so `GlobalTransform` is taken first, matching `byroredux/src/render/static_meshes.rs`. One-line change, no behaviour difference.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other `MeshHandle`/`GlobalTransform` pairs, other non-scheduler acquisition sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
