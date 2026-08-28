# Issue #3510: precombine shared geometry is uploaded once per placed instance — up to 4.6x VRAM and one BLAS per instance on a single _oc.nif

**Filed**: 2026-08-27 · **Source**: `docs/audits/AUDIT_FO4_2026-08-27.md`

- **Severity**: MEDIUM
- **Dimension**: 1 (M49 precombined geometry — spawn)
- **Location**: `byroredux/src/cell_loader/precombined.rs:762-771` (`build_precombine_meshes`, fn at `:697`); cache-key consumer at `byroredux/src/cell_loader/spawn/mesh_instance.rs:429-443`; cache-key definition at `crates/renderer/src/mesh.rs:229-235` (`MeshCacheKey`)
- **Source**: `docs/audits/AUDIT_FO4_2026-08-27.md` — finding `FO4-2026-08-27-D1-01`

## Description

`build_precombine_meshes` decodes each `BSPackedGeomObject`'s shared geometry **once** and then materialises one full `ImportedMesh` per `BSPackedGeomDataCombined` instance transform:

```rust
for inst in &geom.instances {
    let mesh = decoded
        .clone()
        .into_imported_mesh(&inst.transform, geom.material.clone());
    meshes.push(mesh);
}
```

`decoded.clone()` deep-copies `positions` / `normals` / `tangents` / `uvs` / `colors` / `indices`; `into_imported_mesh` then differs only in `translation` / `rotation` / `scale` (`crates/nif/src/import/precombine.rs:66-85`). Downstream, the GPU-mesh dedup layer keys on `MeshCacheKey = (String, u32)` — *lowercased model path* plus *sub-mesh index* (`crates/renderer/src/mesh.rs:229-235`) — and `prepare_mesh_uploads` uses the mesh's position in the flattened `Vec` (`mesh_instance.rs:429`, `for (sub_mesh_index, mesh) in imported.iter().enumerate()`).

Each instance therefore claims a **distinct** cache slot: distinct `GpuMesh` (vertex + index buffers), distinct BLAS build, and — because the draw batcher batches on mesh handle — no instanced draw merging either. Nothing on the path recognises that N of these meshes are byte-identical geometry differing only by transform, which is precisely the case `MeshCacheKey` exists to collapse for ordinary REFR placements (`#879`).

## Evidence

Measured over a 4 000-file sample of the 120 387 `meshes\precombined\*_oc.nif` entries in `Fallout4 - MeshesExtra.ba2`, resolving every object against `Fallout4 - Geometry.csg`, sizing at the engine's 104-byte `Vertex` + `u32` index:

```
nifs 4 000, objects 41 574, instances 67 585        (1.63 instances / object)
unique geometry 1 904.1 MiB, uploaded-as-placed 2 617.2 MiB   (x1.37 overall)
max instances on one object: 146
instances-per-object histogram (16 = 16+):
  {1: 30160, 2: 6654, 3: 2173, 4: 1131, 5: 397, 6: 299, 7: 168,
   8: 165, 9: 93, 10: 53, 11: 35, 12: 52, 13: 30, 14: 21, 15: 18, 16+: 125}
worst single _oc.nif: meshes\precombined\0004b2c0_00120a99_oc.nif
  -> 2 257 meshes, unique 13.6 MiB, uploaded 62.3 MiB   (x4.6)
```

The distribution matters as much as the mean: 72% of objects are placed once and cost nothing extra, but the tail is where the cells are — a single precombine tile can carry 2 257 mesh entities and 2 257 BLAS builds where ~500 distinct geometries would do.

## Impact

Three compounding costs on FO4 cell load, all in the tail cells (dense settlements, the Institute, Diamond City) rather than uniformly: VRAM for duplicated vertex/index buffers (up to 4.6× on the measured worst tile), redundant BLAS builds against the `AccelerationManager`'s reserve floors and eviction thresholds, and a lost instanced-draw batch (N draws where 1 instanced draw would serve). It is a performance/VRAM defect, not a correctness one — the geometry rendered is right.

## Related

- `#879` / CELL-PERF-01 — established the refcounted `(path, sub_mesh_index)` dedup this path bypasses
- `a30c088a` — the LOD-tier selection just above, which is correct (one tier per object, verified: `precombined.rs:719-726` still takes `max_by_key(|&(c, _)| c)` over the three `lod_counts`)

## Suggested Fix

Give the precombine path a cache key that separates the *object* from its *placement* — e.g. key the shared geometry as `(oc_nif_path, object_ordinal)` and hand every instance of that object the same handle, letting the transform live only on the spawned entity's `Transform`. That collapses the uploads and the BLASes, and lets the existing draw batcher instance them. `PrecombineGeometry` would need to be uploaded once and placed N times rather than cloned N times, i.e. the split belongs between `build_precombine_meshes` and `spawn_placed_instances`, not inside the mesh registry.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawn paths that flatten a multi-mesh import into `sub_mesh_index` cache slots)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
