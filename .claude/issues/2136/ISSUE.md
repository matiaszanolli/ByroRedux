# 2136: CONC-D5-03: dump_awake_fallers holds PhysicsWorld while acquiring RapierHandles, inverting every other site in the crate

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2136
**Labels**: bug, medium, sync

---

## Severity
MEDIUM

## Dimension
RwLock Patterns (Resource↔Storage, Physics) — `/audit-concurrency` 2026-07-25

## Location
`crates/physics/src/sync.rs:237-266` (`dump_awake_fallers`)

## Description
The #1698 awake-faller diagnostic takes `world.resource::<PhysicsWorld>()` at line 240 and holds it to the end of the function, then acquires `RapierHandles`, `RenderLayer`, `FormIdComponent`, `PhysicsSourceForm`, `FormIdPool` underneath it — the opposite order from `push_kinematic`/`pull_dynamic`/`apply_buoyancy`, all of which acquire `RapierHandles` before `PhysicsWorld`.

## Evidence
`sync.rs:240` — no scope, no `drop`, on `PhysicsWorld`; `sync.rs:251` acquires `RapierHandles` underneath it → edge `PhysicsWorld → RapierHandles`. Reverse edge from `sync.rs:539,543`. Both occur inside the same `physics_sync_system` call on one thread, so the same-thread tracker won't fire, but the cross-thread graph (`global_order::record_and_check`) will, since `push_kinematic` runs earlier in the same frame and already recorded the forward edge.

## Impact
Doubly gated (`BYRO_PROFILE_FALLERS` env var + one-shot `AtomicBool` + ≥16-awake-body floor), so lower likelihood than the two HIGH findings (#2134, #2135), but the diagnostic an operator reaches for during a settle-storm investigation will, on a debug build with the order checker on, panic the frame instead of dumping — and the panic poisons `PhysicsWorld`'s `RwLock`, taking the rest of the session down. It also permanently seeds a `PhysicsWorld → RapierHandles` edge in the process-wide graph, which then makes unrelated later acquisitions panic too.

## Trigger Conditions
`BYRO_PROFILE_FALLERS` set, ≥16 awake dynamic bodies, first occurrence in the process. Deadlock (rather than panic) additionally needs a second `Stage::Physics` system.

## Related
#1698 (closed), `b5e38c22`, `byroredux/src/boot.rs:894-902` (the Access declaration already acknowledges this hidden read surface). Same finding class as CONC-D5-01/-02 (#2134, #2135).

## Suggested Fix
Collect the awake-body snapshot (handle → translation.y, linvel().y pairs) into a `Vec` under the `PhysicsWorld` guard, `drop(pw)`, then open `RenderLayer`/`FormIdComponent`/`PhysicsSourceForm`/`FormIdPool`. Restores `RapierHandles → PhysicsWorld` ordering and shortens the hold on the hottest resource in the engine.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
