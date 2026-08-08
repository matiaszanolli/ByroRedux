# SK-D1-03: packed-vertex parser ignores all ten BSVertexDesc offset nibbles; the VF_UVS_2 trailing-skip rationale is asserted without evidence

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2578
**Finding ID**: SK-D1-03

**Severity**: LOW
**Dimension**: BSTriShape Packed Geometry + SSE Skinned Reconstruction
**Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:205-237,951-1097`; same shape in `sse_recon.rs:276-436`
**Status**: NEW (adjacent to #336, CLOSED)

## Description
`decode_bs_vertex_stream` walks a fixed field order and never consults the ten 4-bit offset nibbles `BSVertexDesc` publishes. The `VF_UVS_2` doc comment claims the reserved bytes are absorbed by "the trailing skip" — per nif.xml the `UV2 Offset` nibble sits *between* `UV1 Offset` and `Normal Offset`, implying UV2 is mid-vertex, in which case every attribute after UV1 would misalign. No sample exists to confirm either reading.

## Evidence
Corpus scan of 81,226 SSE `BSTriShape` blocks: `UVS_2 = 0`, `LAND_DATA = 0`, `INSTANCE = 0` occurrences — no vanilla Skyrim SE content exercises this path, and the fixed-order walk is provably correct for all 22 descriptors that do occur.

## Impact
Zero on vanilla Skyrim SE. On a mod-authored mesh setting bit 2, the whole post-UV1 attribute set would silently misalign with no diagnostic.

## Related
#336, #358, #359

## Suggested Fix
Soften the comment to state the assumption, not the conclusion; add a cheap post-walk offset comparison + `log::warn!` on mismatch (silent on 100% of vanilla content today, so free insurance).

## Completeness Checks
- [ ] **TESTS**: N/A for vanilla content; the suggested post-walk comparison itself would act as a runtime regression guard
