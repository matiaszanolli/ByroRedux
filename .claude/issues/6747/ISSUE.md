# REN-D7-01: `shadow_filter::Push` SAFETY comment still says "all-`u32` fields ... 16-byte block"

Labels: low,renderer,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6747

**Severity**: LOW · **Dimension**: Soft Shadows

## Location
`crates/renderer/src/vulkan/shadow_filter.rs`, `unsafe impl super::buffer::NoUninit for Push`

## Description
`a2180dd18` added `pixel_angle: f32` and `reach_max: f32` to `Push` (now 24 bytes, pinned by `push_constants_match_the_shader_block`). The SAFETY justification on the `NoUninit` impl was not updated and is now factually wrong. The impl itself remains sound (no padding, all plain scalars).

## Evidence
The comment reads "`#[repr(C)]`, all-`u32` fields tiling the 16-byte block with no padding"; the struct is `[u32;2], u32, u32, f32, f32`.

## Impact
None at runtime; audit hygiene for an `unsafe impl`.

## Suggested Fix
Reword to "`[u32;2]`, two `u32` and two `f32` scalars tiling the 24-byte block".

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **UNSAFE**: The SAFETY comment states the invariant the impl actually upholds
- [ ] **SIBLING**: Other `NoUninit` impls' SAFETY text checked against their structs
