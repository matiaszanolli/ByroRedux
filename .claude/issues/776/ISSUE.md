**Severity**: CRITICAL
**Dimension**: Material Table (R1)
**Source**: AUDIT_RENDERER_2026-05-01.md

## Locations
- [crates/renderer/shaders/ui.vert:73](../../tree/main/crates/renderer/shaders/ui.vert#L73) — `fragTexIndex = materials[inst.materialId].textureIndex;`
- [crates/renderer/src/vulkan/context/draw.rs:961-970](../../tree/main/crates/renderer/src/vulkan/context/draw.rs#L961-L970) — UI instance pushed with `..GpuInstance::default()` (which sets `material_id = 0`)
- [crates/renderer/src/vulkan/scene_buffer.rs:172-176](../../tree/main/crates/renderer/src/vulkan/scene_buffer.rs#L172-L176) — Rust-side docstring explicitly anticipates this failure mode

## Description

R1 Phase 5 (commit `7a7c145`) migrated `ui.vert` to read texture index from the per-frame `MaterialBuffer` SSBO, but the UI quad never interns a material. At `draw.rs:963`, `gpu_instances.push(GpuInstance { texture_index: ui_tex, ..GpuInstance::default() })` leaves `material_id` at its default of `0`.

The materials buffer at the time of this draw contains scene materials in insertion order — `materials[0]` is the **first scene draw's material**, not anything UI-related. `ui.vert:73` therefore writes that first scene material's `textureIndex` into `fragTexIndex`, and `ui.frag:15` samples `textures[fragTexIndex]` — yielding the wrong texture.

## Evidence

```glsl
// ui.vert:68-74
void main() {
    gl_Position = vec4(inPosition.xy, 0.0, 1.0);
    fragUV = inUV;
    // R1 Phase 5 — read texture index from the material table.
    GpuInstance inst = instances[gl_InstanceIndex];
    fragTexIndex = materials[inst.materialId].textureIndex;
}
```

```rust
// draw.rs:961-970 — UI instance push
let ui_instance_idx =
    if let (Some(ui_tex), Some(_)) = (ui_texture_handle, self.ui_quad_handle) {
        let idx = gpu_instances.len() as u32;
        gpu_instances.push(GpuInstance {
            texture_index: ui_tex,
            ..GpuInstance::default()  // material_id = 0
        });
        Some(idx)
    } else { None };
```

```rust
// scene_buffer.rs:172-176 — design contract that R1 Phase 5 violated
/// Diffuse / albedo bindless texture index. Held on the per-instance
/// struct (not migrated to the material table) because the UI quad
/// path appends an instance with a per-frame texture handle without
/// going through the material table; keeping it here costs 4 B per
/// instance and avoids a UI-specific material-intern dance.
pub texture_index: u32, // 4 B, offset 64
```

`triangle.vert:157` still reads `inst.textureIndex` directly — only `ui.vert` was incorrectly migrated.

## Impact

Every UI overlay frame samples an arbitrary scene texture instead of the Ruffle-rendered overlay. When the scene material count is zero (no draws — early menu / loading screen), `materials[0]` is undefined / out-of-bounds — driver-dependent. Visible breakage on every frame that renders both scene + UI together (the entire normal play loop).

## Suggested Fix

Revert `ui.vert:73` to `fragTexIndex = inst.textureIndex;` — matches `triangle.vert:157`, honours the `GpuInstance.texture_index` design contract documented in `scene_buffer.rs:172-176`, and avoids the alternative (more invasive) fix of interning a per-UI-frame material.

After fixing, recompile `ui.vert.spv`:
```bash
cd crates/renderer/shaders
glslangValidator -V ui.vert -o ui.vert.spv
```

## Completeness Checks

- [ ] **UNSAFE**: N/A — fix is GLSL-only
- [ ] **SIBLING**: Verify `triangle.vert:157` still reads `inst.textureIndex` (correct path); confirm no other shader reads `materials[…].textureIndex` outside RT hit lookups
- [ ] **DROP**: N/A — no Vulkan object lifecycle changes
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a build-time grep test in `crates/renderer/build.rs` (or equivalent) that asserts `ui.vert` contains `inst.textureIndex` to prevent re-regression. Consider extending `material.rs` test module with a parallel `ui_vert_reads_per_instance_texture_index` check.
