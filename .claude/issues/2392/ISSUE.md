# #2392 — ECS-D7-2026-08-07-02: Seat reservations are never released when the claiming actor despawns but its furniture survives

- **Severity**: LOW
- **Domain**: ecs
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2392


- **Severity**: LOW
- **Dimension**: 7 — Component Lifecycles (M42 AI-package seat claims)
- **Location**: `byroredux/src/cell_loader/references/mod.rs:1639-1647`, `byroredux/src/systems/sandbox.rs:206-250`, `byroredux/src/components.rs:1207`
- **Status**: NEW (adjacent to closed #2147, which fixed the opposite half of this problem)

**Description**

`SeatReservations` is `HashSet<(EntityId /*furniture*/, u32 /*marker*/)>` — it records the seat but not the claimant. The only release path, `prune_seat_reservations`, retains a claim iff its furniture entity is still live. There is no path that releases a claim when the actor that made it is despawned. Since the set does not store the claimant, no such path can be written without a schema change.

**Evidence**:

```rust
fn prune_seat_reservations(world: &byroredux_core::ecs::World) {
    let live_furniture: HashSet<EntityId> = world
        .query::<byroredux_core::ecs::components::Furniture>()
        .map(|q| q.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    if let Some(mut r) = world.try_resource_mut::<crate::components::SeatReservations>() {
        r.0.retain(|(furniture, _)| live_furniture.contains(furniture));   // furniture only
    }
}
```

Claim site (`sandbox.rs:218`) inserts `seat_id` alone: `reservations.0.insert(seat_id);`. `Seated { furniture }` on the actor is the only record of the pairing, and it is destroyed by `World::despawn` without touching the resource.

**Impact**

Under exterior grid streaming (`radius > 0`) an actor in cell A can claim a seat on furniture in cell B (the seat search radius crosses the boundary). When A unloads and B stays resident, the `(furniture_B, marker)` claim is permanently stranded: no other NPC can ever take that seat for the rest of the session. Entity IDs are never recycled, so this cannot alias a new furniture entity — it only leaks availability. Bounded blast radius: the whole feature is opt-in behind `BYRO_SANDBOX_SIT`, hence LOW.

**Related**: #2147 (fixed the wholesale-clear that released *live* claims) — this is the mirror-image gap left by that fix.

**Suggested Fix**: Change the resource to `HashMap<(EntityId, u32), EntityId /*claimant*/>` and extend `prune_seat_reservations` to also drop entries whose claimant no longer has a `Seated` component (or no longer exists). Keep the furniture-liveness retain as-is.

## Completeness Checks
- [ ] **SIBLING**: Check the other six M42 AI-package systems (`wander`/`travel`/`follow`/`escort`/`guard`/`patrol`) for the same claimant-not-recorded shape in their own state resources
- [ ] **TESTS**: A regression test spawning an actor across a simulated cell boundary, despawning it, and asserting the seat becomes claimable again
- [ ] **DOC**: Update the audit-ecs skill's stale checklist bullet (see ECS-D7-2026-08-07-03) alongside this fix so the two don't drift again

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
