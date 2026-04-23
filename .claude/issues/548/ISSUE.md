# NIF-03: NiBoolTimelineInterpolator missing from dispatch (8,450 blocks across FO3+FNV+SE)

**Severity**: CRITICAL
**Dimension**: Coverage Gaps
**Game Affected**: FO3, FNV, Skyrim SE
**Audit**: docs/audits/AUDIT_NIF_2026-04-22.md § NIF-03

## Summary

`NiBoolTimelineInterpolator` (nif.xml: subclass of `NiBoolInterpolator` with an extra `Timeline: TimeBool100` field) drives binary visibility / enabled animations on a huge fraction of Skyrim SE content — 6,796 blocks, second-most-common unknown after `bhkRigidBody`. Every affected animation is a silent no-op. Renderer sees the affected objects as permanently visible (or permanently hidden, depending on default).

## Evidence

- Skyrim SE: 6,796 blocks
- FNV: 1,118 blocks
- FO3: 536 blocks
- Total: **8,450 blocks** across three games

## Location

- `crates/nif/src/blocks/mod.rs:505` has `NiBoolInterpolator` but not the Timeline variant
- `crates/nif/src/blocks/interpolator.rs` (add new struct + parse impl)

## Suggested fix

Thin extension of `NiBoolInterpolator::parse` with the extra `TimeBool100` byte. Dispatch arm next to the existing `NiBoolInterpolator =>` at `blocks/mod.rs:505`. ~20 LOC.

## Completeness Checks
- [ ] **SIBLING**: Check anim.rs extract paths handle the Timeline variant when resolving controlled-block targets
- [ ] **TESTS**: Synthetic block + round-trip
- [ ] **REAL-DATA**: All three unknown sweeps drop `NiBoolTimelineInterpolator` to 0

Fix with: /fix-issue <number>
