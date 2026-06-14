# Renderer Audit — 2026-06-14

**Scope**: Focused single-dimension run — `--focus 16` (Tangent-Space & Normal
Maps, M-NORMALS). Depth: **deep** (data-flow trace + numeric invariant
validation). Other 22 renderer dimensions were not run this session.

**Orchestrator verification**: the one reported finding was independently
re-derived by the orchestrator (scalar-triple-product antisymmetry traced
through both inline paths against the pinned canonical convention) before
inclusion — it is not a pattern-match.

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 1 |
| LOW      | 0 |

One NEW correctness finding: an **inverted bitangent-sign convention** on the
two inline packed-vertex tangent paths (FO4 / Skyrim SE / FO76 BSTriShape and
SSE skin-reconstruction). The sign disagrees with the authored-blob and
synthesized paths and is unpinned by any test. Seven of the eight checklist
items verified correct; the DBG_* catalog "drift" is checklist staleness, not a
code bug (code/pins pass 15/15).

## Rasterization / Normal-Mapping Assessment

The tangent-space pipeline is in good shape overall: the authored-blob decoder
honors Bethesda's CalcTangentSpace ∂P/∂V↔∂P/∂U swap (#786), the synthesized
fallback produces unit-length Gram-Schmidt tangents pinned by
`tangent_convention_tests.rs`, Z-up→Y-up conversion is applied to T, B and N in
lockstep, `perturbNormal` is correctly default-on with the `DBG_BYPASS_NORMAL_MAP`
opt-out wired, and the DBG_* bit catalog is generated from a single Rust
source-of-truth with both positive/negative pin tests passing.

The single gap is that the **inline** packed-vertex sign formula was never
brought under the same pin as the synthesized path, and it carries a swapped
scalar-triple-product operand order that negates the result. Because it affects
the primary geometry path for three games and silently disagrees with a sibling
path on identical content, it is the priority fix.

## RT Pipeline Assessment

Not in scope this run (Dimensions 8–10, 20 not executed).

---

## Findings

