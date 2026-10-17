# PHYS-D2-2026-10-16-01: The #5544 spawn re-sleep swallows a user wake (and zeroes the velocity) of a newcomer that has not yet had a stepping frame

Labels: low,physics,bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/world/mod.rs:968-1002` (`resleep_spawned_newcomers`, drift test at 984, `body.sleep()` at 987); producer `crates/physics/src/sync.rs:1108-1111`; consumer path `byroredux/src/systems/cinematic.rs:376` (`ApplyHavokImpulse` -> `PhysicsWorld::apply_impulse`, `world/mod.rs:1094-1105`)
- **Status**: NEW (the skill's "confirm a same-frame wake cannot be swallowed" check; read from code and rapier source, not probe-measured)
- **Trigger Conditions**: a dynamic prop that streamed in but has had no stepping frame since (a quiescent cell: no animated actor pushing kinematic targets, nothing else armed `pending_wake`), then a script `ApplyHavokImpulse` (M47.2, 696da4f47) whose resulting speed moves the body less than `SPAWN_SLEEP_DRIFT_BU` (4 BU) during that frame, i.e. below roughly 240 BU/s over one 1/60 s substep.
- **Description**: `spawn_sleep_pending` is filled at registration and drained only by a stepping frame. `apply_impulse` wakes the body and arms `pending_wake`, so the next step runs and integrates the impulse. After the step, `resleep_spawned_newcomers` re-sleeps every pending body that is dynamic, awake and within 4 BU of its spawn pose. That test cannot tell rapier's begin-touch strong-wake (the thing it exists to undo) from a deliberate engine wake. Only the water pass's targets are exempt (`water_force_targets`). Rapier's `RigidBody::sleep()` also zeroes the linear and angular velocity (rapier3d-0.36.1 `rigid_body.rs:915-918`), so the impulse is erased, not merely paused.
- **Evidence**: `grep spawn_sleep_pending` shows exactly one push (sync.rs:1110) and one `take` (mod.rs:969); neither `apply_impulse` nor `set_motion_type` removes the handle. The new unit test `spawn_resleep_drops_only_settled_newcomers_force_events` covers settled-vs-drifted only, with no engine-wake case.
- **Impact**: a gentle scripted nudge on the first stepping frame after an object streams in is lost once (the pending list is then consumed, so a repeat works). Strong impulses (above ~3.4 m/s) are unaffected. No instability, no state corruption.
- **Related**: #5544, #5700, #3969 (spawn wake exemption), M47.2 `ApplyHavokImpulse`.
- **Suggested Fix**: drop the handle from `spawn_sleep_pending` (or its drift reference) whenever an engine path deliberately wakes it (`apply_impulse`, `set_motion_type(wake_up)`, `add_force(wake_up=true)`), or exempt bodies whose velocity was set by the engine in the same frame.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
