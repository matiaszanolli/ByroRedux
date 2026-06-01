## ANIM-05: extract_first_color_curve() passes BSPSysSimpleColorModifier RGBA values with no is_finite() or FLT_MAX guard

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** anim
**File:** `crates/nif/src/import/walk/mod.rs:652`

## Recommended Fix

Add is_valid_color(c: [f32; 4]) -> bool guard analogous to sane() in extract_emitter_rate().

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*