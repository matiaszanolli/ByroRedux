# Issue #3446: CONC-D3-2026-08-27b-04: `cinematic_animation_event_system` is a second `StringPool`-before-storage site, demoting `StringPool` from a lock-order sink to a mid-graph node

**Finding ID**: CONC-D3-2026-08-27b-04
**Labels**: bug, ecs, low, concurrency
**Filed from**: `docs/audits/AUDIT_CONCURRENCY_2026-08-27b.md`
**Audited at**: HEAD = 969d81c8

---

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-08-27b.md` — finding `CONC-D3-2026-08-27b-04` (LOW, Dimension 3: ECS Lock Ordering & Deadlock). Audited at `HEAD = 969d81c8`; re-verified against current code at publish time.

**Location**: `byroredux/src/systems/cinematic.rs` (`cinematic_animation_event_system`)

## Description

```rust
// byroredux/src/systems/cinematic.rs
pub(crate) fn cinematic_animation_event_system(world: &World, _dt: f32) {
    let deliveries: Vec<(EntityId, CinematicAnimationEvent)> = {
        let Some(pool) = world.try_resource::<StringPool>() else { return; };
        let Some(event_query) = world.query::<AnimationTextKeyEvents>() else { return; };
```

`StringPool` sits at the tail of `docs/engine/ecs.md`'s canonical order precisely so that no lock is ever taken beneath it. Six in-tree sites respect that (`byroredux/src/commands/shared.rs`, `byroredux/src/commands/assets.rs`, and four sites in `crates/debug-server/src/evaluator.rs`); this one and the MEDIUM sibling's `studio_host.rs` do not.

The system's exclusivity (`add_exclusive_with_access`, `byroredux/src/boot.rs`) is the whole safety argument, and — unlike `pool_regen_tick_system`, whose #2391 declaration exists specifically to surface such a contract — nothing at this site says so.

## Evidence

The snippet above, plus the six correctly-ordered sites listed.

## Trigger conditions

Every frame in which any entity carries `AnimationTextKeyEvents` (the M47.2 cinematic slice, MQ101). **Latent only** — no in-tree site acquires `AnimationTextKeyEvents` before `StringPool` today (`byroredux/src/systems/animation.rs` takes `AnimationTextKeyEvents` under the `AnimationClipRegistry` guard, never `StringPool`).

## Verification path

Source-only. It would become a detector abort — and, if either side moved to a parallel lane, a real hang — the moment any code path reads a `Name`/`AnimationTextKeyEvents` pair in the other order.

## Impact

None today. The cost is that the "`StringPool` is a lock-order sink" invariant, which is what makes the canonical order's tail cheap to reason about, is no longer true; two independent sites now record edges out of it, and a third would only need to be a parallel system to matter.

## Suggested fix

Reorder to `AnimationTextKeyEvents` then `StringPool` (both are reads; nothing in the block needs the pool before the query), and add `StringPool`'s sink property as an explicit sentence in `docs/engine/ecs.md`'s canonical-order section so the next site has something to violate visibly.

## Related

The MEDIUM sibling `CONC-D3-2026-08-27b-03` (`studio_host::snapshot` — same shape, but a reachable cycle), #313, #2388, #2391.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the `studio_host.rs` sibling, and any future `StringPool`-first acquisition
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved; `StringPool` stays a sink in `docs/engine/ecs.md`'s canonical order
- [ ] **TESTS**: A regression test pins this specific fix (source-assert on the acquisition order in this system)
