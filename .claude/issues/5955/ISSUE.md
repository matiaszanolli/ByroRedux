# CONC-2026-10-17-C2: `projectile_flight_system` resolves actor ownership (storage reads) while holding the `PhysicsWorld` read guard

Labels: medium,concurrency,gameplay,physics,bug
Source: docs/audits/AUDIT_CONCURRENCY_2026-10-17.md

- **Severity**: MEDIUM (NEW, introduced by `99997e0e4` "perf(gameplay): cache projectile launcher bodies", 2026-10-16)
- **Dimension**: RwLock Patterns (Resource <-> Storage) / ECS Lock Ordering (out of volumetrics scope)
- **Location**: `byroredux/src/systems/projectiles.rs:228-262` (`is_actor_body` closure -> `resolve_actor` -> `world.get::<ActorColliderOwner>` / `world.get::<ActorVitals>`, called from the `cast_ray_corridor_where` filter while `let physics = world.try_resource::<PhysicsWorld>()` is alive)
- **Status**: NEW
- **Description**: Before `99997e0e4` the system built `actor_bodies` for every owner "before the physics world is taken" (the comment above `owners` still says the lock order is resolve ownership first). The perf change made actor resolution lazy and moved it into the ray-cast body filter, i.e. under the `PhysicsWorld` guard. `PhysicsWorld` is a sink in `docs/engine/ecs.md`'s canonical order (acquired last, nothing beneath it; #5371, #5556). Detector output: `ActorVitals -> ActorValues -> GlobalTransform -> PhysicsWorld -> ActorVitals`, failing `a_same_frame_melee_hit_survives_a_projectile_hit`, `flight_into_an_actor_queues_a_hit_and_into_a_wall_only_ends`, `flight_ignores_the_launchers_cached_body`.
- **Trigger Conditions**: lane only. The system is registered with `add_exclusive(Stage::Update, ...)` (`boot/schedule/update.rs:569`), which runs serially after the parallel batch, so there is no live second thread to complete the cycle today. It still makes the sink rule false and would become a real ABBA the moment it is demoted into the parallel batch or any physics-guarded caller is added.
- **Fix shape**: snapshot-then-acquire. Keep the lazy cost saving but resolve outside the guard: take the `PhysicsWorld` guard only for the cast, collect candidate bodies from the cast, or precompute the actor-body set once per frame from `owners` before the guard (the pre-`99997e0e4` shape, which was O(owners) once per frame, not per projectile). Do not hold the guard and "be careful".
- **Related**: #5371, #5556, #5535.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other init/teardown sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_CONCURRENCY_2026-10-17.md` (area-scoped `volumetrics-deep` suite run, HEAD e16a624e9). Fix with `/fix-issue <number>`._
