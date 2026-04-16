# N2-01: BsTriShape vertex_desc missing VF_UVS_2 and VF_LAND_DATA flags

## Finding: N2-01 (LOW)

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-15.md`
**Dimension**: NIF Format Readiness
**Games Affected**: FO4, FO76, Starfield
**Location**: `crates/nif/src/blocks/tri_shape.rs:267-276`

## Description

BSVertexDesc flag constants define bits 0,1,3,4,5,6,8,10 but omit:
- **bit 2** (VF_UVS_2 = 0x004) — second UV set
- **bit 7** (VF_LAND_DATA = 0x080) — landscape heightmap vertex data

The trailing skip guard (`consumed < vertex_size_bytes` at ~line 402) prevents parse corruption — unrecognized data is skipped as padding. However, second UV coordinates and landscape vertex data are silently discarded.

## Evidence

No constant or decode branch for 0x004 or 0x080 in tri_shape.rs.

## Impact

Meshes with dual UV sets (detail maps, lightmaps) lose their second UV channel. Landscape BSTriShape meshes (FO4+ terrain) lose per-vertex land blend data. No parse failure — data loss only.

## Suggested Fix

Add `VF_UVS_2 = 0x004` and `VF_LAND_DATA = 0x080` constants. Read second UV into `uv_sets_2` field. Read land data into a dedicated field for terrain pipeline.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-15.md`._
