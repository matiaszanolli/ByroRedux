# OB-D1-01: inline block-type-name inference keys off block_types.is_empty() rather than the version threshold

**Issue**: #4257 — https://github.com/matiaszanolli/ByroRedux/issues/4257
**Labels**: low,nif-parser,nif,game:oblivion,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 1 — NIF Version Handling
**Location**: `crates/nif/src/lib.rs:404`
**Status**: NEW

## Description
Inline block-type-name inference (pre-Gamebryo NetImmerse files, NIF v < 5.0.0.1) decides whether to read inline sized-string type names by checking `header.block_types.is_empty() && header.num_blocks > 0`, rather than checking the version threshold directly (the constant exists at `crates/nif/src/header.rs:231`).

## Evidence
`crates/nif/src/lib.rs:404`: `let inline_type_names = header.block_types.is_empty() && header.num_blocks > 0;` — a diagnostic/dispatch decision derived from an incidental table-emptiness signal instead of the version gate that actually determines this format difference.

## Impact
Unreachable on any shipping content today (every version-appropriate header either has a populated block-type table or is legitimately pre-5.0.0.1). Purely a diagnostic-quality nit: on hand-corrupted or fuzzed input where the table could be spuriously empty for an unrelated reason, this would misclassify the block layout.

## Related
None filed.

## Suggested Fix
Key `inline_type_names` off the same version constant `header.header.rs:231` uses, rather than off `block_types.is_empty()`, so the decision is not incidentally correct.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_OBLIVION_2026-09-11.md — findings verified against live code during this publish run.*
