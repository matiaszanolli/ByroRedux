# 3266: CONC-D5-2026-08-24-03: dump_awake_fallers and spawn_collider_census_report hold storage guards across FormIdPool acquisition

**Severity**: LOW · **Report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-24.md` (CONC-D5-2026-08-24-03)

## Description

Both functions snapshot `PhysicsWorld` and drop that guard before opening ECS storages (citing #2136 explicitly), but invert the same discipline for the resource↔storage pair: `RenderLayer`+`FormIdComponent`+`PhysicsSourceForm` read guards stay open across `world.try_resource::<FormIdPool>()`.

## Location

`crates/physics/src/sync.rs:311-349` (`dump_awake_fallers`) and `crates/physics/src/sync.rs:594-624` (`spawn_collider_census_report`)

## Impact

Read-only both sides, `FormIdPool` has no runtime writer — no deadlock today. Precedent concern: these are the two functions that document lock discipline explicitly, and they model it correctly for one pair while inverting it for another. `spawn_collider_census_report` is reachable via the `phys.census` debug console command (an un-owned audit surface).

## Related

#2136; #3265 (CONC-D5-2026-08-24-02, same inversion shape, hot path).

## Suggested Fix

Resolve `FormIdPool` lookups into an owned snapshot before the storage guards open, or resolve form ids in a second pass after they drop.

## Completeness Checks
- [ ] **LOCK_ORDER**: `FormIdPool` acquisition moved outside the storage-guard span
