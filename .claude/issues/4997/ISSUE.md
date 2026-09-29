# #4997: CONC-D5-2026-09-28-02: Nothing pins that `register_newcomers`' rayon section runs with no World guard live and no `&World` inside the closure

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,concurrency,physics,test-gap,bug
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: LOW (latent; no current trigger)
- **Dimension**: RwLock Patterns (Resource↔Storage, Physics)
- **Location**: `crates/physics/src/sync.rs:1005-1043` (`register_newcomers`; `into_par_iter` at 1035-1041, `resource_mut::<PhysicsWorld>` at 1043)
- **Status**: NEW. The pattern was introduced by ad1d53a11 (today).
- **Description**: The conversion itself is clean (see Verified clean §1). The closure captures only the owned `Newcomer` and `&cfg`, a `Copy` snapshot of `ContactConfig` taken at `:1010-1013` whose guard dies in `.map(|r| *r)`. No World guard is live at the `par_iter`.

  Two properties are load-bearing, though, and the lock-order detector cannot see either:
  1. **Cross-thread waits are invisible.** Pool workers run with an empty thread-local held set. A future closure that touched `world` (for example, reading `ContactConfig` per newcomer, or `ActorBoneCollider`), while the caller held a conflicting guard, would block across threads. This shape does not record an edge. It is exactly the blind spot #313 / O-1 describe.
  2. **The waiting thread can run other jobs.** `physics_sync_system` itself runs on a rayon worker (the scheduler's `par_iter_mut`, `scheduler.rs:500-503`). While it waits in `collect`, rayon may run another pool job on the same thread. Today Stage::Physics has one parallel entry, so there is nothing else to steal. If a system joins Stage::Physics and a guard is later hoisted above the `par_iter` (for example, moving `let mut pw = …` up), the stolen system could try to take that lock on the thread that already holds it, and `std` RwLock is not reentrant.

  Neither is true at HEAD. Only the ordering of the source lines stands between this code and both hazards.
- **Evidence**: Current order at `sync.rs:1010-1043`:
  1. `try_resource::<ContactConfig>().map(|r| *r)`: guard dropped
  2. `newcomers.into_par_iter().map(|n| collision_shape_to_parts(&n.shape, n.global.scale, &cfg)).collect()`: no guard live, no `world`
  3. `resource_mut::<PhysicsWorld>()`

  The same function is also reached from the main thread via `register_newcomers_and_refresh_queries`. Callers were checked at `scene.rs:1037`, `systems/character.rs:941` and `commands/view.rs:377`: none holds a guard at the call. The `if try_resource::<PhysicsWorld>().is_some()` at `view.rs:457-459` is a plain `if` condition, so its temporary drops before the body runs.
- **Trigger Conditions**: A future edit that moves any `world.*` acquisition into the closure, or hoists a guard above line 1035.
- **Impact**: Latent. It would be a cross-thread deadlock or self-deadlock that the `BYRO_LOCK_ORDER_CHECK` lane cannot catch.
- **Verification Path**: Read `sync.rs:1005-1043`.
- **Related**: #313 (tracker cannot see cross-thread ABBA), O-1, #2404 (snapshot-then-acquire discipline).
- **Suggested Fix**: Add a `source_scan` test next to `physics_diagnostics_resolve_forms_after_storage_guards_drop`. It should assert two things about the `into_par_iter` closure body in `register_newcomers`:
  - the closure body contains no `world`;
  - the `resource_mut::<PhysicsWorld>()` offset is greater than the `.collect()` offset that ends the parallel map.

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D5-2026-09-28-02) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related systems / CI steps
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition and the `docs/engine/ecs.md` canonical order are preserved
- [ ] **TESTS**: A regression test pins this specific fix
