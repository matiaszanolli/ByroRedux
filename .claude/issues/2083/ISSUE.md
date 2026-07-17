# FNV-D7-01: activate_ragdoll has no re-activation guard — second trigger leaks the first ragdoll's Rapier bodies

- **Severity**: MEDIUM (elevation risk to HIGH once gameplay-triggered re-hit reaches this path)
- **Labels**: medium, memory, bug
- **Location**: `byroredux/src/ragdoll.rs:204-318`; trigger `byroredux/src/commands/scene.rs:690-703`; cleanup `byroredux/src/cell_loader/unload.rs:418`

## Description
`activate_ragdoll` never checks whether the actor is already `RagdollActive`. A second activation re-seeds from current (simulated) bone poses, builds a fresh full set of Rapier bodies/colliders/multibody joints, and `insert`s a new `Ragdoll` component that overwrites (drops) the first one's handles without calling `PhysicsWorld::remove_ragdoll` — which is only ever invoked from cell unload over *live* components. The orphaned first set (~18 bodies + ~17 joints for a humanoid) is never freed and keeps simulating at the same bone positions, fighting the live solver.

## Evidence
`ragdoll.rs:204` `activate_ragdoll` builds a fresh `RagdollSpec`/Rapier body set unconditionally and does `query_mut::<Ragdoll>().insert(actor, ragdoll)` (overwriting any existing `Ragdoll` component) with no check for `RagdollActive`/existing `Ragdoll` beforehand. `PhysicsWorld::remove_ragdoll` exists (`crates/physics/src/ragdoll.rs`) but is never called from `activate_ragdoll`, and `commands/scene.rs`'s ragdoll-trigger command calls `activate_ragdoll` directly with no prior-state check either.

## Impact
Today reachable only via the console `ragdoll <id>` command (slice-1 scope, caps at MEDIUM). Elevation risk: PHYSAL rollout step 4 (death/hit-react AI triggers) would make a second hit on an already-ragdolled actor a per-event leak, escalating to HIGH.

## Suggested Fix
Make activation idempotent — precheck `RagdollActive`/existing `Ragdoll` and either early-return or call `remove_ragdoll` on the existing component before rebuilding. Add a unit test asserting body count doesn't grow on double-activation.

## Completeness Checks
- [ ] **SIBLING**: Check other Rapier-body-creating entry points (e.g. cell-load ragdoll spawn, if any) for the same missing-idempotency pattern
- [ ] **TESTS**: A regression test pins this specific fix (double-`activate_ragdoll` on the same actor, assert Rapier body/joint count doesn't grow and old handles are freed)
