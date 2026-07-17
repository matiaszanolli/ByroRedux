# TD1-010: nif/import/material/walker.rs — extract_material_info_from_refs is a 1008-line function (91% of the file)

**GitHub Issue**: #2059
**Labels**: low,nif-parser,tech-debt,bug

**Severity**: LOW
**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `crates/nif/src/import/material/walker.rs:103-1110` (`extract_material_info_from_refs`)

## Description
The NIFAL material-translation single-sink boundary is essentially one function. Any per-game material quirk fix touches this whole function.

## Evidence
Confirmed live: `crates/nif/src/import/material/walker.rs` is 1110 LOC total; `pub(crate) fn extract_material_info_from_refs(` starts at line 103, matching the report's claimed location — 1008/1110 ≈ 91% of the file, matching the report's stated proportion.

## Related
#1454/#1455 (closed BGSM field-forwarding fixes touched this exact path).

## Suggested Fix
Split by property-source axis: shader-property / texturing-property / alpha-property extraction / BGSM-BGEM merge, feeding one small aggregator.

**Effort**: medium

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: This function feeds the NIFAL material single-sink boundary (`translate_material` / `Material::resolve_pbr`) — the split must keep all per-game logic inside this parser-side function, never push any of it into the boundary or downstream shader/renderer code
- [ ] **SIBLING**: #1454/#1455 both touched this exact function — verify the split doesn't reintroduce either bug
- [ ] **TESTS**: A regression test pins that the property-source split produces byte-identical `MaterialInfo` for the existing BGSM/BGEM fixture set
