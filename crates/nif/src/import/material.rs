//! NIF material and texture property extraction.

use crate::blocks::properties::{
    NiAlphaProperty, NiMaterialProperty, NiStencilProperty, NiTexturingProperty,
};
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
    pub two_sided: bool,
    pub is_decal: bool,
    pub emissive_color: [f32; 3],
    pub emissive_mult: f32,
    pub specular_color: [f32; 3],
    pub specular_strength: f32,
    pub glossiness: f32,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub alpha: f32,
    pub env_map_scale: f32,
    pub has_material_data: bool,
}

impl Default for MaterialInfo {
    fn default() -> Self {
        Self {
            texture_path: None,
            normal_map: None,
            alpha_blend: false,
            two_sided: false,
            is_decal: false,
            emissive_color: [0.0, 0.0, 0.0],
            emissive_mult: 1.0,
            specular_color: [1.0, 1.0, 1.0],
            specular_strength: 1.0,
            glossiness: 80.0,
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            alpha: 1.0,
            env_map_scale: 1.0,
            has_material_data: false,
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
            if alpha.flags & 1 != 0 {
                info.alpha_blend = true;
            }
        }
    }

    // FO3/FNV/Oblivion: single pass over properties list
    for prop_ref in &shape.av.properties {
        let Some(idx) = prop_ref.index() else {
            continue;
        };

        if !info.alpha_blend {
            if let Some(alpha) = scene.get_as::<NiAlphaProperty>(idx) {
                if alpha.flags & 1 != 0 {
                    info.alpha_blend = true;
                }
            }
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

        if info.texture_path.is_none() {
            if let Some(tex_prop) = scene.get_as::<NiTexturingProperty>(idx) {
                if let Some(ref base) = tex_prop.base_texture {
                    if let Some(src_idx) = base.source_ref.index() {
                        if let Some(src_tex) = scene.get_as::<NiSourceTexture>(src_idx) {
                            if let Some(ref f) = src_tex.filename {
                                info.texture_path = Some(f.to_string());
                            }
                        }
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
    }

    info
}

/// Texture path only — delegates to extract_material_info.
pub(super) fn find_texture_path(scene: &NifScene, shape: &NiTriShape) -> Option<String> {
    extract_material_info(scene, shape).texture_path
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
