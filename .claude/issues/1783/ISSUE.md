# CONC-D2-01: skin_palette init failure not coupled to skin_compute — skin chain can run against an uninitialised palette SSBO

_Filed as #1783 from `docs/audits/AUDIT_CONCURRENCY_2026-07-02.md`._

**Severity**: MEDIUM · **Dimension**: Compute → AS → Fragment Chains · Source: `AUDIT_CONCURRENCY_2026-07-02` (CONC-D2-01)

## Location
- `crates/renderer/src/vulkan/context/mod.rs:1788-1829` (independent init gates)
- `crates/renderer/src/vulkan/context/draw.rs:1440` (refit gated on `skin_compute` + `accel_manager` only)
- `crates/renderer/src/vulkan/context/draw.rs:2675` (palette dispatch gated on `skin_palette` only)
- `crates/renderer/src/vulkan/scene_buffer/buffers.rs:457` (palette buffers are `create_device_local_uninit`)

## Description
`SkinComputePipeline` and `SkinPaletteComputePipeline` are created by two independent `match … Ok/Err → Some/None` blocks, each degrading to `None` on failure with only a `log::warn!`. If `skin_palette` creation fails while `skin_compute` succeeds (partial init failure — mid-init OOM or pipeline-cache corruption), the per-frame chain still runs its downstream links: `record_skinned_blas_refit` (draw.rs:1440 checks only `skin_compute`/`accel_manager`) dispatches `skin_vertices.comp`, which reads the bone-palette SSBO (`bone_buffers()[frame]`) that no GPU pass ever wrote — the buffer is `create_device_local_uninit` (buffers.rs:457) and the sole producer (the palette dispatch at draw.rs:2694) is gated out. `triangle.vert`'s inline-skinning path reads the same unwritten palette. The mod.rs comment acknowledges "downstream `skin_palette.is_some()` checks skip the dispatch (no CPU-multiply fallback exists)" but the paired consumer gates were never coupled.

## Evidence
```rust
// mod.rs:1788 — independent gate 1
let skin_compute = if device_caps.ray_query_supported {
    match SkinComputePipeline::new(...) { Ok(sc) => Some(sc), Err(e) => { log::warn!(...); None } }
} else { None };
// mod.rs:1816 — independent gate 2 (failure here does NOT clear skin_compute)
let skin_palette = if device_caps.ray_query_supported {
    match SkinPaletteComputePipeline::new(&device, pipeline_cache) { Ok(sp) => Some(sp), Err(e) => { log::warn!(...); None } }
} else { None };
// draw.rs:1440 — refit chain checks only skin_compute + accel
// buffers.rs:457 — palette contents undefined until first palette dispatch:
bone_device_buffers.push(GpuBuffer::create_device_local_uninit(
```

## Impact
Garbage (undefined-memory) bone matrices → garbage skinned vertices → per-entity BLAS built/refit over garbage geometry (potentially NaN/huge AABBs degrading TLAS traversal) → garbage skinned silhouettes in RT shadows/reflections/GI, plus garbage rasterized skinned meshes via the inline path. No memory corruption / UAF (accesses stay in-bounds); impact class is broken-geometry visual artifact for every skinned entity. Trigger: `SkinPaletteComputePipeline::new` fails while `SkinComputePipeline::new` succeeds on an RT-capable device (rare partial init failure), then any skinned draw.

## Related
None.

## Suggested Fix
Couple the gates: after the `skin_palette` match, if `skin_palette.is_none()` force `skin_compute = None`. Alternatively gate `record_skinned_blas_refit` and the palette dispatch on the SAME `skin_compute.is_some() && skin_palette.is_some()` predicate. One-line coupling + a note in the mod.rs comment.

## Completeness Checks
- [ ] **SIBLING**: Both consumer gates (`record_skinned_blas_refit` refit dispatch and the inline `triangle.vert` skinning path) covered by the coupled predicate
- [ ] **TESTS**: A fault-injection seam / test exercises `skin_palette=None, skin_compute=Some` and asserts the skin chain is fully skipped
