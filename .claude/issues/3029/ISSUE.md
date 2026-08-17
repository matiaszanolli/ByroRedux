# ECS-2026-08-16-01: disable_actor_ai is a divergent copy of clear_ambient_behavior dropping seat release

**Issue**: #3029
**Severity**: MEDIUM
**Dimension**: 7 — Component Lifecycles
**Labels**: `medium,ecs,bug`
**Source report**: `docs/audits/AUDIT_ECS_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_ECS_2026-08-16.md` (Dimension 7 — Component Lifecycles, P2 gameplay slice).

**Location**: `byroredux/src/combat.rs`:288-311, against `byroredux/src/npc_spawn/ai_package.rs`:326-353

## Description

`combat::disable_actor_ai` is a **divergent copy** of `clear_ambient_behavior`. The two remove the same sixteen behaviour components in near-identical order — but `disable_actor_ai` **drops the seat-reservation release**.

## Evidence

`clear_ambient_behavior` releases the reservation first:
```rust
// npc_spawn/ai_package.rs:326-353
if let Some(mut reservations) = world.try_resource_mut::<SeatReservations>() {
    …                                    // release this actor's seat
}
remove_component::<SandboxBehavior>(world, actor);
remove_component::<Seated>(world, actor);
…
```

`disable_actor_ai` goes straight to the removals:
```rust
// combat.rs:288-311 — no SeatReservations block
remove_component::<SandboxBehavior>(world, actor);
remove_component::<Seated>(world, actor);
…
```

Re-verified 2026-08-17: `SeatReservations` appears in `clear_ambient_behavior` and **not** in `disable_actor_ai`; both then remove the same set.

## Impact

Killing a seated NPC removes its `Seated` component but leaves the reservation held in `SeatReservations`, keyed `(furniture, marker index)`. That seat is **permanently unavailable** to every other actor for the session — `sandbox_seat_system` sees it as occupied by a corpse that no longer claims it.

The duplication is the underlying defect: two copies of one teardown, and the newer copy silently lost a step.

## Suggested Fix

Have `disable_actor_ai` call `clear_ambient_behavior` rather than re-implementing it. If combat genuinely needs a different set, express the difference explicitly instead of by divergence — per the project's "improve existing code rather than duplicating logic" rule.

## Related

- #3027-adjacent: ECS-2026-08-16-02 (#3030) — the same kill path's other lifecycle gap
- #2976 (TD6-2026-08-16-01) — the same combat slice

## Completeness Checks
- [ ] **NO-DUPLICATION**: One teardown implementation, not two
- [ ] **SEAT-RELEASE**: A killed seated NPC frees its seat reservation
- [ ] **SIBLING**: Any other caller that tears down AI behaviour uses the same path
- [ ] **LOCK_ORDER**: The `SeatReservations` resource acquisition preserves TypeId-sorted ordering when merged
- [ ] **TESTS**: A regression test kills a seated actor and asserts the seat is reusable

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3029 --json state` when live state is needed.*
