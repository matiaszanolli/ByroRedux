//! Items extracted from ../mod.rs (refactor stage C).
//!
//! Lead types: extract_vertex_colors, extract_material_info, extract_material_info_from_refs.

use super::*;
use byroredux_core::string::StringPool;

/// Extract vertex colors using a pre-computed `MaterialInfo`.
///
/// Reads `mat.vertex_color_mode` and `mat.diffuse_color` directly instead
/// of re-walking the property list. Pre-#438 this function ignored its
/// `_mat` parameter and re-scanned the shape + inherited properties twice
/// (once for vertex-color mode, once for diffuse fallback), costing 3×
/// the property-list work per NiTriShape on top of the initial
/// `extract_material_info` scan at the caller.
pub(crate) fn extract_vertex_colors(
    _scene: &NifScene,
    _shape: &NiTriShape,
    data: &GeomData,
    _inherited_props: &[BlockRef],
    mat: &MaterialInfo,
) -> Vec<[f32; 4]> {
    let num_verts = data.vertices.len();

    let use_vertex_colors =
        !data.vertex_colors.is_empty() && mat.vertex_color_mode == VertexColorMode::AmbientDiffuse;

    // Keep the alpha lane — authored per-vertex modulation on hair tip
    // cards, eyelash strips, and BSEffectShader meshes is the source of
    // truth for those surfaces. See #618.
    if use_vertex_colors {
        return data.vertex_colors.to_vec();
    }

    let d = mat.diffuse_color;
    vec![[d[0], d[1], d[2], 1.0]; num_verts]
}

/// Extract all material properties from a NiTriShape in a single pass.
///
/// `inherited_props` carries property BlockRefs accumulated from parent
/// NiNodes during the scene graph walk. Gamebryo propagates properties
/// down the hierarchy — child shapes inherit parent properties unless
/// they override them with their own. Shape-level properties take
/// priority; inherited properties fill in any gaps. See #208.
pub(crate) fn extract_material_info(
    scene: &NifScene,
    shape: &NiTriShape,
    inherited_props: &[BlockRef],
    pool: &mut StringPool,
) -> MaterialInfo {
    extract_material_info_from_refs(
        scene,
        shape.shader_property_ref,
        shape.alpha_property_ref,
        &shape.av.properties,
        inherited_props,
        pool,
    )
}

