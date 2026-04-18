# REN-PIPE-C2: No SPIR-V reflection — descriptor layout drift silently produces wrong reads

**Issue**: #427 — https://github.com/matiaszanolli/ByroRedux/issues/427
**Labels**: bug, renderer, critical

---

## Finding

Hand-written `DescriptorSetLayoutBinding` arrays in `crates/renderer/src/vulkan/descriptors.rs` are not cross-checked against `layout(set=N, binding=M)` declarations in the GLSL source. There is no reflection step (no `spirv-reflect`, no `rspirv` extraction).

Per `feedback_shader_struct_sync.md`, `GpuInstance` alone lives in 4 shaders that must be updated in lockstep (see also REN-SHADER-H1 #417).

## Impact

- Vulkan does **not** strictly validate shader bindings against the layout — it only validates count/type overlap at pipeline creation.
- A binding mismatch passes pipeline creation silently.
- The bug surfaces as a **silently wrong read** (e.g. sampling the sampler at the wrong binding index) or a validation-layer `DESCRIPTOR_SET_NOT_BOUND` only when the draw actually reaches the missing binding.

This is a recurring class of regressions any time scene / instance / GPU struct layout changes (recent examples in this codebase: #344 SK-D3-02 material_kind dispatch, #343 SK-D3-01 extended MaterialInfo fields, REN-SHADER-H1).

## Fix

Introduce SPIR-V reflection at pipeline build time. Options:

**(a) Runtime reflection with `spirv-reflect` crate**:
```rust
use spirv_reflect::ShaderModule;

let reflected = ShaderModule::load_u8_data(spirv_bytes)?;
for binding in reflected.enumerate_descriptor_bindings(None)? {
    assert_bindings_match(&descriptor_layout, &binding);
}
```

Run at `VulkanContext::new` for each pipeline. Panic with a descriptive message on mismatch (e.g. `"shader triangle.frag expects set=1 binding=2 as accelerationStructureEXT, descriptor layout says uniform"`).

**(b) Compile-time reflection via `naga`**:
Generate Rust descriptor layout from the SPIR-V at build time and include it via `include!`. Heavier build-system lift but eliminates the discrepancy entirely.

Option (a) is the minimal intervention; (b) is the architectural end-state.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Cross-check applies to EVERY pipeline: main (triangle.vert/frag), UI (ui.vert), composite, SVGF, SSAO, cluster_cull, caustic_splat. Confirm each has its descriptor layout introspected.
- [ ] **DROP**: N/A (runtime reflection holds no GPU resources post-load).
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A (crate is pure-Rust).
- [ ] **TESTS**: A deliberately-mismatched synthetic test — change a GLSL binding index, expect assertion failure at startup with a clear message.

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 3 C2.
