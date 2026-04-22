# #562 — SK-D3-01: triangle.frag dispatches only MATERIAL_KIND_GLASS

**Severity:** HIGH
**Labels:** bug, high, renderer, vulkan
**Source:** AUDIT_SKYRIM_2026-04-22.md (follow-up to closed #344)
**GitHub:** https://github.com/matiaszanolli/ByroRedux/issues/562

## Location
- `crates/renderer/shaders/triangle.frag:716-719` (only GLASS=100 branches)
- `crates/nif/src/import/material.rs:234-236, 586` (CPU forwards correctly)

## One-line
#344 plumbed `materialKind` into GpuInstance + added GLASS=100 dispatch, but the actual Skyrim variant ladder (SkinTint=5, HairTint=6, ParallaxOcc=7, MultiLayer=11, EyeEnvmap=16) was never added. All render as generic PBR.

## Fix sketch
Add `else if` ladder in triangle.frag after the albedo sample. Follow #344's suggested cases 5/6/7/11/16. Per `feedback_shader_struct_sync`, lockstep across `triangle.vert`, `triangle.frag`, `ui.vert`, `caustic_splat.comp`.

## Depends on
- #559 (SK-D5-02) for actor bodies to import at all

## Next
`/fix-issue 562`
