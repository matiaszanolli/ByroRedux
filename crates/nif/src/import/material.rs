//! NIF material and texture property extraction.

use crate::blocks::properties::{
    NiAlphaProperty, NiFlagProperty, NiMaterialProperty, NiStencilProperty, NiTexturingProperty,
    NiVertexColorProperty, TexDesc,
};
use crate::blocks::shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderNoLightingProperty,
    BSShaderPPLightingProperty, BSShaderTextureSet, ShaderTypeData,
};
use crate::blocks::texture::NiSourceTexture;
use crate::blocks::tri_shape::{BsTriShape, NiTriShape};
use crate::blocks::NiObject;
use crate::scene::NifScene;
use crate::types::BlockRef;

use super::mesh::GeomData;

pub(super) const DECAL_SINGLE_PASS: u32 = 0x04000000;
pub(super) const DYNAMIC_DECAL: u32 = 0x08000000;
const ALPHA_DECAL_F2: u32 = 0x00200000;
const SF_DOUBLE_SIDED: u32 = 0x1000;

/// How a `NiVertexColorProperty` wants per-vertex colors to participate
/// in shading, mirroring Gamebryo's `NiVertexColorProperty::SourceMode`.
///
/// `NiTexturingProperty` / `NiMaterialProperty` meshes can opt out of
/// vertex-color contribution entirely (`Ignore`) or route it through a
/// different shader channel (`Emissive`). Pre-#214 the importer always
/// used vertex colors as diffuse regardless of the stored mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum VertexColorMode {
    /// `SRC_IGNORE` — the mesh has vertex colors in the data block but
    /// the material explicitly disables them. Treat as if absent.
    Ignore = 0,
    /// `SRC_EMISSIVE` — vertex colors drive per-vertex self-illumination
    /// rather than diffuse. Gamebryo uses this for flickering torches,
    /// signs, and glowing effects baked into the geometry.
    Emissive = 1,
    /// `SRC_AMB_DIFF` — default / pre-10.0 behavior: vertex colors act
    /// as per-vertex diffuse + ambient.
    AmbientDiffuse = 2,
}

impl VertexColorMode {
    /// Decode the Gamebryo source-mode u32. Unknown values fall back to
    /// `AmbientDiffuse` — the value Gamebryo uses when the field is
    /// missing — so legacy content stays visually unchanged.
    pub(super) fn from_source_mode(raw: u32) -> Self {
        match raw {
            0 => Self::Ignore,
            1 => Self::Emissive,
            _ => Self::AmbientDiffuse,
        }
    }
}

/// Material properties extracted from a NiTriShape's property list in a single pass.
#[derive(Debug)]
pub(super) struct MaterialInfo {
    pub texture_path: Option<String>,
    /// BGSM/BGEM material file reference (FO4+). Present when the
    /// BSLightingShaderProperty has a non-empty name.
    pub material_path: Option<String>,
    pub normal_map: Option<String>,
    /// Glow / self-illumination texture (NiTexturingProperty slot 4).
    /// Filled on Oblivion/FO3/FNV meshes where a dedicated emissive
    /// map supplements or replaces `NiMaterialProperty.emissive`. See #214.
    pub glow_map: Option<String>,
    /// Detail overlay texture (NiTexturingProperty slot 2). Blends with
    /// the base texture at higher frequency; used for terrain detail
    /// variation and clothing micro-texture.
    pub detail_map: Option<String>,
    /// Specular-mask / gloss texture (NiTexturingProperty slot 3).
    /// Per-texel specular strength; enables armor highlights masked
    /// by leather/fabric regions.
    pub gloss_map: Option<String>,
    /// Dark / multiplicative lightmap texture (NiTexturingProperty slot 1).
    /// Baked shadow/grime modulation on Oblivion interior architecture.
    /// Applied as `albedo.rgb *= dark_sample.rgb`. See #264.
    pub dark_map: Option<String>,
    /// How vertex colors should participate in shading. See #214 /
    /// `VertexColorMode`. Defaults to `AmbientDiffuse` — the value
    /// Gamebryo uses when the NIF has no `NiVertexColorProperty`.
    pub vertex_color_mode: VertexColorMode,
    pub alpha_blend: bool,
    /// Source blend factor from NiAlphaProperty flags bits 1–4.
    /// Maps to Gamebryo's AlphaFunction enum:
    ///   0=ONE, 1=ZERO, 2=SRC_COLOR, 3=INV_SRC_COLOR, 4=DEST_COLOR,
    ///   5=INV_DEST_COLOR, 6=SRC_ALPHA, 7=INV_SRC_ALPHA, 8=DEST_ALPHA,
    ///   9=INV_DEST_ALPHA, 10=SRC_ALPHA_SATURATE.
    /// Default: 6 (SRC_ALPHA).
    pub src_blend_mode: u8,
    /// Destination blend factor from NiAlphaProperty flags bits 5–8.
    /// Same enum as src_blend_mode. Default: 7 (INV_SRC_ALPHA).
    pub dst_blend_mode: u8,
    /// Alpha-tested (cutout) rendering — vertices whose sampled texture
    /// alpha falls below `alpha_threshold` should be `discard`-ed in the
    /// fragment shader. Set when NiAlphaProperty.flags has bit 9 (0x200).
    /// Mutually exclusive with `alpha_blend` in the importer: when a
    /// material sets both bits (common on Gamebryo foliage and hair),
    /// alpha-test wins because the discard + depth-write path sorts
    /// cleanly, while alpha-blend produces z-sort artifacts.
    pub alpha_test: bool,
    /// Cutoff threshold for `alpha_test`, in the [0.0, 1.0] range —
    /// `NiAlphaProperty.threshold` (u8) divided by 255.
    pub alpha_threshold: f32,
    /// Alpha test comparison function from `NiAlphaProperty.flags`
    /// bits 10–12. Maps to Gamebryo's `TestFunction` enum:
    ///   0=ALWAYS, 1=LESS, 2=EQUAL, 3=LESSEQUAL,
    ///   4=GREATER, 5=NOTEQUAL, 6=GREATEREQUAL, 7=NEVER.
    /// Default: 6 (GREATEREQUAL) — keep fragments where alpha >= threshold.
    pub alpha_test_func: u8,
    pub two_sided: bool,
    pub is_decal: bool,
    pub emissive_color: [f32; 3],
    pub emissive_mult: f32,
    pub specular_color: [f32; 3],
    pub specular_strength: f32,
    /// True when the mesh has no `NiSpecularProperty` or the property's
    /// enable flag (bit 0) is set. Many Oblivion/FNV matte surfaces
    /// (stone walls, plaster, unfinished wood) explicitly disable
    /// specular via a `NiSpecularProperty { flags: 0 }` block; honoring
    /// that flag prevents bright specular hotspots that look like
    /// lighting glitches in the new PBR pipeline.
    pub specular_enabled: bool,
    pub glossiness: f32,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub alpha: f32,
    pub env_map_scale: f32,
    pub has_material_data: bool,
    /// Depth test enabled (from NiZBufferProperty). Default: true.
    pub z_test: bool,
    /// Depth write enabled (from NiZBufferProperty). Default: true.
    pub z_write: bool,

