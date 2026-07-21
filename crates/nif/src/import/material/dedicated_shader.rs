//! Skyrim+ dedicated shader/alpha property extraction — split out of
//! `extract_material_info_from_refs` (#2059) to shrink that 1008-line
//! orchestrator. `NiTriShape.shader_property_ref` / `.alpha_property_ref`
//! bind these directly (no legacy `NiProperty` chain walk); see
//! `legacy_properties.rs` for the FO3/FNV/Oblivion property-chain sibling.
//!
//! Each `apply_*` function does its own `scene.get_as::<T>(idx)` lookup —
//! `apply_dedicated_shader_property` calls all four in the exact sequence
//! the monolithic function used to run them in, so a mesh binding more
//! than one shader-property type (never happens in vanilla content, but
//! the original code tolerated it) still resolves identically.

use super::*;

/// Skyrim+: dedicated `alpha_property_ref`. Must run BEFORE
/// [`apply_dedicated_shader_property`] so the BSEffectShader implicit-
/// blend gate (#1202) can consult `alpha_property_consumed`.
pub(super) fn apply_dedicated_alpha_property(
    scene: &NifScene,
    alpha_property_ref: BlockRef,
    info: &mut MaterialInfo,
) {
    // Skyrim+: dedicated alpha_property_ref — processed BEFORE the
    // shader-property block so the BSEffectShader implicit-blend gate
    // (#1202) can consult `alpha_property_consumed`. A `flags=0`
    // NiAlphaProperty here records the explicit-opaque intent and
    // keeps the implicit blend in `apply_bs_effect_shader` from firing.
    if let Some(idx) = alpha_property_ref.index() {
        if let Some(alpha) = scene.get_as::<NiAlphaProperty>(idx) {
            apply_alpha_flags(info, alpha);
        }
    }
}

/// Skyrim+: dedicated `shader_property_ref`. Dispatches to whichever of
/// the four BS*ShaderProperty variants the block resolves as — mirrors
/// the sequential `if let Some(shader) = scene.get_as::<X>(idx)` checks
/// the monolithic function used to run inline.
pub(super) fn apply_dedicated_shader_property(
    scene: &NifScene,
    shader_property_ref: BlockRef,
    pool: &mut StringPool,
    info: &mut MaterialInfo,
) {
    // Skyrim+: dedicated shader_property_ref
    if let Some(idx) = shader_property_ref.index() {
        apply_bs_lighting_shader(scene, idx, pool, info);
        apply_bs_effect_shader(scene, idx, pool, info);
        apply_bs_sky_shader(scene, idx, pool, info);
        apply_bs_water_shader(scene, idx, info);
    }
}

