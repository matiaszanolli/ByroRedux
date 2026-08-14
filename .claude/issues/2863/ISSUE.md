# PHYS-D2-02

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2863

---

Found by `/audit-physics` Dimension 2 (Step Determinism & Budget). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `crates/physics/src/world.rs:186-197`

## Trigger Conditions
A sleeping dynamic body is supported by (or in contact with) a collider whose body is removed via `PhysicsWorld::remove_body`, while `active_dynamic_bodies()` is empty and nothing else arms `pending_wake` that frame.

## Description
Every other mutator on `PhysicsWorld` that can change the world's contact state calls `self.wake()`:
- `add_force` (`:257`), `apply_impulse` (`:275`), `set_motion_type` (`:321`)
- `set_linear_velocity` (`sync.rs:56`), `set_kinematic_translation` (`sync.rs:90`), `push_kinematic` (`sync.rs:738`)
- the WATAL dry->wet transition (`water.rs:471-473`), `build_ragdoll` (`ragdoll.rs:263`)

`remove_body` does not:

```rust
// world.rs:186-197 — no self.wake()
pub fn remove_body(&mut self, handle: RigidBodyHandle) -> bool {
    self.bodies.remove(handle, &mut self.islands, &mut self.colliders,
                       &mut self.impulse_joints, &mut self.multibody_joints,
                       /* remove_attached_colliders = */ true).is_some()
}
```

Rapier *does* wake the neighbours of a removed collider, but only inside `NarrowPhase::handle_user_changes` -> `remove_collider` (`rapier3d-0.22.0/src/geometry/narrow_phase.rs:271-312`), which runs **inside `pipeline.step()`**. With the static-scene fast path engaged there is no step, so the removal is never processed and the supported body hangs in mid-air. `ColliderSet::remove` only wakes the collider's **own parent** — the body being deleted — and pushes the handle onto `removed_colliders` for the next step to consume.

Live callers: `byroredux/src/cell_loader/unload.rs:474` (cell unload), `byroredux/src/ragdoll.rs:393` (ragdoll teardown / re-activation), `crates/physics/src/ragdoll.rs:482`.

## Impact
Clutter left floating after the thing it rested on is unloaded or a ragdoll is deactivated; exterior cell-boundary unloads can strand clutter in the still-loaded neighbouring cell. Cosmetic-to-gameplay, self-heals as soon as anything else wakes the sim — hence MEDIUM, not HIGH. Also removes the guarantee that `removed_colliders` is drained promptly.

**Strictly worse in combination with PHYS-D2-01**, which can make "anything else wakes the sim" never happen.

## Suggested Fix
```rust
let removed = self.bodies.remove(...).is_some();
if removed { self.wake(); }
removed
```
One step then drains `removed_colliders` and lets Rapier wake the real neighbours through its own path.

## Related
- PHYS-D2-01 (the fast-path wake stall this compounds)