    // ── BSLightingShaderProperty.shader_type dispatch (SK-D3-01) ────
    // Each variant of `ShaderTypeData` exposes different trailing
    // fields. Capturing them at import time lets the renderer later
    // branch on `material_kind` without re-reading the NIF. Renderer-
    // side dispatch is tracked separately (SK-D3-02); until that lands
    // these values ride unused on `MaterialInfo`.
    /// Raw `BSLightingShaderProperty.shader_type` (0–19). 0 when the
    /// shape has no BSLightingShaderProperty (pre-Skyrim).
    pub material_kind: u8,
    /// SkinTint (type 5) — race/character skin color. FO76 variant
    /// stores alpha in `skin_tint_alpha`.
    pub skin_tint_color: Option<[f32; 3]>,
    /// FO76 SkinTint (type 4, BSShaderType155) — alpha channel that
    /// the Color4 variant of SkinTint carries in addition to RGB.
    pub skin_tint_alpha: Option<f32>,
    /// HairTint (type 6) — per-NPC hair color multiplier.
    pub hair_tint_color: Option<[f32; 3]>,
    /// EyeEnvmap (type 16) — cubemap reflection strength on eye shapes.
    pub eye_cubemap_scale: Option<f32>,
    /// EyeEnvmap left-eye reflection center, world-space.
    pub eye_left_reflection_center: Option<[f32; 3]>,
    /// EyeEnvmap right-eye reflection center, world-space.
    pub eye_right_reflection_center: Option<[f32; 3]>,
    /// ParallaxOcc (type 7) — height-sample passes (stepping quality).
    pub parallax_max_passes: Option<f32>,
    /// ParallaxOcc (type 7) — height-map scale.
    pub parallax_height_scale: Option<f32>,
    /// MultiLayerParallax (type 11) — inner layer thickness.
    pub multi_layer_inner_thickness: Option<f32>,
    /// MultiLayerParallax (type 11) — refraction scale.
    pub multi_layer_refraction_scale: Option<f32>,
    /// MultiLayerParallax (type 11) — inner texture scale u/v.
    pub multi_layer_inner_layer_scale: Option<[f32; 2]>,
    /// MultiLayerParallax (type 11) — envmap strength.
    pub multi_layer_envmap_strength: Option<f32>,
    /// SparkleSnow (type 14) — packed rgba parameters (rgb color +
    /// alpha strength).
    pub sparkle_parameters: Option<[f32; 4]>,

    /// Rich Skyrim+ effect-shader (`BSEffectShaderProperty`) data —
    /// soft falloff cone, greyscale palette, FO4+/FO76 companion
    /// textures, lighting influence, etc. `None` for non-effect
    /// materials. The parser already extracted every field; before
    /// #345 the importer dropped all but `texture_path`, `emissive_*`
    /// and `uv_*`. Now they ride through to the renderer (separate
    /// dispatch hookup tracked at SK-D3-02).
    pub effect_shader: Option<BsEffectShaderData>,
}

/// Fields imported from a `BSEffectShaderProperty` block. Only present
/// on materials backed by an effect shader (VFX surfaces, force fields,
/// glow-edged shields, Dwemer steam, BGEM-keyed surfaces). See #345 /
/// audit S4-01.
#[derive(Debug, Clone, PartialEq)]
pub struct BsEffectShaderData {
    /// Soft falloff cone — start angle (cos) where alpha = `start_opacity`.
    pub falloff_start_angle: f32,
    /// Soft falloff cone — stop angle (cos) where alpha = `stop_opacity`.
    pub falloff_stop_angle: f32,
    pub falloff_start_opacity: f32,
    pub falloff_stop_opacity: f32,
    /// Soft-particles depth — fades the surface as it approaches the
    /// scene depth behind it. 0.0 = no soft-particle effect.
    pub soft_falloff_depth: f32,
    /// Greyscale palette / gradient lookup texture (fire / electricity
    /// gradients reference this). `None` when the effect shader
    /// supplies an empty path.
    pub greyscale_texture: Option<String>,
    /// Environment map texture (FO4+ — BSVER >= 130).
    pub env_map_texture: Option<String>,
    /// Normal texture (FO4+ — BSVER >= 130).
    pub normal_texture: Option<String>,
    /// Environment mask texture (FO4+ — BSVER >= 130).
    pub env_mask_texture: Option<String>,
    /// Environment-map scale (FO4+ — BSVER >= 130).
    pub env_map_scale: f32,
    /// FO76 refraction power (BSVER == 155). `None` on pre-FO76.
    pub refraction_power: Option<f32>,
    /// Lighting influence 0–255 — how much the scene's directional
    /// light tints the effect. Carried as raw u8 to avoid lossy
    /// normalisation; the shader path can divide by 255 when sampling.
    pub lighting_influence: u8,
    /// Environment-map minimum mip-level clamp (raw u8).
    pub env_map_min_lod: u8,
    /// Texture clamp mode: `0=Clamp_S_Clamp_T`, `1=Clamp_S_Wrap_T`,
    /// `2=Wrap_S_Clamp_T`, `3=Wrap_S_Wrap_T` (the Skyrim default).
    /// Raw u8 — renderer maps to `vk::SamplerAddressMode` per axis.
    pub texture_clamp_mode: u8,
}

impl Default for MaterialInfo {
    fn default() -> Self {
        Self {
            texture_path: None,
            material_path: None,
            normal_map: None,
            glow_map: None,
            detail_map: None,
            gloss_map: None,
            dark_map: None,
            vertex_color_mode: VertexColorMode::AmbientDiffuse,
            alpha_blend: false,
            src_blend_mode: 6, // SRC_ALPHA — Gamebryo default
            dst_blend_mode: 7, // INV_SRC_ALPHA — Gamebryo default
            alpha_test: false,
            alpha_threshold: 0.0,
            alpha_test_func: 6, // GREATEREQUAL — Gamebryo default
            two_sided: false,
            is_decal: false,
            emissive_color: [0.0, 0.0, 0.0],
            emissive_mult: 0.0,
            specular_color: [1.0, 1.0, 1.0],
            specular_strength: 1.0,
            specular_enabled: true,
            glossiness: 80.0,
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            alpha: 1.0,
            env_map_scale: 0.0,
            has_material_data: false,
            z_test: true,
            z_write: true,
            material_kind: 0,
            skin_tint_color: None,
            skin_tint_alpha: None,
            hair_tint_color: None,
            eye_cubemap_scale: None,
            eye_left_reflection_center: None,
            eye_right_reflection_center: None,
            parallax_max_passes: None,
            parallax_height_scale: None,
            multi_layer_inner_thickness: None,
            multi_layer_refraction_scale: None,
            multi_layer_inner_layer_scale: None,
            multi_layer_envmap_strength: None,
            sparkle_parameters: None,
            effect_shader: None,
        }
    }
}