/// `BSLightingShaderProperty` — the Skyrim+/FO4/FO76/Starfield primary
/// PBR-ish shader property. See body comments for the full per-game
/// history; this is the single largest branch in the material walker.
fn apply_bs_lighting_shader(
    scene: &NifScene,
    idx: usize,
    pool: &mut StringPool,
    info: &mut MaterialInfo,
) {
    if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
        // Delegate to the shared helper so `.bgsm`, `.bgem`, and `.mat`
        // (Starfield JSON materials) are all captured, and trailing
        // whitespace / null bytes are trimmed. Pre-#976 this used an
        // inline suffix check that missed `.mat` entirely. Mirrors the
        // BSEffectShaderProperty branch below. See #749.
        //
        // #1183 / SF-D1-NEW-01 — Starfield falls back to the
        // BSLightingShaderProperty `Root Material` sidecar string when
        // `net.name` carried a non-material editor label. The fallback
        // runs through the same `is_material_reference` suffix gate so a
        // Root Material that's also a non-material name is a no-op.
        info.material_path = crate::import::mesh::material_path_from_name(
            shader.net.name.as_deref(),
            pool,
        )
        .or_else(|| {
            crate::import::mesh::material_path_from_name(shader.root_material_path.as_deref(), pool)
        });
        if let Some(ts_idx) = shader.texture_set_ref.index() {
            if let Some(tex_set) = scene.get_as::<BSShaderTextureSet>(ts_idx) {
                if let Some(path) = tex_set.textures.first() {
                    info.texture_path = intern_texture_path(pool, path);
                }
                // Normal map is textures[1] in BSShaderTextureSet.
                if let Some(normal) = tex_set.textures.get(1) {
                    info.normal_map = intern_texture_path(pool, normal);
                }
                // Glow / emissive map is textures[2].
                if info.glow_map.is_none() {
                    if let Some(glow) = tex_set.textures.get(2) {
                        info.glow_map = intern_texture_path(pool, glow);
                    }
                }
                // Parallax / height (textures[3]). Used by
                // BSLightingShaderProperty ParallaxOcc +
                // MultiLayerParallax shader-type variants. The
                // scale / passes scalars already arrive via
                // `apply_shader_type_data`; pair them with the
                // texture here. #452.
                if info.parallax_map.is_none() {
                    if let Some(px) = tex_set.textures.get(3) {
                        info.parallax_map = intern_texture_path(pool, px);
                    }
                }
                // Slot 4 / 5 / 7 routing branches on
                // `BSLightingShaderType` per nif.xml. Pre-#563 the
                // importer treated slot 4 as the env-cube on every
                // shader_type variant, which positively misbinds
                // FaceTint (4) — its slot 4 is `Detail`, NOT
                // envmap — and silently drops slot 7
                // (FaceTint's `Tint` and MultiLayerParallax's
                // inner `Layer`). EyeEnvmap (16) is the one
                // variant that actually does carry env at slot 4,
                // and falls through to the default arm.
                match shader.shader_type {
                    4 => {
                        // FaceTint — "Enables Detail(TS4), Tint(TS7)".
                        // Slot 4 here is the per-face detail
                        // overlay (skin freckles / pores), NOT
                        // env. Route into the existing
                        // `detail_map` slot (NiTexturingProperty
                        // slot 2 already targets the same field
                        // on pre-Skyrim content).
                        if info.detail_map.is_none() {
                            if let Some(detail) = tex_set.textures.get(4).filter(|s| !s.is_empty())
                            {
                                info.detail_map = intern_texture_path(pool, detail);
                            }
                        }
                        if info.tint_map.is_none() {
                            if let Some(tint) = tex_set.textures.get(7).filter(|s| !s.is_empty()) {
                                info.tint_map = intern_texture_path(pool, tint);
                            }
                        }
                    }
                    11 => {
                        // MultiLayerParallax — "Enables …
                        // Layer(TS7)". Slot 4 still carries the
                        // env cube here per nif.xml, paired with
                        // the `envmap_strength` scalar from
                        // `ShaderTypeData::MultiLayerParallax`.
                        if info.env_map.is_none() {
                            if let Some(env) = tex_set.textures.get(4).filter(|s| !s.is_empty()) {
                                info.env_map = intern_texture_path(pool, env);
                            }
                        }
                        if info.env_mask.is_none() {
                            if let Some(mask) = tex_set.textures.get(5).filter(|s| !s.is_empty()) {
                                info.env_mask = intern_texture_path(pool, mask);
                            }
                        }
                        if info.inner_layer_map.is_none() {
                            if let Some(inner) = tex_set.textures.get(7).filter(|s| !s.is_empty()) {
                                info.inner_layer_map = intern_texture_path(pool, inner);
                            }
                        }
                    }
                    5 | 6 => {
                        // SkinTint (5) / HairTint (6) — nif.xml
                        // BSLightingShaderType: "Enables Skin/Hair
                        // Tint Color". These drive a tint COLOUR
                        // (Color4 / Color3 shader fields), NOT a
                        // texture set slot — they declare no TS slot
                        // 4 or 5. Pre-#1350 they fell into the
                        // default arm, which would route a non-empty
                        // slot 4 → `env_map`; vanilla content leaves
                        // those slots empty so the empty-filter hid
                        // the misroute, but a modded / mis-exported
                        // SkinTint NIF with a stray slot-4 string
                        // would spuriously bind an env cube. Skip
                        // slots 4/5 explicitly so that can't happen.
                        // (The tint colour itself is a separate
                        // capture path, not a texture set lookup.)
                    }
                    _ => {
                        // Default arm — EnvironmentMap (1),
                        // EyeEnvmap (16), and every other variant
                        // route slot 4 → env cube, slot 5 →
                        // env mask. Variants whose nif.xml entry
                        // doesn't reference slot 4/5 (Default 0,
                        // Glow 2, Parallax 3, ParallaxOcc 7,
                        // Landscape 8-10, etc.) either author empty
                        // strings or skip the slots entirely — the
                        // empty-filter skips them silently.
                        if info.env_map.is_none() {
                            if let Some(env) = tex_set.textures.get(4).filter(|s| !s.is_empty()) {
                                info.env_map = intern_texture_path(pool, env);
                            }
                        }
                        if info.env_mask.is_none() {
                            if let Some(mask) = tex_set.textures.get(5).filter(|s| !s.is_empty()) {
                                info.env_mask = intern_texture_path(pool, mask);
                            }
                        }
                    }
                }
            }
        }
        // Skyrim/FO4 Double_Sided lives on flags2 bit 4 on
        // `BSLightingShaderProperty` per nif.xml `SkyrimShaderPropertyFlags2`
        // / `Fallout4ShaderPropertyFlags2`. See #441 for why this
        // check is NOT shared with the FO3/FNV PPLighting path.
        //
        // For BSVER >= 132 (FO76 / Starfield) the parser stores the
        // legacy u32 fields as literal zeros (`shader.rs:604-608`)
        // and writes the same flag identifiers into `sf1_crcs` /
        // `sf2_crcs` instead. The helpers below also test the CRC
        // arrays so FO76+ meshes route through the right path. See
        // #712 / NIF-D4-01.
        if is_two_sided_from_modern_shader_flags(
            shader.shader_flags_1,
            shader.shader_flags_2,
            &shader.sf1_crcs,
            &shader.sf2_crcs,
        ) {
            info.two_sided = true;
        }
        // Skyrim+/FO4 decal path — flags2 bit 21 is `Anisotropic_Lighting`
        // on Skyrim AND FO4 (nif.xml SkyrimShaderPropertyFlags2 /
        // Fallout4ShaderPropertyFlags2 bit 21), NOT a decal bit. Skyrim
        // `Cloud_LOD` is the separate bit 20. See #414 / #1879.
        if is_decal_from_modern_shader_flags(
            shader.shader_flags_1,
            shader.shader_flags_2,
            &shader.sf1_crcs,
            &shader.sf2_crcs,
        ) {
            info.is_decal = true;
        }
        // #1592 — FO4 NIF shader-flag bits the BGSM merge can't see on
        // inline / modded content. `BSLightingShaderProperty` is shared
        // with Skyrim under a *different* bit vocabulary, so gate on
        // `bsver >= FALLOUT4`. Exactly two bits are OR'd into MaterialInfo
        // here — F4SF1 bit 12 (`Model_Space_Normals`) and F4SF2 bit 25
        // (`Alpha_Test`) — both of which mean other things on Skyrim
        // (which routes alpha-test through `NiAlphaProperty` instead). The
        // `Glow_Map` bit (F4SF2 bit 6) is NOT sourced here — glow comes
        // from the texture-set / BGSM, not this flag (FO4-2026-06-23-L01).
        // These are a LOWER-priority source than the BGSM merge — vanilla
        // FO4 leaves them unset and sources the same attributes from the
        // `.bgsm` (authoritative); `asset_provider`'s BGSM merge
        // OR-upgrades, so vanilla content is unchanged. See FO4-D5-MEDIUM-01.
        if scene.bsver >= crate::version::bsver::FALLOUT4 {
            use crate::shader_flags::bs_shader_crc32::{contains_any, MODELSPACENORMALS};
            // Model-space normals — F4SF1 bit 12 (same position on
            // Skyrim, but kept FO4-gated to leave the validated Skyrim
            // path untouched) OR the FO76/Starfield CRC. Drives the MSN
            // normal-decode branch via `ImportedMesh::model_space_normals`.
            if shader.shader_flags_1 & crate::shader_flags::fo4_slsf1::MODEL_SPACE_NORMALS != 0
                || contains_any(&shader.sf1_crcs, &[MODELSPACENORMALS])
                || contains_any(&shader.sf2_crcs, &[MODELSPACENORMALS])
            {
                info.model_space_normals = true;
            }
            // Alpha-test cutout — F4SF2 bit 25 (FO4-only; nif.xml lists
            // no CRC identifier, and the typed field is zero on
            // BSVER >= 132, so this is a no-op for FO76+). The
            // `NiAlphaProperty` path already covers meshes that ship one
            // (`apply_dedicated_alpha_property` ran earlier and owns the authored threshold/func);
            // this catches inline FO4 NIFs that signal cutout via the
            // shader flag alone. Seed Bethesda's conventional 128/255
            // cutout threshold when no NiAlphaProperty supplied one:
            // `triangle.frag` gates the discard on `alphaThreshold > 0.0`,
            // so the `MaterialInfo::default()` 0.0 would leave the flag
            // inert (a solid opaque quad). `alpha_test_func` stays at its
            // GREATEREQUAL default. See #1985 (FO4-D5-01).
            if shader.shader_flags_2 & crate::shader_flags::fo4_slsf2::ALPHA_TEST != 0 {
                info.alpha_test = true;
                if !info.alpha_property_consumed {
                    info.alpha_threshold = 128.0 / 255.0;
                }
            }
        }
        // Capture rich material data.
        info.emissive_color = shader.emissive_color;
        info.emissive_mult = shader.emissive_multiple;
        info.emissive_source = byroredux_core::ecs::components::material::EmissiveSource::Lighting;
        info.specular_color = shader.specular_color;
        info.specular_strength = shader.specular_strength;
        info.glossiness = shader.glossiness;
        info.uv_offset = shader.uv_offset;
        info.uv_scale = shader.uv_scale;
        info.has_uv_transform = true;
        info.alpha = shader.alpha;
        // PBR scalars on every BSLSP body — none of these were
        // surfaced before #1241 (NIF-DIM4-NEW-01). The parser
        // captures them per BSVER gate at `shader.rs:679-695`;
        // out-of-band BSVERs leave the parser-side defaults
        // (matching MaterialInfo's own defaults), so the copy
        // is a literal forward in every era.
        info.refraction_strength = shader.refraction_strength;
        info.lighting_effect_1 = shader.lighting_effect_1;
        info.lighting_effect_2 = shader.lighting_effect_2;
        info.subsurface_rolloff = shader.subsurface_rolloff;
        info.rimlight_power = shader.rimlight_power;
        info.backlight_power = shader.backlight_power;
        info.grayscale_to_palette_scale = shader.grayscale_to_palette_scale;
        info.fresnel_power = shader.fresnel_power;
        // No narrowing here — pre-#570 the cast was `as u8` which
        // silently masked any `shader_type >= 256`. Both sides of
        // the pipeline are u32 now (parser → ImportedMesh → ECS
        // Material → GpuMaterial); see #570 (SK-D3-03).
        info.material_kind = shader.shader_type;
        apply_shader_type_data(info, &shader.shader_type_data);
        info.has_material_data = true;
    }
}

