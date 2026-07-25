**Severity**: MEDIUM · **Dimension**: 7 — Component Lifecycles (M42 seat claims)
**Source**: `docs/audits/AUDIT_ECS_2026-07-25.md` (ECS-2507-01)
**Status**: NEW

## Description
`load_references` clears the entire `SeatReservations` set on every invocation.
`load_references` is called **once per cell**, on both the interior path
(`cell_loader/load.rs:377`) and the exterior grid path (`cell_loader/exterior.rs:418`,
inside the per-`(gx, gy)` cell loader). On an exterior grid load with `--radius 3`
that is 49 wholesale clears; during boundary-crossing streaming it happens again
for every newly-streamed cell while previously-loaded cells (and their seated
actors) are still resident. `Seated` is a one-shot terminal marker, so an actor
that already sat never re-claims its marker after the clear — the seat is
permanently released while still physically occupied, and the next unseated
actor within `SEAT_SEARCH_RADIUS` can claim the same `(furniture entity, marker
index)`.

The in-code rationale is also factually wrong: the comment at
`references/mod.rs:189` says "clear stale seat reservations from the previous
cell (entity ids reset on unload)". Entity IDs are **never** reset or reclaimed
— `World::despawn` explicitly documents this (`crates/core/src/ecs/world.rs:114-118`,
#372) and `next_entity` only ever grows. Stale entries can therefore never
alias a new furniture entity; the clear is only preventing a slow set-growth
leak, at the cost of correctness.

## Evidence
```rust
// byroredux/src/cell_loader/references/mod.rs:195
if let Some(mut r) = world.try_resource_mut::<crate::components::SeatReservations>() {
    r.0.clear();
}
```

```rust
// byroredux/src/systems/sandbox.rs:206-217 — claims are never re-asserted
let mut reservations = world.resource_mut::<SeatReservations>();
for (npc, behavior) in sandbox_q.iter() {
    if seated_q.as_ref().is_some_and(|s| s.contains(npc)) {
        continue; // already seated (one-shot guard) — never re-inserts its claim
    }
    …
    reservations.0.insert(seat_id);
```

## Impact
Two NPCs occupying the same furniture marker in an exterior multi-cell scene.
Gated behind `BYRO_SANDBOX_SIT` (off by default), so no default-configuration
impact today, but it silently breaks the per-marker exclusivity that
`SeatReservations` exists to provide as soon as M42 seating is turned on for
exteriors.

## Suggested Fix
Replace the wholesale `clear()` with a targeted prune — retain only
reservations whose furniture entity still exists
(`r.0.retain(|(furn_e, _)| world.has::<Furniture>(*furn_e))`), which is both
leak-free and cross-cell-safe. Alternatively, have `sandbox_seat_system`
re-assert each `Seated` actor's claim each tick (store the claimed `seat_id`
on the `Seated` marker) so a clear is self-healing. Fix the "entity ids reset
on unload" comment either way.

## Related
#372 (IDs never reclaimed); the M42 seat-claim design in
`docs/engine/npc-spawn-ai-packages.md`.

## Completeness Checks
- [ ] **SIBLING**: Check for the same wholesale-clear-on-cell-load pattern against other per-cell resources (e.g. any other `SparseSetStorage`-backed reservation/claim resource keyed by cross-cell entity identity)
- [ ] **TESTS**: A regression test pins seat reservations surviving a sibling cell load while the seat is still occupied
