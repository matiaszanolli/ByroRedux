# 2134: CONC-D5-01: PhysicsWorld before GlobalTransform in follow/escort/travel/guard inverts b5e38c22's established order

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2134
**Labels**: bug, high, sync

---

## Severity
HIGH

## Dimension
RwLock Patterns (Resource↔Storage, Physics) — `/audit-concurrency` 2026-07-25

## Location
- `byroredux/src/systems/follow.rs:106,134`
- `byroredux/src/systems/escort.rs:143,186`
- `byroredux/src/systems/travel.rs:140,90`
- `byroredux/src/systems/guard.rs:121,73`
- versus `crates/physics/src/sync.rs:539-543` (`push_kinematic`) and `byroredux/src/ragdoll.rs:385-390`

## Description
`b5e38c22` normalized `GlobalTransform`-before-`PhysicsWorld` acquisition in `ragdoll.rs` to match `physics_sync_system`'s `push_kinematic`, but four AI locomotion systems acquire the pair in the opposite order and were not touched by that fix. Each holds `world.try_resource::<PhysicsWorld>()` live across a Pass-1 loop while calling `world.get::<GlobalTransform>(target)` (a tracked read lock) inside that scope, then passes the still-held physics guard on to `step_toward`.

Confirmed against current code: `follow.rs:106` binds `physics`; `follow.rs:134` calls `world.get::<GlobalTransform>(target_entity)` with `physics` still in scope. Same shape in `escort.rs:143→186`, `travel.rs:140→90` (via `resolve_destination`), `guard.rs:121→73` (via `resolve_anchor`). The reverse edge is recorded in `sync.rs`'s `push_kinematic` (`GlobalTransform` read at line 539, then `PhysicsWorld` write at 543) and `ragdoll.rs:385-390` (`GlobalTransform` write, then `PhysicsWorld` read).

## Evidence
```rust
// follow.rs:106
let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();
// ... (physics still held) ...
// follow.rs:134
let Some(target_gt) = world.get::<GlobalTransform>(target_entity) else { ... };
```
```rust
// crates/physics/src/sync.rs push_kinematic — reverse order
let Some(global_q) = world.query::<GlobalTransform>() else { ... };  // line 539
let mut pw = world.resource_mut::<PhysicsWorld>();                   // line 543
```

## Impact
1. Latent cross-thread ABBA: `ragdoll_writeback_system` holds a `GlobalTransform` **write** guard while waiting on `PhysicsWorld`; any of these four systems holds `PhysicsWorld` read while waiting on `GlobalTransform` read. Add a thread queued for a `PhysicsWorld` write (`register_newcomers`, `set_linear_velocity`, `apply_buoyancy`) and a real 3-way cycle closes — a hard hang.
2. Immediate and non-hypothetical: a debug build with `BYRO_LOCK_ORDER_CHECK=1` against any cell with a `FollowBehavior`/`EscortBehavior`/`TravelBehavior`/`GuardBehavior` NPC that resolves a live target will abort with "ECS cross-thread deadlock risk (ABBA)" on the first tick.

## Trigger Conditions
A live `PhysicsWorld` resource (any cell load) plus at least one actor with a resolvable follow/escort/travel/guard target/anchor. All four unit tests for these systems deliberately run **without** a `PhysicsWorld` resource (`try_resource` returns `None`), which is why `BYRO_LOCK_ORDER_CHECK=1` in CI has never seen this pair (see CONC-D4-NEW-01, filed separately).

## Related
`b5e38c22`, `crates/core/src/ecs/lock_tracker.rs::global_order::record_and_check`, same class as CONC-D5-02 (different pair, filed separately) and CONC-D4-NEW-01/-03 (CI coverage gaps that explain why this was never caught).

## Suggested Fix
In all four systems, resolve targets/anchors and snapshot the needed `GlobalTransform.translation` values into scratch structs **before** `let physics = world.try_resource::<PhysicsWorld>()`, so no `world.get::<GlobalTransform>`/`resolve_entity_by_global_form_id` call happens under the physics guard. Add a regression test per system that installs a real `PhysicsWorld` resource before calling `*_system_inner` under `BYRO_LOCK_ORDER_CHECK=1`.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SIBLING**: Same pattern checked in related files (other AI-procedure systems: sandbox.rs, wander.rs, patrol.rs)
- [ ] **TESTS**: A regression test pins this specific fix (per-system, with a real `PhysicsWorld` installed, under `BYRO_LOCK_ORDER_CHECK=1`)
