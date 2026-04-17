# OBL-D4-C1: BlendType::from_nif_blend collapses 7+ Gamebryo AlphaFunction pairs to generic alpha

**Issue**: #392 — https://github.com/matiaszanolli/ByroRedux/issues/392
**Labels**: bug, renderer, pipeline, critical

---

## Finding

`crates/renderer/src/vulkan/pipeline.rs:46-53`:

```rust
pub fn from_nif_blend(src: u8, dst: u8) -> Self {
    match (src, dst) {
        (6, 0) | (0, 0) => Self::Additive,
        _ => Self::Alpha,
    }
}
```

Gamebryo's AlphaFunction has 11 source factors × 11 destination factors = 121 valid pairs; Oblivion/FNV/Skyrim ship dozens of them. Our renderer ships 3 static pipelines (`opaque`/`alpha`/`additive`) with factors baked in at pipeline creation time.

`GpuInstance` at `crates/renderer/src/vulkan/scene_buffer.rs:44-93` has no `src_blend_factor` / `dst_blend_factor` fields, so even if `Material` extracts them correctly they have no path to the GPU.

## Impact (Oblivion content)

- **Glass / alchemist glassware**: authored `DEST_COLOR / ONE` for modulated brightening → currently draws as alpha transparency.
- **Flame decals, projectile trails**: `ONE / ONE` additive premultiplied → sometimes routed via `(6,0)` path, sometimes collapses to alpha.
- **Lens flares, magic projectiles**: `ONE / INV_SRC_ALPHA` premultiplied-alpha → wrong compositing.
- **Enchantment shaders, sigils**: authored specific source/dst for glow-over-base → wrong.

Most Oblivion magic FX and stained-glass rendering is visibly wrong on a first pass.

## Fix

1. Extend `GpuInstance` with `src_blend_factor: u8` + `dst_blend_factor: u8` (Gamebryo enum values).
2. Either:
   - (a) Use Vulkan dynamic state `vkCmdSetColorBlendEnableEXT` / `vkCmdSetColorBlendEquationEXT` (requires `VK_EXT_extended_dynamic_state3`), or
   - (b) Keep the 3-pipeline model but add a lookup-table pipeline per distinct (src, dst) combo seen at load time.
3. Map Gamebryo `AlphaFunction` → `vk::BlendFactor` properly (all 11 src + 11 dst values, including the `DST_COLOR` / `INV_DST_COLOR` / `CONSTANT_ALPHA` family).

Option (a) is cleaner but gates on the extension; option (b) is portable but complicates pipeline cache management.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Update all 3 shaders that read GpuInstance (triangle.vert, triangle.frag, ui.vert) — see memory `feedback_shader_struct_sync.md` for the shader-struct sync protocol.
- [ ] **DROP**: If pipeline cache grows, verify Drop tears down all variants.
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Render test — magic projectile mesh with `ONE / INV_SRC_ALPHA` compositing matches a reference screenshot; glass mesh with `DEST_COLOR / ONE` matches.

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 4 C4-01.
