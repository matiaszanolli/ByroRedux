# #1067 — REN-D14-NEW-07: No static guard preventing water.vert/water.frag from acquiring a GpuMaterial binding

**Severity**: INFO  
**Audit**: `docs/audits/AUDIT_RENDERER_2026-05-14_DIM14.md`  
**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` (guard absent)

## Summary

`ui.vert` is guarded against acquiring a `MaterialBuffer`/`GpuMaterial` binding by `ui_vert_reads_texture_index_from_instance_not_material_table`. No equivalent guard exists for `water.vert`/`water.frag`. Currently correct (grep confirms zero `MaterialBuffer`/`GpuMaterial` refs). Guard needed before water shader feature work expands.

## Fix

Add `water_shaders_do_not_read_material_buffer` test in `gpu_instance_layout_tests.rs`:
```rust
#[test]
fn water_shaders_do_not_read_material_buffer() {
    for (name, src) in [
        ("water.vert", include_str!("../../../shaders/water.vert")),
        ("water.frag", include_str!("../../../shaders/water.frag")),
    ] {
        assert!(!src.contains("struct GpuMaterial"), "{name}: must NOT declare GpuMaterial");
        assert!(!src.contains("buffer MaterialBuffer"), "{name}: must NOT bind MaterialBuffer");
        assert!(!src.contains("materials["), "{name}: must NOT index materials[]");
    }
}
```
