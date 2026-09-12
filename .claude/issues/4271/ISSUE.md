# SF-2026-09-11-D2-04: Stage A has none of the #2357 resolve logging Stage B got, and the #1232 tangent-synthesis positive arm has no end-to-end test

**Issue**: #4271 — https://github.com/matiaszanolli/ByroRedux/issues/4271
**Labels**: low,nif-parser,nif,test-gap,game:starfield,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 2 — BSGeometry Mesh Extraction
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs (Stage A); #2357 resolve logging; #1232 tangent synthesis`
**Status**: NEW

## Description
Two independent coverage gaps on the Stage A (inline) `BSGeometry` path: (1) it has none of the per-resolve diagnostic logging #2357 added to Stage B (`.mesh`-sourced), making Stage A failures harder to diagnose from a log; (2) #1232's tangent-synthesis positive arm (successfully synthesizing tangents when none are authored) has no end-to-end test covering Stage A specifically.

## Evidence
Verified during this audit by comparing Stage A and Stage B code paths and their respective test modules: #2357's resolve-logging calls are present only on the Stage B call sites; no Stage A test exercises a successful #1232 tangent-synthesis outcome end-to-end.

## Impact
Diagnostic/coverage gap only — no confirmed behavioral defect. A Stage A resolve failure is harder to triage from logs than the equivalent Stage B failure, and a regression in Stage A's tangent synthesis would not be caught by an existing test.

## Related
None filed. Adjacent to #2357 and #1232.

## Suggested Fix
Add the same per-resolve logging #2357 introduced for Stage B to the Stage A path, and add an end-to-end test exercising #1232's tangent-synthesis positive arm on Stage A input.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
