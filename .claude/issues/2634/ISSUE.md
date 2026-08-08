# SF-D3-06: StringTable::get doc comment contradicts its own correct offset-0 behaviour

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2634
**Finding ID**: SF-D3-06

**Severity**: LOW
**Dimension**: 3 (CDB Material Database)
**Location**: `crates/sfmaterial/src/string_table.rs:26-28`
**Status**: NEW

## Description
`StringTable::get`'s doc comment contradicts its own (correct) offset-0
behaviour. The comment claims empty string at `offset == 0`; the code
correctly reads the NUL-terminated string *at* offset 0 (matching Gibbed),
and a synthetic fixture (`reader.rs:813`) depends on that being right. The
comment describes a behaviour that would break class-name resolution if
"fixed."

## Evidence
`crates/sfmaterial/src/string_table.rs:26-28` doc comment vs. actual
implementation and its own test fixture at `reader.rs:813`.

## Impact
Doc-only — misleading comment could cause a future "fix" that breaks
class-name resolution.

## Suggested Fix
Delete the incorrect doc clause.

## Related
SF-D3-07 (same dimension, a sibling doc-precision gap on `Value::Ref`).

## Completeness Checks
- [ ] **TESTS**: N/A — doc-only change
