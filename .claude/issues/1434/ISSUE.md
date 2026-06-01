## NIFAL-S5: NiPSysGrowFadeModifier.base_scale carries raw NIF float with no is_finite() guard before particle size computation

**Severity:** INFO | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** nifal
**File:** `crates/nif/src/import/walk/mod.rs:687`

## Recommended Fix

Wrap extraction: base_scale = base_scale.filter(|s| s.is_finite() && *s > 0.0) in extract_emitter_params.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at NIFAL parser to Material boundary; never pushed into shaders. See /audit-nifal.

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*