### REN-D16-01: BSTriShape inline + SSE-reconstruction tangent paths use an inverted bitangent-sign convention
- **Severity**: MEDIUM
- **Dimension**: Tangent-Space & Normal Maps (M-NORMALS)
- **Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:1001-1011` and `crates/nif/src/import/mesh/sse_recon.rs:351-372`
- **Status**: NEW
- **Description**:
  The renderer fixes one global convention: `Vertex.tangent.xyz = ∂P/∂U`, and the
  shader reconstructs `B = vertexTangent.w * cross(N, T)` (`triangle.frag:1225`).
  For `B` to land on the true `∂P/∂V`, the authored `w` must equal
  `sign(dot(∂P/∂V, cross(N, ∂P/∂U)))`. That is exactly the formula used by
  `extract_tangents_from_extra_data` (`tangent.rs:101-108`) and `synthesize_tangents`,
  and it is the convention pinned by `tangent_convention_tests.rs` (textbook
  right-handed winding ⇒ sign = +1).

  The two inline packed-vertex paths compute the triple product with the two
  operand vectors **swapped**: they take `cross(N, t_xyz)` where `t_xyz` is the
  on-disk tangent triplet = `∂P/∂V`, then dot it with `[bx,by,bz]` = the
  bitangent triplet = `∂P/∂U` (the value stored in `tangent.xyz`). Since the
  scalar triple product is antisymmetric under swapping two of its three vectors
  (`det[∂P/∂V, N, ∂P/∂U] = -det[∂P/∂U, N, ∂P/∂V]`), the inline paths emit the
  **opposite** sign for the same geometry. The comments at `bs_tri_shape.rs:1005`
  and `sse_recon.rs:353` claim parity with `extract_tangents_from_extra_data`,
  but T and B are interchanged relative to that function.
- **Evidence**:
  Numeric reproduction on the textbook RH fixture (N=+Z, ∂P/∂U=+X, ∂P/∂V=+Y) —
  the same case `tangent_convention_tests.rs` pins at +1:
  - authored/synthesized `dot(∂P/∂V, cross(N, ∂P/∂U))` = +1.0 → sign **+1** (B reconstructs to +∂P/∂V, correct)
  - inline `dot(∂P/∂U, cross(N, ∂P/∂V))` = −1.0 → sign **−1** (B reconstructs to −∂P/∂V, inverted)

  Independently re-derived by the orchestrator: `cross(N, ∂P/∂V) = −∂P/∂U` for a
  right-handed orthonormal frame, so `dot(∂P/∂U, −∂P/∂U) < 0`. No unit test
  exercises the inline/SSE sign — the convention tests only call
  `synthesize_tangents{,_yup}`, so the inversion has never been pinned. The
  inline `w` reaches the shader untouched (`bs_tangents_zup_to_yup` preserves
  `t[3]`); both inline and SSE arrays land non-empty in `Vertex.tangent`, so the
  shader takes its Path-1 branch (`dot(vertexTangent.xyz) > 1e-4`,
  `triangle.frag:1217`).
- **Impact**:
  The reconstructed bitangent B is negated for every BSTriShape mesh shipping
  inline `VF_TANGENTS` (the common case for Skyrim SE / FO4 / FO76) and every
  SSE skin-reconstructed body/creature. The tangent-space normal map's V (green)
  channel is read with flipped handedness → normal-map detail leans the wrong way
  along V. Subtle on flat/symmetric detail; on directional carved normals it
  reads as a consistent "inside-out groove" / mis-lit look. It also makes
  handedness **disagree between sibling paths**: a mesh with baked inline tangents
  renders inverted while an otherwise-identical mesh that falls through to
  `synthesize_tangents` renders correctly. Blast radius: all Skyrim+/FO4/FO76
  BSTriShape + SSE-reconstructed geometry. (Rated MEDIUM — consistent global
  flip on a subtle channel, not corruption/crash — but arguably HIGH for the
  affected games given it is their primary geometry path.)
- **Suggested Fix**:
  Swap the operands so the inline/SSE formula matches the pinned convention —
  compute `sign(dot(t_xyz /*∂P/∂V*/, cross(N, [bx,by,bz] /*∂P/∂U*/)))`, or
  equivalently negate the existing `dot_b_cross`. Then add an inline-path case to
  `tangent_convention_tests.rs` (RH fixture ⇒ +1) so both packed paths are pinned
  to the same canonical sign as `synthesize_tangents`.

---

## Prioritized Fix Order

1. **REN-D16-01** (MEDIUM, correctness) — negate/swap the inline + SSE
   bitangent-sign operands and add a pin test. Single-site logic fix on each of
   two functions plus one new test; no API or struct change.

## Verification Log (no-finding items)

All confirmed against current code; none produced a finding:

1. Authored-blob path (`extract_tangents_from_extra_data`) — honors the
   CalcTangentSpace ∂P/∂V↔∂P/∂U swap and the #786 handedness fix; size-mismatch
   guard warns and skips. **Correct.**
2. FO4+ inline decode gating — keyed on the vertex-descriptor `VF_TANGENTS`/
   `VF_NORMALS` flags, not BSVER, so it fires for FO4/FO76/SSE alike (the sign
   bug is orthogonal to gating). **Gating correct.**
3. Synthesized fallback — unit-length Gram-Schmidt tangents, degenerate
   fallback, canonical sign; pinned by 4 tests. **Correct.**
5. Z-up→Y-up conversion applied to T, B and the sign-N in lockstep; no path
   converts N without T. **Correct.**
6. `perturbNormal` default-on; `DBG_BYPASS_NORMAL_MAP = 0x10` opt-out wired
   (`triangle.frag:1493`). **Correct.**
7. DBG_* catalog — Rust source-of-truth ↔ generated GLSL header in lockstep for
   all bits; both positive/negative pin tests iterate the shared catalog;
   `cargo test -p byroredux-renderer shader_constants` → 15/15 pass. The
   checklist's documented `0x200` ceiling is stale (catalog has grown to
   `0x1000`); the **checklist** is the stale party, matching open doc issue
   #1501/REN2-16. **Code correct.**
8. "Chrome posterized walls" red herring — not used to manufacture a finding;
   REN-D16-01 rests on a numeric proof, not a visual artifact.

### Dedup notes
- #1104 (Path-2 screen-space UV-mirror handedness): FIXED-VERIFIED per
  AUDIT_RENDERER_2026-05-26_DIM16 (`38ba5506`). Not re-raised.
- #1086 / #1232 (BSGeometry UDEC3 / `synthesize_tangents_yup` fallback): FIXED.
  Not re-raised.
- #1501 / REN2-16 (DBG bit doc-count drift): pre-existing OPEN doc issue; the
  code/pins are correct, so no new finding.
- REN-D16-01 is NEW: prior audits (AUDIT_FO4_2026-05-18, AUDIT_NIF_2026-05-12,
  AUDIT_FO4_2026-05-15) assert the inline #795/#796 path is "intact/correct" but
  none verified the sign formula's parity with the authored convention; the
  inversion has no test and no prior issue.
