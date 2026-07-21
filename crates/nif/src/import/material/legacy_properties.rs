//! FO3/FNV/Oblivion legacy `NiProperty`-chain extraction — split out of
//! `extract_material_info_from_refs` (#2059) to shrink that 1008-line
//! orchestrator. Skyrim+ binds material properties via the dedicated
//! `shader_property_ref` / `alpha_property_ref` fields instead (see
//! `dedicated_shader.rs`); pre-Skyrim content walks a `NiProperty` list
//! per shape, so this file drives one loop over that list, calling a
//! per-property-type helper for every entry in the exact sequence the
//! monolithic function used to check them in.

use super::*;

/// FO3/FNV/Oblivion: single pass over shape + inherited properties.
/// Shape properties first so they take priority (#208). Empty for
/// BsTriShape (Skyrim+ binds via shader_property_ref only).
pub(super) fn apply_legacy_property_chain(
    scene: &NifScene,
    direct_properties: &[BlockRef],
    inherited_props: &[BlockRef],
    pool: &mut StringPool,
    info: &mut MaterialInfo,
) {
    for prop_ref in direct_properties.iter().chain(inherited_props.iter()) {
        let Some(idx) = prop_ref.index() else {
            continue;
        };

        apply_legacy_alpha_property(scene, idx, info);
        apply_zbuffer_property(scene, idx, info);
        apply_material_property(scene, idx, info);
        apply_texturing_property(scene, idx, pool, info);
        apply_pp_lighting_property(scene, idx, pool, info);
        apply_no_lighting_property(scene, idx, pool, info);
        apply_misc_shader_properties(scene, idx, pool, info);
        apply_base_only_shader_property(scene, idx, info);
        apply_stencil_property(scene, idx, info);
        apply_flag_property(scene, idx, info);
        apply_vertex_color_property(scene, idx, info);

        // #1224 / D4-NEW-02 — NiFogProperty is parsed (see
        // `crates/nif/src/blocks/properties.rs::NiFogProperty`) but
        // intentionally NOT dispatched here. Per-node fog overrides
        // have no landing site on the `Material` ECS component, and
        // the renderer's fog path reads cell-scope `CellLighting`
        // exclusively. Adding a per-node fog component + shader
        // branch for an observed corpus of 1 block in vanilla FO3
        // is not justified. Documented so future audits don't refile
        // the gap. The 2026-04-30 audit's claim that NiFog was
        // "wired (#558 / #607)" referred to the per-node fog ENABLE
        // bit on inherited-property chains, not the NiFogProperty
        // record itself.
    }
}

/// Legacy `NiAlphaProperty` — the FO3/FNV/Oblivion counterpart to
/// `dedicated_shader.rs::apply_dedicated_alpha_property` (Skyrim+ binds
/// alpha via the dedicated ref instead of this property-chain walk).
fn apply_legacy_alpha_property(scene: &NifScene, idx: usize, info: &mut MaterialInfo) {
    // #1201 — gate on `alpha_property_consumed`, not on the
    // `!alpha_blend && !alpha_test` value-shape. A shape that
    // authors `NiAlphaProperty { flags: 0 }` (explicit "no
    // blending, no test") leaves both fields false but `apply_
    // alpha_flags` marks consumption — so the cascade gate must
    // honour the intent, not just the resulting bit values.
    // #982 added the data plumbing but the consumer was missed.
    if !info.alpha_property_consumed {
        if let Some(alpha) = scene.get_as::<NiAlphaProperty>(idx) {
            apply_alpha_flags(info, alpha);
        }
    }
}

/// `NiZBufferProperty` — depth test/write mode + comparison function.
fn apply_zbuffer_property(scene: &NifScene, idx: usize, info: &mut MaterialInfo) {
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
}

