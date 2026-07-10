# REN-D10-01: GpuCamera.render_origin doc says "w = unused" but the same field is overloaded as is_exterior in VolumetricsParams

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1928

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:259-285` (`GpuCamera.render_origin` doc), `crates/renderer/shaders/triangle.vert:79`
**Status**: NEW

## Description
The `render_origin` field doc says "w = unused" — technically accurate for the `GpuCamera` UBO (CPU uploads w=0.0 there) — but the same field name is overloaded on the volumetrics path: `VolumetricsParams::render_origin.w` packs `is_exterior` (read by `volumetrics_inject.comp`). A future dev reading the `GpuCamera` doc could assume `.w` is free everywhere and repurpose it, colliding with the volumetrics overload one struct over.

## Evidence
`gpu_types.rs:261` "w = unused"; `draw.rs:2706` uploads `[x,y,z,0.0]` for GpuCamera; contrast `draw.rs:716` (`is_exterior ? 1.0 : 0.0`) and `volumetrics_inject.comp:316`.

## Impact
None at runtime — doc/latent-footgun concern only.

## Related
REN-D3-02 item 2 (separate `dof_params.zw` doc-rot, different field)

## Suggested Fix
Amend the `GpuCamera.render_origin` doc to note the w-field is separately overloaded as `is_exterior` in `VolumetricsParams`, so it isn't naively reclaimed.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
