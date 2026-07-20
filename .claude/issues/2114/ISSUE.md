# #2114: D8-02: dhat geometry bound never exercises the packed-vertex allocation path it guards

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2114
**Labels**: bug, nif-parser, low, performance

---

**Severity**: low
**Dimension**: NIF Parse
**Location**: `crates/nif/tests/heap_allocation_bounds.rs:204-237,344-379`; guarded code `crates/nif/src/import/mesh/bs_tri_shape.rs:903-923` (`decode_bs_vertex_stream`)
**Status**: NEW

## Description
Both dhat geometry fixtures in `heap_allocation_bounds.rs` build 0-vertex `BSTriShape` blocks (`num_vertices = 0`, `data_size = 0`) — confirmed at `bs_tri_shape_block()`, which writes `w16(&mut d, 0); // num_vertices` and `w32(&mut d, 0); // data_size — 0 ⇒ no vertex/triangle loops`. This means the six `allocate_vec(nv)?` output vecs plus the de-interleave loop in `decode_bs_vertex_stream` — the actual site `#833`/`#831` were filed against — never execute under this guard. A regression that reverted the bulk `allocate_vec` path back to `Vec::new()` + per-element `push` growth would still pass both dhat gates, since the guarded loop body never runs on zero elements.

## Evidence
```rust
// heap_allocation_bounds.rs — fixture always writes 0 vertices
w16(&mut d, 0); // num_triangles (SSE bsver<130: u16)
w16(&mut d, 0); // num_vertices
w32(&mut d, 0); // data_size — 0 ⇒ no vertex/triangle loops
```

## Impact
The dhat allocation-bound test suite has a coverage gap for the exact code path (#833/#831 packed-vertex de-interleave) it was written to guard. A regression here would be silent (test passes, real content pays the reverted allocation pattern).

## Suggested Fix
Add a non-trivial `BSVertexDesc` fixture (~16 packed half-float vertices) alongside the existing 0-vertex fixture, and pin `max_bytes < ceil(1.3 × Σ output-vec bytes)` on it so the bulk-allocation path is actually exercised.

## Related
#833, #831 (the original packed-vertex allocation findings this test guards)

## Completeness Checks
- [ ] **TESTS**: New fixture exercises the non-zero-vertex path; existing 0-vertex fixture stays as an edge case, not the only case

