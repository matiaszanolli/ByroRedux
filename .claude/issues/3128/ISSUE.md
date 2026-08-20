# PHYS-D5-2026-08-20-08: advance_breath refills the breath reserve on a zero-dt tick

**Issue**: #3128 — https://github.com/matiaszanolli/ByroRedux/issues/3128
**Finding**: `PHYS-D5-2026-08-20-08`
**Labels**: bug, low, legacy-compat
**Filed**: 2026-08-20 (comprehensive `/audit-suite` sweep, 25 reports)

---

**Audit**: `docs/audits/AUDIT_PHYSICS_2026-08-20.md` — Dimension 5 (Character Controller)
**Severity**: LOW · **Status**: NEW

## Location
`byroredux/src/systems/character.rs:994-1010` (`advance_breath`)

## Trigger conditions
`character_controller_system` invoked with `dt <= 0.0` while the player's head is submerged — a paused / zero-delta frame, or any re-entrant call that passes 0.0.

## Description
The guard collapses two distinct cases:

```rust
if !head_submerged || dt <= 0.0 {
    return (MAX_BREATH, DrowningDamage { whole: 0.0, remainder: 0.0 });
}
```

`!head_submerged` correctly means "surfaced, refill". `dt <= 0.0` means "no time passed" and should be a no-op — but it returns `MAX_BREATH` and discards the accumulated fractional damage, resetting a drowning player to a full 15-second reserve.

Verified at HEAD (`character.rs:1002`): the two conditions still share one early return.

## Impact
A drowning player survives indefinitely across any sequence that interleaves a zero-dt tick. Reachability in the shipping loop is not established (the scheduler passes the real frame delta), so this is a correctness / hardening issue rather than an observed bug.

## Related
PHYS-D5-2026-08-20-06 (same new controller).

## Suggested fix
Split the branches — return `(MAX_BREATH, zero)` for `!head_submerged`, and `(previous_breath, DrowningDamage { whole: 0.0, remainder: previous_damage_remainder })` for `dt <= 0.0`.

## Completeness Checks
- [ ] **SIBLING**: Same collapsed `!condition || dt <= 0.0` guard checked in the other accumulators in `byroredux/src/systems/character.rs`
- [ ] **TESTS**: A regression test pins this specific fix
