# Issue #879 (OPEN): CELL-PERF-01: per-REFR-placement re-uploads the same cached mesh as a fresh GPU buffer

URL: https://github.com/matiaszanolli/ByroRedux/issues/879

---

## Description

`spawn_placed_instances` (`byroredux/src/cell_loader.rs:1968-2029`) re-uploads each `ImportedMesh` as a fresh GPU vertex+index buffer pair for every REFR placement, even when those placements share the same `Arc<CachedNifImport>` (#381).

The cache today saves only the **parse** + **import** work; GPU buffers are not deduplicated. Each `MeshRegistry::upload_scene_mesh` call internally issues two synchronous `with_one_time_commands` fence-waits (`crates/renderer/src/vulkan/buffer.rs:854-859` — vertex buffer + index buffer). For 40 chairs sharing one `chair.nif`, that's 80 synchronous `wait_for_fences(.., u64::MAX)` calls on the main thread — not 2.

This is the dominant cell-load wall-clock cost.

## Evidence

```rust
// cell_loader.rs:1967-2029 — inside spawn_placed_instances, called PER REFR
let mut blas_specs: Vec<(u32, u32, u32)> = Vec::new();
for mesh in imported {                       // imported = &cached.meshes (Arc<CachedNifImport>)
    let num_verts = mesh.positions.len();
    let vertices: Vec<Vertex> = (0..num_verts).map(|i| { /* ... */ }).collect();
    // ...
    let mesh_handle = ctx.mesh_registry.upload_scene_mesh(   // ← fresh GPU upload per placement
        &ctx.device, alloc, &ctx.graphics_queue, ctx.transfer_pool,
        &vertices, &mesh.indices, ctx.device_caps.ray_query_supported, None,
    ).unwrap();
    blas_specs.push((mesh_handle, num_verts as u32, mesh.indices.len() as u32));
    // ...
}
```

```rust
// crates/renderer/src/mesh.rs:140-159 — upload_scene_mesh body
let vertex_buffer = GpuBuffer::create_vertex_buffer(/* ... */)?;  // fence-wait #1
let index_buffer  = GpuBuffer::create_index_buffer(/* ... */)?;   // fence-wait #2
```

```rust
// crates/renderer/src/vulkan/texture.rs:802 — what every fence-wait pays
device.wait_for_fences(&[fence], true, u64::MAX)
    .context("wait for one-time commands")?;
```

Megaton (929 REFRs, hundreds of mesh-bearing) → ~2× num_unique_placements synchronous fence-waits per cell load.

## Why it matters

The R1 instance SSBO architecture (112 B per instance, post-#797/#807) already supports N instances per shared mesh — the bottleneck is purely the upload stage. The whole point of caching `ImportedMesh` is to share the *vertex stream* across placements; the cache currently shares CPU bytes only.

Compounds with **CELL-PERF-02** (#NPC-spawn re-parses) and **CELL-PERF-03** (#texture-upload-budget). Three cell-load critical-path issues stacked.

## Proposed Fix

Add a second cache layer keyed on `(cache_key, sub_mesh_index)` → `GpuMeshHandle`:

1. First placement uploads + populates the cache entry, refcount = 1
2. Subsequent placements look up the existing handle, increment refcount, skip the upload
3. `unload_cell` decrements refcount; drop the entry when refcount → 0

Mirrors the `TextureRegistry::acquire_by_path` refcounted pattern (#524).

The instance SSBO already encodes per-instance transforms (R1 architecture, `GpuInstance` 112 B) — the GPU draw path supports N instances per shared mesh today.

## Cost Estimate

For Megaton (929 REFRs, ~hundreds of mesh-bearing): currently ~2× num_unique_placements fence-waits per cell. With dedup + refcount: 2× num_unique_meshes fence-waits per cell. For 40 chairs → 1 chair upload, not 40.

## Completeness Checks

- [ ] **UNSAFE**: New cache hash construction is safe (path string → u64 hash). The unsafe ash calls are unchanged.
- [ ] **SIBLING**: Confirm the same dedup applies to NPC body/skeleton uploads (couples with **CELL-PERF-02**)
- [ ] **DROP**: GPU buffers must be dropped exactly when refcount → 0; verify no leak on cell unload, no UAF if a placement holds a stale handle. `MeshRegistry::Drop` must continue to free everything.
- [ ] **LOCK_ORDER**: New cache likely lives on `MeshRegistry` (already a `Resource`); preserve existing lock-order constraints
- [ ] **FFI**: N/A
- [ ] **TESTS**: Regression test — load Megaton, count `upload_scene_mesh` calls, assert ≤ num_unique_meshes; load it twice, assert second load reuses cached handles (~0 uploads)

## Profiling-Infrastructure Gap

This is a wall-clock issue, not an allocation issue. dhat won't measure it. Needs a `tracing` span ladder around `consume_streaming_payload → finish_partial_import → load_one_exterior_cell → load_references → spawn_placed_instances`. File a separate "wire `tracing` for cell-load critical path" follow-up; that single piece of infra gives this finding + CELL-PERF-02/03/05/06/07 their regression guard.

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06b.md` (CELL-PERF-01)
- Pairs naturally with: CELL-PERF-02, CELL-PERF-03 (cell-load wall-clock trio)
- Mirrors pattern from: #524 (TextureRegistry::acquire_by_path refcounted dedup)
- Builds on: #381 (NIF cache process-lifetime)
