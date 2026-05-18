# #1077 — FO4-D6-003: Forward BGSM pbr / translucency / model_space_normals flags to renderer

Labels: enhancement, renderer, medium
State: OPEN

## Description

`BgsmFile` in `crates/bgsm/src/bgsm.rs` parses several boolean flags that affect FO4 rendering, but `merge_bgsm_into_mesh` never forwards them to `ImportedMesh` or `GpuMaterial`:

| BGSM field | FO4 use case | Forwarded? |
|---|---|---|
| `pbr: bool` (v>2) | Switch to PBR roughness/metalness path | ✗ |
| `translucency: bool` + suite (v>=8) | Subsurface scattering (skin, vegetation, glass) | ✗ |
| `model_space_normals: bool` | Object-space vs tangent-space normal decode | ✗ |
| `glowmap: bool` | Enable glow/emissive texture | ✗ |
| `tessellate: bool` | Enable tessellation displacement | ✗ |

The renderer's `GpuMaterial` (in `scene_buffer/gpu_types.rs`) has `roughness` and `metalness` fields but no `pbr` flag to switch shading paths. FO4 surfaces authored with `pbr=true` render through the Gamebryo-legacy specular path. The `translucency` suite (translucency_color, translucency_scale, translucency_turbulence) drives subsurface scattering on NPCs and vegetation — currently absent.

## Location

- `byroredux/src/asset_provider.rs` — `merge_bgsm_into_mesh`
- `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` — `GpuMaterial`
- `crates/renderer/src/vulkan/scene_buffer/upload.rs` — material upload

## Suggested Fix

Phase 1 (data propagation): Add `is_pbr: bool`, `has_translucency: bool`, `model_space_normals: bool` to `ImportedMesh` / `Material`; populate in `merge_bgsm_into_mesh`.

Phase 2 (renderer): Gate PBR vs Gamebryo shading path in `triangle.frag` based on `is_pbr` flag in `GpuMaterial`.

## Source

Audit: `docs/audits/AUDIT_FO4_2026-05-15.md` § FO4-D6-003 (MEDIUM)  
Related: FO4-D6-002 (texture slots), FO4-D6-004 (roughness path)

## Completeness Checks

- [ ] **SIBLING**: Verify `GpuMaterial` struct size stays aligned to std430 layout rules after adding flags (see `gpu_instance_is_112_bytes_std430_compatible` test pattern)
- [ ] **SHADER_SYNC**: Any new `GpuMaterial` field requires lockstep update across all shaders that declare the struct
- [ ] **TESTS**: Add a material-upload test that checks `is_pbr` propagates from `BgsmFile.pbr = true` through to the GPU buffer byte
