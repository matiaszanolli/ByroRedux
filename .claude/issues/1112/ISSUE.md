# Issue #1112 — TD3-202: 4096.0 exterior-cell-unit literal consolidation

**Source**: AUDIT_TECH_DEBT_2026-05-16 (Top 5 Medium #1)
**Severity**: MEDIUM
**Status**: CLOSED in 4358bb87

## Resolution

Promoted to `byroredux_core::math::coord::EXTERIOR_CELL_UNITS` + `cell_grid_to_world_yup` helper. 7 call sites migrated across 6 files. Resolves TD3-110 sign-divergence as side effect.

+2 tests pinning the constant and grid-origin formula.
