## NIFAL-S7: Particle spawn loop guards em.life with .max(0.05) but not em.rate or em.start_size against NaN

**Severity:** INFO | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** nifal
**File:** `byroredux/src/systems/particle.rs:332`

## Recommended Fix

Add guard at top of per-emitter block: if !em.rate.is_finite() || em.rate <= 0.0 || !em.start_size.is_finite() || em.start_size <= 0.0 { continue; }

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at NIFAL parser to Material boundary; never pushed into shaders. See /audit-nifal.

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
