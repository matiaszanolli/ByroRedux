# SF-2026-09-11-D3-05: stale line-citation in starfield_mat.rs fixture doc points at probe_header's body instead of the index_chunks arithmetic it actually justifies

**Issue**: #4276 — https://github.com/matiaszanolli/ByroRedux/issues/4276
**Labels**: low,import-pipeline,doc-rot,game:starfield,legacy-compat,documentation

**Severity**: LOW
**Dimension**: Dimension 3 — CDB Material Database Correctness
**Location**: `crates/sfmaterial/src/starfield_mat.rs (or sibling fixture module) — doc comment line-citation`
**Status**: NEW

## Description
A fixture doc comment in the CDB test module cites a line range in `probe_header`'s body to justify a piece of test-fixture arithmetic, but the arithmetic it is actually justifying belongs to `index_chunks`'s chunk-count/reservation logic, not `probe_header` itself (`probe_header` is a thin wrapper that calls `index_chunks`).

## Evidence
Verified during this audit by reading the cited line range against the actual code at that location.

## Impact
Documentation-only — a future reader following the citation to understand the fixture's arithmetic lands in the wrong function.

## Related
None filed.

## Suggested Fix
Update the citation to point at the correct `index_chunks` line range.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