/// `BSEffectShaderProperty` — Skyrim+ effect/BGEM shader (glow rings,
/// magic flares, dust planes, smoke cards, force fields).
fn apply_bs_effect_shader(
    scene: &NifScene,
    idx: usize,
    pool: &mut StringPool,
    info: &mut MaterialInfo,
) {
    if let Some(shader) = scene.get_as::<BSEffectShaderProperty>(idx) {
        if info.material_path.is_none() {
            info.material_path =
                crate::import::mesh::material_path_from_name(shader.net.name.as_deref(), pool);
        }
        if info.texture_path.is_none() {
            info.texture_path = intern_texture_path(pool, &shader.source_texture);
        }
        if !info.has_material_data {
            // BSEffect's base_color is semantically a diffuse
            // tint, not emissive (#166 renamed from emissive_*).
            // We still route it into emissive_color/emissive_mult
            // because the effect shader's visible "glow" comes
            // from `base_color * base_color_scale` in the current
            // fragment-shader path. A proper diffuse-tint
            // remapping is downstream work once effect-shader
            // surfaces get their own render path.
            info.emissive_color = [
                shader.base_color[0],
                shader.base_color[1],
                shader.base_color[2],
            ];
            info.emissive_mult = shader.base_color_scale;
            // #1280 step 4 — tag the BSEffect source. Semantic is
            // diffuse-tint scale (per #166), not emissive; the
            // discriminator lets a future render path distinguish.
            info.emissive_source =
                byroredux_core::ecs::components::material::EmissiveSource::Effect;
            info.uv_offset = shader.uv_offset;
            info.uv_scale = shader.uv_scale;
            info.has_uv_transform = true;
            // `base_color[3]` is BGEM's alpha — the existing
            // `NiAlphaProperty` / `info.alpha_blend` path owns
            // binary transparency, but `mat_alpha` rides through
            // to the shader as a per-instance multiplier.
            // Pre-#129 the BsTriShape path captured this
            // explicitly and the NiTriShape path lost it.
            info.alpha = shader.base_color[3];
            // FO4+ effect shaders (BSVER >= 130) carry their own
            // normal + env maps alongside the greyscale palette.
            // Pre-#129 only the BsTriShape path read them.
            if info.normal_map.is_none() {
                info.normal_map = intern_texture_path(pool, &shader.normal_texture);
            }
            info.env_map_scale = shader.env_map_scale;
            // FO4+ BSEffectShaderProperty (BSVER >= 130) carries env_map_texture /
            // env_mask_texture alongside the normal map. Forward them into the
            // standard MaterialInfo slots so the renderer's env-map branch fires
            // the same way it does for BSLightingShaderProperty. Pre-#719 these
            // fields were captured only into effect_shader.env_map_texture, leaving
            // mat.env_map = None and silently disabling env reflections on all
            // FO4+ effect-shader surfaces. (#719 / NIF-D4-03)
            if info.env_map.is_none() {
                info.env_map = intern_texture_path(pool, &shader.env_map_texture);
            }
            if info.env_mask.is_none() {
                info.env_mask = intern_texture_path(pool, &shader.env_mask_texture);
            }
            info.has_material_data = true;
        }
        // Double_Sided (`shader_flags_2 & 0x10`) and the decal
        // flags apply on BSEffectShaderProperty with the same
        // semantics as BSLightingShaderProperty. Pre-#129 the
        // BsTriShape path checked them explicitly via
        // `bs_tri_shape_two_sided` / `find_decal_bs`; folding those
        // checks in here keeps both paths in lockstep. The CRC
        // fallback covers FO76 / Starfield where the legacy u32
        // fields are zero — see #712 / NIF-D4-01.
        if is_two_sided_from_modern_shader_flags(
            shader.shader_flags_1,
            shader.shader_flags_2,
            &shader.sf1_crcs,
            &shader.sf2_crcs,
        ) {
            info.two_sided = true;
        }
        // Skyrim+/FO4 effect-shader decal path — same rationale as
        // the BSLightingShaderProperty branch above. See #414.
        if is_decal_from_modern_shader_flags(
            shader.shader_flags_1,
            shader.shader_flags_2,
            &shader.sf1_crcs,
            &shader.sf2_crcs,
        ) {
            info.is_decal = true;
        }
        // Capture the rich effect-shader fields (falloff cone,
        // greyscale palette, FO4+/FO76 companion textures, etc.)
        // so downstream consumers can route them when the renderer-
        // side dispatch lands. See #345 / audit S4-01.
        let effect = capture_effect_shader_data(shader);
        // #610 — mirror the effect's `texture_clamp_mode` onto
        // `MaterialInfo` so the per-mesh export only needs to
        // forward one field. Effect-shader meshes (force fields,
        // glow edges, scope reticles, fire planes) are heavy
        // CLAMP authors so this path is the dominant fix path
        // on Skyrim+ content.
        info.texture_clamp_mode = effect.texture_clamp_mode;
        info.effect_shader = Some(effect);
        // #706 / FX-1 — flag the material as effect-shader for the
        // renderer's `material_kind` dispatch. Routes through the
        // existing u8 ladder (same plumbing the BSLightingShaderProperty
        // shader_type uses) into `triangle.frag`'s `MATERIAL_KIND_EFFECT_SHADER`
        // branch, which short-circuits lit shading and emits only
        // `emissive_color * emissive_mult * texColor.rgba`. Without
        // this flag, fire / magic / glow planes get scene-lit by
        // every nearby point light + ambient + RT GI bounce — pure
        // emissive surfaces are then modulated against scene colors
        // and render rainbow. See #706.
        //
        // 101 fits in the `u8` field (max 255). The contract on
        // `MaterialInfo.material_kind` widens here: 0..=19 is the
        // BSLightingShaderProperty shader_type; >= 100 is an
        // engine-synthesized kind (mirrors the Glass = 100 pattern
        // already shipped in scene_buffer.rs). The variant-specific
        // packs in render.rs gate on `base_material_kind == N` for
        // N in {5, 6, 11, 14, 16}, none of which collide with 101.
        // 101 = MATERIAL_KIND_EFFECT_SHADER (defined in
        // `byroredux-renderer/src/vulkan/scene_buffer.rs`). Inlined
        // here as a literal because the nif crate is upstream of
        // renderer in the dep graph; the existing test
        // `effect_shader_sets_material_kind_to_101` pins the value.
        info.material_kind = 101;
        // Effect-shader surfaces are non-occluding glows / light
        // shafts / dust planes — they belong in the transparent
        // pass with depth-WRITE off (depth-test stays on so they
        // sort against opaque geometry). Default `z_write = true`
        // made FO4 god-ray cones (`meshes\effects\ambient\
        // lightbeamthindusty*.nif`, a stack of 3 additive
        // BSTriShapes) write depth and hard-edge against each
        // other — visible banding within the shaft. These NIFs
        // ship no `NiZBufferProperty`, so nothing else sets
        // z_write. An explicit NiZBufferProperty in the property
        // chain (processed later for the rare NiTriShape effect
        // mesh) still overrides this default. 2026-05-27.
        info.z_write = false;
        // Implicit alpha blend: BSEffectShaderProperty is the
        // Skyrim+ transparency source of truth. Bethesda effect
        // NIFs frequently omit NiAlphaProperty entirely because
        // BGEM/shader data owns the blend — without this flag,
        // `meshes/effects/*.nif` (glow rings, magic flares, dust
        // planes, smoke cards) render as opaque planes with hard
        // edges. Only flip when the shape hasn't already bound a
        // NiAlphaProperty (that path owns explicit src/dst blend
        // factors and must not be overwritten). See #354 / audit
        // S4-03.
        //
        // #1202 — gate on `alpha_property_consumed` instead of the
        // value-shape `!alpha_blend && !alpha_test`: a
        // `NiAlphaProperty { flags: 0 }` (explicit opaque) leaves
        // both bits false but signals an explicit choice that this
        // implicit-blend write must NOT overwrite. The
        // `alpha_property_ref` Skyrim+ branch now runs before this
        // shader block so the flag is up to date by the time we
        // reach here.
        if !info.alpha_property_consumed {
            info.alpha_blend = true;
            // Own_Emit (SLSF1 bit 22) — the surface self-illuminates
            // and must additively composite onto the scene (src=ONE,
            // dst=ONE). Standard alpha-over at the default SRC_ALPHA /
            // INV_SRC_ALPHA would clip high-emissive values to white
            // instead of blooming them correctly (see: nuclear warhead
            // glows in Lonesome Road, power-armor auras).
            // OWN_EMIT is bit 22 (0x0040_0000) across all game variants
            // (fo3nv_f1 / skyrim_slsf1 / fo4_slsf1 — same value, confirmed
            // by nif.xml). Use the fo3nv constant as the canonical name.
            if shader.shader_flags_1 & crate::shader_flags::fo3nv_f1::OWN_EMIT != 0 {
                info.src_blend_mode = 0; // ONE
                info.dst_blend_mode = 0; // ONE
            }
            // Otherwise keep the default SRC_ALPHA / INV_SRC_ALPHA
            // (correct for falloff cones, dust planes, smoke cards).
        }
    }
}

