# LC-D7-01: Oblivion/FO3/FNV distant-object LOD (_far.nif placement scheme) unimplemented

**Issue**: #1726
**Source audit**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-06-23.md`
**Severity**: MEDIUM · **Labels**: medium, legacy-compat, bug
**Dimension**: 7 (subsystem coverage) / 5 (EXAL LOD)
**Location**: `byroredux/src/cell_loader/object_lod.rs` (Skyrim/FO4 `.bto`-only); no `PlacementLodProvider` for the `DistantLOD\<W>_<x>_<y>.lod` → `_far.nif` scheme

## Description

Distant object LOD is implemented only for Skyrim/FO4 (baked `.bto` quad atlases). The older-game scheme — `DistantLOD\<W>_<x>_<y>.lod` placement files instancing per-object `_far.nif` low-poly meshes — is unimplemented (`exal.md` §5). Distant terrain for these games is covered by heightmap synthesis fallback, so this is objects-only.

## Evidence

`object_lod.rs` `stream_object_lod_blocks` only walks `.bto` blocks. No grep hit for `_far.nif`/`DistantLOD` reading in non-comment source — the only `DistantLOD` hits are NIF shader-property block names (`DistantLODShaderProperty`), unrelated. `exal.md` lists the older-game source as `DistantLOD\<W>_<x>_<y>.lod` → `_far.nif`, unimplemented.

## Impact

Oblivion / FO3 / FNV exteriors render no distant object LOD — horizon beyond the loaded radius is missing buildings/rocks/landmarks. Terrain horizon present; object silhouettes not. Content gap on FNV (reference title) + three of six target games. No parse failure.

## Related

LC-D7-02 (VWD flag); `exal.md` §5.4.

## Suggested Fix

Add a `PlacementLodProvider` parsing `DistantLOD\<W>_<x>_<y>.lod` entries and instancing the corresponding `_far.nif`, spawned as `IsLodTerrain`-style LOD entities like the `.bto` path.
