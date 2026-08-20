# PHYS-D6-2026-08-20-07: clear_stale_water_contacts is skipped when a current marker outlives the water plane

**Issue**: #3127 — https://github.com/matiaszanolli/ByroRedux/issues/3127
**Finding**: `PHYS-D6-2026-08-20-07`
**Labels**: bug, low, legacy-compat
**Filed**: 2026-08-20 (comprehensive `/audit-suite` sweep, 25 reports)

---

**Audit**: `docs/audits/AUDIT_PHYSICS_2026-08-20.md` — Dimension 6 (Water / Buoyancy)
**Severity**: LOW · **Status**: NEW — the guard was widened from `if surfaces.is_empty()` to `if surfaces.is_empty() && current_volumes.is_empty()` by `7e65c46c`

## Location
`crates/physics/src/water.rs:492-495`

## Trigger conditions
A cell transition that unloads every `WaterPlane` while at least one `WaterCurrentVolume` entity remains resident, with a sleeping dynamic body still carrying a wet `WaterContact`.

## Description
The surfaces-empty branch is the only caller of `clear_stale_water_contacts` (the restore-on-unload path added by `808ecfae` for #2870's sibling case). With currents still present the function falls through to the quiesced fast path (`water.rs:509`), which returns before the per-body loop that would otherwise have taken the `surface == None && prior_wet` restore branch. The body keeps `linear_damping = angular_damping = 1.5` and its latched buoyancy `user_force` instead of its authored values.

Verified at HEAD:
```rust
// water.rs:492-495
if surfaces.is_empty() && current_volumes.is_empty() {
    clear_stale_water_contacts(world);
    return;
}
```

## Evidence
The restore in the main loop (`water.rs:753-760`) is only reachable once the scan runs; with nothing awake and `waves_active` false (no surfaces to make it true) the scan never runs.

## Impact
A sleeping crate keeps water damping and a body-weight up-force in air. Invisible until something wakes it, at which point it drifts upward. Narrow: needs a current marker to survive a plane unload.

## Related
#2870 (CLOSED), PHYS-D6-2026-08-20-04, PHYS-D6-2026-08-20-01 (the latched `user_force` this leaves behind is the same force this cycle's other finding fails to reset).

## Suggested fix
Call `clear_stale_water_contacts(world)` whenever `surfaces.is_empty()`, regardless of `current_volumes`, and keep the early `return` only for the both-empty case.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — unload the plane while a current volume survives, assert the sleeping body's damping is restored
