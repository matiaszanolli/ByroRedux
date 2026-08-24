# 3256: ECS-2026-08-24-08: resolve_cached_waypoints has no navmesh-residency invalidation

**Severity**: LOW (latent) · **Report**: `docs/audits/AUDIT_ECS_2026-08-24.md` (ECS-2026-08-24-08)

## Description

The waypoint-cache hit arm keys only on goal distance, with no notion of which `NavmeshTile` produced the path or whether that tile is still resident. Combined with "cache the empty result too" and a `0.0` threshold on a frozen goal: an empty `NavPath` cached on the first bad tick is never retried, even after the relevant tile streams in.

## Location

`byroredux/src/systems/navmesh_path.rs:343-362`; consumers `travel.rs:229-231`, `guard.rs:187`, `escort.rs:237/276/299`, `wander.rs:286`, `patrol.rs:124`, `follow.rs:215-221`

## Impact

Verified NOT currently reachable through the normal streaming path — NAVM tiles are always spawned before their own actors, and cross-tile destinations can't path yet (Phase 2 cross-tile search unbuilt). Becomes reachable once Phase 2 lands, NAVM tiles gain independent eviction, or an actor is teleported into a non-resident tile while holding a live frozen-goal state.

## Related

Adjacent to open umbrella `#2372`, not covered by it.

## Suggested Fix

Add a residency epoch — a `u64` bumped in `spawn_navmesh_tiles`/`unload_cell`, stored on `NavPath`, compared in the cache-hit arm.

## Completeness Checks
- [ ] **TESTS**: A regression test for residency-epoch invalidation once reachability preconditions land
