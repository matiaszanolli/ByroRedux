# ECS-2026-08-16-05: combat_input_system burns the attack edge before the PlayerMode::Character gate

**Issue**: #3033
**Severity**: LOW
**Dimension**: 7 — Component Lifecycles
**Labels**: `low,ecs,bug`
**Source report**: `docs/audits/AUDIT_ECS_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_ECS_2026-08-16.md` (Dimension 7 — Component Lifecycles, P2 gameplay slice).

**Location**: `byroredux/src/combat.rs`:80-99

## Description

`combat_input_system` consumes `attack_pressed`, increments `attacks_started` and arms `cooldown_remaining = MELEE_COOLDOWN_SECONDS` **before** checking that the player is in `PlayerMode::Character`.

In fly-cam mode the counter climbs and the cooldown arms, even though no swing, ray cast or `HitEvent` can occur.

## Evidence

```rust
// combat.rs:80-99
let attack_ready = if let Some(mut state) = world.try_resource_mut::<CombatState>() {
    …
    if attack_pressed && state.cooldown_remaining <= 0.0 {
        state.cooldown_remaining = MELEE_COOLDOWN_SECONDS;
        state.attacks_started = state.attacks_started.saturating_add(1);
        true
    } else { false }
} else { false };
if !attack_ready
    || !world.try_resource::<PlayerMode>()
        .is_some_and(|mode| *mode == PlayerMode::Character)
{ return; }
```

`record_miss` is **not** called on the mode bail either, so `CombatState.last` keeps a stale entry while the counter moves.

Re-verified 2026-08-17.

## Impact

`attacks_started` overstates real swings after any fly-cam session, and `combat.status` reports a cooldown the player never incurred. Since `combat.status` is the console surface the P2 gate reads, the telemetry is the thing being corrupted.

LOW because it affects diagnostics rather than gameplay outcome.

## Suggested Fix

Move the `PlayerMode::Character` gate **before** the `CombatState` mutation block, so the edge and cooldown are only consumed when a swing can actually happen. Decide explicitly whether the mode bail should `record_miss` or leave `last` untouched, and comment the choice.

## Related

- #3008 (RT-2026-08-16-09 — the gate that reads `combat.status`)
- #2976 (TD6-2026-08-16-01 — the same system's `Block` handling)

## Completeness Checks
- [ ] **GATE-FIRST**: The mode check precedes any state mutation
- [ ] **STALE-LAST**: The `record_miss`-on-bail decision is made deliberately and commented
- [ ] **SIBLING**: Other input systems checked for edge consumption before their eligibility gate
- [ ] **TESTS**: A regression test presses attack in fly-cam and asserts the counter does not move

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3033 --json state` when live state is needed.*
