# #1443 — LC-D5-04: Mainline keyframe-stream converters lack finite/FLT_MAX guard

_Snapshot as filed (2026-06-02) from AUDIT_LEGACY_COMPAT_2026-06-02.md. GitHub is authoritative for live state._

- **Severity**: LOW
- **Dimension**: D5 (Animation Readiness)
- **Location**: `crates/nif/src/anim/channel.rs:62-70,151-177,336-344`; `crates/nif/src/anim/keys.rs:17-49,185-198`
- **Status**: NEW

## Description
The B-spline static-fallback and constant-transform paths apply `is_flt_max` filtering (`transform.rs`, `bspline.rs`), but the mainline keyframe-stream converters (`convert_vec3_keys`, `convert_quat_keys`, `convert_float_keys`, and the float/color/bool channel extractors) copy raw NIF floats with no `is_finite` / FLT_MAX filter — the same class #772 fixed for the B-spline *pose* path.

## Impact
A corrupt key value reaches the sampler and the bone/shader uniform (NaN skinning matrix). Lower likelihood than the particle paths (vanilla key streams are clean) → LOW, but an inconsistency worth a single shared sanitizer.

## Suggested Fix
Route all keyframe-stream value reads through one `sanitize_key_value` helper that drops/clamps non-finite + FLT_MAX, mirroring the emitter-rate `sane()` gate.

## Related
#1393, #1382, #1434, #1409 (same finite-guard family, distinct sites). LC-D9-02 (#TBD).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (KF-sequence path vs embedded path; other channel converters)
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→Material/clip boundary, never pushed to shaders/renderer or re-derived at render time. See /audit-nifal.
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from [docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md](../blob/main/docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md)_
