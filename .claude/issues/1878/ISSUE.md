**Severity**: LOW · **Dimension**: BSTriShape Packed Geometry · **Source**: `docs/audits/AUDIT_SKYRIM_2026-07-04.md` (SKY-D1-001)
**Status**: Regression of closed #1559 (SK-D1-AUDIT-02 "has_tangents gate diverges between inline and SSE-recon decoders") — the divergence is back, now in the opposite direction: the SSE path under-gates.
**Location**: `crates/nif/src/import/mesh/sse_recon.rs::decode_sse_packed_buffer` (the `has_tangents` binding + its use at the `Tangent` / `bitangent_z` read)

## Description
nif.xml `BSVertexDataSSE` has two distinct tangent-related predicates:
- `Bitangent X` gated on `(ARG & 0x11) == 0x11` (VF_VERTEX && VF_TANGENTS)
- `Tangent` + `Bitangent Z` quad gated on `(ARG & 0x18) == 0x18` (**VF_NORMALS && VF_TANGENTS**)

The inline decoder `decode_bs_vertex_stream` (`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs`) models both correctly. The SSE global-buffer decoder collapses both onto one boolean `has_tangents = vertex_attrs & VF_TANGENTS != 0` and reuses it for the tangent quad — correct for `bitangent_x`, wrong (missing the `&& VF_NORMALS` / 0x18 term) for the quad. #1559 removed the previously-correct `&& VF_NORMALS` from the SSE path while claiming to unify the two decoders.

## Evidence
SSE decoder single gate:
```rust
let has_tangents = vertex_attrs & VF_TANGENTS != 0;
if has_tangents { tangent_xyz = Some([...]); bitangent_z = Some(...); off += 4; }
```
Inline decoder two-predicate model:
```rust
if vertex_attrs & VF_TANGENTS != 0 && vertex_attrs & VF_NORMALS != 0 { /* 0x18 tangent quad */ }
```
nif.xml: `Tangent` / `Bitangent Z` both `cond="(#ARG# #BITAND# 0x18) == 0x18"`.

## Impact
**No live impact** — a descriptor with VF_TANGENTS set and VF_NORMALS clear is nonsensical (tangent space needs a normal) and does not occur in shipped Skyrim SE content, so both gates evaluate identically for every real body (this is why the SSE-recon chrome/magenta path is not regressed). The real risk is a **maintenance trap**: the #1559 comment asserts the inline decoder "gates on VF_TANGENTS alone" (it does not — it gates the quad on 0x18); a future maintainer trusting that comment could align the correct inline path to the wrong SSE gate and break the path that parses all 18862 Skyrim meshes at 100%. Also a latent stride hazard on synthetic/modded content: a same-stride misalignment would corrupt vertex colors / skin / eye-data silently (the `off > vertex_size` guard only catches spill past the declared stride).

## Suggested Fix
In `decode_sse_packed_buffer`, split the gate — keep `bitangent_x` on `VF_TANGENTS`, add `has_tangent_quad = has_tangents && vertex_attrs & VF_NORMALS != 0` for the `Tangent`/`bitangent_z` read, and correct the comment to cite the 0x18 predicate. Output is unchanged for all real content; this only fixes the byte-alignment gate. Add a regression test with a synthetic VF_TANGENTS-without-VF_NORMALS descriptor asserting the SSE and inline decoders consume identical byte counts.

## Related
Regression of #1559 · #796 (SSE tangent reconstruction) · #795 (inline tangent convention)

## Completeness Checks
- [ ] **SIBLING**: Confirm the inline `decode_bs_vertex_stream` gate stays on 0x18 (do not "align" it down to the SSE gate — fix the SSE side up)
- [ ] **CANONICAL-BOUNDARY**: n/a — parse-side only, no material_translate touch
- [ ] **TESTS**: A synthetic VF_TANGENTS-without-VF_NORMALS descriptor pins SSE≡inline byte-count parity
