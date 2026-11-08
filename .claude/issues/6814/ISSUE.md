# CONC-2026-11-08-5.1: probe_spawn_ground takes ContactConfig while holding a PhysicsWorld read guard

_Immutable snapshot of the issue as filed (GitHub is authoritative for state). Source: docs/audits/AUDIT_CONCURRENCY_2026-11-08.md. Labels: low,concurrency,physics,bug_

Found by `/audit-concurrency` 2026-11-08 (`docs/audits/AUDIT_CONCURRENCY_2026-11-08.md`, HEAD 0419437ab).

- **Severity**: LOW (lock-hygiene / sink-rule violation; no cycle possible today)
- **Dimension**: RwLock Patterns (Resource↔Storage, Physics)
- **Location**: `byroredux/src/scene/character_spawn.rs` — `probe_spawn_ground` holds `let pw = world.resource::<PhysicsWorld>()` through the `match pw.cast_ray_down(..)`, whose `Grounded` arm calls `character_spawn_center_y(world, ..)`, which does `world.try_resource::<ContactConfig>()`

## Description
This records a `PhysicsWorld → ContactConfig` edge. `PhysicsWorld` is documented as a sink (last, nothing taken under it; `docs/engine/ecs.md`) and every other consumer of `ContactConfig` snapshots it before taking `PhysicsWorld` (`register_newcomers` in `crates/physics/src/sync.rs`, `ragdoll.rs`, `systems/character.rs`).

## Evidence
Not a cycle today: `ContactConfig` is only ever read via `try_resource` (no writer anywhere), no site holds a `ContactConfig` guard across a `PhysicsWorld` acquire, and `probe_spawn_ground` runs only on the main-thread boot spawn planner with read guards.

## Trigger Conditions
Becomes a real ABBA only if a future writer of `ContactConfig` (e.g. a console tuning command using `resource_mut::<ContactConfig>`) runs on another thread, or a site later holds `ContactConfig` across `PhysicsWorld`.

## Impact
None at present; erodes the sink invariant and adds lock-order-detector graph surface.

## Suggested Fix
Snapshot-then-acquire: compute the kcc offset (or call `character_spawn_center_y`) before taking `pw`, or copy the `cast_ray_down` / `static_colliders_aabb` results into locals, `drop(pw)`, then build `GroundProbe::Grounded`.

Related: #5371, #5556 (same class, fixed elsewhere).

## Completeness Checks
- [ ] **SIBLING**: other `PhysicsWorld` guard holders that call a helper acquiring another resource
- [ ] **LOCK_ORDER**: PhysicsWorld remains a sink after the change
- [ ] **TESTS**: a spawn-path test under `BYRO_LOCK_ORDER_CHECK=1` records no `PhysicsWorld → ContactConfig` edge