/// `NiMaterialProperty` — specular/emissive/shininess/alpha for
/// pre-Skyrim content that has no BSLightingShaderProperty.
fn apply_material_property(scene: &NifScene, idx: usize, info: &mut MaterialInfo) {
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
            // #1280 step 4 — tag the legacy Oblivion/FO3/FNV source.
            info.emissive_source =
                byroredux_core::ecs::components::material::EmissiveSource::Material;
            info.has_material_data = true;
        }
    }
}

/// `NiTexturingProperty` — up to 8 legacy texture slots (base, normal/
/// bump, glow, detail, gloss, dark, parallax) plus UV transform + clamp
/// mode. Pre-Skyrim static-mesh texturing.
fn apply_texturing_property(
    scene: &NifScene,
    idx: usize,
    pool: &mut StringPool,
    info: &mut MaterialInfo,
) {
    if let Some(tex_prop) = scene.get_as::<NiTexturingProperty>(idx) {
        if info.texture_path.is_none() {
            info.texture_path = tex_desc_source_path(scene, tex_prop.base_texture.as_ref(), pool);
        }
        // Oblivion stores tangent-space normal maps in the `bump_texture`
        // slot (the dedicated `normal_texture` slot landed later in FO3).
        // Skyrim+ meshes use BSShaderTextureSet handled elsewhere, so
        // this branch is specifically for pre-Skyrim static meshes.
        // See issue #131.
        if info.normal_map.is_none() {
            info.normal_map = tex_desc_source_path(scene, tex_prop.normal_texture.as_ref(), pool)
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
            info.glow_map = tex_desc_source_path(scene, tex_prop.glow_texture.as_ref(), pool);
        }
        if info.detail_map.is_none() {
            info.detail_map = tex_desc_source_path(scene, tex_prop.detail_texture.as_ref(), pool);
        }
        if info.gloss_map.is_none() {
            info.gloss_map = tex_desc_source_path(scene, tex_prop.gloss_texture.as_ref(), pool);
        }
        // Dark / multiplicative lightmap (slot 1). Baked shadow data
        // on Oblivion interior architecture — `albedo *= dark`. #264.
        if info.dark_map.is_none() {
            info.dark_map = tex_desc_source_path(scene, tex_prop.dark_texture.as_ref(), pool);
        }
        // Parallax height-map (slot 7, v20.2.0.5+). Pre-#450 the
        // parser consumed + dropped this slot so FO3 meshes that
        // kept the legacy `NiTexturingProperty` chain alongside a
        // `BSShaderPPLightingProperty` lost their parallax bake.
        // Feed the same downstream field as the BSShaderTextureSet
        // slot 3 path in `apply_pp_lighting_property` so the shader does not need to
        // distinguish the two sources.
        if info.parallax_map.is_none() {
            info.parallax_map =
                tex_desc_source_path(scene, tex_prop.parallax_texture.as_ref(), pool);
            // #725 / NIF-D4-06 — when a NiTexturingProperty parallax
            // slot binds WITHOUT a co-bound BSShaderPPLightingProperty
            // (rare on FO3 / FNV with an Oblivion-style property
            // chain), the scalar pair stays None and the consumer's
            // `unwrap_or(0.04, 4.0)` fallback (`render.rs:573`,
            // `cell_loader.rs:2463`, `scene.rs:1917`) compensates.
            // Setting the engine defaults at the producer-side keeps
            // the import-side `Option` semantics honest: "Some =
            // import committed to a value, None = no parallax
            // authoring at all". Defaults match `GpuMaterial`'s
            // `parallax_height_scale = 0.04, parallax_max_passes =
            // 4.0` (`renderer/src/vulkan/material.rs:216-217`).
            if info.parallax_map.is_some() {
                if info.parallax_max_passes.is_none() {
                    info.parallax_max_passes = Some(4.0);
                }
                if info.parallax_height_scale.is_none() {
                    info.parallax_height_scale = Some(0.04);
                }
            }
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
}

/// `BSShaderPPLightingProperty` — FO3/FNV primary lit shader (the
/// pre-Skyrim sibling of `BSLightingShaderProperty`).
fn apply_pp_lighting_property(
    scene: &NifScene,
    idx: usize,
    pool: &mut StringPool,
    info: &mut MaterialInfo,
) {
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
        // #773 / FO3-4-01 — `texture_clamp_mode` mirror. Pre-fix
        // the FO3/FNV PPLighting branch dropped this u32 enum
        // (parsed at `blocks/shader.rs:84` via
        // `BSShaderPropertyData::parse_fo3`), so CLAMP-authored
        // decals / scope reticles / glow planes silently fell
        // back to default WRAP. The Skyrim+ BSEffectShader path
        // already mirrored its own copy in `apply_bs_effect_shader` (#610) and
        // the NiTexturingProperty path mirrored the per-slot
        // `flags & 0xF` (#761) — only this PPLighting site was
        // missing. nif.xml enum range is 0..=3 → `as u8` is safe.
        info.texture_clamp_mode = shader.texture_clamp_mode as u8;
        // #773 / FO3-4-02 — `env_map_scale` mirror. Pre-fix the
        // env-cube + env-mask textures arrived via
        // `texture_set[4]/[5]` (#452) but the scalar that
        // modulates them was dropped, so FO3/FNV glass / polished
        // metal / power armor rendered with `env_map_scale = 0.0`
        // (the `MaterialInfo::default()` value) — texture bound,
        // multiplier zero. The BSEffectShader path captures this
        // in `apply_bs_effect_shader` and the Skyrim+ EnvironmentMap variant via
        // `apply_shader_type_data`; only this site was missing.
        // Field path: `BSShaderPPLightingProperty.shader:
        // BSShaderPropertyData → .env_map_scale`.
        info.env_map_scale = shader.shader.env_map_scale;
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
}

/// `BSShaderNoLightingProperty` — FO3/FNV fullbright/unlit shader
/// (terminal screens, HUD/scope overlays, blood-splat decals).
fn apply_no_lighting_property(
    scene: &NifScene,
    idx: usize,
    pool: &mut StringPool,
    info: &mut MaterialInfo,
) {
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
        // #773 / FO3-4-PPMAT SIBLING —
        // `BSShaderNoLightingProperty` carries the same
        // `texture_clamp_mode` (parsed at `blocks/shader.rs:140`)
        // and `BSShaderPropertyData.env_map_scale` (line 139's
        // embedded base) as the PPLighting block above. Pre-#773
        // both fields fell off the import path here too. CLAMP-on-
        // edge HUD scope crosshairs / VATS overlays / blood
        // splats authoring `texture_clamp_mode != 3` (WRAP)
        // silently fell back to default. Last-writer-wins matches
        // the established precedence for `info.is_decal` in
        // `apply_pp_lighting_property` — PP and NoLighting rarely
        // coexist on a single mesh in vanilla content.
        info.texture_clamp_mode = shader.texture_clamp_mode as u8;
        info.env_map_scale = shader.shader.env_map_scale;
        // FO3/FNV `BSShaderNoLightingProperty` is the original
        // engine's fullbright / unlit shader — the texture (× vertex
        // color) IS the final pixel: terminal screens, computer text,
        // neon/sign faces, HUD/scope overlays, blood-splat decals. Tag
        // it `MATERIAL_KIND_NO_LIGHTING` (102) so `triangle.frag`
        // short-circuits the lit path and emits the texture directly —
        // NO scene lighting, NO GI, NO camera-distance term. Pre-tag
        // these went through the full lit pipeline (`material_kind = 0`)
        // and dimmed with distance as their GI contribution faded at
        // the rtLOD tier — the user-reported "self-illumination dims
        // with distance" (2026-05-27).
        //
        // Inlined literal (nif is upstream of renderer in the dep
        // graph, same as the effect-shader `= 101` above); pinned by
        // `nolighting_sets_material_kind_to_102`. Guarded on the
        // still-default kind so a classification already made this
        // pass isn't demoted (PP / effect / NoLighting are mutually
        // exclusive in vanilla, but the guard keeps last-writer-wins
        // honest).
        if info.material_kind == 0 {
            info.material_kind = 102;
        }
    }
}

/// `TileShaderProperty` / `SkyShaderProperty` / `TallGrassShaderProperty`
/// / `WaterShaderProperty` (non-BS, FO3/FNV legacy) — each shares the
/// `BSShaderPropertyData` base for `env_map_scale`; the first three also
/// carry a `file_name`.
fn apply_misc_shader_properties(
    scene: &NifScene,
    idx: usize,
    pool: &mut StringPool,
    info: &mut MaterialInfo,
) {
    // #940 / NIF-D4-NEW-01 — `TileShaderProperty` /
    // `SkyShaderProperty` / `TallGrassShaderProperty` each carry a
    // `file_name: String` populated by their dedicated parsers
    // (#455 / #550 / #474) — the import walker was never updated
    // to consume them, so FO3/FNV HUD tiles, sky domes, and tall
    // grass imported with `texture_path = None` and the renderer
    // fell back to the magenta placeholder. Last-writer-wins
    // matches the existing `texture_path.is_none()` policy on the
    // PP / NoLighting branches.
    //
    // #1243 / NIF-DIM4-NEW-02 — the `WaterShaderProperty` (non-BS,
    // FO3/FNV legacy) was omitted by the #940 pass on the (stale)
    // reasoning that "our BSShaderProperty base data isn't yet
    // plumbed into MaterialInfo." It already was — the Tile / Sky
    // / TallGrass branches below all reach `shader.shader.env_map_scale`
    // through the same field. `WaterShaderProperty` has no `file_name`
    // (the water texture lives outside the property), so only the
    // env_map_scale rides through.
    if let Some(shader) = scene.get_as::<TileShaderProperty>(idx) {
        if info.texture_path.is_none() {
            info.texture_path = intern_texture_path(pool, &shader.file_name);
        }
        info.texture_clamp_mode = shader.texture_clamp_mode as u8;
        info.env_map_scale = shader.shader.env_map_scale;
    }
    if let Some(shader) = scene.get_as::<SkyShaderProperty>(idx) {
        if info.texture_path.is_none() {
            info.texture_path = intern_texture_path(pool, &shader.file_name);
        }
        info.texture_clamp_mode = shader.texture_clamp_mode as u8;
        info.env_map_scale = shader.shader.env_map_scale;
    }
    if let Some(shader) = scene.get_as::<TallGrassShaderProperty>(idx) {
        if info.texture_path.is_none() {
            info.texture_path = intern_texture_path(pool, &shader.file_name);
        }
        info.env_map_scale = shader.shader.env_map_scale;
    }
    if let Some(shader) = scene.get_as::<WaterShaderProperty>(idx) {
        info.env_map_scale = shader.shader.env_map_scale;
    }
}

/// `BSShaderPropertyBaseOnly` — `HairShaderProperty` /
/// `VolumetricFogShaderProperty` / `DistantLODShaderProperty` /
/// `BSDistantTreeShaderProperty`: base-only shape, `env_map_scale` only.
fn apply_base_only_shader_property(scene: &NifScene, idx: usize, info: &mut MaterialInfo) {
    // #1244 / NIF-DIM4-NEW-03 — `HairShaderProperty`,
    // `VolumetricFogShaderProperty`, `DistantLODShaderProperty`,
    // and `BSDistantTreeShaderProperty` all share the
    // `BSShaderPropertyBaseOnly` parser shape (#717): only
    // `NiObjectNETData` + `BSShaderPropertyData` base, no
    // `file_name`, no `texture_clamp_mode` (the base struct holds
    // shader_type / flags / env_map_scale; clamp lives on the
    // BSShaderLightingProperty layer, which these subclasses do
    // NOT inherit per nif.xml lines 6346/6350/6359/6363). Only
    // `env_map_scale` is plumbable here. Oblivion-era hair NIFs
    // are the most visible case — reflective hair never received
    // its authored env modulator pre-fix.
    if let Some(shader) = scene.get_as::<BSShaderPropertyBaseOnly>(idx) {
        info.env_map_scale = shader.shader.env_map_scale;
    }
}

/// `NiStencilProperty` — two-sided promotion + full stencil state
/// capture for the future renderer-side pipeline-variant landing.
fn apply_stencil_property(scene: &NifScene, idx: usize, info: &mut MaterialInfo) {
    // NiStencilProperty — proper parser replaces NiUnknown heuristic.
    // Two-sided promotion is the 95% case (`draw_mode` 0 / 3); the
    // remaining stencil test/write fields ride on
    // `info.stencil_state` for the future renderer-side pipeline-
    // variant landing. See [`StencilState`] docs and #337.
    if let Some(stencil) = scene.get_as::<NiStencilProperty>(idx) {
        if !info.two_sided && stencil.is_two_sided() {
            info.two_sided = true;
        }
        info.stencil_state = Some(super::StencilState {
            enabled: stencil.stencil_enabled,
            function: stencil.stencil_function,
            reference: stencil.stencil_ref,
            mask: stencil.stencil_mask,
            fail_action: stencil.fail_action,
            z_fail_action: stencil.z_fail_action,
            pass_action: stencil.pass_action,
        });
    }
}

/// `NiFlagProperty` subtypes — `NiSpecularProperty` / `NiWireframeProperty`
/// / `NiShadeProperty` / `NiDitherProperty` (bit 0 of `flags` is the
/// enable toggle shared by all four; `type_name` distinguishes them).
fn apply_flag_property(scene: &NifScene, idx: usize, info: &mut MaterialInfo) {
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
            // used by FO3/FNV mods. Consumed (#869): the bool propagates
            // through ImportedMesh and selects the `vk::PolygonMode::LINE`
            // pipeline variant (`PipelineKey::Opaque { wireframe }` /
            // `Blended { .., wireframe }` in pipeline.rs) at draw time.
            "NiWireframeProperty" if flag_prop.enabled() => {
                info.wireframe = true;
            }
            // NiShadeProperty: flags=0 requests flat shading (no
            // per-vertex normal interpolation — faceted look). Used on a
            // handful of Oblivion architectural pieces. Consumed (#869):
            // sets `INSTANCE_FLAG_FLAT_SHADING` (shader_constants_data.rs),
            // OR'd into the GpuInstance flags in the draw path and read by
            // the shader.
            "NiShadeProperty" if !flag_prop.enabled() => {
                info.flat_shading = true;
            }
            // NiDitherProperty: flags=1 enables 16-bit color dithering,
            // a legacy hint with no Vulkan analogue. Safe to ignore.
            _ => {}
        }
    }
}

/// `NiVertexColorProperty` — controls how per-vertex colors participate
/// in shading (`extract_vertex_colors` enforces the actual Ignore split).
fn apply_vertex_color_property(scene: &NifScene, idx: usize, info: &mut MaterialInfo) {
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
    //
    // #1208 — gate on `!info.has_material_data`. A Skyrim+ mesh
    // that authors both `BSLightingShaderProperty` (Skyrim+ shader
    // path; default AmbientDiffuse is the intended mode) AND a
    // legacy `NiVertexColorProperty` in the inherited NiNode
    // property chain previously let the legacy property silently
    // overwrite the Skyrim+ intent. Mirrors the
    // `if info.texture_path.is_none()` precedence pattern used by
    // every other secondary-source consumer in this loop.
    if !info.has_material_data {
        if let Some(vcol) = scene.get_as::<NiVertexColorProperty>(idx) {
            info.vertex_color_mode =
                VertexColorMode::from_property(vcol.vertex_mode, vcol.lighting_mode);
        }
    }
}
