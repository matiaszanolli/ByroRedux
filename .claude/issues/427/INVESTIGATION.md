# Issue #427 — Investigation

## Current state

- `crates/renderer/src/vulkan/descriptors.rs` is a **4-line placeholder** — all descriptor layouts are declared inline inside their respective pipeline modules.
- There is no SPIR-V reflection anywhere in the renderer. Shader SPV bytes are embedded via `include_bytes!` and fed straight to `vkCreateShaderModule`; the hand-written `DescriptorSetLayoutBinding` arrays are passed to `vkCreateDescriptorSetLayout` without cross-checking against the SPIR-V.
- No existing dependency in the workspace parses SPIR-V (`grep` for `rspirv` / `spirv-reflect` / `naga` = 0 hits).

## Descriptor layout creation sites (8 total)

| File | Pipeline | Bindings |
|---|---|---|
| `scene_buffer.rs:313–396` | main graphics (set 1) | 10 — instances, camera, lights, TLAS, bones, gbuffer/composite handoffs, SSAO output |
| `texture_registry.rs:91` | main graphics (set 0, bindless) | 1 — `textures[]` runtime array |
| `compute.rs:138` | cluster_cull.comp | 2 — clusters SSBO, camera UBO |
| `composite.rs:409` | composite.vert/frag | 6 — HDR, indirect, albedo, params, depth, caustic (plus set 1 bindless) |
| `svgf.rs:278` | svgf_temporal.comp | 9 — curr indirect, motion, mesh id, prev mesh id, prev indirect, prev moments, out indirect, out moments, params |
| `taa.rs:243` | taa.comp | 7 — curr HDR, motion, curr/prev mesh id, prev history, output, params |
| `caustic.rs:297` | caustic_splat.comp | 9 — depth, normal, mesh id, sun light, camera, instances, TLAS, caustic accum, params |
| `ssao.rs:263` | ssao.comp | 3 — depth, AO output, params |

## Shader SPV inclusion sites (11 total)

- `pipeline.rs:142,346,495` — triangle.vert/frag, ui.vert/frag
- `ssao.rs:281`, `compute.rs:156`, `composite.rs:535`, `svgf.rs:296`, `caustic.rs:315`, `taa.rs:260`

## Shader GLSL binding declarations

`grep -rn 'layout(set' crates/renderer/shaders/*.{vert,frag,comp}` → **40 distinct bindings** across 11 shader stages.

## Recurring regression class

Issue notes:
- #344 SK-D3-02 `material_kind` dispatch
- #343 SK-D3-01 extended `MaterialInfo` fields
- #417 REN-SHADER-H1 (`caustic_splat.comp _pad1 → materialKind`, extended sync list)

Every `GpuInstance` / `GpuLight` / material struct change requires a manual audit across 4+ shaders.

## Fix options

**(a) Runtime reflection (issue-proposed, minimal intervention)**
- New dep: `rspirv` (pure-Rust, no FFI, maintained by gfx-rs).
- New module `crates/renderer/src/vulkan/reflect.rs` exposing `validate_shader_bindings(spirv_bytes, expected: &[vk::DescriptorSetLayoutBinding], stage_flags, shader_name) -> Result<()>`.
- Each pipeline calls it once before `vkCreateDescriptorSetLayout`.
- Panics with a descriptive message on mismatch.

**(b) Compile-time codegen (architectural end-state)**
- `naga` in `build.rs` → generate Rust layouts → `include!` into each pipeline.
- Eliminates drift by construction but significant build-system lift.

**(c) Test-only validation** (alternative, lower blast radius)
- Single `#[test]` in renderer that iterates every (spv, layout) pair once.
- Requires exposing each pipeline's `(spv_bytes, layout_bindings)` as `pub(crate) fn` or a central registry.
- Fails at `cargo test` time, not at runtime. Catches every same-sort regression but not in production builds.

## Scope estimate

Option (a): **~10 files modified**
- `Cargo.toml` (workspace + renderer) — add `rspirv`
- New `reflect.rs` (~150 lines)
- 8 pipeline modules to invoke it

This exceeds the 5-file scope check. Pausing to confirm direction.

## Recommendation

Option (a) — minimal intervention, matches the issue's proposal, catches the regression class on every run (not just tests), and `rspirv` is a pure-Rust dep with no transitive surprises.
