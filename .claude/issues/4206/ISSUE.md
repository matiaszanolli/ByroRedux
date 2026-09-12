# PERF-D8-2026-09-11-01: `decode_bs_vertex_stream`'s tangent array grows by repeated push instead of the pre-sized allocation its sibling decoder already uses

Labels: medium,performance,nif-parser,nif,bug

**Description**: `decode_bs_vertex_stream` — the shared packed-vertex decoder for essentially all Skyrim SE+/FO4+/Starfield static and skinned geometry — pre-sizes `vertices`/`uvs`/`normals`/`vertex_colors` unconditionally and `bone_weights`/`bone_indices` conditionally on `is_skinned` (both correct). `tangents` is left `Vec::new()` regardless of `vertex_attrs`, even though whether it will be pushed on every iteration is exactly as knowable in advance (`VF_VERTEX && VF_TANGENTS && VF_NORMALS`, fixed for the whole call) as the skinning case. The sibling SSE-reconstruction decoder for the same on-disk format (`sse_recon.rs`, #559) already applies this exact conditional pre-size for `tangents` — the fix is known and landed elsewhere but wasn't carried to the more heavily-hit inline `BsTriShape::parse` path.

**Evidence**:
`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:1138` (`let mut tangents: Vec<[f32; 4]> = Vec::new();`, confirmed never resized) vs `sse_recon.rs:341-345` (`if has_tangent_quad { Vec::with_capacity(num_vertices) } else { Vec::new() }`). Existing dhat coverage (`heap_allocation_bounds.rs`'s `bs_tri_shape_block_with_vertices` fixture) explicitly omits `VF_TANGENTS` per its own comment, so this path has never been under a heap-allocation bound.

**Impact**: For every normal-mapped Skyrim SE+/FO4+/Starfield mesh — the dominant case, since `VF_TANGENTS` is set pervasively wherever a mesh authors a normal map — `tangents` grows via default doubling-capacity reallocation instead of one reservation: `log2(num_vertices)` extra realloc+copy cycles per mesh block. Bounded (amortized O(1), not O(n^2)) but exactly the regression class #3691 fixed elsewhere in the same file family.

**Related**: #3691 (skin buffer pre-sizing, sibling fix); #2114/D8-02 (the dhat fixture with the coverage gap this finding identifies).

**Suggested Fix**: Hoist the `has_tangent_quad` check before the loop and pre-size via `stream.allocate_vec(nv_u32)?`, mirroring the `is_skinned` gate and `sse_recon.rs` verbatim. Extend the dhat fixture with `VF_TANGENTS` set so a future revert trips the existing gate.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