/// Lift a `BSEffectShaderProperty` into the importer's
/// [`BsEffectShaderData`] capture struct. Empty string fields collapse
/// to `None`. Pre-FO76 inputs leave `refraction_power = None`. See
/// #345 / audit S4-01.
fn capture_effect_shader_data(shader: &BSEffectShaderProperty) -> BsEffectShaderData {
    fn opt(s: &str) -> Option<String> {
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    }
    BsEffectShaderData {
        falloff_start_angle: shader.falloff_start_angle,
        falloff_stop_angle: shader.falloff_stop_angle,
        falloff_start_opacity: shader.falloff_start_opacity,
        falloff_stop_opacity: shader.falloff_stop_opacity,
        soft_falloff_depth: shader.soft_falloff_depth,
        greyscale_texture: opt(&shader.greyscale_texture),
        env_map_texture: opt(&shader.env_map_texture),
        normal_texture: opt(&shader.normal_texture),
        env_mask_texture: opt(&shader.env_mask_texture),
        env_map_scale: shader.env_map_scale,
        // refraction_power is FO76-only; the parser fills it with 0.0
        // on pre-FO76. Surface as `None` so the shader-side dispatch
        // can branch on `Some(p)` instead of guessing whether 0.0
        // means "off" or "FO76 with literal 0".
        refraction_power: (shader.refraction_power != 0.0).then_some(shader.refraction_power),
        lighting_influence: shader.lighting_influence,
        env_map_min_lod: shader.env_map_min_lod,
        texture_clamp_mode: shader.texture_clamp_mode,
    }
}

/// Extract vertex colors using a pre-computed `MaterialInfo`.
///
/// Avoids the double `extract_material_info` that previously occurred when
/// `extract_material` called `find_texture_path` (which internally called
/// `extract_material_info`) followed by a second direct call. #279 D5-10.
pub(super) fn extract_vertex_colors(
    scene: &NifScene,
    shape: &NiTriShape,
    data: &GeomData,
    inherited_props: &[BlockRef],
    _mat: &MaterialInfo,
) -> Vec<[f32; 3]> {
    let num_verts = data.vertices.len();

    let vertex_mode = vertex_color_mode_for(scene, shape, inherited_props);
    let use_vertex_colors =
        !data.vertex_colors.is_empty() && vertex_mode == VertexColorMode::AmbientDiffuse;

    if use_vertex_colors {
        return data
            .vertex_colors
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect();
    }

    // Fall back to NiMaterialProperty diffuse or white.
    let mut diffuse = [1.0f32; 3];
    for prop_ref in shape.av.properties.iter().chain(inherited_props.iter()) {
        if let Some(idx) = prop_ref.index() {
            if let Some(mat) = scene.get_as::<NiMaterialProperty>(idx) {
                diffuse = [mat.diffuse.r, mat.diffuse.g, mat.diffuse.b];
                break;
            }
        }
    }
    vec![diffuse; num_verts]
}

/// Look up `NiVertexColorProperty` on the shape and return the decoded
/// vertex-color source mode. Absent property → `AmbientDiffuse` (the
/// Gamebryo default). Helper for `extract_material` and the unit tests
/// below.
fn vertex_color_mode_for(
    scene: &NifScene,
    shape: &NiTriShape,
    inherited_props: &[BlockRef],
) -> VertexColorMode {
    // Shape-level properties take priority over inherited.
    for prop_ref in shape.av.properties.iter().chain(inherited_props.iter()) {
        let Some(idx) = prop_ref.index() else {
            continue;
        };
        if let Some(vcol) = scene.get_as::<NiVertexColorProperty>(idx) {
            return VertexColorMode::from_source_mode(vcol.vertex_mode);
        }
    }
    VertexColorMode::AmbientDiffuse
}

