# Issue #484

FNV-1-L1: num_decals v20.2.0.5 boundary lacks pinpoint unit test

---

## Severity: Low (test coverage)

**Location**: `crates/nif/src/blocks/properties.rs:327-331`

## Problem

The #400/#429 decal-slot off-by-one fix uses:
- `saturating_sub(8)` for v20.2.0.5+
- `saturating_sub(6)` for older

Existing tests at `:759` and `:1013` cover `count ≤ threshold` and 2-decal cases, but not the boundary:
- `count == 8 → num_decals == 0` (v20.2.0.5+)
- `count == 9 → num_decals == 1` (v20.2.0.5+)
- `count == 6 → num_decals == 0` (Oblivion)
- `count == 7 → num_decals == 1` (Oblivion)

## Impact

A future rewrite could silently reintroduce the off-by-one without any test failure.

## Fix

Add four boundary tests mirroring the existing pattern in `properties.rs`. ~40 lines.

## Completeness Checks

- [ ] **TESTS**: All four boundary cases added and passing
- [ ] **SIBLING**: Check for other saturating_sub boundary conditions lacking tests

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-1-L1)
