# Investigation — #1353 (D8-07): FO4 BGSM grayscale-to-palette

## Reframed scope (vs. the issue's "forward the path")
The issue framed this as data-forwarding, but the lit-material shader consumer did NOT
exist — the greyscale-LUT block in `triangle.frag:1398-1440` is inside the
`materialKind == MATERIAL_KIND_EFFECT_SHADER (101)` branch and samples by **alpha**
(`vec2(sourceColor.a, 0.5)`, the FX-atlas convention). FO4 lit BGSM grayscale-to-palette
(`SLSF1::Greyscale_To_PaletteColor`) samples by the **diffuse greyscale**. So a real fix
needs a new lit-path remap block, not just forwarding.

## Flag reuse (no new bit)
`material_flag::EFFECT_PALETTE_COLOR` (`1 << 2`) IS `SLSF1::Greyscale_To_PaletteColor`
(material.rs:413-420) — the correct flag, currently only set by the FX path
(`pack_effect_shader_flags`). The BGSM lit path reuses it; the shader picks the coordinate
by material kind (FX → alpha in the 101 branch; lit → luminance in the new block). No new
flag bit, no `shader_constants.glsl` regen.

## Coordinate (user accepted, pending RenderDoc)
Lit block samples `vec2(luma(texColor.rgb), 0.5)` with `luma = dot(rgb, vec3(0.2126,
0.7152, 0.0722))` (Rec.709, matching the `spec_lum` weights at asset_provider.rs:1097).
For an authored-greyscale mask (R=G=B) this equals `.r`, the Bethesda convention. The
`grayscale_to_palette_scale` modulator is NOT plumbed to GpuMaterial yet (would need a
layout change) — deferred; the LUT lookup uses scale=1.0 (direct). User to RenderDoc-validate
the tint + coordinate on real FO4 content.

## Data path (no leak)
`merge_bgsm_into_mesh` sets `ImportedMesh.bgsm_greyscale_lut_path` from `leaf.greyscale_texture`
→ `ResolvedPaths.greyscale_texture` (fallback after `effect_shader`) → existing spawn-side
`resolve_texture` + `GreyscaleLutHandle` attach (refcounted; dropped on unload via the
#1341 walk). `pack_bgsm_material_flags` ORs `EFFECT_PALETTE_COLOR` when the path is present.

## Files
- `crates/nif/src/import/types.rs` — `ImportedMesh.bgsm_greyscale_lut_path` field (+ all explicit
  construction sites, compiler-guided — same tax #1241 paid for grayscale_to_palette_scale).
- `byroredux/src/asset_provider.rs` — merge sets the field.
- `byroredux/src/cell_loader/spawn.rs` + `byroredux/src/scene/nif_loader.rs` — ResolvedPaths fallback.
- `byroredux/src/cell_loader.rs` — `pack_bgsm_material_flags` sets `EFFECT_PALETTE_COLOR`.
- `crates/renderer/shaders/triangle.frag` — lit-path palette remap block (+ SPIR-V recompile).

## Caveat
The shader rendering change is additive + flag-gated (`EFFECT_PALETTE_COLOR` is only set for
BGSM-greyscale lit meshes), so it cannot regress non-greyscale content. The tint correctness +
sample coordinate need RenderDoc sign-off on real FO4 content.