/// Extract all material properties from a NiTriShape in a single pass.
///
/// `inherited_props` carries property BlockRefs accumulated from parent
/// NiNodes during the scene graph walk. Gamebryo propagates properties
/// down the hierarchy — child shapes inherit parent properties unless
/// they override them with their own. Shape-level properties take
/// priority; inherited properties fill in any gaps. See #208.
pub(super) fn extract_material_info(
    scene: &NifScene,
    shape: &NiTriShape,
    inherited_props: &[BlockRef],
) -> MaterialInfo {
    let mut info = MaterialInfo::default();

    // Skyrim+: dedicated shader_property_ref
    if let Some(idx) = shape.shader_property_ref.index() {
        if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
            if let Some(name) = shader.net.name.as_deref() {
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".bgsm") || lower.ends_with(".bgem") {
                    info.material_path = Some(name.to_string());
                }
            }
            if let Some(ts_idx) = shader.texture_set_ref.index() {
                if let Some(tex_set) = scene.get_as::<BSShaderTextureSet>(ts_idx) {
                    if let Some(path) = tex_set.textures.first() {
                        if !path.is_empty() {
                            info.texture_path = Some(path.clone());
                        }
                    }
                    // Normal map is textures[1] in BSShaderTextureSet.
                    if let Some(normal) = tex_set.textures.get(1) {
                        if !normal.is_empty() {
                            info.normal_map = Some(normal.clone());
                        }
                    }
                    // Glow / emissive map is textures[2].
                    if info.glow_map.is_none() {
                        if let Some(glow) = tex_set.textures.get(2).filter(|s| !s.is_empty()) {
                            info.glow_map = Some(glow.clone());
                        }
                    }
                }
            }
            if shader.shader_flags_2 & 0x10 != 0 {
                info.two_sided = true;
            }
            if shader.shader_flags_1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0 {
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
            info.alpha = shader.alpha;
            info.material_kind = shader.shader_type as u8;
            apply_shader_type_data(&mut info, &shader.shader_type_data);
            info.has_material_data = true;
        }
        if let Some(shader) = scene.get_as::<BSEffectShaderProperty>(idx) {
            if info.material_path.is_none() {
                info.material_path =
                    crate::import::mesh::material_path_from_name(shader.net.name.as_deref());
            }
            if info.texture_path.is_none() && !shader.source_texture.is_empty() {
                info.texture_path = Some(shader.source_texture.clone());
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
                info.has_material_data = true;
            }
            // Capture the rich effect-shader fields (falloff cone,
            // greyscale palette, FO4+/FO76 companion textures, etc.)
            // so downstream consumers can route them when the renderer-
            // side dispatch lands. See #345 / audit S4-01.
            info.effect_shader = Some(capture_effect_shader_data(shader));
        }
    }

    // Skyrim+: dedicated alpha_property_ref
    if let Some(idx) = shape.alpha_property_ref.index() {
        if let Some(alpha) = scene.get_as::<NiAlphaProperty>(idx) {
            apply_alpha_flags(&mut info, alpha);
        }
    }

    // FO3/FNV/Oblivion: single pass over shape + inherited properties.
    // Shape properties first so they take priority (#208).
    for prop_ref in shape.av.properties.iter().chain(inherited_props.iter()) {
        let Some(idx) = prop_ref.index() else {
            continue;
        };

        if !info.alpha_blend && !info.alpha_test {
            if let Some(alpha) = scene.get_as::<NiAlphaProperty>(idx) {
                apply_alpha_flags(&mut info, alpha);
            }
        }

        // NiZBufferProperty — depth test/write mode.
        if let Some(zbuf) = scene.get_as::<crate::blocks::properties::NiZBufferProperty>(idx) {
            info.z_test = zbuf.z_test_enabled;
            info.z_write = zbuf.z_write_enabled;
        }

        // NiMaterialProperty — capture specular/emissive/shininess/alpha.
        if !info.has_material_data {
            if let Some(mat) = scene.get_as::<NiMaterialProperty>(idx) {
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
                if let Some(path) = tex_desc_source_path(scene, tex_prop.base_texture.as_ref()) {
                    info.texture_path = Some(path);
                }
            }
            // Oblivion stores tangent-space normal maps in the `bump_texture`
            // slot (the dedicated `normal_texture` slot landed later in FO3).
            // Skyrim+ meshes use BSShaderTextureSet handled elsewhere, so
            // this branch is specifically for pre-Skyrim static meshes.
            // See issue #131.
            if info.normal_map.is_none() {
                if let Some(path) = tex_desc_source_path(scene, tex_prop.normal_texture.as_ref())
                    .or_else(|| tex_desc_source_path(scene, tex_prop.bump_texture.as_ref()))
                {
                    info.normal_map = Some(path);
                }
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
                if let Some(path) = tex_desc_source_path(scene, tex_prop.glow_texture.as_ref()) {
                    info.glow_map = Some(path);
                }
            }
            if info.detail_map.is_none() {
                if let Some(path) = tex_desc_source_path(scene, tex_prop.detail_texture.as_ref()) {
                    info.detail_map = Some(path);
                }
            }
            if info.gloss_map.is_none() {
                if let Some(path) = tex_desc_source_path(scene, tex_prop.gloss_texture.as_ref()) {
                    info.gloss_map = Some(path);
                }
            }
            // Dark / multiplicative lightmap (slot 1). Baked shadow data
            // on Oblivion interior architecture — `albedo *= dark`. #264.
            if info.dark_map.is_none() {
                if let Some(path) = tex_desc_source_path(scene, tex_prop.dark_texture.as_ref()) {
                    info.dark_map = Some(path);
                }
            }
            // Propagate the base slot's UV transform to the shared
            // `uv_offset` / `uv_scale` fields. The renderer shader applies
            // them per-vertex to every sampled texture — fine for the
            // common case where base, detail, glow and parallax share a
            // UV set, which holds for Oblivion/FO3/FNV static meshes. See
            // issue #219. Only overwrite the defaults — a BSShader path
            // earlier in the pass may have already set these.
            if !info.has_material_data {
                if let Some(base) = tex_prop.base_texture.as_ref() {
                    if let Some(tx) = base.transform {
                        info.uv_offset = tx.translation;
                        info.uv_scale = tx.scale;
                    }
                }
            }
        }

        if let Some(shader) = scene.get_as::<BSShaderPPLightingProperty>(idx) {
            if let Some(ts_idx) = shader.texture_set_ref.index() {
                if let Some(tex_set) = scene.get_as::<BSShaderTextureSet>(ts_idx) {
                    if info.texture_path.is_none() {
                        if let Some(path) = tex_set.textures.first() {
                            if !path.is_empty() {
                                info.texture_path = Some(path.clone());
                            }
                        }
                    }
                    // Normal map is textures[1] in BSShaderTextureSet (same layout as Skyrim).
                    if info.normal_map.is_none() {
                        if let Some(normal) = tex_set.textures.get(1) {
                            if !normal.is_empty() {
                                info.normal_map = Some(normal.clone());
                            }
                        }
                    }
                    // Glow / emissive map is textures[2].
                    if info.glow_map.is_none() {
                        if let Some(glow) = tex_set.textures.get(2).filter(|s| !s.is_empty()) {
                            info.glow_map = Some(glow.clone());
                        }
                    }
                }
            }
            if shader.shader.shader_flags_1 & SF_DOUBLE_SIDED != 0 {
                info.two_sided = true;
            }
            if shader.shader.shader_flags_1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0
                || shader.shader.shader_flags_2 & ALPHA_DECAL_F2 != 0
            {
                info.is_decal = true;
            }
        }

        if let Some(shader) = scene.get_as::<BSShaderNoLightingProperty>(idx) {
            if info.texture_path.is_none() && !shader.file_name.is_empty() {
                info.texture_path = Some(shader.file_name.clone());
            }
            if shader.shader.shader_flags_1 & SF_DOUBLE_SIDED != 0 {
                info.two_sided = true;
            }
            if shader.shader.shader_flags_1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0 {
                info.is_decal = true;
            }
        }

        // NiStencilProperty — proper parser replaces NiUnknown heuristic.
        if !info.two_sided {
            if let Some(stencil) = scene.get_as::<NiStencilProperty>(idx) {
                if stencil.is_two_sided() {
                    info.two_sided = true;
                }
            }
        }

        // NiSpecularProperty (issue #220) — bit 0 of flags is the enable
        // toggle. Many matte surfaces in Oblivion/FNV set flags=0 here to
        // explicitly disable specular; without honoring this, every wall
        // and ceiling panel gets a bright PBR specular highlight from
        // point lights that looks like a lighting artifact.
        //
        // We use a type_name match because `NiFlagProperty` is shared by
        // NiSpecular/Wireframe/Dither/Shade and we only care about
        // specular here.
        if let Some(flag_prop) = scene.get_as::<NiFlagProperty>(idx) {
            if flag_prop.block_type_name() == "NiSpecularProperty" && !flag_prop.enabled() {
                info.specular_enabled = false;
            }
        }

        // NiVertexColorProperty (#214) — controls how per-vertex colors
        // participate in shading. The default is AmbientDiffuse; the
        // mesh may instead request Ignore (don't use vertex colors at
        // all) or Emissive (route them to self-illumination). The
        // actual behavior split on Ignore is enforced by
        // `extract_material` below when it decides whether to return
        // the vertex color vec or fall back to the material diffuse.
        if let Some(vcol) = scene.get_as::<NiVertexColorProperty>(idx) {
            info.vertex_color_mode = VertexColorMode::from_source_mode(vcol.vertex_mode);
        }
    }

    // Zero out specular strength when the property is disabled. We do
    // this once at the end so later code (pipeline selection, draw
    // command population) doesn't need to know about the flag.
    if !info.specular_enabled {
        info.specular_strength = 0.0;
    }

    info
}

