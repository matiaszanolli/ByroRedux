# SF-2026-09-11-D3-01: duplicate CDB class name_offset silently last-wins via HashMap::insert, where the reference implementation hard-fails

**Issue**: #4272 — https://github.com/matiaszanolli/ByroRedux/issues/4272
**Labels**: medium,import-pipeline,game:starfield,legacy-compat,bug

**Severity**: MEDIUM
**Dimension**: Dimension 3 — CDB Material Database Correctness
**Location**: `crates/sfmaterial/src/reader.rs:85 (class_by_name_offset.insert)`
**Status**: NEW

## Description
The CDB class index (`class_by_name_offset: HashMap<i32, usize>`) is built with plain `HashMap::insert` at `crates/sfmaterial/src/reader.rs:85`, so a duplicate `name_offset` across two `CLAS` chunks silently last-wins (the earlier class definition is discarded with no diagnostic). The reference Gibbed.Starfield C# implementation hard-fails on this condition. This is the unfixed sibling of #2633, which fixed the same class of defect for CDB fields.

## Evidence
`crates/sfmaterial/src/reader.rs:85`: `state.class_by_name_offset.insert(class.name_offset, idx);` — plain insert, return value (the previous entry, if any) discarded.

## Impact
Not reachable on vanilla retail Starfield CDB content (measured clean during this audit), but on a malformed or hostile CDB file, this silently drops a class definition instead of erroring, which is a correctness risk for the CDB Phase 2 (#3398) reader this audit's finding is adjacent to — any Phase-2 reader inherits this gap if unfixed.

## Related
Sibling of the already-fixed #2633 (same last-wins-vs-hard-fail defect, for CDB fields rather than class names). Adjacent to #3398 (CDB Phase 2, the tracker this hardens).

## Suggested Fix
Detect the duplicate at insert time (check the previous entry) and return an error (matching the reference implementation's hard-fail), consistent with how #2633 fixed the equivalent field-name case.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
