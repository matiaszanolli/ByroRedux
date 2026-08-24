# 3276: SPT-D2-2026-08-24-01: two SpeedTreeWind-adjacent field docstrings still describe the CNAM-derived wind model 4e1afcbe deleted

**Severity**: LOW · **Report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-24.md` (SPT-D2-2026-08-24-01)

## Description

The fix for #3190 replaced the CNAM-derived wind computation with a hardcoded neutral constant (`(1.0, 0.0)`) because CNAM's field layout is unpinned, and correctly updated `TreeRecord::canopy_params`'s and `SpeedTreeWind`'s docs to say so. It missed two sibling docs one hop further down the same data path.

## Location

`crates/spt/src/import/mod.rs:70-77` (`SptImportParams::wind`), `byroredux/src/cell_loader/nif_import_registry.rs:156-157` (`CachedNifImport::speedtree_wind`)

## Evidence

Both still claim the two `f32`s are read from `TREE.CNAM`'s "first two finite values." Not true anywhere in the codebase — the sole production writer (`references/import.rs:328-332`) reads no `TreeRecord` field at all; it's a compile-time constant.

## Impact

Documentation-only; no behavioural effect today. A future contributor reading the field doc instead of the call site could reintroduce the No-Guessing violation #3190 fixed.

## Related

#3190 (the fix this documentation lagged), #3080 (sibling doc-rot in the same file).

## Suggested Fix

Update both docstrings to match `tree.rs`'s current wording: "Currently a neutral runtime constant (`(1.0, 0.0)`); `TreeRecord.canopy_params` (CNAM) is parsed-but-not-consumed until a citable field layout lands (#3190)."

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix
