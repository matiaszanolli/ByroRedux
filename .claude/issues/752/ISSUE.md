# #752: SF-D5-01 / SF-D4-01: BSGeometry parses but no importer reads it — every Starfield mesh extracts zero geometry

URL: https://github.com/matiaszanolli/ByroRedux/issues/752
Labels: bug, nif-parser, import-pipeline, high, legacy-compat

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 5 SF-D5-01, Dim 4 SF-D4-01, Dim 6 SF-D6-01 — single root cause)
**Severity**: HIGH
**Status**: NEW (parser side closed by **#708**; this is the unfiled importer-side follow-up)

## Description

`BSGeometry` has a parser at `crates/nif/src/blocks/bs_geometry.rs` (dispatch in `crates/nif/src/blocks/mod.rs:308`) but **zero** import-side handling. The mesh importer in `crates/nif/src/import/mesh.rs` covers only `NiTriShape`, `NiTriStripsData`, `BSTriShape`, `BSDynamicTriShape`. The "100% / 31058" parse rate on `Meshes01.ba2` is fake-100% from the renderer's perspective: every Starfield mesh silently produces zero triangles.

```bash
$ grep -rn "BSGeometry" crates/nif/src/import/
(no matches)
```

## Evidence

5 representative meshes traced through `import_nif_scene` via `crates/nif/examples/d5_starfield_import.rs` against `Starfield - Meshes01.ba2`:

| Sample | NIF | Blocks | BSGeometry | ImportedMesh |
|--------|-----|--------|-----------|--------------|
| Clutter | `meshes\setdressing\exotic_clutter\exoticplayingcard_heart_q.nif` | 16 | 4 | **0** |
| Ship hull | `meshes\ships\modules\hab\smod\smod_hab_hope_3l2w1h.nif` | 97 | 31 | **0** |
| Character | `meshes\actors\bipeda\characterassets\skeleton.nif` | 25 | 0 | **0** |
| Weapon | `meshes\weapons\maelstrom\6.nif` | 5 | 1 | **0** |
| Landscape rock | `meshes\landscape\rocks\rough\rockroughboulder02.nif` | 7 | 1 | **0** |

37 `BSGeometry` blocks across the 5 samples, zero converted to `ImportedMesh`.

## Vertex format that needs to land (already parsed correctly)

From `bs_geometry.rs:250-313` — the parser produces `BSGeometryMeshData` with these channels:

| Channel | Wire format | Decode | vs FO4 BSTriShape |
|---|---|---|---|
| Position | 3× i16 NORM × `havok_scale 69.969` | per-axis NORM | new (was f16/f32) |
| UV0/UV1 | 2× f16 each | `half_to_f32` | dual UV is new |
| Color | RGBA u8 | direct | same |
| Normal | u32 UDEC3 (10:10:10:2) | `unpack_udec3_xyzw`, ignore W | new packing |
| Tangent | u32 UDEC3, W = bitangent sign | `unpack_udec3_xyzw` | new packing + new sign encoding |
| Skin weights | variable count `(u16 bone, u16 weight NORM)` | `weight / 65535.0` | was fixed 4× |
| Indices | u16 only | direct | FO4 used u32 |
| LOD lists | `Vec<Vec<[u16; 3]>>` per LOD | direct | new |
| Meshlets | DX12 `(vert_count, vert_offset, prim_count, prim_offset)` × n | direct | new |
| Cull data | `(center: vec3, expand: vec3)` per meshlet | direct | new |

## Compounding factor

Vanilla Starfield separates the actual vertex/index data into external `geometries/<sha1>/<sha1>.mesh` files inside the same BA2 (per `bs_geometry.rs:204-209` docstring, `FLAG_INTERNAL_GEOM_DATA = 0x200` is never set on vanilla). Even with importer wiring, vanilla content needs the external `.mesh` parser too. Tracked separately as SF-D4-02.

## Suggested Fix (two-stage rollout)

**Stage A (small, this issue)**: When `BSGeometry::has_internal_geom_data()` returns true, emit an `ImportedMesh` from the inline `BSGeometryMeshData` payload (positions, half-float UVs, UDEC3 normals/tangents, indices). Material falls back to checkerboard handle 0 — no `.mat` resolve. Lands inline-geom debug NIFs.

**Stage B (medium, SF-D4-02)**: When `has_internal_geom_data()` is false, resolve `BSGeometryMesh::mesh_name` against the open Materials/Meshes BA2 chain and decode the external `.mesh` file. Reference: nifly `MeshFile.cpp / MeshFile.hpp`.

Success criterion for this issue: `cargo run -- some_starfield_debug_mesh.nif` produces visible geometry (untextured) instead of an empty scene.

Test: extend `crates/nif/tests/parse_real_nifs.rs` with a `render_bsgeometry_inline` integration test that asserts ≥1 `ImportedMesh` emerges for an internal-geom-data NIF.

## Completeness Checks

- [ ] **SIBLING**: After landing, verify `walk_node_lights` / `walk_node_particle_emitters_flat` traverse `BSGeometry` parents correctly (cross-ref #718).
- [ ] **TESTS**: New regression test for inline-geom path; gate on `BYROREDUX_STARFIELD_DATA`.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.

## Related

- Closed #708 (BSGeometry parser dispatch — completed the parser side; this is the importer follow-up)
- #709 (Starfield SkinAttach — paired with BSGeometry for skinned meshes)
- #726 (Starfield BoneTranslations — paired with BSGeometry for skinned meshes)
- SF-D4-02 (external .mesh decoder — Stage B)
