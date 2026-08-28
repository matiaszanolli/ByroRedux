# Issue #3445: CONC-D3-2026-08-27b-03: `studio_host::snapshot` inverts the canonical order's `Name → StringPool` tail, closing a 2-cycle against `resolve_entity_name` and the debug evaluator

**Finding ID**: CONC-D3-2026-08-27b-03
**Labels**: bug, ecs, medium, concurrency
**Filed from**: `docs/audits/AUDIT_CONCURRENCY_2026-08-27b.md`
**Audited at**: HEAD = 969d81c8

---

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-08-27b.md` — finding `CONC-D3-2026-08-27b-03` (MEDIUM, Dimension 3: ECS Lock Ordering & Deadlock). Audited at `HEAD = 969d81c8`; re-verified against current code at publish time.

**Location**: `byroredux/src/studio_host.rs` (`snapshot`). Reverse edges at `byroredux/src/commands/shared.rs` (`resolve_entity_name`), `byroredux/src/commands/assets.rs` (`skin.list`), `crates/debug-server/src/evaluator.rs`.

## Description

`docs/engine/ecs.md` fixes one process-wide order for the hierarchy/skinning/naming cluster, ending `… → Name → StringPool`. `studio_host::snapshot` acquires `StringPool` **first** and then walks storages beneath it:

```rust
// byroredux/src/studio_host.rs
pub(crate) fn snapshot(world: &World) -> Option<StudioSnapshot> {
    let session = world.try_resource::<StudioSession>()?.clone();
    let pool = world.try_resource::<StringPool>();          // ← guard held for the whole walk
    let objects = session.objects.iter().filter_map(|&entity| {
        let transform = world.get::<Transform>(entity)?;    // ← StringPool → Transform
        …
        let name = world
            .get::<byroredux_core::ecs::Name>(entity)       // ← StringPool → Name
            .and_then(|name| pool.as_ref().and_then(|pool| pool.resolve(name.0)))
```

`World::get` takes a tracked storage read lock, so these are real edges. The established reverse edges are explicit and commented:

```rust
// byroredux/src/commands/shared.rs — resolve_entity_name
let name_q = world.query::<Name>()?;
let name = name_q.get(entity)?;
let pool = world.try_resource::<StringPool>()?;             // ← Name → StringPool
```

```rust
// byroredux/src/commands/assets.rs
// Name before StringPool — matches `resolve_entity_name`'s order
// for this pair (#313).
let name_q = world.query::<Name>();
let pool = world.try_resource::<StringPool>();
```

`crates/debug-server/src/evaluator.rs` additionally establishes `Transform → … → StringPool`, and that crate carries a dedicated source-assert regression test (`debug_evaluator_acquires_locks_in_canonical_order`) added under #2388 for precisely this violation.

The sibling function in the same snapshot bridge gets it right and says why:

```rust
// byroredux/src/inventory.rs
// Clone each component before acquiring the next storage lock. The
// menu is off the hot path, and this preserves the ECS invariant that
// callers never hold independently-acquired component locks in an
// arbitrary order.
let inventory = (*world.get::<Inventory>(player)?).clone();
```

`studio_host.rs` is the newer file and did not inherit that discipline.

## Evidence

The four snippets above, all present at publish time.

## Trigger conditions

A `--studio <mesh>.nif` session with the debug-UI panel snapshot running — `build_panel_snapshot` calls `studio_host::snapshot` unconditionally and the function only short-circuits if `StudioSession` is absent — plus any console command that resolves an entity name (`prid`, `entities`, `skin.list`, the debug server's `EntityList`).

## Verification path

Source-only for the ordering; observable as a `BYRO_LOCK_ORDER_CHECK=1` abort in a debug `--studio` run that also issues one name-resolving console command.

## Impact

No live deadlock — the panel snapshot runs on the main thread in the frame loop and console commands run under the `Stage::Late` exclusive `DebugDrainSystem`, so the two orders are never concurrent. The damage is the detector abort plus the loss of `StringPool`'s lock-order **sink** property (see the LOW sibling `CONC-D3-2026-08-27b-04`). Reachability is gated on `--studio`, which is why this is MEDIUM rather than HIGH.

## Suggested fix

Move the `StringPool` acquisition inside the per-entity closure, *after* the `Name` read, mirroring `resolve_entity_name` — or better, resolve names into an owned `Vec<(EntityId, String)>` up front under `Name → StringPool` and drop both guards before the `Transform`/`Material` walk, mirroring `inventory::snapshot`. Consider extending `debug-server`'s `debug_evaluator_acquires_locks_in_canonical_order` pattern to a shared source-assert covering `studio_host.rs`, since this is now the second recurrence.

## Related

#313 and #2388 (the canonical order and the last time this exact pair was inverted), #3261 (canonical-order doc completeness), and the LOW sibling `CONC-D3-2026-08-27b-04` (`cinematic_animation_event_system`, the same shape).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — `cinematic_animation_event_system` (the LOW sibling), the rest of `studio_host.rs`, and any other `StringPool`-first walk
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved; the `Name → StringPool` tail of `docs/engine/ecs.md`'s canonical order is respected
- [ ] **TESTS**: A regression test pins this specific fix (extend the `debug_evaluator_acquires_locks_in_canonical_order` source-assert pattern to `studio_host.rs`)
