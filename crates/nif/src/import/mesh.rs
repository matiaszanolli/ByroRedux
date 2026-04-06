//! Geometry extraction from NiTriShape and BsTriShape blocks.

use crate::blocks::properties::NiAlphaProperty;
use crate::blocks::shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderTextureSet, ShaderTypeData,
};
use crate::blocks::tri_shape::{BsTriShape, NiTriShape, NiTriShapeData, NiTriStripsData};
use crate::scene::NifScene;
use crate::types::{NiPoint3, NiTransform};

use super::coord::zup_matrix_to_yup_quat;
use super::material::{extract_material, extract_material_info, find_decal_bs};
use super::ImportedMesh;

/// Intermediate geometry data extracted from either NiTriShapeData or NiTriStripsData.
#[allow(dead_code)]
pub(super) struct GeomData<'a> {
    pub vertices: &'a [NiPoint3],
    pub normals: &'a [NiPoint3],
    pub vertex_colors: &'a [[f32; 4]],
    pub uv_sets: &'a [Vec<[f32; 2]>],
    pub triangles: std::borrow::Cow<'a, [[u16; 3]]>,
}

/// Extract an ImportedMesh from an NiTriShape and its referenced data block.
pub(super) fn extract_mesh(
    scene: &NifScene,
    shape: &NiTriShape,
    world_transform: &NiTransform,
) -> Option<ImportedMesh> {
    let data_idx = shape.data_ref.index()?;

    // Try NiTriShapeData first, then NiTriStripsData
    let geom = if let Some(data) = scene.get_as::<NiTriShapeData>(data_idx) {
        GeomData {
            vertices: &data.vertices,
            normals: &data.normals,
            vertex_colors: &data.vertex_colors,
            uv_sets: &data.uv_sets,
            triangles: std::borrow::Cow::Borrowed(&data.triangles),
        }
    } else if let Some(data) = scene.get_as::<NiTriStripsData>(data_idx) {
        GeomData {
            vertices: &data.vertices,
            normals: &data.normals,
            vertex_colors: &data.vertex_colors,
            uv_sets: &data.uv_sets,
            triangles: std::borrow::Cow::Owned(data.to_triangles()),
        }
    } else {
        return None;
    };

    if geom.vertices.is_empty() || geom.triangles.is_empty() {
        return None;
    }

    // Convert positions: Gamebryo Z-up → renderer Y-up: (x,y,z) → (x,z,-y)
    let positions: Vec<[f32; 3]> = geom.vertices.iter().map(|v| [v.x, v.z, -v.y]).collect();

    // Convert indices (u16 → u32). Winding order preserved — the Z-up → Y-up
    // transform is a proper rotation (det=+1), not a reflection.
    let indices: Vec<u32> = geom
        .triangles
        .iter()
        .flat_map(|tri| [tri[0] as u32, tri[1] as u32, tri[2] as u32])
        .collect();

    // Convert normals with same axis swap (fall back to +Y up if none)
    let normals: Vec<[f32; 3]> = if !geom.normals.is_empty() {
        geom.normals.iter().map(|n| [n.x, n.z, -n.y]).collect()
    } else {
        vec![[0.0, 1.0, 0.0]; positions.len()]
    };

    // Get UVs from first UV set (if available)
    let uvs = geom.uv_sets.first().cloned().unwrap_or_default();

    // Determine vertex colors: prefer per-vertex colors, then material diffuse, then white
    let (colors, texture_path) = extract_material(scene, shape, &geom);

    // Apply Z-up → Y-up to the entity transform.
    let t = &world_transform.translation;
    let r = &world_transform.rotation;

    // Convert the Z-up rotation matrix to Y-up, then extract a robust quaternion.
    let quat = zup_matrix_to_yup_quat(r);

    // Single-pass material property extraction (alpha, two-sided, decal).
    let mat = extract_material_info(scene, shape);

    Some(ImportedMesh {
        positions,
        colors,
        normals,
        uvs,
        indices,
        translation: [t.x, t.z, -t.y],
        rotation: quat,
        scale: world_transform.scale,
        name: shape.av.net.name.as_deref().map(str::to_string),
        texture_path,
        has_alpha: mat.alpha_blend,
        two_sided: mat.two_sided,
        is_decal: mat.is_decal,
        normal_map: mat.normal_map,
        emissive_color: mat.emissive_color,
        emissive_mult: mat.emissive_mult,
        specular_color: mat.specular_color,
        specular_strength: mat.specular_strength,
        glossiness: mat.glossiness,
        uv_offset: mat.uv_offset,
        uv_scale: mat.uv_scale,
        mat_alpha: mat.alpha,
        env_map_scale: mat.env_map_scale,
        parent_node: None,
    })
}

/// Extract an ImportedMesh with local transform (for hierarchical import).
pub(super) fn extract_mesh_local(scene: &NifScene, shape: &NiTriShape) -> Option<ImportedMesh> {
    extract_mesh(scene, shape, &shape.av.transform)
}

