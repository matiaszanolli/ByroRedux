**Severity**: MEDIUM
**Dimension**: Tangent-Space & Normal Maps (M-NORMALS)
**Status**: NEW / CONFIRMED
**Source**: `docs/audits/AUDIT_RENDERER_2026-06-14.md` (REN-D16-01)
**Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:1001-1011` and `crates/nif/src/import/mesh/sse_recon.rs:351-372`

## Description

The renderer fixes one global convention: `Vertex.tangent.xyz = ∂P/∂U`, and the shader reconstructs `B = vertexTangent.w * cross(N, T)` (`triangle.frag:1225`). For `B` to land on the true `∂P/∂V`, the authored `w` must equal `sign(dot(∂P/∂V, cross(N, ∂P/∂U)))`. That is exactly the formula used by `extract_tangents_from_extra_data` (`tangent.rs:101-108`) and `synthesize_tangents`, and it is the convention pinned by `tangent_convention_tests.rs` (textbook right-handed winding ⇒ sign = +1).

The two **inline** packed-vertex paths compute the triple product with the two operand vectors **swapped**: they take `cross(N, t_xyz)` where `t_xyz` is the on-disk tangent triplet = `∂P/∂V`, then dot it with `[bx,by,bz]` = the bitangent triplet = `∂P/∂U` (the value stored in `tangent.xyz`):

```rust
// bs_tri_shape.rs:1006-1010  (identical shape in sse_recon.rs:365-369)
let cnx = n[1]*t_xyz[2] - n[2]*t_xyz[1]; ...   // cross(N, ∂P/∂V)
let dot_b_cross = bx*cnx + by*cny + bz*cnz;     // dot(∂P/∂U, cross(N, ∂P/∂V))
let sign = if dot_b_cross >= 0.0 { 1.0 } else { -1.0 };
```

Since the scalar triple product is antisymmetric under swapping two of its three vectors (`det[∂P/∂V, N, ∂P/∂U] = -det[∂P/∂U, N, ∂P/∂V]`), the inline paths emit the **opposite** sign from the authored/synthesized paths for the same geometry. The comments at `bs_tri_shape.rs:1005` / `sse_recon.rs:353` claim parity with `extract_tangents_from_extra_data`, but T and B are interchanged relative to that function.

## Evidence

Numeric reproduction on the textbook RH fixture (N=+Z, ∂P/∂U=+X, ∂P/∂V=+Y) — the same case `tangent_convention_tests.rs` pins at +1:
- authored/synthesized `dot(∂P/∂V, cross(N, ∂P/∂U))` = +1.0 → sign **+1** (B reconstructs to +∂P/∂V, correct)
- inline `dot(∂P/∂U, cross(N, ∂P/∂V))` = −1.0 → sign **−1** (B reconstructs to −∂P/∂V, inverted)

Re-derived: `cross(N, ∂P/∂V) = −∂P/∂U` for a right-handed orthonormal frame, so `dot(∂P/∂U, −∂P/∂U) < 0`. No unit test exercises the inline/SSE sign — the convention tests only call `synthesize_tangents{,_yup}`, so the inversion has never been pinned. The inline `w` reaches the shader untouched (`bs_tangents_zup_to_yup` preserves `t[3]`); both arrays land non-empty in `Vertex.tangent`, so the shader takes its Path-1 branch (`triangle.frag:1217`).

## Impact

The reconstructed bitangent B is negated for every BSTriShape mesh shipping inline `VF_TANGENTS` (common case for **Skyrim SE / FO4 / FO76**) and every SSE skin-reconstructed body/creature. The tangent-space normal map's V (green) channel is read with flipped handedness → detail leans the wrong way along V; reads as a consistent "inside-out groove" on directional carved normals. It also makes handedness **disagree between sibling paths**: a mesh with baked inline tangents renders inverted while an identical mesh that falls through to `synthesize_tangents` renders correctly. Blast radius: all Skyrim+/FO4/FO76 BSTriShape + SSE-reconstructed geometry. (Rated MEDIUM — consistent global flip on a subtle channel, not corruption — but arguably HIGH for the affected games given it is their primary geometry path.)

## Suggested Fix

Swap the operands so the inline/SSE formula matches the pinned convention — compute `sign(dot(t_xyz /*∂P/∂V*/, cross(N, [bx,by,bz] /*∂P/∂U*/)))`, or equivalently negate the existing `dot_b_cross`. Then add an inline-path case to `tangent_convention_tests.rs` (RH fixture ⇒ +1) so both packed paths are pinned to the same canonical sign as `synthesize_tangents`.

## Completeness Checks
- [ ] **SIBLING**: Both inline sites fixed (`bs_tri_shape.rs` AND `sse_recon.rs`); grep for any other in-tree bitangent-sign computation to confirm no third copy drifts.
- [ ] **CANONICAL-BOUNDARY**: Fix stays in the NIF import/tangent-extraction layer — convention must match `extract_tangents_from_extra_data` / `synthesize_tangents`; do not introduce a per-game branch downstream.
- [ ] **TESTS**: Add an inline + SSE case to `tangent_convention_tests.rs` (RH fixture ⇒ +1, mirrored-UV ⇒ −1) so both packed paths are pinned alongside the synthesized path.
- [ ] **VISUAL**: Spot-check a Skyrim SE / FO4 mesh with directional carved normals before/after to confirm the green-channel handedness flips as expected.
