# REN-D3-2026-08-07-02: DalcCubeUBO block size is unpinned despite the #1447 reflection tooling existing

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2464
**Finding ID**: REN-D3-2026-08-07-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 3 — GPU-Struct Layout
**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:410` (`GpuDalcCube`) ↔ `crates/renderer/shaders/include/bindings.glsl:344` (`uniform DalcCubeUBO`)
**Status**: NEW

## Description
`GpuDalcCube` (8 × `[f32;4]` = 128 B) is uploaded to set 1 / binding 14 as a UBO whose GLSL mirror is a hand-written inline block. #1447 established `reflect::uniform_block_size_by_name` precisely to catch a `#[repr(C)]` UBO struct growing on the Rust side without the GLSL block, and `reflect.rs` uses it for `CameraUBO` and the volumetrics UBOs. `DalcCubeUBO` was never added to either list, and there is no Rust-side `size_of::<GpuDalcCube>()` pin either.

## Evidence
`spirv-dis triangle.frag.spv` → `DalcCubeUBO` members at 0/16/32/48/64/80/96/112 (128 B total) — currently correct. `grep -rn "GpuDalcCube" crates/renderer/src` yields only `buffers.rs:440` (`size_of::<GpuDalcCube>()` for allocation), `upload.rs:183`, and a `Default` construction. `grep "DalcCube" crates/renderer/src/vulkan/reflect.rs` → nothing.

## Impact
A Rust-side append (the doc comment already earmarks `specular_fresnel` as "reserved for future per-cell specular tint plumbing", i.e. a field that is *expected* to grow a consumer) that misses the GLSL block silently shifts every ambient-cube axis the fragment shader reads, mis-tinting interior ambient on all Skyrim WTHR.DALC cells. Cheaper to catch than the `GpuTerrainTile` gap: the tooling already exists.

## Related
#1447 (`CameraUBO` size hazard), sibling `GpuTerrainTile` finding (this report).

## Suggested Fix
Add `"DalcCubeUBO" → size_of::<GpuDalcCube>()` to the existing `.spv`-reflection size table in `reflect.rs::tests` alongside the `CameraUBO` / volumetrics entries — a few lines, no new machinery.

## Completeness Checks
- [ ] **TESTS**: `DalcCubeUBO` added to the `.spv`-reflection size table alongside `CameraUBO`
