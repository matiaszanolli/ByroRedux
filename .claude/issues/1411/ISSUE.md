## NIFAL-S3: extract_emitter_params passes NIF binary scalars (speed, life_span, initial_radius) with no finite/positive guard

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** nifal
**File:** `crates/nif/src/import/walk/mod.rs:688`

## Recommended Fix

Add sane() filter inside extract_emitter_params checking life_span.is_finite() && life_span > 0.0, initial_radius.is_finite() && initial_radius >= 0.0, speed.is_finite(). Return None on non-finite values to block apply_emitter_params.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at NIFAL parser to Material boundary; never pushed into shaders. See /audit-nifal.

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
