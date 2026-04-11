//! NIF material and texture property extraction.

use crate::blocks::properties::{
    NiAlphaProperty, NiFlagProperty, NiMaterialProperty, NiStencilProperty, NiTexturingProperty,
    TexDesc,
};
use crate::blocks::NiObject;
use crate::blocks::shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderNoLightingProperty,
    BSShaderPPLightingProperty, BSShaderTextureSet, ShaderTypeData,
};
use crate::blocks::texture::NiSourceTexture;
use crate::blocks::tri_shape::{BsTriShape, NiTriShape};
use crate::scene::NifScene;

use super::mesh::GeomData;

pub(super) const DECAL_SINGLE_PASS: u32 = 0x04000000;
pub(super) const DYNAMIC_DECAL: u32 = 0x08000000;
const ALPHA_DECAL_F2: u32 = 0x00200000;
const SF_DOUBLE_SIDED: u32 = 0x1000;

/// Material properties extracted from a NiTriShape's property list in a single pass.
#[derive(Debug)]
pub(super) struct MaterialInfo {
    pub texture_path: Option<String>,
    pub normal_map: Option<String>,
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
) -> (Vec<[f32; 3]>, Option<String>) {
    let num_verts = data.vertices.len();

    // Per-vertex colors take priority
    if !data.vertex_colors.is_empty() {
        let colors = data
            .vertex_colors
            .iter()
            .map(|c| [c[0], c[1], c[2]]) // drop alpha
            .collect();
        let tex = find_texture_path(scene, shape);
        return (colors, tex);
    }

    // Search properties for NiMaterialProperty
    let mut diffuse = [1.0f32; 3]; // default white
    for prop_ref in &shape.av.properties {
        if let Some(idx) = prop_ref.index() {
            if let Some(mat) = scene.get_as::<NiMaterialProperty>(idx) {
                diffuse = [mat.diffuse.r, mat.diffuse.g, mat.diffuse.b];
                break;
            }
        }
    }

    let colors = vec![diffuse; num_verts];
    let tex = find_texture_path(scene, shape);
    (colors, tex)
}

/// Extract all material properties from a NiTriShape in a single pass.
pub(super) fn extract_material_info(scene: &NifScene, shape: &NiTriShape) -> MaterialInfo {
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

    // FO3/FNV/Oblivion: single pass over properties list
    for prop_ref in &shape.av.properties {
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
pub(super) fn find_texture_path(scene: &NifScene, shape: &NiTriShape) -> Option<String> {
    extract_material_info(scene, shape).texture_path
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
