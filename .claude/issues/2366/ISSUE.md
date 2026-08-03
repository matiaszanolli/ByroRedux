# SF-D9-2026-08-03-02: BgemFile::effect_pbr_specular still has zero consumers after #1358 closed

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2366
**Labels**: bug,import-pipeline,low,legacy-compat

---

**Severity**: LOW
**Dimension**: 9 — BGSM/BGEM External Material Flow (Starfield audit, 2026-08-03)
**Location**: `crates/bgsm/src/bgem.rs:76,169-171`; merge arm at `byroredux/src/asset_provider/material.rs:1097-1241`
**Status**: Residual of #1358 (CLOSED)

## Description

#1358 named three BGEM scalars; two landed (`base_color`, `soft_depth`). The third, `effect_pbr_specular` (BGEM `version >= 20`), is parsed and never read — the BGEM merge arm never sets `material.is_pbr`, so a `version >= 20` BGEM opting into PBR specular still shades on whatever NIF-derived `is_pbr` was.

## Evidence

Confirmed: `grep -rn "effect_pbr_specular"` across the workspace returns only `bgem.rs:76` (field decl), `:170` (parse), and a `:231` test comment — zero consumers in `byroredux/src/asset_provider/material.rs` or anywhere else.

## Impact

Small and bounded — a missed opt-in, not a wrong-write. #1358 is not fully closed as titled (its own scope named all three scalars).

## Suggested Fix

Forward `effect_pbr_specular` into `material.is_pbr` in the BGEM merge arm, mirroring #1352's BGSM policy for the equivalent scalar.

## Completeness Checks
- [ ] **SIBLING**: Confirm BGSM's equivalent (#1352) landed correctly as the reference pattern to mirror
- [ ] **TESTS**: A regression test pins a `version >= 20` BGEM with `effect_pbr_specular = true` setting `material.is_pbr = true` after the fix
