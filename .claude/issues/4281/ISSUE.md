# SF-2026-09-11-D6-03: parse_fo76_plus keeps a third inline copy of the CRC-array head that #3845's consolidation does not cover, while a neighboring comment claims full coverage

**Issue**: #4281 — https://github.com/matiaszanolli/ByroRedux/issues/4281
**Labels**: low,nif-parser,nif,tech-debt,doc-rot,game:starfield,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 6 — NIF Shader Blocks, BSVER 155+
**Location**: `crates/nif/src/blocks/shader.rs (parse_fo76_plus, CRC-array head)`
**Status**: NEW

## Description
`parse_fo76_plus` keeps its own inline copy of the CRC-array head (the same leading structure #3845's consolidation was meant to unify across call sites), which #3845 does not actually cover — leaving a third, independent copy of logic that should be shared. A neighboring comment claims full coverage by #3845. The copy itself is behaviorally correct (no functional defect); the comment describing it as consolidated is stale.

## Evidence
Verified during this audit by reading `parse_fo76_plus` alongside #3845's actual consolidated call sites and confirming this one is not among them, despite the adjacent comment's claim.

## Impact
No behavioral defect today — the inline copy is correct. Tech-debt / doc-rot risk: a future change to the shared CRC-array-head logic would need to remember this third, uncovered copy exists, and the stale comment actively hides that it does.

## Related
Adjacent to #3845 (the consolidation this copy was missed by).

## Suggested Fix
Fold `parse_fo76_plus`'s inline copy into #3845's shared helper, and correct the stale comment in the meantime if the consolidation is deferred.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
