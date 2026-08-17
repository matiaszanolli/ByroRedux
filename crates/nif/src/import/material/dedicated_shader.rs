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

/// Effective `BSLightingShaderType` for slot→role resolution (#2695).
///
/// The numeric `shader_type` alone is not sufficient to identify the tint
/// family. Skyrim FaceTint parses to `ShaderTypeData::None` (that variant's own
/// doc lists "Type … 4 (Face Tint)"), and FO76 carries its skin tint in
/// `Fo76SkinTint`, so a mesh can be a skin-tint material while its numeric type
/// says otherwise. #2694 traced every vanilla Skyrim head binding its
/// `*_sk.dds` skin-tint mask as a GLOW map to exactly this gap.
///
/// The raw integer is first translated from the active game's enum (#2579),
/// then strengthened by parsed trailing data when that carries a more precise
/// semantic tag than the numeric field (#2694).
fn normalize_shader_type(shader: &BSLightingShaderProperty, layout: TextureSlotLayout) -> u32 {
    match &shader.shader_type_data {
        ShaderTypeData::SkinTint { .. } | ShaderTypeData::Fo76SkinTint { .. } => {
            slot_role::bs_lighting::SKIN_TINT
        }
        ShaderTypeData::HairTint { .. } => slot_role::bs_lighting::HAIR_TINT,
        ShaderTypeData::EyeEnvmap { .. } => slot_role::bs_lighting::EYE_ENVMAP,
        _ => canonical_shader_type(layout, shader.shader_type),
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
        let slot_layout = TextureSlotLayout::from_bsver(scene.bsver);
        let slot2_glow_enabled = match slot_layout {
            TextureSlotLayout::Skyrim => {
                shader.shader_flags_2 & crate::shader_flags::skyrim_slsf2::GLOW_MAP != 0
            }
            TextureSlotLayout::Fallout4 => {
                shader.shader_flags_2 & crate::shader_flags::fo4_slsf2::GLOW_MAP != 0
            }
            TextureSlotLayout::Fallout76 | TextureSlotLayout::Starfield => {
                crate::shader_flags::bs_shader_crc32::contains_any(
                    &shader.sf1_crcs,
                    &[crate::shader_flags::bs_shader_crc32::GLOWMAP],
                ) || crate::shader_flags::bs_shader_crc32::contains_any(
                    &shader.sf2_crcs,
                    &[crate::shader_flags::bs_shader_crc32::GLOWMAP],
                )
            }
        };
        info.texture_slot_layout = slot_layout;
        info.slot2_glow_enabled = slot2_glow_enabled;
        info.shader_type = normalize_shader_type(shader, slot_layout);

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
        // #2353 — a material-reference stub contains only the external path.
        // Every remaining field is a parser placeholder, not NIF-authored
        // material data; copying it would falsely suppress the external CDB
        // values when Starfield material resolution is completed.
        if shader.material_reference {
            return;
        }
        use crate::shader_flags::bs_shader_crc32::{contains_any, MODELSPACENORMALS};
        let model_space_normals =
            shader.shader_flags_1 & crate::shader_flags::skyrim_slsf1::MODEL_SPACE_NORMALS != 0
                || contains_any(&shader.sf1_crcs, &[MODELSPACENORMALS])
                || contains_any(&shader.sf2_crcs, &[MODELSPACENORMALS]);
        if model_space_normals {
            info.model_space_normals = true;
        }
        if let Some(ts_idx) = shader.texture_set_ref.index() {
            if let Some(tex_set) = scene.get_as::<BSShaderTextureSet>(ts_idx) {
                // #2695 — slot→role resolution now goes through the single
                // shared table in `super::slot_role`, which the REFR texture
                // overlay also calls. Before this, the overlay resolved the
                // same NIF slot indices through its own shader-type-agnostic
                // table and the two already disagreed on slots 2, 3, 4/5 and 7;
                // every per-slot rule and its supporting occupancy evidence now
                // lives in one place. `None` means the slot has no canonical
                // destination for this shader type — drop it rather than guess.
                //
                // The importer's `shader_type` is normalised first: Skyrim
                // FaceTint parses to `ShaderTypeData::None`, and some FO76
                // meshes carry the tint family only in `shader_type_data`, so
                // the numeric type alone would miss them (#2694).
                let effective_type = info.shader_type;
                let context = TextureSlotContext {
                    layout: slot_layout,
                    shader_type: effective_type,
                    glow_map: slot2_glow_enabled,
                    model_space_normals,
                };
                for slot in 0..8u32 {
                    let Some(raw) = tex_set.textures.get(slot as usize) else {
                        continue;
                    };
                    // Slots 0-3 historically accepted an empty string (it
                    // interns to `None` anyway); the per-type arms filtered
                    // explicitly. Filter uniformly — an empty slot never
                    // carries a role.
                    if raw.is_empty() {
                        continue;
                    }
                    let Some(role) = slot_to_role(context, slot) else {
                        slot_role::record_unrouted_texture_slot(context, slot);
                        continue;
                    };
                    // First-wins across every source, matching the pre-#2695
                    // `if info.X.is_none()` guards: a BGSM/BGEM merge or an
                    // earlier property may already own the role.
                    let dest = match role {
                        TextureRole::BaseColor => &mut info.texture_path,
                        TextureRole::Normal => &mut info.normal_map,
                        TextureRole::Emissive => &mut info.glow_map,
                        TextureRole::Tint => &mut info.tint_map,
                        TextureRole::Detail => &mut info.detail_map,
                        TextureRole::Height => &mut info.parallax_map,
                        TextureRole::GreyscaleLut => &mut info.greyscale_lut_map,
                        TextureRole::Environment => &mut info.env_map,
                        TextureRole::EnvironmentMask => &mut info.env_mask,
                        TextureRole::InnerLayer => &mut info.inner_layer_map,
                        TextureRole::Specular => &mut info.specular_map,
                    };
                    if dest.is_none() {
                        *dest = intern_texture_path(pool, raw);
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
        // `Glow_Map` bit (F4SF2 bit 6) participates only in the texture-slot
        // vocabulary above; the BGSM merge remains authoritative for external
        // FO4 material files.
        // These are a LOWER-priority source than the BGSM merge — vanilla
        // FO4 leaves them unset and sources the same attributes from the
        // `.bgsm` (authoritative); `asset_provider`'s BGSM merge
        // OR-upgrades, so vanilla content is unchanged. See FO4-D5-MEDIUM-01.
        if scene.bsver >= crate::version::bsver::FALLOUT4 {
            // Alpha-test cutout — F4SF2 bit 25 (FO4-only; nif.xml lists
            // no CRC identifier, and the typed field is zero on
            // BSVER >= 132, so this is a no-op for FO76+). The
            // `NiAlphaProperty` path already covers meshes that ship one
            // (`apply_dedicated_alpha_property` ran earlier and owns the authored threshold/func);
            // this catches inline FO4 NIFs that signal cutout via the
            // shader flag alone. Seed Bethesda's conventional 128/255
            // cutout threshold whenever the resolved threshold is still
            // 0.0: `triangle.frag` gates the discard on
            // `alphaThreshold > 0.0`, so the `MaterialInfo::default()` 0.0
            // would leave the flag inert (a solid opaque quad).
            //
            // Gating on `alpha_threshold == 0.0` rather than
            // `!alpha_property_consumed` (#2091 / FO4-D5-01 residual): a
            // blend-only or explicit-opaque `NiAlphaProperty` runs
            // `apply_alpha_flags`, which sets `alpha_property_consumed = true`
            // but leaves `alpha_threshold` at 0.0 (it only writes a
            // threshold when the property's own test bit fired). The old
            // `!consumed` guard skipped the seed in that case, leaving
            // `alpha_test = true` with a 0.0 threshold — the exact inert
            // state #1985 fixed for the no-property case. A property that
            // authored a real test threshold already has
            // `alpha_threshold > 0.0`, so this never overrides authored
            // intent (#1201/#1202). `alpha_test_func` stays at its
            // GREATEREQUAL default. See #1985 (FO4-D5-01) and #2091.
            if shader.shader_flags_2 & crate::shader_flags::fo4_slsf2::ALPHA_TEST != 0 {
                info.alpha_test = true;
                if info.alpha_threshold == 0.0 {
                    info.alpha_threshold = 128.0 / 255.0;
                }
            }
        }
        // Capture rich material data.
        info.emissive_color = shader.emissive_color;
        info.emissive_mult = shader.emissive_multiple;
        // #2591 (SKY-D7-03) — only tag `Lighting` when this BSLSP actually
        // authored a non-zero emissive. Vanilla Skyrim ships the vast
        // majority of BSLightingShaderProperty blocks with the unauthored
        // `[0,0,0]` / `1.0` default; tagging those `Lighting` anyway
        // degenerated the discriminator to "has a BSLightingShaderProperty"
        // rather than "has an authored emissive", contradicting
        // `EmissiveSource::None`'s own doc.
        if byroredux_core::ecs::components::material::emissive_contribution_is_authored(
            shader.emissive_color,
            shader.emissive_multiple,
        ) {
            info.emissive_source =
                byroredux_core::ecs::components::material::EmissiveSource::Lighting;
        }
        info.specular_color = shader.specular_color;
        info.specular_authored = true;
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
        info.material_kind = info.shader_type;
        apply_shader_type_data(info, &shader.shader_type_data);
        // Skyrim/FO4 fire heat-haze planes are ordinary
        // BSLightingShaderProperty meshes whose diffuse and normal slots
        // both point at a tangent-space normal texture. Their authored
        // SLSF1 Refraction + Fire_Refraction pair is the discriminator:
        // rendering the source texture as lit albedo produces the familiar
        // opaque rainbow slab and hides the actual BSEffect flame cards.
        //
        // 103 is an engine-synthesized material kind (100=glass,
        // 101=effect, 102=no-lighting). The nif crate is upstream of the
        // renderer, so keep the literal here and pin it in the importer
        // regression below. The renderer-side named constant owns the
        // public value.
        let fire_refraction_flags = crate::shader_flags::skyrim_slsf1::REFRACTION
            | crate::shader_flags::skyrim_slsf1::FIRE_REFRACTION;
        if shader.shader_flags_1 & fire_refraction_flags == fire_refraction_flags {
            info.material_kind = 103;
            // Fire refraction is a screen-composition proxy. It must not
            // claim depth or behave like ordinary opaque BSLighting
            // geometry: doing either hides the separately-authored flame
            // cards sitting immediately behind the haze plane.
            //
            // Synthesize conventional source-alpha-over state here even
            // when the NIF omitted a NiAlphaProperty. The renderer gives
            // kind 103 its own ordering phase between opaque geometry and
            // every ordinary effect/transparent draw.
            info.alpha_blend = true;
            info.src_blend_mode = 6; // SRC_ALPHA
            info.dst_blend_mode = 7; // INV_SRC_ALPHA
            info.z_write = false;
        }
        info.has_material_data = true;
        // #2457 — narrow flag for the #1208 vertex-color precedence gate;
        // see `MaterialInfo::has_bs_lighting_shader`'s doc for why
        // `has_material_data` (also set by the unrelated legacy
        // `NiMaterialProperty` arm) was the wrong thing to gate on.
        info.has_bs_lighting_shader = true;
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
        // #2617 / SF-D8-2026-08-07-01 — mirrors the `apply_bs_lighting_shader`
        // guard above (#2353): a material-reference stub's remaining fields
        // are parser placeholders (`base_color=[1,1,1,1]`, `source_texture=
        // ""`, `falloff_start_opacity=falloff_stop_opacity=0.0`, …), not
        // NIF-authored data. Pre-fix, every externally-referenced Starfield
        // BSEffectShaderProperty (the DOMINANT case there — Starfield FX
        // materials are authored in materialsbeta.cdb and referenced by
        // name) copied the placeholder falloff-opacity pair straight into
        // `MaterialInfo`, and `triangle.frag`'s cone-fade math (which
        // assumes the identity default `start_op = stop_op = 1.0`) instead
        // saw `0.0` — with `start_angle == stop_angle` (also placeholder)
        // zeroing the fade denominator too, `coneFade` stayed `0.0` and
        // every such surface rendered fully, silently transparent. Keep
        // the material_kind=101 tag (still true — this IS an effect
        // shader) but drop the rest of the placeholder payload, exactly
        // like the BSLSP guard does for its own `has_bs_lighting_shader`
        // and downstream fields.
        if shader.material_reference {
            info.material_kind = 101;
            return;
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
            // #2591 (SKY-D7-03) — gated on non-zero authoring, same as
            // the BSLightingShaderProperty site above.
            if byroredux_core::ecs::components::material::emissive_contribution_is_authored(
                info.emissive_color,
                info.emissive_mult,
            ) {
                info.emissive_source =
                    byroredux_core::ecs::components::material::EmissiveSource::Effect;
            }
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
