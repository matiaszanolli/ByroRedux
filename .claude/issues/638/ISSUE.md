# M29-1: SSE BSTriShape per-vertex skin data dropped when geometry lives in SseSkinGlobalBuffer

**Severity:** LOW (dormant until NPC spawning lands)
**Labels:** bug, nif-parser
**Surfaced by:** M29 e2e verification on `meshes\actors\character\character assets\malebody_0.nif`

## Problem
SSE BSTriShape blocks whose geometry lives in `SseSkinGlobalBuffer`
(bsver ∈ [100, 130)) silently drop per-vertex bone indices/weights at
import. Every vertex imports with `bone_weights = [0, 0, 0, 0]`, hits
the `wsum < 0.001` rigid fallback in `triangle.vert:151`, and the
mesh renders in bind pose.

Root cause: `decode_sse_packed_buffer` skips the 12-byte VF_SKINNED
payload (`mesh.rs:598-605`); `extract_skin_bs_tri_shape:803` reads
`shape.bone_weights.clone()` which is empty in this path.

## Fix (audit-prescribed)
- When `decode_sse_packed_buffer` sees `VF_SKINNED`, decode the 12-byte
  skin payload (4× half-float weights + 4× u8 indices) into the returned
  `DecodedPackedBuffer`.
- In `extract_skin_bs_tri_shape`, when `shape.bone_weights.is_empty()`,
  fall back to the decoded global-buffer payload via the partition's
  `vertex_map`.

## SIBLING
FO4 BSSkinInstance + packed vertex buffer follows the same gating?

## TESTS
`cargo test -p byroredux --test skinning_e2e -- --ignored vertex_indices_within_palette_bounds_sse` reproduces. Hard-fail flip after fix.
