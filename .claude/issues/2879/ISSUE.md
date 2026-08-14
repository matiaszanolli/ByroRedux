# PHYS-D2-04

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2879

---

Found by `/audit-physics` Dimension 2 (Step Determinism & Budget). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `crates/physics/src/world.rs:333-346`, `:909-1534` (test module)

## Trigger Conditions
n/a — this is the coverage gap that lets **PHYS-D2-01 (HIGH)** ship green.

## Description
Three separate gaps around the accumulator's input guard.

1. **No sub-tick `dt` is ever tested.** Every `step()` call in the 72-test suite passes either exactly `PHYSICS_DT` or `100.0` (surveyed: 21 call sites, no other value). The `accumulator < PHYSICS_DT` branch — the entire >60 fps regime, which is the project's own target on an RTX 4070 Ti — is never exercised. `static_scene_skips_step_when_nothing_awake` (`:1285-1297`) and `wake_re_engages_stepping` (`:1304-1312`) both use `step(PHYSICS_DT)`, the one dt value at which PHYS-D2-01 cannot reproduce.

2. **The NaN guard is unpinned and incidental.** `frame_dt.max(0.0)` (`world.rs:346`) is correct for NaN only because Rust's `f32::max` returns the non-NaN operand, so `f32::NAN.max(0.0) == 0.0`. Nothing in the code, comments or tests states that this is intentional rather than incidental, and no test pins it. A future refactor to `if frame_dt > 0.0 { ... }`-style code, or to `f32::maximum` (which **propagates** NaN), silently turns the accumulator into NaN and wedges the loop forever.

3. **The doc over-claims.** `step()`'s doc says *"At least one substep always runs (the check is after the step), so a genuinely slow frame still advances the simulation"* (`world.rs:342-343`). True for the budget bail-out, false in general — zero substeps run whenever `accumulator < PHYSICS_DT`, which is the majority of frames above 60 fps.

## Impact
No runtime impact on its own; it is **why a HIGH-severity total physics stall is invisible to CI**.

## Suggested Fix
Add `step(PHYSICS_DT / 2.0)` variants of `wake_re_engages_stepping` and `static_scene_skips_step_when_nothing_awake`, plus a `step(f32::NAN)` / `step(-1.0)` guard test asserting the accumulator is unchanged. Correct the "at least one substep" sentence to scope it to the budget bail-out.

## Related
- **PHYS-D2-01** (the HIGH this gap conceals)
