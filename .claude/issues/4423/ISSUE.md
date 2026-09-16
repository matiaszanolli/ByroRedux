# #4423: REN-2026-09-16-D7-02: The tint role multiplies albedo by the `_sk.dds` RGB with weight = its alpha, and vanilla `_sk` maps are alpha-less DXT1, so Skyrim skin albedo is scaled by (0.09, 0.02, 0.02)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4423
- **Labels**: high,renderer,shaders,game:skyrim,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: HIGH. Skin colour is wrong on every vanilla Skyrim actor head,
  body and hands, on both primary and secondary rays.
- **Dimension**: Material Table (texture-role consumption)
- **Game Affected**: Skyrim (live on 4,100 vanilla shapes). FO4 and FO76 tint
  families route the same role (`slot_to_role` slot 2); their `_sk` content was
  not measured in this sweep.
- **Location**:
  - `crates/renderer/shaders/triangle.frag`: the `mat.tintMapIndex` block,
    `albedo = mix(albedo, albedo * tintSample.rgb, tintSample.a)`.
  - The same line is in `rayHitAlbedo` (`crates/renderer/shaders/include/ray_hit.glsl`).
  - The colour space is `tint: slot(&textures.tint, srgb)` in
    `map_secondary_texture_handles`.
  - The producer is the tint-family arm of `slot_to_role`
    (`crates/nif/src/import/material/slot_role.rs`), which documents the role as
    "Skin- or hair-tint mask (`*_sk.dds`)".
- **Status**: NEW. The consumer was added in `1d94eb246` (the unification commit
  this preset follows up) without a recorded measurement of `_sk` semantics.
  #2694 later widened the producer to FaceTint.
- **Description**:
  - **How the shader reads the role.** It treats `tint.rgb` as a multiplicative
    albedo colour and `tint.a` as the blend weight.
  - **What vanilla content supplies.** All nine distinct vanilla `_sk` paths
    reached through this role are DXT1. None of the decoded head, body and hand
    maps below contains a punch-through (alpha 0) texel. The sampled alpha is
    therefore 1.0 on every texel, and the full multiply always applies.
  - **What the RGB contains.** The RGB is a dark, saturated red. Whatever `_sk`
    encodes (NIFAL calls it a mask; the modding community commonly describes it
    as the skin subsurface/tone map, which is not verified against a primary
    source here), it is not an albedo multiplier whose neutral value is 1.0.
  - **Effect of the sRGB decode.** It pushes the multiplier down by another
    factor of 3–5.
- **Evidence** (same census and archives as D7-01; 4,100 shapes with a `tint`
  role across 9 paths, all DXT1, 0 missing). Full BC1 decode with alpha, reported
  as the effective per-channel multiplier
  `mean((1 − a) + a · srgb_decode(rgb))`:

  | texture | shapes | encoded mean | albedo multiplier |
  |---|---|---|---|
  | `male\malehead_sk.dds` | 2,110 | (0.309, 0.147, 0.127) | **(0.086, 0.021, 0.016)** |
  | `female\femalehead_sk.dds` | 1,078 | (0.506, 0.300, 0.271) | ≈(0.22, 0.07, 0.06) (from mean) |
  | `male\malebody_1_sk.dds` | 329 | (0.279, 0.137, 0.119) | (0.064, 0.017, 0.013) |
  | `male\malehands_1_sk.dds` | 123 | (0.291, 0.138, 0.119) | (0.069, 0.017, 0.013) |
  | `female\femalehands_1_sk.dds` | 119 | (0.542, 0.356, 0.319) | (0.256, 0.115, 0.089) |
  | `malechild\*_sk.dds` | 75 | (0.063, 0.016, 0.031) | (0.005, 0.001, 0.002) |

  Alpha-0 texel fraction on every decoded map: 0.000.
- **Impact**:
  - Skyrim skin renders as a dark red-brown at between 1/5 and 1/200 of its
    diffuse, depending on channel and texture.
  - On FaceGeom heads it stacks with D7-01, taking a male face to about
    (0.009, 0.002, 0.002) × diffuse before lighting.
  - GI bounce and reflections of actors inherit the same colour through
    `rayHitAlbedo`.
  - No debug flag bypasses this term.
- **Related**: `1d94eb246`, #2694, #1350, #3458, REN-2026-09-16-D7-01
- **Suggested Fix**:
  1. Establish `_sk` semantics from a source before changing the math.
  2. If `_sk` is a subsurface or skin-tone input rather than an albedo multiplier,
     route it at the NIFAL boundary to the role or parameter that consumes it
     (for example the translucency/subsurface colour), and stop multiplying it
     into albedo.
  3. If a multiplicative use is confirmed, the weight cannot come from DXT1 alpha.
     The consumer also needs the same "vanilla neutral maps to 1.0" pin proposed
     in D7-01.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SHADER-SYNC**: `triangle.frag` and `include/ray_hit.glsl` (`rayHitAlbedo`) change in lockstep; `.spv` recompiled with plain `-V`
- [ ] **TESTS**: A regression test pins this specific fix
