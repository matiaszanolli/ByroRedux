# SF-D6-03: Starfield shader test fixtures are tautological, mirror parser field order, could never catch SF-D6-01

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2624
**Finding ID**: SF-D6-03

**Severity**: MEDIUM
**Dimension**: 6 (NIF Shader Blocks, BSVER 155+)
**Location**: `crates/nif/src/blocks/shader_tests/mod.rs:414-449` (`build_starfield_bs_lighting_minimal`), consumed by `crates/nif/src/blocks/shader_tests/starfield.rs:16,50,82`
**Status**: NEW

## Description
The fixture builder mirrors `parse_fo76_plus` line for line, comments
included (`// NO BSShaderType155 (FO76 == 155 only)`, `// root_material
(>= 130)`) — it emits nothing where the real stream carries `shader_type`
and emits a `root_material` word the real stream doesn't carry. Every test
built on it (`parse_bs_lighting_starfield_captures_trailing_tail`,
`..._tail_empty_without_size_or_drift`, `..._minimal_omits_fo76_only_tail`)
therefore asserts "the parser reads what the parser writes" — field *order*
is unfalsifiable by construction, and all three pass today against a parser
that mis-decodes 100% of real Starfield blocks.

## Evidence
`shader_tests/mod.rs:414-449` — the fixture builder's field order and
inline comments directly mirror `parse_fo76_plus`'s (buggy) field order
rather than the real on-disk layout.

## Impact
The regression guard is real for `NiUnknown` count but hollow for
field-level correctness; two prior audits and three fixes (#1510, #1606,
#1881) shipped on top of it.

## Suggested Fix
Add one fixture captured verbatim from retail data (the 166-byte block 6 of
`shiplandingmarker_lod_3.nif` is ideal — constant across the LOD corpus) and
assert semantic invariants: `sf1_crcs == [VERTEX_COLORS]`,
`texture_set_ref.is_null()`, `emissive_color == [1.0,1.0,1.0]`,
`uv_scale == [1.0,1.0]`, all-finite emissive. Any one of these would have
caught SF-D6-01.

## Related
SF-D6-01; the same tautological-fixture pattern does not apply to
`build_starfield_bs_effect_minimal`.

## Completeness Checks
- [ ] **TESTS**: This finding IS the test-fixture fix — add the real-data-derived fixture described above
