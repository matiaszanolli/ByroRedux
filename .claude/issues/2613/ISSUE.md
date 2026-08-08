# SF2D2-D2-01: Starfield skinned meshes render in bind pose - decoded skin weights never plumbed through

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2613
**Finding ID**: SF2D2-D2-01

**Severity**: HIGH
**Dimension**: 2 (BSGeometry Mesh Extraction), corroborated by Dimension 7
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:249-260` (call site — `mesh_data` is live but not passed), `crates/nif/src/import/mesh/skin.rs:233-275` (`extract_skin_bs_geometry`, canonical fix site), `crates/nif/src/blocks/bs_geometry.rs:283,479-495` (`BSGeometryMeshData.skin_weights`, fully decoded)
**Status**: NEW — #1827 (CLOSED) carries the same, now-incorrect, premise ("the packed BSGeometry vertex bone channel is not decoded") and sizes the remaining work as a separate milestone; the actual remaining work is a plumbing change, not a decode change

## Description
`extract_skin_bs_geometry` hardcodes `vertex_bone_indices: Vec::new(),
vertex_bone_weights: Vec::new()`. Its own comment ("the BSGeometry parser
doesn't surface them yet") is factually wrong:
`BSGeometryMeshData.skin_weights` is fully decoded at parse time via
`read_pod_vec::<BoneWeight>`, grouped by `weights_per_vert`, and
`BoneWeight { bone_index: u16, weight: u16 /* NORM/65535 */ }` already
indexes the same `BsSkinInstance.bone_refs` array `extract_skin_bs_geometry`
walks to build `ImportedSkin.bones`. A repo-wide grep shows zero consumers
of `skin_weights` outside the parser and its own unit test — the data is
decoded, unit-tested, and unused.

## Evidence
```rust
// crates/nif/src/import/mesh/skin.rs:39-40
vertex_bone_indices: Vec::new(),
vertex_bone_weights: Vec::new(),
```
Dimension 2 confirmed the gap at the source (the call site has `mesh_data`
in scope and simply doesn't pass it). Dimension 7 independently corroborated
with production-scale real-data evidence, tracing two real vanilla meshes
end-to-end through `import_nif_scene`: `naked_f.nif` (6,616 verts, 38 bones)
and `femalehead_facebones.nif` (15,370 verts, 50 bones) both report
`has_skin=true` with correctly resolved bones/bind matrices but
`vbi_len=0 vbw_len=0 vbw_nonzero=0` unconditionally regardless of vertex
count. The existing regression test
(`crates/nif/src/import/mesh/bs_geometry_skin_tests.rs:118-121`) *asserts
the empty arrays as correct*, citing a stale "#1203 deferred scope"
rationale that was already resolved by the time `skin_weights` decoding
landed — the gap has gone silent rather than failing loud.

## Impact
Every Starfield skinned mesh — all NPCs, all creatures, all skinned
armor/apparel, all FaceMeshes content (`Starfield - FaceMeshes.ba2` is
14.27% `BSGeometry` blocks) — renders in bind pose. `nif_loader.rs`'s
`.filter(|s| !s.vertex_bone_indices.is_empty() && ...)` silently drops every
vertex to the rigid path. This is a rendering-correctness defect on the
largest animated-content class in the game, confirmed on two independent
real production meshes, not a synthetic-fixture artifact.

## Suggested Fix
Change `extract_skin_bs_geometry`'s signature to accept `mesh_data`; when
`weights_per_vert > 0 && !skin_weights.is_empty()`, map each row to
`[u16; 4]` indices + `[f32; 4]` weights (top-4-by-weight when `> 4`,
zero-pad when `< 4`, `weight as f32 / 65535.0`, renormalize through the
existing `crates/nif/src/blocks/tri_shape/mod.rs::renormalize_skin_weights`
helper shared with FO4). Guard on `skin_weights.len() == vertices.len()`,
fall back to bind-pose on mismatch. Update the stale test at
`bs_geometry_skin_tests.rs:118-121` to assert the new non-empty behavior.

## Related
#1827 (CLOSED, stale premise) — needs a comment correction alongside the
fix; the FO4 `BsTriShape` path (already implements top-4-by-weight +
renormalize for the same contract — reuse, don't re-derive).

## Completeness Checks
- [ ] **SIBLING**: Reuse the FO4 `BsTriShape` top-4-by-weight/renormalize helper rather than re-deriving it
- [ ] **CANONICAL-BOUNDARY**: N/A — this is import-side mesh data, not the Material boundary
- [ ] **TESTS**: Update the stale `bs_geometry_skin_tests.rs:118-121` assertion; add a real-data-derived fixture (naked_f.nif-shaped) asserting non-empty weights
