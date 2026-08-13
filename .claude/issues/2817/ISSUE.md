# REN-D19-05: bs_tri_shape.rs 4th tangent branch guard is vacuous, feeds fabricated normals to tangent synthesis

Labels: low, nif-parser, bug

## Description

Guards on `!normals.is_empty()`, which is **vacuous** — `normals` is unconditionally populated earlier (`sse_normals`, else mapped `shape.normals`, else `vec![[0,1,0]; positions.len()]`), so the condition is equivalent to `!positions.is_empty()`, already tested. With no authored normals the branch hands the fabricated placeholder to `synthesize_tangents_yup`, producing a tangent basis derived from data that was never authored — exactly the defect #2363 fixed in `bs_geometry.rs` (guard changed to a separate `normals_authored` flag, pinned by `placeholder_normals_with_uvs_do_not_trigger_tangent_synthesis`); this sibling was not updated. `sse_recon.rs` has the same shape, so an SSE buffer with neither `VF_NORMALS` nor `VF_UVS` reaches it with *both* inputs fabricated.

## Location

`crates/nif/src/import/mesh/bs_tri_shape.rs` (the 4th tangent branch)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D19-05).

https://github.com/matiaszanolli/ByroRedux/issues/2817
