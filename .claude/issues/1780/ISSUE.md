# D14-LOW-01: caustic_splat.comp + water.frag missing from constants-header lockstep test

**Issue**: #1780
**Severity**: LOW
**Dimension**: Caustics (renderer)
**Labels**: low, renderer, tech-debt, bug
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-06-28_DIM12_DIM14.md`
**Status (as filed)**: NEW — orchestrator-confirmed

## Description
`caustic_splat.comp` `#include`s the generated `include/shader_constants.glsl`
(at `caustic_splat.comp:7`) and depends on it for `INSTANCE_FLAG_CAUSTIC_SOURCE`
(used at the source-gate, `caustic_splat.comp:200`) plus `WORKGROUP_X` /
`WORKGROUP_Y`. The `affected_shaders_include_constants_header` test in
`crates/renderer/src/shader_constants.rs` enumerates the shaders that MUST keep
that `#include` so the single-source-of-truth flag/constant contract can't
silently drift — but its list omits `caustic_splat.comp`.

If the include line were dropped in a refactor, `INSTANCE_FLAG_CAUSTIC_SOURCE`
would become an undefined identifier and no `cargo test` would catch it (the
SPIR-V is pre-compiled; the failure would only surface on a hand-regenerated
`.spv`). `water.frag` is in the same boat — also absent (only an indirect
`water_frag_motion_enum_matches` guard).

## Evidence
- `grep 'shader_constants.glsl' crates/renderer/shaders/caustic_splat.comp` → line 7.
- `awk '/fn affected_shaders_include_constants_header/,/^}/'` over
  `crates/renderer/src/shader_constants.rs` enumerates exactly: cluster_cull.comp,
  triangle.frag, triangle.vert, skin_vertices.comp, skin_palette.comp,
  composite.frag, bloom_downsample.comp, bloom_upsample.comp,
  volumetrics_inject.comp, volumetrics_integrate.comp. Neither caustic_splat.comp
  nor water.frag appears.

## Impact
Missing regression guard, not a runtime bug — the include is present today.
Blast radius is a future refactor that strips the include and is not caught
until the `.spv` is hand-regenerated.

## Suggested Fix
Add `("caustic_splat.comp", include_str!("../shaders/caustic_splat.comp"))`
(and `("water.frag", include_str!("../shaders/water.frag"))`) to the
`affected_shaders_include_constants_header` tuple list in
`crates/renderer/src/shader_constants.rs`.

## Completeness Checks
- [ ] **SIBLING**: `water.frag` covered in the same fix; no other shader with the
  `#include "include/shader_constants.glsl"` line is unlisted
- [ ] **TESTS**: the fix IS the test — confirm the expanded
  `affected_shaders_include_constants_header` passes and would fail if the
  include were removed
