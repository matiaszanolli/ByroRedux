## NIFAL-S4: Collision shape radii and half-extents carry raw NIF binary floats with no is_finite() guard

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** nifal
**File:** `crates/nif/src/import/collision.rs:291`

## Recommended Fix

Add is_finite() guards at CollisionShape construction sites for radius, sphere_center, and dimensions elements. Return None on non-finite values so the trimesh fallback fires.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at NIFAL parser to Material boundary; never pushed into shaders. See /audit-nifal.

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
