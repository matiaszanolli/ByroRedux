# REN-D23-2026-08-07-02: fsr_gated_dof keys off fsr_temporal.is_some(), so DOF stays disabled in the FSR-failed fallback where there is no FSR jitter to conflict with

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2518
**Finding ID**: REN-D23-2026-08-07-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 23 — FSR Upscaler
**Location**: `crates/renderer/src/vulkan/context/draw.rs::fsr_gated_dof` (call site `draw.rs:1628`)
**Status**: NEW

## Description
`let active_dof = fsr_gated_dof(dof, self.fsr_temporal.is_some());` forces `aperture = 0.0` whenever FSR *mode* is selected. `fsr_temporal` is `Some` for the whole of `UpscalerMode::Fsr3(..)`, including when the FSR context never got created or `dispatch_failure` has latched — states where the frame runs completely unjittered on the native blit. The documented rationale ("combining the independent Halton(5,7) lens sequence with FSR's own projection jitter would violate the motion/reprojection contract") does not apply there, since FSR's projection jitter is exactly what has been switched off.

## Evidence
`draw.rs:1573-1604` sets `fsr_jitter_pixel = None` and `jx/jy = 0.0` when `!upscaler.is_fsr_dispatch_active()`, yet `draw.rs:1628` still passes `self.fsr_temporal.is_some()` (unchanged by dispatch failure) to `fsr_gated_dof`.

## Impact
Authored DOF is silently dropped in the degraded FSR path. Visual-only, only in an already-degraded state. Also a latent inconsistency if a future change makes the two predicates matter independently.

## Related
REN-D23-2026-08-07-01 (this report — same "FSR mode selected != FSR running" conflation).

## Suggested Fix
Pass the same predicate the jitter gate uses — `self.frame_upscaler.as_ref().is_some_and(|u| u.is_fsr_dispatch_active())` — so DOF and jitter are gated on one fact.

## Completeness Checks
- [ ] **SIBLING**: Fix in the same pass as REN-D23-2026-08-07-01 since both stem from the same "mode selected vs. actually running" conflation
- [ ] **TESTS**: A regression test confirms DOF stays active in the `BYRO_FSR_FORCE_DISPATCH_FAIL=1` fallback path
