# REN-D21-2026-08-07-01: Cornell can never exercise the Disney BSDF branch -- the diffuse lobe every BGSM-sourced game takes

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2477
**Finding ID**: REN-D21-2026-08-07-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 21 — Cornell Harness
**Location**: `byroredux/src/cornell.rs:matte/pbr/glass/emissive/fire_refraction` + `byroredux/src/commands/scene.rs:MatSetCommand::execute`
**Status**: NEW

## Description
Every Cornell probe is built from `Material { .. ..Default::default() }`, and `Material::default()` sets `effect_shader_flags: 0` (`crates/core/src/ecs/components/material.rs:394`). `collect_static_mesh_draws` forwards that verbatim into `GpuMaterial.material_flags`. The shared direct-lighting BRDF branches on that bit: `include/lighting.glsl:155` `if ((mat.materialFlags & MAT_FLAG_PBR_BSDF) != 0u) { ...disneyDiffuseSplit... } else { diffuseBrdf = kD * albedo; }` (same gate at `triangle.frag:2322`). So *all* Cornell probes — including the two sweep rows built specifically to read metalness/roughness response — are shaded through the legacy Lambert path, while every BGSM/BGEM-sourced surface (FO4, Skyrim SE, FO76, Starfield; `material_flag::PBR_BSDF` is set for all `is_pbr` content since #1352) takes the Disney path. `mat.set` has no arm for the material-flags word nor for `subsurface`/`sheen`/`sheen_tint`/`anisotropic`, so the harness cannot be flipped into that branch at runtime either.

## Evidence
`cornell.rs` `pbr()` returns `Material { diffuse_color, metalness, roughness, ..Default::default() }` → `effect_shader_flags == 0`; `MatSetCommand` field table is `metalness|roughness|alpha|glossiness|emissive_mult|specular_strength|env_map_scale|ior|color|diffuse_color|emissive_color|specular_color|material_kind` — no `material_flags`, no Disney scalars.

## Impact
The reference scene silently answers for the wrong BRDF on the majority of target content. A regression isolated to `disneyDiffuseSplit` / the sheen-subsurface lobe (e.g. the sibling REN-D17-NEW-01 π disagreement) bisects clean in Cornell and then reproduces in-game — the exact false-all-clear failure mode #1942 fixed for the sun path. It also means the standing "metalness looks off" observation cannot be reproduced under the shading path FO4 content actually uses.

## Related
#1942 (same class of harness blind spot, sun path); REN-D21-2026-08-07-03 (this report — same class: a material field the harness structurally could not reach); REN-D17-NEW-01 (this report — the specific defect this gap would hide).

## Suggested Fix
Add a `mat.set <id> material_flags <u32>` (or a named `pbr_bsdf on|off`) arm plus `subsurface|sheen|sheen_tint|anisotropic` scalar arms wired to the corresponding `Material` fields, and spawn at least one probe row with `effect_shader_flags |= MAT_FLAG_PBR_BSDF` so both diffuse branches are on screen side by side.

## Completeness Checks
- [ ] **TESTS**: Cornell harness gains at least one probe row exercising `MAT_FLAG_PBR_BSDF`, verifiable by console command
