# PERF-D3-NEW-03: memory-budget.md links the BGSM cache section to a path deleted by the asset_provider module split

**Issue**: #1807
**Labels**: low,documentation
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D3-NEW-03)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D3-NEW-03)

## Location
`docs/engine/memory-budget.md:149`

## Description
The doc links `byroredux/src/asset_provider.rs`, which no longer exists — the module split into `byroredux/src/asset_provider/{mod,archive,material,script,texture,tests}.rs`. All documented values remain correct; pure link rot.

## Evidence
`memory-budget.md:149` `[byroredux/src/asset_provider.rs](../../byroredux/src/asset_provider.rs)`; the file is now a directory (`byroredux/src/asset_provider/`), with the BGSM cache logic living in `material.rs`.

## Impact
Doc-only.

## Related
Session 34/35 module-split notes.

## Suggested Fix
Point the link at `byroredux/src/asset_provider/material.rs`, and sweep the doc's other relative links.

## Completeness Checks
- [ ] **SIBLING**: Other doc cross-references checked for the same rot

