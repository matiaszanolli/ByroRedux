# PHYS-D7-02

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2873

---

Found by `/audit-physics` Dimension 7 (Queries & Diagnostics). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `byroredux/src/systems/locomotion.rs:49-70` (`step_toward`); `crates/physics/src/world.rs:563-588` (`cast_ray_down`); `byroredux/src/npc_spawn.rs:169-198` (`keyframe_live_ragdoll_bones`)

## Trigger Conditions
One of the six env-gated locomotion AI systems is enabled (`BYRO_WANDER` / `BYRO_TRAVEL` / `BYRO_FOLLOW` / `BYRO_ESCORT` / `BYRO_GUARD` / `BYRO_PATROL`), a `PhysicsWorld` resource exists, and the moving actor is a live NPC whose skeleton NIF authored bhk ragdoll bodies (Oblivion / FO3 / FNV / Skyrim — the classic-chain games where the bone bodies actually decode).

Not reachable in the default scheduler, which is the only reason the severity is not higher.

## Description
`step_toward` ground-snaps the actor by casting straight down from `(new_pos.x, current.y + 256, new_pos.z)` and assigning the hit Y to `new_pos.y`. The cast is `PhysicsWorld::cast_ray_down`, whose only filter is `exclude_dynamic`.

`keyframe_live_ragdoll_bones` (#1698) deliberately flips every live actor's ragdoll bone from `Dynamic` to `Keyframed` *before* first registration, so each bone registers as a `KinematicPositionBased` Rapier body with a real collider — its own doc puts this at *"~18 bones/NPC"* and *"~480+ such bodies across a dense interior"*.

`exclude_dynamic` does **not** filter kinematic bodies, and the ray origin is 256 BU directly above the actor's root, so the first thing the ray meets on its way down is the actor's **own** upper-body bone collider, not the floor.

There is no way for the caller to fix this locally: `cast_ray_down` accepts no exclusion, and `step_toward` receives only `Option<&PhysicsWorld>` — it is handed neither the actor's `EntityId` nor its `RapierHandles`.

## Evidence
Empirically confirmed with a throwaway test against the real `PhysicsWorld` (reverted after running):

```
// Fixed floor cuboid, top surface y=1.0, plus a KinematicPositionBased ball(6) at y=120
// (stand-in for a keyframed head/spine bone), ray from y=256 as locomotion.rs does:
locomotion ground ray -> Some(126.0)     // its own bone, not the floor at 1.0
```

`locomotion.rs:64-66` then does `new_pos.y = ground_y`, writing the actor's *root* to what is really its own bone's top surface.

## Impact
An actor under any of the six locomotion procedures is re-seated each tick at its own bone height rather than the ground. Because the bones are children driven from the actor's animated `GlobalTransform` (via `push_kinematic`), the whole rig rises with the root, so the next tick's ray hits the bone again from an even higher origin — **a monotonic elevator, not a one-off offset**. Symptom is NPCs ascending out of the cell while "walking". Also silently corrupts every `Travel`/`Escort` arrival test that depends on real ground Y.

## Suggested Fix
Thread the moving actor's `RigidBodyHandle` into `step_toward` and pass it to an exclusion-aware `cast_ray_down` (the same signature change PHYS-D7-01 needs — fix them together).

Excluding a single body handle is **not sufficient on its own** — each bone is a *separate* body. The cleanest fix is a `QueryFilter` predicate that rejects colliders whose parent body resolves to an entity under the actor's skeleton root, or (cheaper) an interaction-group / collision-group bit for "actor bone" that ground probes mask out.

## Related
- PHYS-D7-01 (same missing-exclusion root cause, different caller)
- `docs/audits/AUDIT_CONCURRENCY_2026-07-16.md:72,170` previously reviewed `step_toward`'s `cast_ray_down` for lock ordering only, never for filter correctness
