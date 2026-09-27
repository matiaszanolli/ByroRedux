# EXT-D6-2026-09-27-07: Object-LOD mesh uploads carry no provenance — `.bto` and FO3/FNV `blocks\` meshes census as `Other`

**Issue**: #4936
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug,tech-debt

**Severity**: LOW (telemetry)
**Dimension**: Distant LOD and trees
**Tier Violated**: n/a
**Game Affected**: Skyrim, FO4, FO76, FO3, FNV
**Status**: NEW (incomplete part of `ff1b48d7c`)
**Location**:
`byroredux/src/cell_loader/object_lod.rs:457-466`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
`terrain_lod`, `terrain_lod_btr` and `placement_lod` tag `MeshUploadSource::Lod`; `object_lod`, the largest LOD family, does not.

## Suggested Fix
- Call `note_mesh_provenance(handle, MeshUploadSource::Lod, false, Some(path))`.
- Add a source-shape test covering every `cell_loader/*lod*.rs` upload.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **TESTS**: A regression test pins this specific fix
