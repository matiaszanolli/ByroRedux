# REN-D19-03: `synthesize_tangents`'s Z-up degenerate branch never got the #2632 orthogonalize-and-normalize fix its Y-up sibling did

- **Severity**: MEDIUM
- **Dimension**: 19 — Tangent-Space
- **Location**: `crates/nif/src/import/mesh/tangent.rs` — `synthesize_tangents`, the `if vec3_is_zero(&tangent_zup) || vec3_is_zero(&bitangent_zup)` arm. Fixed sibling with the identical predicate: `synthesize_tangents_yup`.
- **Status**: NEW
- **Description**: Both synthesis functions fall back to nifly's "permute the normal's components" trick when a vertex accumulates a zero ∂P/∂u or ∂P/∂v. A raw cyclic permutation of N is not generally orthogonal to N (and is exactly N when its components are equal), so #2632 added a Gram-Schmidt projection + normalize before the cross product — but only in `synthesize_tangents_yup`. The Z-up flavour still emits the raw permutation.
- **Evidence**: `synthesize_tangents` (Z-up): `let t_z = [n_zup.y, n_zup.z, n_zup.x]; let t_y = zup_to_yup_pos(t_z); let b_y = cross(n_yup, t_y);` — no `dot(n, t)` projection, no `normalize_inplace`. `synthesize_tangents_yup` (fixed): builds `t_y_raw`, subtracts `n_yup * dot_nt`, calls `normalize_inplace(&mut t_y)`, then crosses. The asymmetry is mirrored in the test suite: `synthesize_tangents_yup_degenerate_fallback_normalizes_and_orthogonalizes_against_n` exists in `crates/nif/src/import/mesh/tangent_convention_tests.rs`; there is no Z-up sibling of it.
- **Impact**: For a degenerate vertex whose normal is near (k,k,k) the stored `Vertex.tangent.xyz` is parallel to N. That value clears `perturbNormal`'s `dot(T,T) > 1e-4` Path-1 gate, and Path 1's un-guarded Gram-Schmidt then evaluates `normalize(vec3(0))` → NaN in the shaded normal. Trigger is narrow: the degenerate arm only fires for vertices whose adjacent triangles all have zero UV area, or that no triangle references. Reached by every Z-up producer — `ni_tri_shape.rs` (both `NiTriShapeData` and de-stripped `NiTriStripsData`) and `bs_tri_shape.rs`'s third tangent branch — i.e. Oblivion / FO3 / FNV interior content, the largest corpus in the project.
- **Related**: #2632 (the Y-up fix); REN-D19-04 (the shader-side guard that would contain it, filed separately if in scope); `bitangent_sign` / #1516 and `clamp_sign` / #2313 (both intact).
- **Suggested Fix**: Port the `synthesize_tangents_yup` degenerate arm verbatim into `synthesize_tangents` (project the permuted vector against `n_yup`, `normalize_inplace`, then cross), and add the Z-up sibling test to `tangent_convention_tests.rs`.

## Completeness Checks
- [ ] **SIBLING**: `synthesize_tangents`'s (Z-up) degenerate fallback ported to match `synthesize_tangents_yup`'s #2632 Gram-Schmidt + normalize treatment exactly
- [ ] **TESTS**: A Z-up sibling of `synthesize_tangents_yup_degenerate_fallback_normalizes_and_orthogonalizes_against_n` added to `tangent_convention_tests.rs`

---
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-12b.md` (finding `REN-D19-03`)
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2828
