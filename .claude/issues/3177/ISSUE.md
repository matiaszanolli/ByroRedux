# NIF-D4-2026-08-20-04: synthesize_tangents (Z-up) never normalizes N while its Y-up sibling does — and it is the producer receiving quantized BSTriShape normbyte normals

Issue: https://github.com/matiaszanolli/ByroRedux/issues/3177
Finding: NIF-D4-2026-08-20-04
Labels: low,nif-parser,nif,bug
Source: docs/audits/AUDIT_NIF_2026-08-20.md

Filed from `docs/audits/AUDIT_NIF_2026-08-20.md` (Dimension 4 — Geometry Extraction & Import Handoff).

**Severity**: LOW
**Game Affected**: Skyrim SE / FO4 / FO76 `BSTriShape` meshes that set `VF_NORMALS | VF_UVS` but not `VF_TANGENTS`. Oblivion / FO3 / FNV `NiTriShape` normals are authored `f32` and are unit in practice, so the legacy path is unaffected in practice.

**Location**: `crates/nif/src/import/mesh/tangent.rs:261-262` (Z-up, no normalize) vs `:462-463` (Y-up, explicit normalize). Caller supplying the quantized input: `crates/nif/src/import/mesh/bs_tri_shape.rs:199-204`.

## Description

#2632 added an explicit per-vertex normalize to the Y-up producer with a comment stating the reason plainly:

```rust
// tangent.rs:454-463
// #2632 / SF2D2-D2-04 — `normals_yup` is unit-length only to
// quantization for a UDEC3-decoded source …; the Gram-Schmidt
// projection below (and the degenerate branch's permutation +
// cross product) is only correct for `|n| == 1`.
let mut n_yup = normals_yup[i];
normalize_inplace(&mut n_yup);
```

The Z-up producer does no such thing — it converts the raw normal and uses it directly:

```rust
// tangent.rs:261-262
let n_zup = normals_zup[i];
let n_yup = byroredux_core::math::coord::zup_to_yup_pos([n_zup.x, n_zup.y, n_zup.z]);
```

and then runs the same `T - N*(N.T)` projection at `:305-311` and `:317-323`. The stated precondition (`|n| == 1`) is not established on this path.

## Evidence

The Z-up producer's third caller is `crates/nif/src/import/mesh/bs_tri_shape.rs:199`, which passes `shape.normals` — decoded as three independent `byte_to_normal` reads with **no vector renormalization** (`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:1194-1205`). That is the same class of quantized, non-unit input #2632 fixed for UDEC3, arriving at the sibling that was left unpatched. #2632's own scope note ("Starfield BSGeometry … SSE-reconstructed BSTriShape") explains why: the Z-up caller was not in view.

## Impact

Small and systematic rather than catastrophic. With `|N|^2 = 1 + eps`, the Gram-Schmidt step over-subtracts by `eps*(N.T)*N`, so the emitted tangent is not exactly orthogonal to the shading normal and the derived bitangent sign can flip on near-perpendicular cases. Normbyte quantization keeps `eps` under about ±1.5%, so the angular error is sub-degree — visible, if at all, as slight normal-map shading drift on Skyrim+/FO4 meshes shipping no authored tangents. No corruption.

## Suggested Fix

Mirror `:462-463` — bind `n_yup` mutably and `normalize_inplace` it once per vertex before the branch, and carry the same #2632 comment so the two producers document one shared precondition. Cheap (one `sqrt` per vertex on a cold path) and a no-op for already-unit input.

## Related

- #2632 (CLOSED) fixed exactly this on the Y-up half.
- Sibling of the degenerate-tangent zero-seed finding filed alongside this one (same function pair, same divergence between the two producers). Land them together.

## Completeness Checks
- [ ] **SIBLING**: both producers end up with one identical, commented precondition (`|n| == 1` established locally, not assumed of callers)
- [ ] **TESTS**: a `BSTriShape`-shaped fixture with deliberately non-unit normbyte normals asserts the emitted tangent is orthogonal to the normalized normal within tolerance
- [ ] **PERF**: the added `sqrt` stays on the import path only (not a per-frame cost)
