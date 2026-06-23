# FO4-2026-06-23-L02: dead take == 0 break in CsgArchive::read_psg

**Issue**: #1735
**Severity**: LOW
**Labels**: low, import-pipeline, bug
**Dimension**: 1 — M49 CSG
**Location**: `crates/bsa/src/csg.rs:207-213`
**Source audit**: AUDIT_FO4_2026-06-23 (FO4-2026-06-23-L02)

## Description
In `read_psg` (loop `while remaining > 0`), after the `local >= chunk.len()` guard returns early (`csg.rs:198-206`), `take = remaining.min(chunk.len() - local)` is always `>= 1` (both operands positive), so the trailing `if take == 0 { break; }` is unreachable dead code.

## Impact
None — correctness unaffected; redundant guard.

## Related
M49 CSG reader (`067adc34`).

## Suggested Fix
Remove the dead branch, or leave as defensive (cost is zero). Cosmetic.
