# #7015 — PHYS-D2-04: The non-finite-impulse refusal added by #6487 covers multibody links only; a free body still takes a NaN impulse

**Labels**: low,physics,bug · **Filed from**: `docs/audits/AUDIT_PHYSICS_2026-11-15.md` (2026-11-15)

- **Severity**: LOW
- **Dimension**: Dim 2 (wake/impulse contract)
- **Location**: `crates/physics/src/world/mod.rs:1209-1222`
- **Trigger Conditions**: `apply_impulse` on a non-link dynamic body with a non-finite `impulse` (script-supplied `ApplyHavokImpulse` vector, `cinematic.rs:511`).
- **Status**: NEW (asymmetry exposed by `eac3f73f9`)
- **Description**: For a link, a non-finite impulse returns `false` before any state changes. For a free body it goes straight to `b.apply_impulse(impulse, true)`, which writes NaN into the body's velocity. `forget_spawn_sleep` and `wake()` still run and the call returns `true`. The next `run_substep` snapshot filters the body out (`body_state_is_finite`), rapier quarantines it, and `restore_invalid_dynamic_bodies` parks it. A bad script argument therefore counts as an explosion-recovery event (error log, `recoveries_total`) instead of a refused call.
- **Evidence**: 1209-1222: `if is_link { if !impulse.is_finite() { return false; } ... } else { b.apply_impulse(impulse, true); }`.
- **Impact**: One parked clutter body and a spurious error log per bad input. Recovery contains it.
- **Related**: #6487, #4687 (non-finite parking).
- **Suggested Fix**: Hoist the `is_finite` refusal above the `is_link` branch so both paths behave alike.

---

Source: `docs/audits/AUDIT_PHYSICS_2026-11-15.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
