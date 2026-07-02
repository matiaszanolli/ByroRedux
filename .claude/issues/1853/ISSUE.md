# FNV-D1-01: Stale doc comment claims FO3/FNV worldspace water default is unimplemented

**Source audit**: `docs/audits/AUDIT_FNV_2026-07-02.md` (finding FNV-D1-01)
**GitHub issue**: https://github.com/matiaszanolli/ByroRedux/issues/1853
**Labels**: low, import-pipeline, legacy-compat, documentation

**Severity**: LOW
**Dimension**: Cell Loading
**Location**: `byroredux/src/cell_loader/exterior.rs:41-50`
**Status**: NEW

## Description

The `default_water_height` field doc says FO3/FNV/Skyrim+ "are excluded pending DNAM parsing." That state no longer exists: the WRLD parser now reads DNAM's second f32 into `record.default_water_height` (`crates/plugin/src/esm/cell/wrld.rs:131-138`, regression-tested by `wrld_dnam_captures_default_water_height`), and `default_water_for_worldspace` (`byroredux/src/env_translate.rs`) consumes it for all non-Oblivion games. FNV coastal/sea cells without XCLW DO inherit the worldspace default water height today.

## Evidence

Doc (stale) `exterior.rs:48-49`; parser (live) `wrld.rs:131` reads DNAM `[4..8]` into `default_water_height`; consumer (live) `env_translate.rs::default_water_for_worldspace` matches on `w.default_water_height`.

## Impact

Misleads maintainers into thinking FNV exterior water inheritance is a gap when it is shipped + tested. No runtime effect.

## Suggested Fix

Rewrite the field doc to state FO3/FNV/Skyrim+ resolve `default_water_height` from the DNAM second f32; drop the obsolete "excluded pending parsing" clause.

## Completeness Checks
- [ ] **TESTS**: No new test needed — `wrld_dnam_captures_default_water_height` and `non_oblivion_uses_dnam_default_water_height` already cover the live behavior this doc should describe
