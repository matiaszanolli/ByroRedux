//! Starfield `BSGeometry` mesh extraction.
//!
//! External-mesh + internal-geom branches, decoded into the engine's
//! universal mesh shape.



use crate::blocks::bs_geometry::unpack_udec3_xyzw;
use crate::blocks::bs_geometry::BSGeometry;
use crate::scene::NifScene;
use crate::types::{NiPoint3, NiTransform};

use super::super::coord::{zup_matrix_to_yup_quat, zup_point_to_yup};
use super::super::{ImportedMesh, MeshResolver};
use super::*;
use byroredux_core::string::StringPool;

pub fn extract_bs_geometry(
    scene: &NifScene,
    shape: &BSGeometry,
    world_transform: &NiTransform,
    pool: &mut StringPool,
    resolver: Option<&dyn MeshResolver>,
) -> Option<ImportedMesh> {
    use crate::blocks::bs_geometry::{BSGeometryMeshData, BSGeometryMeshKind};

    // Try each LOD slot in order; use the first one that yields geometry.
    let mesh_data_owned: Option<BSGeometryMeshData>;
    let mesh_data: &BSGeometryMeshData = if shape.has_internal_geom_data() {
        // Stage A: inline geometry embedded in the NIF.
        let m = shape.meshes.first().and_then(|m| match &m.kind {
            BSGeometryMeshKind::Internal { mesh_data } => Some(mesh_data),
            BSGeometryMeshKind::External { .. } => None,
        })?;
        m
    } else {
        // Stage B: external `.mesh` companion file. Try each LOD slot until
        // one resolves. When no resolver is provided, skip external geometry.
        let resolver = resolver?;
        let mut found = None;
        for m in &shape.meshes {
            if let BSGeometryMeshKind::External { mesh_name } = &m.kind {
                if let Some(bytes) = resolver.resolve(mesh_name) {
                    match BSGeometryMeshData::parse_from_bytes(&bytes) {
                        Ok(data) => {
                            found = Some(data);
                            break;
                        }
                        Err(e) => {
                            log::debug!(
                                "BSGeometry external mesh '{}' parse error: {}",
                                mesh_name,
                                e
                            );
                        }
                    }
                }
            }
        }
        mesh_data_owned = found;
        mesh_data_owned.as_ref()?
    };

    if mesh_data.vertices.is_empty() || mesh_data.triangles.is_empty() {
        return None;
    }

    // Positions are already Y-up (decoded by the BSGeometryMeshData parser).
    let positions: Vec<[f32; 3]> = mesh_data.vertices.to_vec();

    let indices: Vec<u32> = mesh_data
        .triangles
        .iter()
        .flat_map(|tri| [tri[0] as u32, tri[1] as u32, tri[2] as u32])
        .collect();

    // Unpack UDEC3 normals from raw u32 (10:10:10:2 unsigned-fixed). Y-up already.
    let normals: Vec<[f32; 3]> = if !mesh_data.normals_raw.is_empty() {
        mesh_data
            .normals_raw
            .iter()
            .map(|&raw| {
                let xyzw = unpack_udec3_xyzw(raw);
                [xyzw[0], xyzw[1], xyzw[2]]
            })
            .collect()
    } else {
        vec![[0.0, 1.0, 0.0]; positions.len()]
    };

    let uvs = mesh_data.uvs0.clone();

    // Authored tangents from the UDEC3-packed `tangents_raw` channel.
    // BSGeometry is Starfield-native Y-up — no Z-up → Y-up axis swap is
    // needed (unlike the Oblivion/Skyrim NiBinaryExtraData path, which
    // is Z-up and requires `bs_tangents_zup_to_yup`). The 2-bit W
    // channel from `unpack_udec3_xyzw` carries the bitangent sign.
    // Without this decode, every Starfield mesh fell through to the
    // shader's screen-space derivative Path-2 in `perturbNormal`,
    // producing lower-quality normal maps and inverted normals on
    // UV-mirrored geometry (#1086 / REN-D16-001).
    let tangents: Vec<[f32; 4]> = if !mesh_data.tangents_raw.is_empty() {
        mesh_data
            .tangents_raw
            .iter()
            .map(|&raw| {
                let xyzw = unpack_udec3_xyzw(raw);
                [xyzw[0], xyzw[1], xyzw[2], xyzw[3]]
            })
            .collect()
    } else {
        // No authored tangents — the renderer falls back to screen-space
        // derivative TBN (Path 2). A future improvement could call
        // synthesize_tangents here, but it requires a Y-up variant since
        // BSGeometry data is already in engine space (unlike the Z-up
        // input that NiTriShape / BSTriShape synthesis expects).
        Vec::new()
    };

    // Vertex colors: u8 RGBA → f32 [0, 1].
    let colors: Vec<[f32; 4]> = if !mesh_data.colors.is_empty() {
        mesh_data
            .colors
            .iter()
            .map(|&[r, g, b, a]| {
                [
                    r as f32 / 255.0,
                    g as f32 / 255.0,
                    b as f32 / 255.0,
                    a as f32 / 255.0,
                ]
            })
            .collect()
    } else {
        vec![[1.0, 1.0, 1.0, 1.0]; positions.len()]
    };

    let mat = super::super::material::extract_material_info_from_refs(
        scene,
        shape.shader_property_ref,
        shape.alpha_property_ref,
        &[],
        &[],
        pool,
    );

    let t = &world_transform.translation;
    let quat = zup_matrix_to_yup_quat(&world_transform.rotation);

    // BSGeometry bounding sphere is already in Y-up (Starfield-native), so
    // use it directly when radius > 0; otherwise fall back to centroid.
    let (local_bound_center, local_bound_radius) = {
        let [cx, cy, cz] = shape.bounding_sphere.0;
        let r = shape.bounding_sphere.1;
        if r > 0.0 {
            ([cx, cy, cz], r)
        } else {
            extract_local_bound(
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                0.0,
                &positions,
            )
        }
    };

    let shader_type_fields = mat.shader_type_fields();

    Some(ImportedMesh {
        positions,
        colors,
        normals,
        tangents,
        uvs,
        indices,
        translation: zup_point_to_yup(t),
        rotation: quat,
        scale: world_transform.scale,
        name: shape.av.net.name.clone(),
        texture_path: mat.texture_path,
        material_path: mat.material_path,
        has_alpha: mat.alpha_blend,
        src_blend_mode: mat.src_blend_mode,
        dst_blend_mode: mat.dst_blend_mode,
        alpha_test: mat.alpha_test,
        alpha_threshold: mat.alpha_threshold,
        alpha_test_func: mat.alpha_test_func,
        two_sided: mat.two_sided,
        is_decal: mat.is_decal,
        normal_map: mat.normal_map,
        glow_map: mat.glow_map,
        detail_map: mat.detail_map,
        gloss_map: mat.gloss_map,
        dark_map: mat.dark_map,
        parallax_map: mat.parallax_map,
        env_map: mat.env_map,
        env_mask: mat.env_mask,
        parallax_max_passes: mat.parallax_max_passes,
        parallax_height_scale: mat.parallax_height_scale,
        vertex_color_mode: mat.vertex_color_mode as u8,
        texture_clamp_mode: mat.texture_clamp_mode,
        emissive_color: mat.emissive_color,
        emissive_mult: mat.emissive_mult,
        specular_color: mat.specular_color,
        diffuse_color: mat.diffuse_color,
        ambient_color: mat.ambient_color,
        specular_strength: mat.specular_strength,
        glossiness: mat.glossiness,
        uv_offset: mat.uv_offset,
        uv_scale: mat.uv_scale,
        mat_alpha: mat.alpha,
        env_map_scale: mat.env_map_scale,
        parent_node: None,
        skin: None,
        z_test: mat.z_test,
        z_write: mat.z_write,
        z_function: mat.z_function,
        local_bound_center,
        local_bound_radius,
        effect_shader: mat.effect_shader,
        material_kind: mat.material_kind,
        shader_type_fields,
        no_lighting_falloff: mat.no_lighting_falloff,
        wireframe: mat.wireframe,
        flat_shading: mat.flat_shading,
        flags: shape.av.flags,
    })
}

/// Extract a BSGeometry with local transform (for hierarchical import).
pub fn extract_bs_geometry_local(
    scene: &NifScene,
    shape: &BSGeometry,
    pool: &mut StringPool,
    resolver: Option<&dyn MeshResolver>,
) -> Option<ImportedMesh> {
    extract_bs_geometry(scene, shape, &shape.av.transform, pool, resolver)
}