/// Decode an NiAlphaProperty onto a `MaterialInfo`. `NiAlphaProperty.flags`
/// packs both alpha-blend (bit 0) and alpha-test (bit 9, mask 0x200) per
/// nif.xml; `threshold` is a u8 in [0, 255]. See issue #152.
///
/// When a material sets both bits (common on Gamebryo foliage, hair,
/// chain-link fences) we prefer alpha-test over alpha-blend — the
/// discard + opaque-depth path gives clean cutouts without the z-sort
/// artifacts that plague back-to-front blend on statics. `alpha_blend`
/// is intentionally set to `false` in that case so the renderer binds
/// the opaque pipeline.
/// Write every `ShaderTypeData` variant's trailing fields onto
/// `MaterialInfo`. Previously only `EnvironmentMap` was consumed; the
/// remaining 8 variants (SkinTint, Fo76SkinTint, HairTint, ParallaxOcc,
/// MultiLayerParallax, SparkleSnow, EyeEnvmap, and the `None`
/// pass-through for types that carry no trailing data) were pattern-
/// matched out and dropped. Issue #343 / SK-D3-01.
///
/// Renderer-side dispatch on `MaterialInfo.material_kind` is tracked
/// separately (SK-D3-02). Until that lands these values ride unused on
/// the `Material` component; the purpose here is to ensure no variant
/// is silently discarded at the import boundary.
pub(super) fn apply_shader_type_data(info: &mut MaterialInfo, data: &ShaderTypeData) {
    // Env-map scale lives on its own field for backwards compatibility with
    // pre-#343 readers; the other variants copy through `ShaderTypeFields`.
    if let ShaderTypeData::EnvironmentMap { env_map_scale } = *data {
        info.env_map_scale = env_map_scale;
    }
    let fields = capture_shader_type_fields(data);
    info.skin_tint_color = fields.skin_tint_color.or(info.skin_tint_color);
    info.skin_tint_alpha = fields.skin_tint_alpha.or(info.skin_tint_alpha);
    info.hair_tint_color = fields.hair_tint_color.or(info.hair_tint_color);
    info.eye_cubemap_scale = fields.eye_cubemap_scale.or(info.eye_cubemap_scale);
    info.eye_left_reflection_center = fields
        .eye_left_reflection_center
        .or(info.eye_left_reflection_center);
    info.eye_right_reflection_center = fields
        .eye_right_reflection_center
        .or(info.eye_right_reflection_center);
    info.parallax_max_passes = fields.parallax_max_passes.or(info.parallax_max_passes);
    info.parallax_height_scale = fields
        .parallax_height_scale
        .or(info.parallax_height_scale);
    info.multi_layer_inner_thickness = fields
        .multi_layer_inner_thickness
        .or(info.multi_layer_inner_thickness);
    info.multi_layer_refraction_scale = fields
        .multi_layer_refraction_scale
        .or(info.multi_layer_refraction_scale);
    info.multi_layer_inner_layer_scale = fields
        .multi_layer_inner_layer_scale
        .or(info.multi_layer_inner_layer_scale);
    info.multi_layer_envmap_strength = fields
        .multi_layer_envmap_strength
        .or(info.multi_layer_envmap_strength);
    info.sparkle_parameters = fields.sparkle_parameters.or(info.sparkle_parameters);
}

/// The 13 shader-type-specific fields pulled off `BSLightingShaderProperty`'s
/// `shader_type_data` variant. Mirrors the flat fields on `MaterialInfo` so
/// both the NiTriShape path (via `MaterialInfo`) and the BsTriShape path
/// (direct) can populate the same `ImportedMesh` fields without duplication.
/// See #430 / NIF-D4-N01.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShaderTypeFields {
    pub skin_tint_color: Option<[f32; 3]>,
    pub skin_tint_alpha: Option<f32>,
    pub hair_tint_color: Option<[f32; 3]>,
    pub eye_cubemap_scale: Option<f32>,
    pub eye_left_reflection_center: Option<[f32; 3]>,
    pub eye_right_reflection_center: Option<[f32; 3]>,
    pub parallax_max_passes: Option<f32>,
    pub parallax_height_scale: Option<f32>,
    pub multi_layer_inner_thickness: Option<f32>,
    pub multi_layer_refraction_scale: Option<f32>,
    pub multi_layer_inner_layer_scale: Option<[f32; 2]>,
    pub multi_layer_envmap_strength: Option<f32>,
    pub sparkle_parameters: Option<[f32; 4]>,
}

/// Pull the shader-type-specific trailing fields out of a `ShaderTypeData`
/// into a flat `ShaderTypeFields` bundle. Complements
/// [`apply_shader_type_data`] — both are exhaustive on the 9 variants so
/// any future addition fails compilation here.
pub(crate) fn capture_shader_type_fields(data: &ShaderTypeData) -> ShaderTypeFields {
    let mut f = ShaderTypeFields::default();
    match *data {
        ShaderTypeData::None | ShaderTypeData::EnvironmentMap { .. } => {}
        ShaderTypeData::SkinTint { skin_tint_color } => {
            f.skin_tint_color = Some(skin_tint_color);
        }
        ShaderTypeData::Fo76SkinTint { skin_tint_color } => {
            f.skin_tint_color = Some([skin_tint_color[0], skin_tint_color[1], skin_tint_color[2]]);
            f.skin_tint_alpha = Some(skin_tint_color[3]);
        }
        ShaderTypeData::HairTint { hair_tint_color } => {
            f.hair_tint_color = Some(hair_tint_color);
        }
        ShaderTypeData::ParallaxOcc { max_passes, scale } => {
            f.parallax_max_passes = Some(max_passes);
            f.parallax_height_scale = Some(scale);
        }
        ShaderTypeData::MultiLayerParallax {
            inner_layer_thickness,
            refraction_scale,
            inner_layer_texture_scale,
            envmap_strength,
        } => {
            f.multi_layer_inner_thickness = Some(inner_layer_thickness);
            f.multi_layer_refraction_scale = Some(refraction_scale);
            f.multi_layer_inner_layer_scale = Some(inner_layer_texture_scale);
            f.multi_layer_envmap_strength = Some(envmap_strength);
        }
        ShaderTypeData::SparkleSnow { sparkle_parameters } => {
            f.sparkle_parameters = Some(sparkle_parameters);
        }
        ShaderTypeData::EyeEnvmap {
            eye_cubemap_scale,
            left_eye_reflection_center,
            right_eye_reflection_center,
        } => {
            f.eye_cubemap_scale = Some(eye_cubemap_scale);
            f.eye_left_reflection_center = Some(left_eye_reflection_center);
            f.eye_right_reflection_center = Some(right_eye_reflection_center);
        }
    }
    f
}

impl MaterialInfo {
    /// Project this `MaterialInfo`'s shader-type fields into a
    /// `ShaderTypeFields` bundle for `ImportedMesh`. See #430.
    pub(super) fn shader_type_fields(&self) -> ShaderTypeFields {
        ShaderTypeFields {
            skin_tint_color: self.skin_tint_color,
            skin_tint_alpha: self.skin_tint_alpha,
            hair_tint_color: self.hair_tint_color,
            eye_cubemap_scale: self.eye_cubemap_scale,
            eye_left_reflection_center: self.eye_left_reflection_center,
            eye_right_reflection_center: self.eye_right_reflection_center,
            parallax_max_passes: self.parallax_max_passes,
            parallax_height_scale: self.parallax_height_scale,
            multi_layer_inner_thickness: self.multi_layer_inner_thickness,
            multi_layer_refraction_scale: self.multi_layer_refraction_scale,
            multi_layer_inner_layer_scale: self.multi_layer_inner_layer_scale,
            multi_layer_envmap_strength: self.multi_layer_envmap_strength,
            sparkle_parameters: self.sparkle_parameters,
        }
    }
}

