# #599: FO4-DIM4-04: scol_parts field on StaticObject documented but never added

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/599
**Labels**: documentation, low, legacy-compat, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 4)
**Severity**: LOW
**Location**: `crates/plugin/src/esm/records/scol.rs:29`

## Description

`records/scol.rs:29` points readers at "the `scol_parts` field on `crate::esm::cell::StaticObject`." `StaticObject` (`cell.rs:330-347`) has six fields — `form_id`, `editor_id`, `model_path`, `light_data`, `addon_data`, `has_script` — none of which is `scol_parts`. The actual SCOL body lives in the separate `EsmCellIndex.scols` map (`cell.rs:426`).

## Evidence

`rg -n "scol_parts|scol_record" crates/plugin/` returns only the doc line at `records/scol.rs:29`.

## Impact

A reader following the doc will grep for nothing. Minor; only trips future contributors.

## Suggested Fix

Change `records/scol.rs:29` from:
> See the `scol_parts` field on `crate::esm::cell::StaticObject`.

to:
> See the `scols` map on `crate::esm::cell::EsmCellIndex` — keyed by SCOL FormID, one `ScolRecord` per entry.

## Completeness Checks

- [ ] **TESTS**: n/a (doc-only change)

## Related

- FO4-DIM4-01 (SCOL runtime expander) — fixing that will make this doc correct by producing the expected behavior.
