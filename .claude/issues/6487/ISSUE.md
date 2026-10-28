# PHYS-D3-2026-10-27-01: PushActorAway's push is discarded: `apply_impulse` on a multibody link is overwritten by rapier's solver

Labels: medium, physics, bug, game:fo3, game:fnv
Source: docs/audits/AUDIT_PHYSICS_2026-10-27.md

# PHYS-D3-2026-10-27-01: PushActorAway's push is discarded: `apply_impulse` on a multibody link is overwritten by rapier's solver

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-27.md` finding `PHYS-D3-2026-10-27-01` (severity MEDIUM, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `byroredux/src/systems/knockdown.rs` (`knock_down`, the per-body `physics.apply_impulse(*handle, direction * speed * mass)` loop); `crates/physics/src/world/mod.rs:1185-1199` (`PhysicsWorld::apply_impulse`).

## Description
`knock_down` activates the ragdoll, then gives every body `direction * speed * mass` through `PhysicsWorld::apply_impulse`, which calls `RigidBody::apply_impulse` (adds `impulse * inv_mass` to `vels.linvel`, rapier 0.36.1 `rigid_body.rs:1451-1459`). Every ragdoll body is a multibody link (the dynamic root through its free joint). At the start of each island solve rapier rebuilds every link's `vels` from the multibody's generalized velocities (`staged_island_solver/helpers.rs:108` -> `Multibody::update_velocities`, `multibody.rs:735-766`); nothing reads `rb.vels` back into those generalized velocities, so the impulse is lost. The physics crate already records this fact in a test comment (`world/mod.rs` ~:2429-2431: "a multibody LINK's velocity is solver-owned") and #5540 already routes link damping through force/torque for the same reason; `apply_impulse` has no such route and still returns `true`.

## Evidence
Probe (scratch crate, rapier 0.36.1, the repo's `PhysicsWorld`): two dynamic balls get `apply_impulse(+X * 300 * mass)` each, then 30 steps at 1/60 s. Free pair: `dx = (150.0, 150.0)`. The same pair joined by a multibody `SphericalJoint`: `dx = (0.0, 0.0)`. Both fall the same distance under gravity (y 100 -> 7.6), so the step does run. Code at HEAD: `apply_impulse` only calls `b.apply_impulse(impulse, true)` for `Dynamic` bodies, then `forget_spawn_sleep` + `wake`, returns true.

## Impact
Every ObScript `PushActorAway` (FO3/FNV quests and traps; the GECK says "Fred will slide past Buddy") collapses the actor in place instead of throwing it. A negative force (a pull) also does nothing. The wake side effect still happens, so the bug is invisible in logs, and `knock_down` logs "knocked down ... at {speed} u/s". Any future caller pushing a ragdoll this way (impact/explosion knockback on a corpse) inherits the same silent no-op. The knockdown tests cover direction maths and refusal paths only; none asserts that a body moved.

## Related
#5540 (link damping via force/torque), #6230 (knockdown builds the ragdoll first), #6231.

## Suggested Fix
Give `PhysicsWorld` a link-aware impulse: for a multibody link convert the impulse to a force applied for one substep (`impulse / PHYSICS_DT`, then cleared), or write the root's free-joint DOFs (`FREE_ROOT_DOFS`, linear first) through the multibody's generalized velocity. Make `apply_impulse` return `false` or delegate on links. Add a test that a jointed ragdoll actually moves (mass > 0, one step before waking - see the physics-test dynamic-body recipe).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites
- [ ] **TESTS**: A regression test pins this specific fix