pub(super) fn apply_alpha_flags(info: &mut MaterialInfo, alpha: &NiAlphaProperty) {
    let blend = alpha.flags & 0x001 != 0;
    let test = alpha.flags & 0x200 != 0;
    // Extract blend factors regardless of which mode wins — they're
    // needed if the mesh later ends up blended (e.g., animated alpha).
    info.src_blend_mode = ((alpha.flags >> 1) & 0xF) as u8; // bits 1–4
    info.dst_blend_mode = ((alpha.flags >> 5) & 0xF) as u8; // bits 5–8
    if test {
        info.alpha_test = true;
        info.alpha_threshold = alpha.threshold as f32 / 255.0;
        // Bits 10-12: alpha test comparison function (3 bits, 0–7).
        info.alpha_test_func = ((alpha.flags & 0x1C00) >> 10) as u8;
        // Prefer cutout to blending when both are set.
        info.alpha_blend = false;
    } else if blend {
        info.alpha_blend = true;
    }
}

/// Resolve a `TexDesc` slot on an `NiTexturingProperty` to a texture
/// filename by following its `source_ref` through the scene's block
/// table and pulling the filename from the referenced
/// `NiSourceTexture`. Returns `None` if the slot is empty, the ref
/// is null, or the source texture has no external filename (embedded
/// NiPixelData is not supported here — the downstream texture
/// provider can't resolve those anyway). See issue #131.
fn tex_desc_source_path(scene: &NifScene, desc: Option<&TexDesc>) -> Option<String> {
    let desc = desc?;
    let src_idx = desc.source_ref.index()?;
    let src_tex = scene.get_as::<NiSourceTexture>(src_idx)?;
    src_tex.filename.as_ref().map(|f| f.to_string())
}

/// Check if a BsTriShape is decal geometry (Skyrim+).
///
/// Both shader-property variants Skyrim+ binds carry the same flag bits:
/// `BSLightingShaderProperty` (the common case for static / clutter /
/// actor meshes) and `BSEffectShaderProperty` (VFX surfaces — blood
/// splats, gore overlays, magic decals, scorch marks). Pre-#346 only
/// the BSLightingShaderProperty branch was checked, so effect-shader
/// decals rendered as opaque coplanar triangles → z-fighting against
/// the surface they overlay.
pub(super) fn find_decal_bs(scene: &NifScene, shape: &BsTriShape) -> bool {
    let Some(idx) = shape.shader_property_ref.index() else {
        return false;
    };
    if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
        if shader.shader_flags_1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0 {
            return true;
        }
    }
    if let Some(shader) = scene.get_as::<BSEffectShaderProperty>(idx) {
        if shader.shader_flags_1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0
            || shader.shader_flags_2 & ALPHA_DECAL_F2 != 0
        {
            return true;
        }
    }
    false
}

/// Resolve the [`BsEffectShaderData`] for a BsTriShape — returns
/// `None` when the linked shader is not a `BSEffectShaderProperty`.
/// Used by [`crate::import::mesh::extract_bs_tri_shape`] to populate
/// the `effect_shader` field that previously was hardcoded to `None`.
/// See #346 / audit S4-02.
pub(super) fn find_effect_shader_bs(
    scene: &NifScene,
    shape: &BsTriShape,
) -> Option<BsEffectShaderData> {
    let idx = shape.shader_property_ref.index()?;
    let shader = scene.get_as::<BSEffectShaderProperty>(idx)?;
    Some(capture_effect_shader_data(shader))
}

#[cfg(test)]
mod alpha_flag_tests {
    //! Regression tests for issue #152 — NiAlphaProperty bit extraction.
    //! Verify the cutout-vs-blend precedence and threshold scaling.
    use super::*;
    use crate::blocks::base::NiObjectNETData;

    fn alpha_prop(flags: u16, threshold: u8) -> NiAlphaProperty {
        NiAlphaProperty {
            net: NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: crate::types::BlockRef::NULL,
            },
            flags,
            threshold,
        }
    }

    #[test]
    fn alpha_blend_only_sets_blend() {
        let mut info = MaterialInfo::default();
        apply_alpha_flags(&mut info, &alpha_prop(0x0001, 128));
        assert!(info.alpha_blend);
        assert!(!info.alpha_test);
        assert_eq!(info.alpha_threshold, 0.0);
    }

    #[test]
    fn alpha_test_only_sets_test_and_scales_threshold() {
        let mut info = MaterialInfo::default();
        apply_alpha_flags(&mut info, &alpha_prop(0x0200, 128));
        assert!(!info.alpha_blend);
        assert!(info.alpha_test);
        assert!((info.alpha_threshold - (128.0 / 255.0)).abs() < 1e-5);
    }

    #[test]
    fn alpha_test_and_blend_prefers_test() {
        // Foliage with both bits set: alpha-test wins because the
        // discard + depth-write path sorts cleanly without back-to-front
        // pre-sort of the alpha-blend pipeline.
        let mut info = MaterialInfo::default();
        apply_alpha_flags(&mut info, &alpha_prop(0x0201, 200));
        assert!(!info.alpha_blend, "alpha_blend should yield to alpha_test");
        assert!(info.alpha_test);
        assert!((info.alpha_threshold - (200.0 / 255.0)).abs() < 1e-5);
    }

    #[test]
    fn neither_bit_leaves_defaults() {
        let mut info = MaterialInfo::default();
        apply_alpha_flags(&mut info, &alpha_prop(0x0000, 255));
        assert!(!info.alpha_blend);
        assert!(!info.alpha_test);
        assert_eq!(info.alpha_threshold, 0.0);
    }

    #[test]
    fn threshold_extremes_clamp_expected_range() {
        let mut info_min = MaterialInfo::default();
        apply_alpha_flags(&mut info_min, &alpha_prop(0x0200, 0));
        assert_eq!(info_min.alpha_threshold, 0.0);

        let mut info_max = MaterialInfo::default();
        apply_alpha_flags(&mut info_max, &alpha_prop(0x0200, 255));
        assert!((info_max.alpha_threshold - 1.0).abs() < 1e-5);
    }

    /// #263: alpha test function bits 10-12 are extracted.
    #[test]
    fn alpha_test_func_greaterequal_default() {
        // flags = 0x1A00: test enable (0x200) + GREATEREQUAL (6 << 10 = 0x1800)
        let mut info = MaterialInfo::default();
        apply_alpha_flags(&mut info, &alpha_prop(0x1A00, 128));
        assert!(info.alpha_test);
        assert_eq!(info.alpha_test_func, 6); // GREATEREQUAL
    }

    #[test]
    fn alpha_test_func_less() {
        // flags = 0x0600: test enable (0x200) + LESS (1 << 10 = 0x400)
        let mut info = MaterialInfo::default();
        apply_alpha_flags(&mut info, &alpha_prop(0x0600, 64));
        assert!(info.alpha_test);
        assert_eq!(info.alpha_test_func, 1); // LESS
    }

    #[test]
    fn alpha_test_func_always() {
        // flags = 0x0200: test enable (0x200) + ALWAYS (0 << 10 = 0x000)
        let mut info = MaterialInfo::default();
        apply_alpha_flags(&mut info, &alpha_prop(0x0200, 128));
        assert!(info.alpha_test);
        assert_eq!(info.alpha_test_func, 0); // ALWAYS
    }

    #[test]
    fn alpha_test_func_default_when_no_test() {
        // When alpha test is disabled, func should stay at default (6).
        let info = MaterialInfo::default();
        assert_eq!(info.alpha_test_func, 6); // GREATEREQUAL default
    }
}

