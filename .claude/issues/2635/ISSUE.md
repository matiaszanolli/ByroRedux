# SF-D3-07: Value::Ref wraps referent unlike Gibbed reference's direct inner-value return, undocumented

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2635
**Finding ID**: SF-D3-07

**Severity**: LOW
**Dimension**: 3 (CDB Material Database)
**Location**: `crates/sfmaterial/src/value.rs` (`Value::Ref`)
**Status**: NEW

## Description
`Value::Ref` wraps its referent in `Value::Ref { type_ref, inner }`;
Gibbed's `ReadPrimitiveRef` returns the inner value directly. This is a
strict superset (no data loss), but a future Phase 2 walker ported from the
Gibbed reference will miss one unwrap level if it isn't documented.

## Evidence
`crates/sfmaterial/src/value.rs`'s `Value::Ref` variant retains
`type_ref` alongside `inner`, where Gibbed's reference returns just the
inner value.

## Impact
None today — purely a porting trap for the upcoming #2359/#1289 Phase 2
walker if it's ported line-for-line from Gibbed without accounting for
this extra wrap level.

## Suggested Fix
Add a one-sentence doc comment on `Value::Ref` noting the extra unwrap
level relative to Gibbed's `ReadPrimitiveRef`.

## Related
SF-D3-06 (same dimension, sibling doc-precision gap).

## Completeness Checks
- [ ] **TESTS**: N/A — doc-only change
