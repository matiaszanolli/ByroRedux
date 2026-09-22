# PERF-D7-2026-09-21-02: The #4206 tangent pre-size has no regression guard, although its commit says it added heap-bound coverage

**Labels**: bug, nif-parser, low, performance, nif, test-gap

Filed via /audit-publish from docs/audits/AUDIT_PERFORMANCE_2026-09-21.md.

**Severity**: LOW (missing regression guard on a closed fix) · **Dimension**: 7 — Streaming & Parse
**Location**:
- `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:1145-1157`: the #4206 tangent pre-size in `decode_bs_vertex_stream`
- Fixture: `crates/nif/tests/heap_allocation_bounds.rs:245-297`, `bs_tri_shape_block_with_vertices`. It still sets only `VF_VERTEX` and says "no `VF_TANGENTS`".

**Status**: NEW. This is a test gap on closed #4206. The fix itself is in place, so it is not a regression.
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

- `ab8779a52`'s message says the fix "gives the path the heap-bound coverage the dhat fixture skipped (VF_TANGENTS deliberately omitted there)".
- The commit touches only `bs_tri_shape.rs` (+13/−1). It adds no fixture, unit test or pin, and `grep -rn 4206 crates/nif/` finds only the code comment.
- The dhat fixture never sets `VF_TANGENTS | VF_NORMALS`, so the tangent branch never runs under a heap bound. A revert to `Vec::new()` stays green, and the dhat suite passes either way.
- #4206's own suggested fix asked for exactly this fixture: "Extend the dhat fixture with `VF_TANGENTS` set so a future revert trips the existing gate". PERF-D8-2026-09-11-01 asked for the same.

## Impact

The fix has no guard. A silent revert would bring back log2(n) realloc+copy cycles per mesh block on every normal-mapped Skyrim SE+/FO4+/Starfield mesh.

## Related

- #4206 (closed): the fix.
- #2114 and #3673 (closed): the dhat fixture's earlier coverage gaps.

## Suggested Fix

Add a `VF_TANGENTS | VF_NORMALS` variant of `bs_tri_shape_block_with_vertices` to the dhat bound. Alternatively, add a unit test asserting `tangents.capacity() == num_vertices` after decoding a stream that carries tangents.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D7-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: The `sse_recon.rs` (#559) sibling decoder's conditional tangent pre-size gets the same guard
- [ ] **TESTS**: The new fixture/test fails when the pre-size is reverted to `Vec::new()` (mutation-check, then revert)