/// `BSSkyShaderProperty` — Skyrim+ sky-dome consumer (#977): clouds,
/// sunglare, moon, stars.
fn apply_bs_sky_shader(
    scene: &NifScene,
    idx: usize,
    pool: &mut StringPool,
    info: &mut MaterialInfo,
) {
    if let Some(shader) = scene.get_as::<BSSkyShaderProperty>(idx) {
        if info.texture_path.is_none() {
            info.texture_path = intern_texture_path(pool, &shader.source_texture);
        }
        if !info.has_material_data {
            info.uv_offset = shader.uv_offset;
            info.uv_scale = shader.uv_scale;
            info.has_uv_transform = true;
            info.has_material_data = true;
        }
        info.is_sky_object = true;
        info.sky_object_type = shader.sky_object_type;
        // Sky surfaces are emissive (unlit) — the renderer-side
        // dispatch on `is_sky_object` is follow-up work; until then
        // the flag rides through as a structural marker so callers
        // can route around scene lighting when the path lands.
    }
}

/// `BSWaterShaderProperty` — Skyrim+ mesh-driven water (#977), companion
/// to [`apply_bs_sky_shader`]. Cell-driven water refs go through M38
/// `WaterPipeline` separately; this only covers legacy mesh-bound water.
fn apply_bs_water_shader(scene: &NifScene, idx: usize, info: &mut MaterialInfo) {
    if let Some(shader) = scene.get_as::<BSWaterShaderProperty>(idx) {
        if !info.has_material_data {
            info.uv_offset = shader.uv_offset;
            info.uv_scale = shader.uv_scale;
            info.has_uv_transform = true;
            info.has_material_data = true;
        }
        info.water_shader_flags = shader.water_shader_flags;
    }
}
