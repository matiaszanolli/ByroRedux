# #2404 — CONC-D5-01: `push_kinematic`/`pull_dynamic` hold Storage read guards across a `PhysicsWorld` resource guard, relying on an unenforced, single-comment convention rather than any structural lock-order guard

- **Severity**: MEDIUM
- **Domain**: sync
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2404


- **Severity**: MEDIUM
- **Dimension**: 5 — RwLock Patterns (Resource↔Storage, Physics)
- **Location**: `crates/physics/src/sync.rs:688-740` (`push_kinematic`), `crates/physics/src/sync.rs:744-795` (`pull_dynamic`)
- **Status**: NEW

**Description**

TypeId-sorting does not cover the Resource↔Storage pair, so no `resource_mut::<PhysicsWorld>()` guard may be held across a `query`/`query_mut` iteration and vice-versa. `collect_newcomers`→`register_newcomers` and `apply_buoyancy` both honor this literally (storage reads collected into an owned `Vec`, guards dropped, *then* the `PhysicsWorld` guard is taken). `push_kinematic` and `pull_dynamic` do not: both acquire `RapierHandles`/`RigidBodyData`(/`GlobalTransform`) read guards at the top of the function and keep them alive for the entire body/loop while a `PhysicsWorld` guard (`resource_mut` in `push_kinematic`, `resource` in `pull_dynamic`) is also held — the two lock domains overlap in scope. The only place in the crate documenting "storage-before-resource, consistently" as the actual deadlock-avoidance convention is a comment inside the unrelated `dump_awake_fallers` diagnostic (sync.rs:240-245); it is not stated anywhere `push_kinematic`/`pull_dynamic` themselves live, is not enforced by `lock_tracker`'s always-on same-thread reentrancy check (which is same-lock reentrancy, not cross-lock ordering), and is only checked by the global lock-order graph when `BYRO_LOCK_ORDER_CHECK=1` is explicitly set.

**Evidence** (re-confirmed at publish time against commit `79bfc76e`):

```rust
// sync.rs:688-740 (push_kinematic) — storage reads held across resource_mut
fn push_kinematic(world: &World) {
    let Some(handles_q) = world.query::<RapierHandles>() else { return; };
    let Some(body_q) = world.query::<RigidBodyData>() else { return; };
    let Some(global_q) = world.query::<GlobalTransform>() else { return; };
    let mut pw = world.resource_mut::<PhysicsWorld>();   // ← taken while the three above are still alive
    for (entity, handles) in handles_q.iter() { ... }
}
```
```rust
// sync.rs:744-795 (pull_dynamic) — same pattern, resource read this time
fn pull_dynamic(world: &World) {
    let Some(handles_q) = world.query::<RapierHandles>() else { return; };
    let Some(body_q) = world.query::<RigidBodyData>() else { return; };
    let mut updates = Vec::new();
    { let pw = world.resource::<PhysicsWorld>(); for (entity, handles) in handles_q.iter() { ... } }
    drop(handles_q); drop(body_q);
}
```

Contrast the compliant sibling in the same file, `register_newcomers` (sync.rs:661): `drop(pw);` happens *before* `world.query_mut::<RapierHandles>()` is taken — full separation, not merely consistent ordering.

**Impact**

Not currently exploitable — `physics_sync_system` is the sole system on `Stage::Physics` (parallel or exclusive), stages execute with a hard barrier between them, the one other `PhysicsWorld`-writing parallel system (`player_controller_system`, `Stage::Early`) fully declares its access so the conflict analyzer would flag a future same-stage collision, and the one other `PhysicsWorld`-touching Late-stage consumer (`ragdoll_writeback_system`) is `add_exclusive`, never in the parallel batch. The risk is latent: nothing but an implicit, single-comment convention prevents a future second parallel `Stage::Physics` system (or a `ragdoll_writeback_system` promoted to parallel) from acquiring `PhysicsWorld` first and then opening `RapierHandles`/`RigidBodyData`/`GlobalTransform`, completing the ABBA cycle against `push_kinematic`/`pull_dynamic` — the same failure class `dump_awake_fallers` was fixed for under #2136, just not generalized.

**Trigger Conditions**: A future scheduler change adds a second parallel `Stage::Physics` system, or promotes a `PhysicsWorld`-touching exclusive system to parallel, without mirroring the storage-before-resource order; only observable as a hang, and only reliably caught under `BYRO_LOCK_ORDER_CHECK=1` (off by default in normal `cargo test`/CI).

**Related**: #2136 (the `dump_awake_fallers` fix confirming this is a known, previously-debugged concern in this exact file); thematically parallel to the already-open #2270 ("scripting's 'snapshot before iterate' lock discipline is undocumented as a house rule") — same failure class, different subsystem.

**Suggested Fix**: Either (a) mirror `apply_buoyancy`'s pattern — collect `(entity, handles)`/`(entity, body_data)` into an owned `Vec` under the read guards, drop them, then take `PhysicsWorld` alone; or (b) if the overlap is kept for its performance benefit, promote the "storage always acquired and held before `PhysicsWorld`" rule out of the one `dump_awake_fallers` comment into a crate-level doc comment on `PhysicsWorld` (`world.rs`) plus a debug-only assertion/`lock_tracker` extension that can detect the reversed order outside `BYRO_LOCK_ORDER_CHECK` runs.

## Completeness Checks
- [ ] **LOCK_ORDER**: Whichever fix is chosen, re-verify `apply_buoyancy`/`register_newcomers`/`push_kinematic`/`pull_dynamic` all now follow one documented, structurally-consistent order
- [ ] **SIBLING**: #2270 (scripting's own undocumented lock-discipline house rule) is the same failure class in a different subsystem — consider whether a shared fix pattern (crate-level doc comment + assertion) should land in both
- [ ] **TESTS**: A regression test mirroring `dump_awake_fallers`'s #2136 fix, applied to `push_kinematic`/`pull_dynamic`

---
Filed from `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md` via `/audit-publish`.
