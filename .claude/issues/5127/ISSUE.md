# PHYS-D2-2026-09-29-02: recovery_counts() mixes units — the lifetime total counts events, last_frame counts bodies

**Labels**: low,bug,physics

**Source**: `docs/audits/AUDIT_PHYSICS_2026-09-29.md`
**Severity**: LOW
**Dimension**: Step & Sync (also Queries & Diagnostics)
**Location**: `crates/physics/src/world.rs` (`recoveries_total` / `recoveries_last_frame` fields, `recovery_counts()`, and the recovery branch in `step`); printed by `byroredux/src/commands/physics.rs` (`phys.stats`) and `byroredux/src/commands/ragdoll_status.rs` (`ragdoll.status`)

**Trigger Conditions**: any recovery that restores more than one body (a multi-bone ragdoll always does).

## Description
- `recoveries_total += 1` once per recovery event.
- `recoveries_last_frame += restored as u32` adds the number of bodies restored.

The field docs call both "recoveries". The step `break`s after a recovery, so the event count per frame is only ever 0 or 1. An 18-bone corpse restore prints `recoveries: total=1 last_frame=18`, so `total >= last_frame` does not hold.

## Evidence
`world.rs`: `self.recoveries_total = self.recoveries_total.saturating_add(1);` / `self.recoveries_last_frame = self.recoveries_last_frame.saturating_add(restored as u32);`

## Impact
A gate or operator comparing the two numbers gets inconsistent semantics; #4683 exists precisely so gates can assert on these counters.

## Related
#4683 (closed), #4772.

## Suggested Fix
Rename the second counter to `bodies_restored_last_frame` and update both command labels, or count events in both and add a separate body count.

Validated at HEAD 9fcfdc3fc: the two `saturating_add` lines (1 vs `restored as u32`) are adjacent in `step`'s recovery branch.

## Completeness Checks
- [ ] **SIBLING**: both `phys.stats` and `ragdoll.status` labels updated together
- [ ] **TESTS**: A multi-body recovery test asserts the counters' documented units