/// Regression tests for issue #345 — `BSEffectShaderProperty` rich
/// material fields used to be dropped on import. The capture path is
/// covered by direct `capture_effect_shader_data` tests; full
/// `extract_material_info` coverage requires a synthetic NIF and is
/// blocked on test infrastructure (`NifScene` doesn't expose enough
/// mutators to wire one up cheaply). The capture helper is the entire
/// transform under test — `extract_material_info` just calls it.
#[cfg(test)]
mod effect_shader_capture_tests {
    use super::*;
    use crate::blocks::base::NiObjectNETData;
    use crate::blocks::shader::BSEffectShaderProperty;
    use crate::types::BlockRef;

    /// Build a fully-populated FO4-style `BSEffectShaderProperty` with
    /// every field set to a distinct, recognisable value.
    fn fully_populated_fo4_shader() -> BSEffectShaderProperty {
        BSEffectShaderProperty {
            net: NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            material_reference: false,
            shader_flags_1: 0,
            shader_flags_2: 0,
            sf1_crcs: Vec::new(),
            sf2_crcs: Vec::new(),
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            source_texture: "fx/glow.dds".to_string(),
            texture_clamp_mode: 3,
            lighting_influence: 200,
            env_map_min_lod: 4,
            falloff_start_angle: 0.95,
            falloff_stop_angle: 0.30,
            falloff_start_opacity: 1.0,
            falloff_stop_opacity: 0.0,
            refraction_power: 0.0, // pre-FO76 default
            base_color: [0.0; 4],
            base_color_scale: 1.0,
            soft_falloff_depth: 8.0,
            greyscale_texture: "fx/grad.dds".to_string(),
            env_map_texture: "fx/env.dds".to_string(),
            normal_texture: "fx/n.dds".to_string(),
            env_mask_texture: "fx/mask.dds".to_string(),
            env_map_scale: 1.5,
            reflectance_texture: String::new(),
            lighting_texture: String::new(),
            emittance_color: [0.0; 3],
            emit_gradient_texture: String::new(),
            luminance: None,
        }
    }

    #[test]
    fn capture_lifts_every_rich_field() {
        let shader = fully_populated_fo4_shader();
        let captured = capture_effect_shader_data(&shader);
        assert_eq!(captured.falloff_start_angle, 0.95);
        assert_eq!(captured.falloff_stop_angle, 0.30);
        assert_eq!(captured.falloff_start_opacity, 1.0);
        assert_eq!(captured.falloff_stop_opacity, 0.0);
        assert_eq!(captured.soft_falloff_depth, 8.0);
        assert_eq!(captured.lighting_influence, 200);
        assert_eq!(captured.env_map_min_lod, 4);
        assert_eq!(captured.texture_clamp_mode, 3);
        assert_eq!(captured.env_map_scale, 1.5);
        assert_eq!(captured.greyscale_texture.as_deref(), Some("fx/grad.dds"));
        assert_eq!(captured.env_map_texture.as_deref(), Some("fx/env.dds"));
        assert_eq!(captured.normal_texture.as_deref(), Some("fx/n.dds"));
        assert_eq!(captured.env_mask_texture.as_deref(), Some("fx/mask.dds"));
        // Pre-FO76: refraction_power = 0.0 surfaces as None.
        assert_eq!(captured.refraction_power, None);
    }

    #[test]
    fn capture_collapses_empty_texture_strings_to_none() {
        let mut shader = fully_populated_fo4_shader();
        shader.greyscale_texture.clear();
        shader.env_map_texture.clear();
        shader.normal_texture.clear();
        shader.env_mask_texture.clear();
        let captured = capture_effect_shader_data(&shader);
        assert_eq!(captured.greyscale_texture, None);
        assert_eq!(captured.env_map_texture, None);
        assert_eq!(captured.normal_texture, None);
        assert_eq!(captured.env_mask_texture, None);
    }

    #[test]
    fn capture_surfaces_fo76_refraction_power() {
        let mut shader = fully_populated_fo4_shader();
        shader.refraction_power = 0.5;
        let captured = capture_effect_shader_data(&shader);
        assert_eq!(captured.refraction_power, Some(0.5));
    }

    #[test]
    fn material_info_default_has_no_effect_shader() {
        // Sibling check — the new field defaults to `None` so non-effect
        // materials don't get spurious capture data.
        let info = MaterialInfo::default();
        assert!(info.effect_shader.is_none());
    }
}

/// Regression tests for issue #343 — exhaustive ShaderTypeData dispatch.
/// Previously only `EnvironmentMap` reached MaterialInfo; the remaining
/// 8 variants (SkinTint, Fo76SkinTint, HairTint, ParallaxOcc,
/// MultiLayerParallax, SparkleSnow, EyeEnvmap, None) were dropped. Each
/// test exercises one arm of `apply_shader_type_data`.
#[cfg(test)]
mod shader_type_data_tests {
    use super::*;

    #[test]
    fn none_variant_leaves_all_shader_type_fields_at_defaults() {
        let mut info = MaterialInfo::default();
        apply_shader_type_data(&mut info, &ShaderTypeData::None);
        assert_eq!(info.env_map_scale, 0.0);
        assert_eq!(info.skin_tint_color, None);
        assert_eq!(info.hair_tint_color, None);
        assert_eq!(info.parallax_max_passes, None);
        assert_eq!(info.multi_layer_inner_thickness, None);
        assert_eq!(info.sparkle_parameters, None);
        assert_eq!(info.eye_cubemap_scale, None);
    }

    #[test]
    fn environment_map_writes_scale() {
        let mut info = MaterialInfo::default();
        apply_shader_type_data(
            &mut info,
            &ShaderTypeData::EnvironmentMap { env_map_scale: 2.5 },
        );
        assert_eq!(info.env_map_scale, 2.5);
    }

    /// #430 — `capture_shader_type_fields` is the shared helper the
    /// BsTriShape import path uses. Exhaustive per-variant check that the
    /// returned bundle matches what `apply_shader_type_data` writes into
    /// MaterialInfo.
    #[test]
    fn capture_helper_parity_with_apply() {
        for data in &[
            ShaderTypeData::None,
            ShaderTypeData::EnvironmentMap { env_map_scale: 2.5 },
            ShaderTypeData::SkinTint {
                skin_tint_color: [0.8, 0.6, 0.5],
            },
            ShaderTypeData::Fo76SkinTint {
                skin_tint_color: [0.9, 0.7, 0.55, 0.25],
            },
            ShaderTypeData::HairTint {
                hair_tint_color: [0.3, 0.15, 0.05],
            },
            ShaderTypeData::ParallaxOcc {
                max_passes: 16.0,
                scale: 0.05,
            },
            ShaderTypeData::MultiLayerParallax {
                inner_layer_thickness: 0.1,
                refraction_scale: 0.5,
                inner_layer_texture_scale: [2.0, 2.0],
                envmap_strength: 1.25,
            },
            ShaderTypeData::SparkleSnow {
                sparkle_parameters: [1.0, 0.5, 0.25, 2.0],
            },
            ShaderTypeData::EyeEnvmap {
                eye_cubemap_scale: 1.5,
                left_eye_reflection_center: [0.1, 0.2, 0.3],
                right_eye_reflection_center: [0.4, 0.5, 0.6],
            },
        ] {
            let mut info = MaterialInfo::default();
            apply_shader_type_data(&mut info, data);
            assert_eq!(
                info.shader_type_fields(),
                capture_shader_type_fields(data),
                "variant {:?} must produce identical fields via apply and capture",
                data
            );
        }
    }

