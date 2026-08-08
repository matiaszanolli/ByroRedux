# SF-D3-05: duplicate field names silently last-wins in CDB reader where Gibbed reference hard-fails

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2633
**Finding ID**: SF-D3-05

**Severity**: LOW
**Dimension**: 3 (CDB Material Database)
**Location**: `crates/sfmaterial/src/reader.rs:444,453,473,493`
**Status**: NEW

## Description
Duplicate field names silently last-wins where the reference hard-fails.
Field values accumulate via `BTreeMap::insert` (silent overwrite); Gibbed
uses `Dictionary.Add` (throws on duplicate key). A `CLAS` declaring the
same field name twice, or a `DIFF` naming the same field index twice,
silently keeps the second value — worst-case outcome for a Phase 2 material
index (silent wrong value, not a parse error).

## Evidence
`crates/sfmaterial/src/reader.rs:444,453,473,493` all use
`BTreeMap::insert` without checking for a pre-existing key.

## Impact
Silent wrong value on malformed/duplicate-field CDB content, worst-case
outcome for the upcoming Phase 2 material index (#2359/#1289).

## Suggested Fix
`debug_assert!(insert(...).is_none())` or a real `Err`.

## Completeness Checks
- [ ] **TESTS**: A duplicate-field-name fixture asserts the new error/assert behavior
