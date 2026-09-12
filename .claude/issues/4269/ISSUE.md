# SF-2026-09-11-D2-02: #3777's remaining()==0 trailer gate is undecidable on the inline (Stage A) BSGeometry path, with zero test coverage

**Issue**: #4269 — https://github.com/matiaszanolli/ByroRedux/issues/4269
**Labels**: low,nif-parser,nif,test-gap,game:starfield,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 2 — BSGeometry Mesh Extraction
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs (Stage A inline path); #3777's trailer gate`
**Status**: NEW

## Description
#3777's `remaining() == 0` trailer gate (used to detect whether a `BSGeometry` block's data continues past the inline payload) is undecidable on the inline ("Stage A") code path — the cursor position it checks does not carry the information needed to distinguish the two cases on that path. None of #3777's three existing tests exercise Stage A, so this gap has no coverage either way.

## Evidence
Verified during this audit: the Stage A path's stream state does not carry a reliable `remaining()` signal at the point #3777's gate checks it, and grepping #3777's own test module shows all three tests exercise the out-of-line ("Stage B", `.mesh`-sourced) path only.

## Impact
No confirmed live defect (undecidable, not incorrect — the audit stopped short of proving a wrong answer), but an unguarded correctness gap: if Stage A ever needs this gate to behave correctly, there is currently no way to know whether it does, and no test would catch a regression either way.

## Related
None filed. Adjacent to #3777 (which added the gate for the out-of-line path).

## Suggested Fix
Determine what `remaining() == 0` should mean on the Stage A path (or explicitly document that the gate is Stage-B-only and how Stage A distinguishes the two cases instead), and add a Stage A test alongside #3777's existing three.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
