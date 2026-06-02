# #1445 — LC-D9-02: extract_emitter_params omits planar_angle fields from finite sweep

_Snapshot as filed (2026-06-02) from AUDIT_LEGACY_COMPAT_2026-06-02.md. GitHub is authoritative for live state._

- **Severity**: LOW
- **Dimension**: D9 (Particle Emitter Translation Parity)
- **Location**: `crates/nif/src/import/walk/mod.rs:696-703`
- **Status**: NEW

## Description
The #1411 finite guard sweeps every lifted scalar except the two planar-angle fields (`planar_angle` / `planar_angle_variation`), which are read into `EmitterBaseParams` but not in the `all_finite` list.

## Impact
Harmless today — `apply_emitter_params` (`byroredux/src/systems/particle.rs:29`) never reads them — but a latent NaN trap if planar angle is ever wired into the spawn cone.

## Suggested Fix
Add both fields to the `is_finite` list at `:696-703`, or gate them when the spawn path begins consuming them.

## Related
#1411, LC-D5-04 (#TBD).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (KF-sequence path vs embedded path; other channel converters)
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→Material/clip boundary, never pushed to shaders/renderer or re-derived at render time. See /audit-nifal.
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from [docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md](../blob/main/docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md)_
