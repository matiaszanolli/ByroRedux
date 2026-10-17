# PHYS-D6-2026-10-16-02: `projectile_flight_system` rebuilds per-entity maps over the whole world every frame a projectile is in flight

Labels: low,physics,performance,bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Queries & Diagnostics (cost)
- **Location**: `byroredux/src/systems/projectiles.rs:196-223` (`owners` Vec, `launcher_bodies` per shooter, `actor_bodies` set), `:303-346` (`launcher_bodies` up to 16 `Parent` lookups per physics entity), `:259-266` (linear `owners.iter().find_map`)
- **Status**: NEW
- **Trigger Conditions**: any `Weapon.Fire` projectile in flight (script-driven: traps, scripted launchers); cost scales with loaded physics entities.
- **Description**: every frame, for each distinct shooter, `launcher_bodies` walks all `RapierHandles` entities with up to 16 `world.get::<Parent>` lock acquisitions each plus a `PhysicsSourceForm` lookup, and `actor_bodies` calls `resolve_actor` (up to 3 `world.get`) for every entity. The launcher's collider set and the actor set are stable across the flight but are recomputed per frame.
- **Evidence**: code structure above (no caching across frames; `launchers` is a local `HashMap`).
- **Impact**: on a 20-95 k-entity exterior, a few hundred thousand storage lock acquisitions per frame for the ~1-2 s of an arrow's flight. Interior cost is small. Not measured.
- **Related**: PHYS-D6-2026-10-16-01 (same pattern), #2923.
- **Suggested Fix**: cache the launcher body list on the `Projectile` (or `ActiveProjectiles`) at launch, and resolve actor-ness only for the bodies a cast returns (the predicate can test `resolve_actor` on demand) instead of precomputing a set over all bodies.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
