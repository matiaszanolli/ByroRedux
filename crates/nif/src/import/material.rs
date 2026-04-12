//! NIF material and texture property extraction.

use crate::blocks::properties::{
    NiAlphaProperty, NiFlagProperty, NiMaterialProperty, NiStencilProperty, NiTexturingProperty,
    NiVertexColorProperty, TexDesc,
};
use crate::blocks::NiObject;
use crate::blocks::shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderNoLightingProperty,
    BSShaderPPLightingProperty, BSShaderTextureSet, ShaderTypeData,
};
use crate::blocks::texture::NiSourceTexture;
use crate::blocks::tri_shape::{BsTriShape, NiTriShape};
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
    /// How vertex colors should participate in shading. See #214 /
    /// `VertexColorMode`. Defaults to `AmbientDiffuse` — the value
    /// Gamebryo uses when the NIF has no `NiVertexColorProperty`.
    pub vertex_color_mode: VertexColorMode,
    pub alpha_blend: bool,
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
}

impl Default for MaterialInfo {
    fn default() -> Self {
        Self {
            texture_path: None,
            normal_map: None,
            glow_map: None,
            detail_map: None,
            gloss_map: None,
            vertex_color_mode: VertexColorMode::AmbientDiffuse,
            alpha_blend: false,
            alpha_test: false,
            alpha_threshold: 0.0,
            two_sided: false,
            is_decal: false,
            emissive_color: [0.0, 0.0, 0.0],
            emissive_mult: 1.0,
            specular_color: [1.0, 1.0, 1.0],
            specular_strength: 1.0,
            specular_enabled: true,
            glossiness: 80.0,
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            alpha: 1.0,
            env_map_scale: 1.0,
            has_material_data: false,
            z_test: true,
            z_write: true,
        }
    }
}

/// Extract vertex colors and texture path from the shape's properties.
pub(super) fn extract_material(
    scene: &NifScene,
    shape: &NiTriShape,
    data: &GeomData,
    inherited_props: &[BlockRef],
) -> (Vec<[f32; 3]>, Option<String>) {
    let num_verts = data.vertices.len();

    // Check for an NiVertexColorProperty that disables vertex colors.
    // When the mesh declares SRC_IGNORE or SRC_EMISSIVE, the data-block
    // vertex colors must NOT be routed to the diffuse channel. Ignore
    // means fall through to the material diffuse path; Emissive means
    // the colors go to a separate shader input (handled downstream via
    // MaterialInfo.vertex_color_mode — for now we still fall back to
    // material diffuse for the per-vertex color vector so unshaded
    // diffuse doesn't get contaminated). See #214.
    let vertex_mode = vertex_color_mode_for(scene, shape, inherited_props);
    let use_vertex_colors =
        !data.vertex_colors.is_empty() && vertex_mode == VertexColorMode::AmbientDiffuse;

    if use_vertex_colors {
        let colors = data
            .vertex_colors
            .iter()
            .map(|c| [c[0], c[1], c[2]]) // drop alpha
            .collect();
        let tex = find_texture_path(scene, shape, inherited_props);
        return (colors, tex);
    }

    // Search shape's own properties first, then inherited, for NiMaterialProperty.
    let mut diffuse = [1.0f32; 3]; // default white
    for prop_ref in shape.av.properties.iter().chain(inherited_props.iter()) {
        if let Some(idx) = prop_ref.index() {
            if let Some(mat) = scene.get_as::<NiMaterialProperty>(idx) {
                diffuse = [mat.diffuse.r, mat.diffuse.g, mat.diffuse.b];
                break;
            }
        }
    }

    let colors = vec![diffuse; num_verts];
    let tex = find_texture_path(scene, shape, inherited_props);
    (colors, tex)
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
            if let ShaderTypeData::EnvironmentMap { env_map_scale } = shader.shader_type_data {
                info.env_map_scale = env_map_scale;
            }
            info.has_material_data = true;
        }
        if let Some(shader) = scene.get_as::<BSEffectShaderProperty>(idx) {
            if info.texture_path.is_none() && !shader.source_texture.is_empty() {
                info.texture_path = Some(shader.source_texture.clone());
            }
            if !info.has_material_data {
                info.emissive_color = [
                    shader.emissive_color[0],
                    shader.emissive_color[1],
                    shader.emissive_color[2],
                ];
                info.emissive_mult = shader.emissive_multiple;
                info.uv_offset = shader.uv_offset;
                info.uv_scale = shader.uv_scale;
                info.has_material_data = true;
            }
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
                if let Some(path) =
                    tex_desc_source_path(scene, tex_prop.normal_texture.as_ref())
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
                if let Some(path) =
                    tex_desc_source_path(scene, tex_prop.glow_texture.as_ref())
                {
                    info.glow_map = Some(path);
                }
            }
            if info.detail_map.is_none() {
                if let Some(path) =
                    tex_desc_source_path(scene, tex_prop.detail_texture.as_ref())
                {
                    info.detail_map = Some(path);
                }
            }
            if info.gloss_map.is_none() {
                if let Some(path) =
                    tex_desc_source_path(scene, tex_prop.gloss_texture.as_ref())
                {
                    info.gloss_map = Some(path);
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

/// Texture path only — delegates to extract_material_info.
pub(super) fn find_texture_path(
    scene: &NifScene,
    shape: &NiTriShape,
    inherited_props: &[BlockRef],
) -> Option<String> {
    extract_material_info(scene, shape, inherited_props).texture_path
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
pub(super) fn apply_alpha_flags(info: &mut MaterialInfo, alpha: &NiAlphaProperty) {
    let blend = alpha.flags & 0x001 != 0;
    let test = alpha.flags & 0x200 != 0;
    if test {
        info.alpha_test = true;
        info.alpha_threshold = alpha.threshold as f32 / 255.0;
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
pub(super) fn find_decal_bs(scene: &NifScene, shape: &BsTriShape) -> bool {
    if let Some(idx) = shape.shader_property_ref.index() {
        if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
            if shader.shader_flags_1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0 {
                return true;
            }
        }
    }
    false
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
    fn default_material_info_has_no_secondary_maps_and_default_mode() {
        let info = MaterialInfo::default();
        assert!(info.glow_map.is_none());
        assert!(info.detail_map.is_none());
        assert!(info.gloss_map.is_none());
        assert_eq!(info.vertex_color_mode, VertexColorMode::AmbientDiffuse);
    }
}
