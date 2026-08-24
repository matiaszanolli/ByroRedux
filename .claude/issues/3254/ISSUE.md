# 3254: ECS-2026-08-24-06: cinematic unload-retention permanently orphans entities out of all cell ownership

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_ECS_2026-08-24.md` (ECS-2026-08-24-06)

## Description

`unload_cell_inner` filters an unload's victim list against a globally-collected "cinematic retained" set (tethered carts, their horses, riders with `ActorCinematicState::vehicle`, plus the full `Children` closure), then strips `CellRoot` from **every** retained entity in the whole world — not just the ones belonging to the unloading cell. There is no re-adoption path: `CellRoot` is only ever inserted at cell load, and `HorseTetherState` is never removed anywhere in production code. Once a cart is tethered and *any* cell anywhere unloads, that cart, its horse, its rider, and their whole render hierarchy become permanently unowned, with GPU handles excluded from every subsequent refcount decrement.

## Location

`byroredux/src/cell_loader/unload.rs:15-45` (`cinematic_retained_entities`), `:139-152` (the `CellRoot`-strip loop, ahead of GPU-handle collection at `:154+`)

## Evidence

```rust
let retained = cinematic_retained_entities(world);   // world-global, not scoped to this cell
victims.retain(|entity| !retained.contains(entity));
if !retained.is_empty() {
    if let Some(mut roots) = world.query_mut::<CellRoot>() {
        for entity in retained {
            roots.remove(entity);          // no path ever re-adds this
        }
    }
}
```

## Impact

Vanilla Skyrim's opening cart convoy is the canonical trigger — after the first exterior-tile unload, the cart/horse/riders become permanently resident and rendered at their last transform, with GPU resources held for the process lifetime. Unbounded in duration.

## Suggested Fix

Make retention explicitly reversible — reparent retained entities onto a dedicated long-lived "cinematic root" registered in `CellRootIndex`, or record stripped entities in a new resource and drain/re-stamp on cinematic completion. At minimum, restrict the strip to `retained ∩ this_unload's_victims`, and add a `HorseTetherState` removal path.

## Completeness Checks
- [ ] **TESTS**: A regression test simulating a tethered cart + unrelated cell unload, asserting `CellRoot` retained/reclaimable