/// Block-ref-parameterised core of [`extract_material_info`].
///
/// Both the `NiTriShape` path (via the thin wrapper above) and the
/// `BsTriShape` path share this implementation so parity drift
/// between them — NIF-404 / NIF-403 — can't re-emerge. BsTriShape
/// passes empty slices for `direct_properties` and `inherited_props`
/// because Skyrim+ geometry binds properties via the dedicated
/// `shader_property_ref` / `alpha_property_ref` fields rather than
/// the legacy NiProperty chain. See #129.
pub(crate) fn extract_material_info_from_refs(
    scene: &NifScene,
    shader_property_ref: BlockRef,
    alpha_property_ref: BlockRef,
    direct_properties: &[BlockRef],
    inherited_props: &[BlockRef],
    pool: &mut StringPool,
) -> MaterialInfo {
    let mut info = MaterialInfo::default();

    // Skyrim+: dedicated shader_property_ref
    if let Some(idx) = shader_property_ref.index() {
        if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
            if let Some(name) = shader.net.name.as_deref() {
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".bgsm") || lower.ends_with(".bgem") {
                    info.material_path = intern_texture_path(pool, name);
                }
            }
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
                    // Env cube (textures[4]) + env mask (textures[5])
                    // — reach the renderer alongside the existing
                    // `env_map_scale`. #452.
                    if info.env_map.is_none() {
                        if let Some(env) = tex_set.textures.get(4) {
                            info.env_map = intern_texture_path(pool, env);
                        }
                    }
                    if info.env_mask.is_none() {
                        if let Some(mask) = tex_set.textures.get(5) {
                            info.env_mask = intern_texture_path(pool, mask);
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
            // Skyrim+/FO4 decal path — flags2 bit 21 is `Cloud_LOD` on
            // Skyrim / `Anisotropic_Lighting` on FO4, NOT a decal bit.
            // See #414.
            if is_decal_from_modern_shader_flags(
                shader.shader_flags_1,
                shader.shader_flags_2,
                &shader.sf1_crcs,
                &shader.sf2_crcs,
            ) {
                info.is_decal = true;
            }
            // Capture rich material data.
            info.emissive_color = shader.emissive_color;
            info.emissive_mult = shader.emissive_multiple;
            info.specular_color = shader.specular_color;
            info.specular_strength = shader.specular_strength;
            info.glossiness = shader.glossiness;
            info.uv_offset = shader.uv_offset;
            info.uv_scale = shader.uv_scale;
            info.has_uv_transform = true;
            info.alpha = shader.alpha;
            info.material_kind = shader.shader_type as u8;
            apply_shader_type_data(&mut info, &shader.shader_type_data);
            info.has_material_data = true;
        }
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
            if !info.alpha_blend && !info.alpha_test {
                info.alpha_blend = true;
                // The src/dst defaults live on `MaterialInfo::default`
                // as SRC_ALPHA / INV_SRC_ALPHA — correct for the
                // falloff-cone case which is the common one. Additive
                // blend (ONE / ONE) for Own_Emit / EnvMap_Light_Fade
                // flagged effect meshes is the remaining half of this
                // issue and needs a per-flag check before the src/dst
                // rewrite; defer to the follow-up.
            }
        }
    }

    // Skyrim+: dedicated alpha_property_ref
    if let Some(idx) = alpha_property_ref.index() {
        if let Some(alpha) = scene.get_as::<NiAlphaProperty>(idx) {
            apply_alpha_flags(&mut info, alpha);
        }
    }

    // FO3/FNV/Oblivion: single pass over shape + inherited properties.
    // Shape properties first so they take priority (#208). Empty for
    // BsTriShape (Skyrim+ binds via shader_property_ref only).
    for prop_ref in direct_properties.iter().chain(inherited_props.iter()) {
        let Some(idx) = prop_ref.index() else {
            continue;
        };

        if !info.alpha_blend && !info.alpha_test {
            if let Some(alpha) = scene.get_as::<NiAlphaProperty>(idx) {
                apply_alpha_flags(&mut info, alpha);
            }
        }

        // NiZBufferProperty — depth test/write mode + comparison function (#398).
        if let Some(zbuf) = scene.get_as::<crate::blocks::properties::NiZBufferProperty>(idx) {
            info.z_test = zbuf.z_test_enabled;
            info.z_write = zbuf.z_write_enabled;
            // Clamp to the 8 Gamebryo TestFunction values; out-of-range
            // (file corruption / unimplemented variant) falls back to
            // LESSEQUAL via the Default.
            if zbuf.z_function < 8 {
                info.z_function = zbuf.z_function as u8;
            }
        }

        // NiMaterialProperty — capture specular/emissive/shininess/alpha.
        if !info.has_material_data {
            if let Some(mat) = scene.get_as::<NiMaterialProperty>(idx) {
                info.diffuse_color = [mat.diffuse.r, mat.diffuse.g, mat.diffuse.b];
                // #221 — `NiMaterialProperty.ambient` was previously
                // discarded; the renderer now consumes it as a
                // per-material modulator on the cell ambient term.
                info.ambient_color = [mat.ambient.r, mat.ambient.g, mat.ambient.b];
                info.specular_color = [mat.specular.r, mat.specular.g, mat.specular.b];
                info.emissive_color = [mat.emissive.r, mat.emissive.g, mat.emissive.b];
                info.glossiness = mat.shininess;
                info.alpha = mat.alpha;
                info.emissive_mult = mat.emissive_mult;
                info.has_material_data = true;
            }
        }

        if let Some(tex_prop) = scene.get_as::<NiTexturingProperty>(idx) {
            if info.texture_path.is_none() {
                info.texture_path =
                    tex_desc_source_path(scene, tex_prop.base_texture.as_ref(), pool);
            }
            // Oblivion stores tangent-space normal maps in the `bump_texture`
            // slot (the dedicated `normal_texture` slot landed later in FO3).
            // Skyrim+ meshes use BSShaderTextureSet handled elsewhere, so
            // this branch is specifically for pre-Skyrim static meshes.
            // See issue #131.
            if info.normal_map.is_none() {
                info.normal_map = tex_desc_source_path(
                    scene,
                    tex_prop.normal_texture.as_ref(),
                    pool,
                )
                .or_else(|| tex_desc_source_path(scene, tex_prop.bump_texture.as_ref(), pool));
            }
            // Secondary texture slots (#214). NiTexturingProperty has
            // up to 8 slots — base and normal/bump are consumed above,
            // the remaining three slots we care about feed separate
            // shader inputs:
            //   * glow_texture  → emissive map (self-illumination)
            //   * detail_texture → high-frequency overlay
            //   * gloss_texture  → per-texel specular strength mask
            // We only overwrite if a Skyrim+ BSShader path hasn't
            // already set them, matching the base/normal policy.
            if info.glow_map.is_none() {
                info.glow_map =
                    tex_desc_source_path(scene, tex_prop.glow_texture.as_ref(), pool);
            }
            if info.detail_map.is_none() {
                info.detail_map =
                    tex_desc_source_path(scene, tex_prop.detail_texture.as_ref(), pool);
            }
            if info.gloss_map.is_none() {
                info.gloss_map =
                    tex_desc_source_path(scene, tex_prop.gloss_texture.as_ref(), pool);
            }
            // Dark / multiplicative lightmap (slot 1). Baked shadow data
            // on Oblivion interior architecture — `albedo *= dark`. #264.
            if info.dark_map.is_none() {
                info.dark_map =
                    tex_desc_source_path(scene, tex_prop.dark_texture.as_ref(), pool);
            }
            // Parallax height-map (slot 7, v20.2.0.5+). Pre-#450 the
            // parser consumed + dropped this slot so FO3 meshes that
            // kept the legacy `NiTexturingProperty` chain alongside a
            // `BSShaderPPLightingProperty` lost their parallax bake.
            // Feed the same downstream field as the BSShaderTextureSet
            // slot 3 path at line 532 so the shader does not need to
            // distinguish the two sources.
            if info.parallax_map.is_none() {
                info.parallax_map =
                    tex_desc_source_path(scene, tex_prop.parallax_texture.as_ref(), pool);
            }
            // NOTE: NiTexturingProperty decal slots 0..=3 are NOT
            // copied to MaterialInfo. #705 / O4-07 removed the
            // extraction (originally added in #400 / OBL-D4-H4)
            // because no descriptor bindings or fragment-shader
            // overlay path consumes them — the import-side cost
            // was paid for a render-side no-op. The block parser
            // still exposes the raw slots on
            // `NiTexturingProperty.decal_textures` so re-extraction
            // is a one-line addition when consumer wiring lands.
            // Propagate the base slot's UV transform to the shared
            // `uv_offset` / `uv_scale` fields. The renderer shader applies
            // them per-vertex to every sampled texture — fine for the
            // common case where base, detail, glow and parallax share a
            // UV set, which holds for Oblivion/FO3/FNV static meshes. See
            // issues #219 and #435. Only overwrite when no shader path
            // earlier in the pass has already supplied a UV transform —
            // gated on `has_uv_transform` rather than `has_material_data`
            // (the latter is set by `NiMaterialProperty`, which carries
            // no UV transform of its own and so was wrongly suppressing
            // this branch when it preceded `NiTexturingProperty` in
            // Oblivion / FO3 / FNV property arrays).
            // Capture the diffuse slot's `clamp_mode` (lower 4 bits of
            // `TexDesc.flags` — see `properties.rs:464`) so the
            // renderer can pick the matching `VkSamplerAddressMode`
            // pair at descriptor-write time. Pre-#610 the value was
            // dropped and every NiTexturingProperty texture rendered
            // with REPEAT/REPEAT — visible as edge bleed on decals,
            // Oblivion architecture trim, and pre-shader skybox seams.
            // Only update when no earlier shader path supplied a
            // non-default clamp_mode (e.g. BSEffectShader's dedicated
            // field) so the more-specific source still wins. Default
            // is `3 = WRAP_S_WRAP_T` per nif.xml — the legacy
            // REPEAT/REPEAT.
            if info.texture_clamp_mode == 3 {
                if let Some(base) = tex_prop.base_texture.as_ref() {
                    info.texture_clamp_mode = (base.flags & 0xF) as u8;
                }
            }
            if !info.has_uv_transform {
                if let Some(base) = tex_prop.base_texture.as_ref() {
                    if let Some(tx) = base.transform {
                        info.uv_offset = tx.translation;
                        info.uv_scale = tx.scale;
                        info.has_uv_transform = true;
                    }
                }
            }
        }

        if let Some(shader) = scene.get_as::<BSShaderPPLightingProperty>(idx) {
            if let Some(ts_idx) = shader.texture_set_ref.index() {
                if let Some(tex_set) = scene.get_as::<BSShaderTextureSet>(ts_idx) {
                    if info.texture_path.is_none() {
                        if let Some(path) = tex_set.textures.first() {
                            info.texture_path = intern_texture_path(pool, path);
                        }
                    }
                    // Normal map is textures[1] in BSShaderTextureSet (same layout as Skyrim).
                    if info.normal_map.is_none() {
                        if let Some(normal) = tex_set.textures.get(1) {
                            info.normal_map = intern_texture_path(pool, normal);
                        }
                    }
                    // Glow / emissive map is textures[2].
                    if info.glow_map.is_none() {
                        if let Some(glow) = tex_set.textures.get(2) {
                            info.glow_map = intern_texture_path(pool, glow);
                        }
                    }
                    // Parallax / height map is textures[3] (FO3/FNV
                    // Parallax_Shader_Index_15 / Parallax_Occlusion).
                    // See #452.
                    if info.parallax_map.is_none() {
                        if let Some(px) = tex_set.textures.get(3) {
                            info.parallax_map = intern_texture_path(pool, px);
                        }
                    }
                    // Environment cubemap is textures[4]. Glass bottles,
                    // power armor, polished metal — pre-#452 the path was
                    // read and thrown away. env_map_scale was captured
                    // but had no texture to route to.
                    if info.env_map.is_none() {
                        if let Some(env) = tex_set.textures.get(4) {
                            info.env_map = intern_texture_path(pool, env);
                        }
                    }
                    // Environment-reflection mask is textures[5]. #452.
                    if info.env_mask.is_none() {
                        if let Some(mask) = tex_set.textures.get(5) {
                            info.env_mask = intern_texture_path(pool, mask);
                        }
                    }
                }
            }
            // `BSShaderPPLightingProperty.parallax_max_passes` /
            // `parallax_scale` (parsed since BSVER >= 24 per
            // `blocks/shader.rs:70`) flow straight through. Only
            // overwrite when the material hasn't already bound them
            // from a Skyrim+ BSLightingShaderProperty ParallaxOcc
            // variant — the shader-type capture path in
            // `apply_shader_type_data` keeps those values. #452.
            if info.parallax_max_passes.is_none() {
                info.parallax_max_passes = Some(shader.parallax_max_passes);
            }
            if info.parallax_height_scale.is_none() {
                info.parallax_height_scale = Some(shader.parallax_scale);
            }
            // FO3/FNV `BSShaderPPLightingProperty` has NO Double_Sided
            // bit on either flag pair — see the SF_DOUBLE_SIDED
            // explanatory block at the top of this file. Leave
            // `two_sided` unset here; the `NiStencilProperty` fallback
            // below handles it correctly for meshes that want
            // back-face-off.
            if is_decal_from_legacy_shader_flags(shader.shader_flags_1(), shader.shader_flags_2()) {
                info.is_decal = true;
            }
        }

        if let Some(shader) = scene.get_as::<BSShaderNoLightingProperty>(idx) {
            if info.texture_path.is_none() {
                info.texture_path = intern_texture_path(pool, &shader.file_name);
            }
            // Same rationale as the PPLighting branch above: no Double_Sided
            // bit on the FO3/FNV flag enum. #441. Pre-#454 this branch
            // was missing the `ALPHA_DECAL_F2` (flag2 bit 21) check, so
            // blood-splat NoLighting meshes that marked themselves decal
            // via only the flag2 bit fell through to the opaque-coplanar
            // path. Shared helper keeps PP + NoLighting in lockstep.
            if is_decal_from_legacy_shader_flags(shader.shader_flags_1(), shader.shader_flags_2()) {
                info.is_decal = true;
            }
            // Capture the soft-falloff cone so the HUD / VATS / scope
            // overlay pipelines can eventually consume it. Pre-#451 the
            // four scalars were silently discarded (parser extracted
            // them but the importer had no field to receive them).
            // Don't overwrite a previously-captured falloff set: if the
            // mesh somehow binds both a NoLighting and an effect block
            // the caller-most wins, matching the other shader-field
            // merging in this loop.
            info.no_lighting_falloff.get_or_insert(NoLightingFalloff {
                start_angle: shader.falloff_start_angle,
                stop_angle: shader.falloff_stop_angle,
                start_opacity: shader.falloff_start_opacity,
                stop_opacity: shader.falloff_stop_opacity,
            });
        }

        // NiStencilProperty — proper parser replaces NiUnknown heuristic.
        if !info.two_sided {
            if let Some(stencil) = scene.get_as::<NiStencilProperty>(idx) {
                if stencil.is_two_sided() {
                    info.two_sided = true;
                }
            }
        }

        // NiFlagProperty subtypes: bit 0 of `flags` is the enable toggle for
        // all four trivial properties that share this struct. The type_name
        // distinguishes them at import time (#703).
        if let Some(flag_prop) = scene.get_as::<NiFlagProperty>(idx) {
            match flag_prop.block_type_name() {
                // NiSpecularProperty (issue #220): flags=0 disables specular.
                // Matte Oblivion/FNV surfaces use this to suppress PBR glare.
                "NiSpecularProperty" if !flag_prop.enabled() => {
                    info.specular_enabled = false;
                }
                // NiWireframeProperty: flags=1 enables wireframe rendering
                // (polygon_mode = LINE). Not present in Oblivion vanilla but
                // used by FO3/FNV mods. Renderer consumption is future work.
                "NiWireframeProperty" if flag_prop.enabled() => {
                    info.wireframe = true;
                }
                // NiShadeProperty: flags=0 requests flat shading (no
                // per-vertex normal interpolation — faceted look). Used on a
                // handful of Oblivion architectural pieces. Renderer consumption
                // (GLSL `flat` qualifier) is future work.
                "NiShadeProperty" if !flag_prop.enabled() => {
                    info.flat_shading = true;
                }
                // NiDitherProperty: flags=1 enables 16-bit color dithering,
                // a legacy hint with no Vulkan analogue. Safe to ignore.
                "NiDitherProperty" | _ => {}
            }
        }

        // NiVertexColorProperty (#214) — controls how per-vertex colors
        // participate in shading. The default is AmbientDiffuse; the
        // mesh may instead request Ignore (don't use vertex colors at
        // all) or Emissive (route them to self-illumination). The
        // actual behavior split on Ignore is enforced by
        // `extract_material` below when it decides whether to return
        // the vertex color vec or fall back to the material diffuse.
        //
        // #694 — the property carries a second enum, `lighting_mode`,
        // that gates which lighting terms actually consume the vertex
        // color. `from_property` collapses the 2D enum into our 1D
        // `VertexColorMode` axis: when LIGHTING_E drops the
        // ambient/diffuse contributions, a SRC_AMB_DIFF vertex_mode
        // becomes effectively invisible — demote to `Ignore` so the
        // renderer skips the `texColor * fragColor` double-count.
        if let Some(vcol) = scene.get_as::<NiVertexColorProperty>(idx) {
            info.vertex_color_mode =
                VertexColorMode::from_property(vcol.vertex_mode, vcol.lighting_mode);
        }
    }

    // Zero out specular strength **and color** when the property is
    // disabled. We do this once at the end so later code (pipeline
    // selection, draw command population) doesn't need to know about
    // the flag.
    //
    // #696 — clearing `specular_strength` alone is insufficient on
    // glass-classified meshes. The IOR glass branch in
    // `triangle.frag:1004` does `specStrength = max(specStrength,
    // 3.0)`, which silently re-promotes the spec term on every glass
    // surface even when the NIF said `NiSpecularProperty { flags: 0 }`.
    // The downstream BRDF multiplies (`specStrength * specColor` at
    // lines 1293 + 1396) then gate on the *color* — zeroing it here
    // collapses both glass-IOR and standard paths to zero spec
    // contribution as the original engine would.
    if !info.specular_enabled {
        info.specular_strength = 0.0;
        info.specular_color = [0.0, 0.0, 0.0];
    }

    info
}