    #[test]
    fn skin_tint_writes_rgb() {
        let mut info = MaterialInfo::default();
        apply_shader_type_data(
            &mut info,
            &ShaderTypeData::SkinTint {
                skin_tint_color: [0.8, 0.6, 0.5],
            },
        );
        assert_eq!(info.skin_tint_color, Some([0.8, 0.6, 0.5]));
        assert_eq!(info.skin_tint_alpha, None);
    }

    #[test]
    fn fo76_skin_tint_splits_rgba_into_rgb_plus_alpha() {
        let mut info = MaterialInfo::default();
        apply_shader_type_data(
            &mut info,
            &ShaderTypeData::Fo76SkinTint {
                skin_tint_color: [0.9, 0.7, 0.55, 0.25],
            },
        );
        assert_eq!(info.skin_tint_color, Some([0.9, 0.7, 0.55]));
        assert_eq!(info.skin_tint_alpha, Some(0.25));
    }

    #[test]
    fn hair_tint_writes_rgb() {
        let mut info = MaterialInfo::default();
        apply_shader_type_data(
            &mut info,
            &ShaderTypeData::HairTint {
                hair_tint_color: [0.3, 0.15, 0.05],
            },
        );
        assert_eq!(info.hair_tint_color, Some([0.3, 0.15, 0.05]));
    }

    #[test]
    fn parallax_occ_writes_passes_and_scale() {
        let mut info = MaterialInfo::default();
        apply_shader_type_data(
            &mut info,
            &ShaderTypeData::ParallaxOcc {
                max_passes: 16.0,
                scale: 0.04,
            },
        );
        assert_eq!(info.parallax_max_passes, Some(16.0));
        assert_eq!(info.parallax_height_scale, Some(0.04));
    }

    #[test]
    fn multi_layer_parallax_writes_all_four_fields() {
        let mut info = MaterialInfo::default();
        apply_shader_type_data(
            &mut info,
            &ShaderTypeData::MultiLayerParallax {
                inner_layer_thickness: 0.1,
                refraction_scale: 1.2,
                inner_layer_texture_scale: [2.0, 3.0],
                envmap_strength: 0.75,
            },
        );
        assert_eq!(info.multi_layer_inner_thickness, Some(0.1));
        assert_eq!(info.multi_layer_refraction_scale, Some(1.2));
        assert_eq!(info.multi_layer_inner_layer_scale, Some([2.0, 3.0]));
        assert_eq!(info.multi_layer_envmap_strength, Some(0.75));
    }

    #[test]
    fn sparkle_snow_writes_all_four_parameters() {
        let mut info = MaterialInfo::default();
        apply_shader_type_data(
            &mut info,
            &ShaderTypeData::SparkleSnow {
                sparkle_parameters: [1.0, 0.5, 0.25, 2.0],
            },
        );
        assert_eq!(info.sparkle_parameters, Some([1.0, 0.5, 0.25, 2.0]));
    }

    #[test]
    fn eye_envmap_writes_scale_and_both_reflection_centers() {
        let mut info = MaterialInfo::default();
        apply_shader_type_data(
            &mut info,
            &ShaderTypeData::EyeEnvmap {
                eye_cubemap_scale: 1.5,
                left_eye_reflection_center: [-0.03, 0.05, 0.0],
                right_eye_reflection_center: [0.03, 0.05, 0.0],
            },
        );
        assert_eq!(info.eye_cubemap_scale, Some(1.5));
        assert_eq!(info.eye_left_reflection_center, Some([-0.03, 0.05, 0.0]));
        assert_eq!(info.eye_right_reflection_center, Some([0.03, 0.05, 0.0]));
    }

    #[test]
    fn environment_map_does_not_touch_other_variants_fields() {
        // Sanity: a mesh with env-map shader leaves skin/hair/eye/etc.
        // fields at None. Previous behavior was an if-let that matched
        // only EnvironmentMap, so this test would have passed before too
        // — but it's a guard against a future "clear all variants"
        // regression where the match arm accidentally stomps fields.
        let mut info = MaterialInfo::default();
        info.hair_tint_color = Some([0.1, 0.2, 0.3]); // pretend something else set this first
        apply_shader_type_data(
            &mut info,
            &ShaderTypeData::EnvironmentMap { env_map_scale: 1.0 },
        );
        assert_eq!(info.hair_tint_color, Some([0.1, 0.2, 0.3]));
    }
}

/// Regression tests for issue #214 — NiTexturingProperty secondary slots
/// and NiVertexColorProperty mode extraction.
#[cfg(test)]
mod secondary_slot_tests {
    use super::*;

    #[test]
    fn vertex_color_mode_decodes_all_three_values() {
        assert_eq!(
            VertexColorMode::from_source_mode(0),
            VertexColorMode::Ignore
        );
        assert_eq!(
            VertexColorMode::from_source_mode(1),
            VertexColorMode::Emissive
        );
        assert_eq!(
            VertexColorMode::from_source_mode(2),
            VertexColorMode::AmbientDiffuse
        );
    }

    #[test]
    fn vertex_color_mode_unknown_falls_back_to_default() {
        // Gamebryo uses values > 2 in some test/mod content — fall back
        // to AmbientDiffuse instead of a hard error.
        assert_eq!(
            VertexColorMode::from_source_mode(99),
            VertexColorMode::AmbientDiffuse
        );
    }

    #[test]
    fn vertex_color_mode_repr_u8_matches_gamebryo_source_mode() {
        // Pin the discriminant layout — `Ignore=0, Emissive=1,
        // AmbientDiffuse=2` matches Gamebryo's nif.xml `SourceMode`
        // enum. ImportedMesh stores this as u8 via `as u8` cast and
        // downstream consumers compare against literal 0/1/2.
        assert_eq!(VertexColorMode::Ignore as u8, 0);
        assert_eq!(VertexColorMode::Emissive as u8, 1);
        assert_eq!(VertexColorMode::AmbientDiffuse as u8, 2);
    }

    #[test]
    fn default_material_info_has_no_dark_map() {
        let info = MaterialInfo::default();
        assert!(info.dark_map.is_none(), "dark_map should default to None");
    }

    #[test]
    fn default_material_info_has_no_secondary_maps_and_default_mode() {
        let info = MaterialInfo::default();
        assert!(info.glow_map.is_none());
        assert!(info.detail_map.is_none());
        assert!(info.gloss_map.is_none());
        assert_eq!(info.vertex_color_mode, VertexColorMode::AmbientDiffuse);
    }
}
