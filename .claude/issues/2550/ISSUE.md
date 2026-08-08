# FO3-D5-NEW-02: HkSubPartData inside hkPackedNiTriStripsData is skip(12)-ed on the FO3 branch

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2550
**Finding ID**: FO3-D5-NEW-02

**Severity**: LOW
**Dimension**: FO3 Collision Import (Havok → CollisionShape)
**Location**: `crates/nif/src/blocks/collision/shape_mesh.rs:196-198`
**Status**: NEW

## Description
`HkSubPartData` inside `hkPackedNiTriStripsData` is `skip(12)`-ed on the FO3 branch. 3,232 packed meshes carry more than one Havok material per mesh; the per-sub-part material assignment is skipped rather than decoded, unrecoverable without a parser change.

## Evidence
Confirmed directly: `shape_mesh.rs:196-198` — `let num_sub_shapes = stream.read_u16_le()? as usize; for _ in 0..num_sub_shapes { stream.skip(12)?; // HkSubPartData: filter(4) + numVerts(4) + material(4) }`.

## Impact
Per-triangle Havok material (surface-sound/impact-effect classification) is lost for any packed mesh with more than one Havok material — the shape still resolves and collides correctly (geometry is intact), only the material-per-sub-part assignment is unrecoverable. Affects 3,232 FO3 packed meshes.

## Suggested Fix
Decode the 12 bytes as `HkSubPartData { filter: u32, num_verts: u32, material: u32 }` instead of skipping, and thread the per-sub-part material array through to wherever collision-surface material lookups happen (if any consumer exists yet; otherwise capture it for a future consumer per the NIFAL "capture now, consume later" convention).

## Completeness Checks
- [ ] **TESTS**: A regression test decodes a real multi-material packed mesh and confirms per-sub-part materials are captured
