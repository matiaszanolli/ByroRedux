# CONC-D3-2026-08-30-02: `save::validate_animation` takes `AnimationPlayer -> AnimationClipRegistry`, inverting `animation_system_inner`'s #2400 outermost-lock order

**Issue**: #3649
**Labels**: bug, ecs, medium, save-load, concurrency
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D3-2026-08-30-02 (MEDIUM, D3 · ECS Lock Ordering & Deadlock).

**Location**: `crates/save/src/validate.rs:336-344`; opposing edge at `byroredux/src/systems/animation.rs:530-532` + `:604` / `:619`.

## Description

`animation_system_inner` documents `AnimationClipRegistry` and `NameIndex` as *"the two outermost locks"* (#2400) and holds the registry read guard for the whole function, acquiring `AnimationPlayer` **for write** twice underneath it.

`validate_animation` does the **opposite**: it takes the `AnimationPlayer` read guard through a `let ... else` (so it lives across the whole loop) and then acquires `AnimationClipRegistry`, using both together in the loop body.

`docs/engine/ecs.md`'s canonical table has **no entry for this pair**, so nothing but the local #2400 comment records the intended direction — and the `save` crate never saw it.

## Evidence

```rust
// byroredux/src/systems/animation.rs:530-532 — registry first, held to end of fn
let Some(registry) = world.try_resource::<AnimationClipRegistry>() else {
    return;
};
// :604 and :619 — AnimationPlayer (WRITE) acquired underneath it
let Some(player_query) = world.query_mut::<AnimationPlayer>() else { return; };
let mut player_query = world.query_mut::<AnimationPlayer>().unwrap();
```
```rust
// crates/save/src/validate.rs:337-343 — AnimationPlayer first, registry second
let Some(q) = world.query::<AnimationPlayer>() else {
    return;
};
let registry = world.try_resource::<AnimationClipRegistry>();

for (entity, player) in q.iter() {
    if let Some(reg) = registry.as_ref() {
```

## Trigger Conditions

Debug build with `BYRO_LOCK_ORDER_CHECK=1`: any frame runs `animation_system_inner` and records `AnimationClipRegistry -> AnimationPlayer`; a subsequent `save` (or any load through `restore_world`, `crates/save/src/driver.rs:168`) records the reverse and closes the cycle.

## Impact

No live deadlock today: `make_animation_system` is the parallel side (`boot.rs:1009` `add_to_with_access`), but every `validate_world` caller is constrained to a quiescent lane — the `save` command's own comment (`byroredux/src/save_io.rs:775-786`, #3113/#2154) documents this.

The costs are:
1. a debug-build detector abort once both sites run; and
2. a latent edge that becomes a genuine ABBA the moment any validation path moves onto a live scheduler lane — and **the edge is write-vs-read, so it is a hard blocking edge, not a reader-reader one**.

## Related

#2400, #3113, #2154; ECS-D1-01 (`docs/audits/AUDIT_ECS_2026-08-30.md`); CONC-D3-2026-08-30-04 (the missing canonical-table entry that let this happen).

## Suggested Fix

In `validate_animation`, acquire `AnimationClipRegistry` **before** `AnimationPlayer` (one statement moved above the `let ... else`), matching `animation_system_inner`'s documented outermost-lock order. Add the pair to the canonical table (see CONC-D3-2026-08-30-04).

## Completeness Checks
- [ ] **LOCK_ORDER**: `AnimationClipRegistry -> AnimationPlayer` holds at both sites after the change
- [ ] **SIBLING**: Every other `validate_*` function in `crates/save/src/validate.rs` audited for a resource-after-storage acquisition
- [ ] **TESTS**: `BYRO_LOCK_ORDER_CHECK=1` with a frame + a save in the same process must not panic
- [ ] **DOCS**: The pair added to `docs/engine/ecs.md`'s canonical order table
