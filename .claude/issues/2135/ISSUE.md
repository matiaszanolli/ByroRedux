# 2135: CONC-D5-02: character_controller_system acquires Transform before RapierHandles; pull_dynamic acquires the reverse

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2135
**Labels**: bug, high, sync

---

## Severity
HIGH

## Dimension
RwLock Patterns (Resource↔Storage, Physics) — `/audit-concurrency` 2026-07-25

## Location
- `byroredux/src/systems/character.rs:171-183`
- versus `crates/physics/src/sync.rs:588-624` (`pull_dynamic`)

## Description
A storage↔storage inversion in the same physics territory, untouched by `b5e38c22`. `character_controller_system`'s snapshot block acquires `CharacterController` → `Transform` → `RapierHandles`, with the `Transform` read guard still live when `RapierHandles` is acquired. `pull_dynamic` does the exact reverse: `handles_q`/`body_q` are held from function entry and are still live when `query_mut::<Transform>()` (a **write** lock) is taken at the end.

Confirmed against current code: `character.rs:171` binds `tq` (Transform query), then `world.query::<RapierHandles>()` at `:179` while `tq` is still in scope → edge `Transform → RapierHandles`. `sync.rs` — `handles_q`/`body_q` bound at entry (`:589,:593`), never dropped before `query_mut::<Transform>()` at `:622` → edges `RapierHandles → Transform`, `RigidBodyData → Transform`.

## Evidence
```rust
// character.rs:171,179
let Some(tq) = world.query::<Transform>() else { ... };
// ... tq still in scope ...
let Some(handles_q) = world.query::<byroredux_physics::RapierHandles>() ...
```
```rust
// crates/physics/src/sync.rs pull_dynamic
let Some(handles_q) = world.query::<RapierHandles>() else { return; };  // :589
let Some(body_q) = world.query::<RigidBodyData>() else { return; };    // :593
// ... handles_q, body_q both still in scope ...
let Some(mut tq) = world.query_mut::<Transform>() else { return; };    // :622  <- WRITE lock
```

## Impact
The more dangerous of the two HIGH concurrency findings because `pull_dynamic`'s `Transform` acquisition is a **write** lock: a thread holding `RapierHandles` read and blocking on `Transform` write, against a thread holding `Transform` read and blocking on `RapierHandles` read, deadlocks directly once a `RapierHandles` writer (`register_newcomers`, or `activate_ragdoll`) is queued. `character_controller_system` runs in `Stage::Early` and `physics_sync_system` in `Stage::Physics`, so today the stage barrier serializes them — protection by scheduling accident, not by an acquisition-order invariant.

## Trigger Conditions
Character mode (`PlayerMode::Character`) with a physics-backed player capsule, plus any of: a second `Stage::Physics` system, `character_controller_system` moved into a parallel batch sharing a stage with physics, or a debug-server console command thread reading both `Transform` and `RapierHandles`. Detector-panic trigger: a debug build with `BYRO_LOCK_ORDER_CHECK=1` in character mode with a dynamic body present.

## Related
`b5e38c22` (same class, missed this pair), CONC-D5-01 (same finding class, different pair, filed separately).

## Suggested Fix
Preferred — in `pull_dynamic`, `drop(handles_q); drop(body_q);` immediately after the collection block and before `query_mut::<Transform>()` (the collected `updates` Vec already carries everything needed; this also matches `sync.rs`'s own two-phase discipline elsewhere). Alternative — in `character.rs`, read `RapierHandles` before `Transform`.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix (drive `character_controller_system` + `pull_dynamic` together under `BYRO_LOCK_ORDER_CHECK=1` with a real dynamic body)