/// Extract an ImportedMesh from a BsTriShape (Skyrim SE+ self-contained geometry).
pub(super) fn extract_bs_tri_shape(
    scene: &NifScene,
    shape: &BsTriShape,
    world_transform: &NiTransform,
) -> Option<ImportedMesh> {
    if shape.vertices.is_empty() || shape.triangles.is_empty() {
        return None;
    }

    let positions: Vec<[f32; 3]> = shape.vertices.iter().map(|v| [v.x, v.z, -v.y]).collect();

    let indices: Vec<u32> = shape
        .triangles
        .iter()
        .flat_map(|tri| [tri[0] as u32, tri[1] as u32, tri[2] as u32])
        .collect();

    let normals: Vec<[f32; 3]> = if !shape.normals.is_empty() {
        shape.normals.iter().map(|n| [n.x, n.z, -n.y]).collect()
    } else {
        vec![[0.0, 1.0, 0.0]; positions.len()]
    };

    let uvs = shape.uvs.clone();

    let colors: Vec<[f32; 3]> = if !shape.vertex_colors.is_empty() {
        shape
            .vertex_colors
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect()
    } else {
        vec![[1.0, 1.0, 1.0]; positions.len()]
    };

    let texture_path = find_texture_path_bs_tri_shape(scene, shape);

    let has_alpha = if let Some(idx) = shape.alpha_property_ref.index() {
        scene
            .get_as::<NiAlphaProperty>(idx)
            .map(|a| a.flags & 1 != 0)
            .unwrap_or(false)
    } else {
        false
    };

    let two_sided = if let Some(idx) = shape.shader_property_ref.index() {
        scene
            .get_as::<BSLightingShaderProperty>(idx)
            .map(|s| s.shader_flags_2 & 0x10 != 0)
            .unwrap_or(false)
    } else {
        false
    };

    let t = &world_transform.translation;
    let quat = zup_matrix_to_yup_quat(&world_transform.rotation);

    let (
        emissive_color,
        emissive_mult,
        specular_color,
        specular_strength,
        glossiness,
        uv_offset,
        uv_scale,
        mat_alpha,
        normal_map,
        env_map_scale,
    ) = if let Some(idx) = shape.shader_property_ref.index() {
        if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
            let nm = shader
                .texture_set_ref
                .index()
                .and_then(|ts_idx| scene.get_as::<BSShaderTextureSet>(ts_idx))
                .and_then(|ts| ts.textures.get(1).cloned())
                .filter(|s| !s.is_empty());
            let ems =
                if let ShaderTypeData::EnvironmentMap { env_map_scale } = shader.shader_type_data {
                    env_map_scale
                } else {
                    1.0
                };
            (
                shader.emissive_color,
                shader.emissive_multiple,
                shader.specular_color,
                shader.specular_strength,
                shader.glossiness,
                shader.uv_offset,
                shader.uv_scale,
                shader.alpha,
                nm,
                ems,
            )
        } else {
            (
                [0.0; 3], 1.0, [1.0; 3], 1.0, 80.0, [0.0; 2], [1.0; 2], 1.0, None, 1.0,
            )
        }
    } else {
        (
            [0.0; 3], 1.0, [1.0; 3], 1.0, 80.0, [0.0; 2], [1.0; 2], 1.0, None, 1.0,
        )
    };

    Some(ImportedMesh {
        positions,
        colors,
        normals,
        uvs,
        indices,
        translation: [t.x, t.z, -t.y],
        rotation: quat,
        scale: world_transform.scale,
        name: shape.av.net.name.as_deref().map(str::to_string),
        texture_path,
        has_alpha,
        two_sided,
        is_decal: find_decal_bs(scene, shape),
        normal_map,
        emissive_color,
        emissive_mult,
        specular_color,
        specular_strength,
        glossiness,
        uv_offset,
        uv_scale,
        mat_alpha,
        env_map_scale,
        parent_node: None,
    })
}

/// Extract a BsTriShape with local transform (for hierarchical import).
pub(super) fn extract_bs_tri_shape_local(
    scene: &NifScene,
    shape: &BsTriShape,
) -> Option<ImportedMesh> {
    extract_bs_tri_shape(scene, shape, &shape.av.transform)
}

/// Find texture path for BsTriShape via its shader_property_ref.
pub(super) fn find_texture_path_bs_tri_shape(
    scene: &NifScene,
    shape: &BsTriShape,
) -> Option<String> {
    if let Some(idx) = shape.shader_property_ref.index() {
        if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
            if let Some(ts_idx) = shader.texture_set_ref.index() {
                if let Some(tex_set) = scene.get_as::<BSShaderTextureSet>(ts_idx) {
                    if let Some(path) = tex_set.textures.first() {
                        if !path.is_empty() {
                            return Some(path.clone());
                        }
                    }
                }
            }
        }
        if let Some(shader) = scene.get_as::<BSEffectShaderProperty>(idx) {
            if !shader.source_texture.is_empty() {
                return Some(shader.source_texture.clone());
            }
        }
    }
    None
}
