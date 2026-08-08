# SK-D1-02: remap_bs_tri_shape_bone_indices's single-partition identity shortcut binds the wrong bone on 59 vanilla Skyrim SE shapes

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2577
**Finding ID**: SK-D1-02

**Severity**: MEDIUM
**Dimension**: BSTriShape Packed Geometry + SSE Skinned Reconstruction
**Location**: `crates/nif/src/import/mesh/skin.rs:338-343`
**Status**: NEW

## Description
The remapper short-circuits to an identity widen whenever `NiSkinPartition` has one partition, on the premise that a single partition's `bones` palette is always identity. Measurably false: 16,737 single-partition SSE skins, 14,195 with a non-identity palette (mostly benign trailing pad). Restricting to in-range, non-zero-weight vertices whose palette entry differs from the slot index: **7,740 vertices across 59 shapes** resolve to a different bone under the shortcut than under the palette lookup (e.g. `facegeom\skyrim.esm\00067667.nif`, palette `[0,1,3,4,5,6]`, local slot 2 → global bone 3, shortcut yields 2). A separate malformed-input class also surfaced (`armor\hide\m\1stpersoncuirassmedium_0.nif`, out-of-range local slot on both paths).

## Evidence
Confirmed directly: `skin.rs:338-343` — `if partition.partitions.len() <= 1 { return identity_remap(); }`.

## Impact
Localised tearing/stretching on ~0.15% of single-partition skinned vertices. Currently **masked** for the FaceGen subset by SK-D1-01 (this session, no weights reach the GPU at all) — fixing SK-D1-01 without fixing this makes the artifact newly visible on head meshes.

## Related
#613 (introduced the shortcut); SK-D1-01 (this session — masks this bug for FaceGen today).

## Suggested Fix
Delete the `<= 1` short-circuit and always resolve through `remap_one` (already degrades to identity when the palette is identity); or gate the fast path on `part.bones.iter().enumerate().all(|(i,&b)| b as usize == i)` instead of partition count.

## Completeness Checks
- [ ] **SIBLING**: Fix should land before or alongside SK-D1-01 (this session) since fixing D1-01 alone makes this newly visible
- [ ] **TESTS**: A regression test using `facegeom\skyrim.esm\00067667.nif`'s palette shape confirms correct bone resolution
