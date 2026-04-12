//! Geometry extraction from NiTriShape and BsTriShape blocks.

use crate::blocks::node::NiNode;
use crate::blocks::properties::NiAlphaProperty;
use crate::blocks::shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderTextureSet, ShaderTypeData,
};
use crate::blocks::skin::{
    BsDismemberSkinInstance, BsSkinBoneData, BsSkinInstance, NiSkinData, NiSkinInstance,
};
use crate::blocks::tri_shape::{BsTriShape, NiTriShape, NiTriShapeData, NiTriStripsData};
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3, NiTransform};

use super::coord::zup_matrix_to_yup_quat;
use super::material::{extract_material, extract_material_info, find_decal_bs};
use super::{ImportedBone, ImportedMesh, ImportedSkin};

/// Intermediate geometry data extracted from either NiTriShapeData or NiTriStripsData.
#[allow(dead_code)]
pub(super) struct GeomData<'a> {
    pub vertices: &'a [NiPoint3],
    pub normals: &'a [NiPoint3],
    pub vertex_colors: &'a [[f32; 4]],
    pub uv_sets: &'a [Vec<[f32; 2]>],
    pub triangles: std::borrow::Cow<'a, [[u16; 3]]>,
    /// NIF-provided bounding sphere center, still in Gamebryo Z-up space.
    /// Zero when the NIF omits a bound — the caller then computes one
    /// from the positions. See #217.
    pub bound_center: NiPoint3,
    /// NIF-provided bounding sphere radius (no axis conversion needed).
    pub bound_radius: f32,
}

/// Extract an ImportedMesh from an NiTriShape and its referenced data block.
pub(super) fn extract_mesh(
    scene: &NifScene,
    shape: &NiTriShape,
    world_transform: &NiTransform,
    inherited_props: &[BlockRef],
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
            bound_center: data.center,
            bound_radius: data.radius,
        }
    } else if let Some(data) = scene.get_as::<NiTriStripsData>(data_idx) {
        GeomData {
            vertices: &data.vertices,
            normals: &data.normals,
            vertex_colors: &data.vertex_colors,
            uv_sets: &data.uv_sets,
            triangles: std::borrow::Cow::Owned(data.to_triangles()),
            bound_center: data.center,
            bound_radius: data.radius,
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
    let (colors, texture_path) = extract_material(scene, shape, &geom, inherited_props);

    // Apply Z-up → Y-up to the entity transform.
    let t = &world_transform.translation;
    let r = &world_transform.rotation;

    // Convert the Z-up rotation matrix to Y-up, then extract a robust quaternion.
    let quat = zup_matrix_to_yup_quat(r);

    // Single-pass material property extraction (alpha, two-sided, decal).
    let mat = extract_material_info(scene, shape, inherited_props);

    // Skinning data (issue #151). Populated when the shape has a
    // NiSkinInstance / BSDismemberSkinInstance backing it.
    let skin = extract_skin_ni_tri_shape(scene, shape, positions.len());

    // Local bounding sphere in Y-up renderer space. Prefer the NIF-provided
    // NiBound on NiGeometryData; fall back to a fresh centroid+max-distance
    // sphere computed from the positions when the NIF omits one (radius 0).
    // See #217.
    let (local_bound_center, local_bound_radius) =
        extract_local_bound(geom.bound_center, geom.bound_radius, &positions);

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
        alpha_test: mat.alpha_test,
        alpha_threshold: mat.alpha_threshold,
        two_sided: mat.two_sided,
        is_decal: mat.is_decal,
        normal_map: mat.normal_map,
        glow_map: mat.glow_map,
        detail_map: mat.detail_map,
        gloss_map: mat.gloss_map,
        vertex_color_mode: mat.vertex_color_mode as u8,
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
        skin,
        z_test: mat.z_test,
        z_write: mat.z_write,
        local_bound_center,
        local_bound_radius,
    })
}

/// Produce a mesh-local bounding sphere in Y-up renderer space.
///
/// If the NIF supplied a non-zero `center`/`radius` (from `NiGeometryData`
/// or `BsTriShape`), convert the center from Gamebryo Z-up to Y-up and
/// return it — this is cheap and matches what the game engine computed
/// at export time. When the NIF bound is zero (legacy content or
/// auto-generated meshes) fall back to computing a centroid+max-distance
/// sphere from the already-converted vertex positions.
fn extract_local_bound(
    nif_center: NiPoint3,
    nif_radius: f32,
    positions_yup: &[[f32; 3]],
) -> ([f32; 3], f32) {
    if nif_radius > 0.0 {
        return ([nif_center.x, nif_center.z, -nif_center.y], nif_radius);
    }
    if positions_yup.is_empty() {
        return ([0.0; 3], 0.0);
    }
    let mut sum = [0.0f32; 3];
    for p in positions_yup {
        sum[0] += p[0];
        sum[1] += p[1];
        sum[2] += p[2];
    }
    let inv_n = 1.0 / positions_yup.len() as f32;
    let center = [sum[0] * inv_n, sum[1] * inv_n, sum[2] * inv_n];
    let mut max_sq = 0.0f32;
    for p in positions_yup {
        let dx = p[0] - center[0];
        let dy = p[1] - center[1];
        let dz = p[2] - center[2];
        let d_sq = dx * dx + dy * dy + dz * dz;
        if d_sq > max_sq {
            max_sq = d_sq;
        }
    }
    (center, max_sq.sqrt())
}

/// Extract an ImportedMesh with local transform (for hierarchical import).
pub(super) fn extract_mesh_local(
    scene: &NifScene,
    shape: &NiTriShape,
    inherited_props: &[BlockRef],
) -> Option<ImportedMesh> {
    extract_mesh(scene, shape, &shape.av.transform, inherited_props)
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

    // NiAlphaProperty: bit 0 = alpha blend, bit 9 (0x200) = alpha test
    // (cutout). See issue #152. Prefer alpha-test over alpha-blend when
    // both bits are set — same policy as the NiTriShape path in
    // `apply_alpha_flags`.
    let (has_alpha, alpha_test, alpha_threshold) =
        if let Some(idx) = shape.alpha_property_ref.index() {
            if let Some(a) = scene.get_as::<NiAlphaProperty>(idx) {
                let blend = a.flags & 0x001 != 0;
                let test = a.flags & 0x200 != 0;
                if test {
                    (false, true, a.threshold as f32 / 255.0)
                } else {
                    (blend, false, 0.0)
                }
            } else {
                (false, false, 0.0)
            }
        } else {
            (false, false, 0.0)
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

    // Skinning data. BSTriShape per-vertex weights live in the packed
    // vertex buffer (VF_SKINNED), decoded at parse time (#177).
    let skin = extract_skin_bs_tri_shape(scene, shape);

    // BSTriShape carries its own bounding sphere (center + radius) on the
    // block. See #217.
    let (local_bound_center, local_bound_radius) =
        extract_local_bound(shape.center, shape.radius, &positions);

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
        alpha_test,
        alpha_threshold,
        two_sided,
        is_decal: find_decal_bs(scene, shape),
        normal_map,
        // BsTriShape (Skyrim+) routes all texture slots through
        // BSShaderTextureSet, which this path reads above. The legacy
        // NiTexturingProperty glow/detail/gloss slots don't apply here,
        // so leave them as `None`. Skyrim+ glow maps live in
        // BSShaderTextureSet slot 2; wiring those is a separate task
        // once we teach the renderer to sample a third slot. See #214.
        glow_map: None,
        detail_map: None,
        gloss_map: None,
        // BsTriShape vertex colors are driven by the shader
        // properties, not an NiVertexColorProperty — pass the default
        // (AmbientDiffuse = 2) so downstream consumers behave the same
        // as before.
        vertex_color_mode: 2,
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
        skin,
        z_test: true,
        z_write: true,
        local_bound_center,
        local_bound_radius,
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

// ── Skinning extraction (issue #151) ──────────────────────────────────

/// Extract `ImportedSkin` for a NiTriShape via `skin_instance_ref`.
///
/// Follows:
///   NiTriShape.skin_instance_ref → NiSkinInstance (or BSDismemberSkinInstance)
///     → NiSkinData.bones[] (bind transforms + sparse vertex weights)
///     → per-bone NiNode refs (names for bone lookup)
///
/// Converts the sparse per-bone weight lists to dense per-vertex
/// `[u8; 4]` indices + `[f32; 4]` weights by keeping the 4 highest
/// contributions per vertex and re-normalizing so the weights sum to 1.
/// Vertices with no bone contribution get weight `[1, 0, 0, 0]` bound
/// to bone 0 (safer than all-zero weights which would collapse the
/// vertex to the origin during skinning).
pub(super) fn extract_skin_ni_tri_shape(
    scene: &NifScene,
    shape: &NiTriShape,
    num_vertices: usize,
) -> Option<ImportedSkin> {
    let skin_idx = shape.skin_instance_ref.index()?;

    // Accept either NiSkinInstance or BSDismemberSkinInstance (the
    // Bethesda extension with body-part flags — we only need the base).
    let (bone_refs, skeleton_root_ref, data_ref) =
        if let Some(inst) = scene.get_as::<NiSkinInstance>(skin_idx) {
            (
                inst.bone_refs.as_slice(),
                inst.skeleton_root_ref,
                inst.data_ref,
            )
        } else if let Some(inst) = scene.get_as::<BsDismemberSkinInstance>(skin_idx) {
            (
                inst.base.bone_refs.as_slice(),
                inst.base.skeleton_root_ref,
                inst.base.data_ref,
            )
        } else {
            return None;
        };

    let data = scene.get_as::<NiSkinData>(data_ref.index()?)?;
    if data.bones.len() != bone_refs.len() {
        log::debug!(
            "NiSkinData bone count ({}) != NiSkinInstance bone_refs count ({})",
            data.bones.len(),
            bone_refs.len(),
        );
        return None;
    }

    // Resolve bone names (the interpolator refers to bones by index
    // into this vec, so the order must match NiSkinInstance.bone_refs).
    let bones = build_imported_bones(scene, bone_refs, data)?;
    let skeleton_root = resolve_node_name(scene, skeleton_root_ref);

    // Build dense per-vertex weight tables.
    let (vertex_bone_indices, vertex_bone_weights) =
        densify_sparse_weights(num_vertices, data);

    Some(ImportedSkin {
        bones,
        skeleton_root,
        vertex_bone_indices,
        vertex_bone_weights,
    })
}

/// Extract `ImportedSkin` for a BSTriShape via `skin_ref`. Walks the
/// skin instance for bone list + bind-inverse transforms, then copies
/// the parsed per-vertex weights + indices from the packed vertex
/// buffer (VF_SKINNED, issue #177).
///
/// Handles both:
///   - NiSkinInstance (Skyrim LE BSTriShape) via NiSkinData
///   - BSSkin::Instance (Skyrim SE / FO4+) via BSSkin::BoneData
pub(super) fn extract_skin_bs_tri_shape(
    scene: &NifScene,
    shape: &BsTriShape,
) -> Option<ImportedSkin> {
    let skin_idx = shape.skin_ref.index()?;

    // Per-vertex weights and indices come from the BSTriShape vertex
    // buffer (VF_SKINNED) — already decoded at parse time (#177). We
    // just clone them through to ImportedSkin. If the vertex buffer
    // lacks the VF_SKINNED bit these will be empty, and downstream
    // should treat the mesh as rigid.
    let vertex_bone_indices = shape.bone_indices.clone();
    let vertex_bone_weights = shape.bone_weights.clone();

    // Skyrim LE path: NiSkinInstance + NiSkinData (bone list + bind transforms).
    if scene.get_as::<NiSkinInstance>(skin_idx).is_some()
        || scene.get_as::<BsDismemberSkinInstance>(skin_idx).is_some()
    {
        let (bone_refs, skeleton_root_ref, data_ref) =
            if let Some(inst) = scene.get_as::<NiSkinInstance>(skin_idx) {
                (
                    inst.bone_refs.clone(),
                    inst.skeleton_root_ref,
                    inst.data_ref,
                )
            } else {
                let inst = scene.get_as::<BsDismemberSkinInstance>(skin_idx)?;
                (
                    inst.base.bone_refs.clone(),
                    inst.base.skeleton_root_ref,
                    inst.base.data_ref,
                )
            };
        let data = scene.get_as::<NiSkinData>(data_ref.index()?)?;
        if data.bones.len() != bone_refs.len() {
            return None;
        }
        let bones = build_imported_bones(scene, &bone_refs, data)?;
        let skeleton_root = resolve_node_name(scene, skeleton_root_ref);
        return Some(ImportedSkin {
            bones,
            skeleton_root,
            vertex_bone_indices,
            vertex_bone_weights,
        });
    }

    // Skyrim SE / FO4+ path: BSSkin::Instance + BSSkin::BoneData.
    if let Some(inst) = scene.get_as::<BsSkinInstance>(skin_idx) {
        let bone_data = scene.get_as::<BsSkinBoneData>(inst.bone_data_ref.index()?)?;
        if bone_data.bones.len() != inst.bone_refs.len() {
            return None;
        }
        let mut bones = Vec::with_capacity(inst.bone_refs.len());
        for (i, bone_ref) in inst.bone_refs.iter().enumerate() {
            let name = resolve_node_name(scene, *bone_ref).unwrap_or_else(|| format!("Bone{}", i));
            let bt = &bone_data.bones[i];
            bones.push(ImportedBone {
                name,
                bind_inverse: bs_bone_to_inverse_matrix(bt),
                bounding_sphere: bt.bounding_sphere,
            });
        }
        let skeleton_root = resolve_node_name(scene, inst.skeleton_root_ref);
        return Some(ImportedSkin {
            bones,
            skeleton_root,
            vertex_bone_indices,
            vertex_bone_weights,
        });
    }

    None
}

/// Build `ImportedBone`s from a NiSkinInstance bone list and NiSkinData
/// bone entries. The two inputs must have matching lengths (checked by
/// the caller). Applies Z-up → Y-up conversion to each bind transform.
fn build_imported_bones(
    scene: &NifScene,
    bone_refs: &[BlockRef],
    data: &NiSkinData,
) -> Option<Vec<ImportedBone>> {
    let mut bones = Vec::with_capacity(bone_refs.len());
    for (i, bone_ref) in bone_refs.iter().enumerate() {
        let name = resolve_node_name(scene, *bone_ref).unwrap_or_else(|| format!("Bone{}", i));
        let bone = &data.bones[i];
        bones.push(ImportedBone {
            name,
            bind_inverse: ni_transform_to_yup_matrix(&bone.skin_transform),
            bounding_sphere: bone.bounding_sphere,
        });
    }
    Some(bones)
}

/// Resolve a BlockRef pointing to a NiNode to the node's name.
/// Returns `None` if the ref is null, the block isn't a NiNode, or the
/// node has no name.
fn resolve_node_name(scene: &NifScene, node_ref: BlockRef) -> Option<String> {
    let idx = node_ref.index()?;
    let node = scene.get_as::<NiNode>(idx)?;
    node.av.net.name.as_deref().map(str::to_string)
}

/// Convert a NiTransform to a column-major 4x4 matrix with the Y-up
/// basis change applied. NiSkinData stores the bind-inverse already —
/// we just need to reorder rows/columns for glam's column-major layout
/// and convert Gamebryo Z-up to engine Y-up (90° rotation around X).
fn ni_transform_to_yup_matrix(t: &NiTransform) -> [[f32; 4]; 4] {
    // Z-up → Y-up basis change matrix C (row vectors for NiMatrix3 style):
    //   C = [[1, 0, 0], [0, 0, 1], [0, -1, 0]]
    // For a NiTransform (R, t, s) in Z-up, the Y-up equivalent is:
    //   R' = C * R * C^T
    //   t' = C * t
    //   s  = s
    let r = &t.rotation.rows;
    let tx = t.translation.x;
    let ty = t.translation.y;
    let tz = t.translation.z;

    // C * R: row-major multiply. C has rows [1,0,0], [0,0,1], [0,-1,0].
    //   cr[0][j] = r[0][j]
    //   cr[1][j] = r[2][j]
    //   cr[2][j] = -r[1][j]
    let cr = [
        [r[0][0], r[0][1], r[0][2]],
        [r[2][0], r[2][1], r[2][2]],
        [-r[1][0], -r[1][1], -r[1][2]],
    ];
    // (C*R) * C^T: columns of C^T are the rows of C.
    //   cr_ct[i][0] = cr[i][0]
    //   cr_ct[i][1] = cr[i][2]
    //   cr_ct[i][2] = -cr[i][1]
    let rr = [
        [cr[0][0], cr[0][2], -cr[0][1]],
        [cr[1][0], cr[1][2], -cr[1][1]],
        [cr[2][0], cr[2][2], -cr[2][1]],
    ];
    // C * t
    let tt = [tx, tz, -ty];

    // Pack into column-major 4x4 with uniform scale baked in.
    let s = t.scale;
    [
        [rr[0][0] * s, rr[1][0] * s, rr[2][0] * s, 0.0],
        [rr[0][1] * s, rr[1][1] * s, rr[2][1] * s, 0.0],
        [rr[0][2] * s, rr[1][2] * s, rr[2][2] * s, 0.0],
        [tt[0], tt[1], tt[2], 1.0],
    ]
}

/// Build a bind-inverse matrix from a BSSkin::BoneData bone entry.
/// The row-major 3x3 rotation + translation + scale layout mirrors
/// NiTransform, so we reuse the same conversion.
fn bs_bone_to_inverse_matrix(b: &crate::blocks::skin::BsSkinBoneTrans) -> [[f32; 4]; 4] {
    let t = NiTransform {
        rotation: crate::types::NiMatrix3 { rows: b.rotation },
        translation: NiPoint3 {
            x: b.translation[0],
            y: b.translation[1],
            z: b.translation[2],
        },
        scale: b.scale,
    };
    ni_transform_to_yup_matrix(&t)
}

/// Densify sparse per-bone weight lists to per-vertex `[bone_idx; 4]` +
/// `[weight; 4]` arrays. Keeps the 4 highest contributions per vertex
/// and re-normalizes so the weights sum to 1.0.
///
/// Vertices with no bone contribution get `([0, 0, 0, 0], [1, 0, 0, 0])`
/// which binds them to bone 0 with full weight — safer than all-zeros
/// which would collapse to the origin during matrix palette skinning.
fn densify_sparse_weights(
    num_vertices: usize,
    data: &NiSkinData,
) -> (Vec<[u8; 4]>, Vec<[f32; 4]>) {
    // Per-vertex sorted top-4 contributions. Initialized to (255, 0.0)
    // so missing slots are obviously invalid until we replace them.
    let mut per_vertex: Vec<[(u8, f32); 4]> = vec![[(255u8, 0.0f32); 4]; num_vertices];

    for (bone_idx, bone) in data.bones.iter().enumerate() {
        // NiSkinData supports more than 256 bones in theory, but the
        // hardware palette limits us to u8. Skip any bone index that
        // can't be represented.
        let bone_u8 = if bone_idx < 256 {
            bone_idx as u8
        } else {
            continue;
        };
        for vw in &bone.vertex_weights {
            let v = vw.vertex_index as usize;
            if v >= num_vertices {
                continue;
            }
            let slots = &mut per_vertex[v];

            // Find the slot with the smallest current weight; replace
            // it if our weight is larger. This runs O(4) per weight
            // entry which is negligible for typical meshes.
            let (min_slot, min_weight) = slots
                .iter()
                .enumerate()
                .min_by(|a, b| a.1 .1.partial_cmp(&b.1 .1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, s)| (i, s.1))
                .unwrap_or((0, 0.0));

            if vw.weight > min_weight {
                slots[min_slot] = (bone_u8, vw.weight);
            }
        }
    }

    let mut vertex_bone_indices = Vec::with_capacity(num_vertices);
    let mut vertex_bone_weights = Vec::with_capacity(num_vertices);

    for slots in &per_vertex {
        let total: f32 = slots
            .iter()
            .filter(|(b, _)| *b != 255)
            .map(|(_, w)| *w)
            .sum();

        if total <= f32::EPSILON {
            // No contribution — bind to bone 0 so matrix palette
            // skinning doesn't collapse the vertex to the origin.
            vertex_bone_indices.push([0, 0, 0, 0]);
            vertex_bone_weights.push([1.0, 0.0, 0.0, 0.0]);
            continue;
        }

        let inv = 1.0 / total;
        let mut idx = [0u8; 4];
        let mut w = [0.0f32; 4];
        for (i, (b, weight)) in slots.iter().enumerate() {
            if *b != 255 {
                idx[i] = *b;
                w[i] = *weight * inv;
            }
        }
        vertex_bone_indices.push(idx);
        vertex_bone_weights.push(w);
    }

    (vertex_bone_indices, vertex_bone_weights)
}

#[cfg(test)]
mod skin_tests {
    use super::*;
    use crate::blocks::skin::{BoneData, BoneVertWeight};
    use crate::types::NiMatrix3;

    fn identity_transform() -> NiTransform {
        NiTransform {
            rotation: NiMatrix3 {
                rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            },
            translation: NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            scale: 1.0,
        }
    }

    fn bone(weights: Vec<(u16, f32)>) -> BoneData {
        BoneData {
            skin_transform: identity_transform(),
            bounding_sphere: [0.0, 0.0, 0.0, 0.0],
            vertex_weights: weights
                .into_iter()
                .map(|(vertex_index, weight)| BoneVertWeight {
                    vertex_index,
                    weight,
                })
                .collect(),
        }
    }

    #[test]
    fn densify_empty_data_gives_default_binding() {
        // No bones at all — every vertex should fall back to bone 0 weight 1.
        let data = NiSkinData {
            skin_transform: identity_transform(),
            bones: Vec::new(),
        };
        let (indices, weights) = densify_sparse_weights(3, &data);
        assert_eq!(indices.len(), 3);
        assert_eq!(weights.len(), 3);
        for i in 0..3 {
            assert_eq!(indices[i], [0, 0, 0, 0]);
            assert_eq!(weights[i], [1.0, 0.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn densify_single_bone_full_weight() {
        // Bone 0 binds vertex 0 with weight 1.0, vertex 1 not bound.
        let data = NiSkinData {
            skin_transform: identity_transform(),
            bones: vec![bone(vec![(0, 1.0)])],
        };
        let (indices, weights) = densify_sparse_weights(2, &data);
        assert_eq!(indices[0], [0, 0, 0, 0]);
        assert!((weights[0][0] - 1.0).abs() < 1e-6);
        // Vertex 1 falls back to bone 0 weight 1.
        assert_eq!(indices[1], [0, 0, 0, 0]);
        assert_eq!(weights[1], [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn densify_two_bones_normalized() {
        // Vertex 0 gets half-and-half from bones 0 and 1.
        let data = NiSkinData {
            skin_transform: identity_transform(),
            bones: vec![bone(vec![(0, 0.5)]), bone(vec![(0, 0.5)])],
        };
        let (indices, weights) = densify_sparse_weights(1, &data);
        // Two slots used, two unused. Weights sum to 1.
        let total: f32 = weights[0].iter().sum();
        assert!((total - 1.0).abs() < 1e-5);
        // Exactly two distinct bones present (0 and 1). Order inside
        // the 4-slot tuple isn't guaranteed by the algorithm.
        let mut seen: Vec<u8> = indices[0]
            .iter()
            .zip(weights[0].iter())
            .filter(|(_, w)| **w > 0.0)
            .map(|(b, _)| *b)
            .collect();
        seen.sort();
        assert_eq!(seen, vec![0, 1]);
    }

    #[test]
    fn densify_more_than_four_bones_keeps_top_four_by_weight() {
        // Five bones all bind vertex 0 with increasing weight. The top
        // 4 (weights 0.2, 0.3, 0.4, 0.5) should survive; the smallest
        // (0.1) should be dropped. After normalization the kept weights
        // sum to 1.
        let data = NiSkinData {
            skin_transform: identity_transform(),
            bones: vec![
                bone(vec![(0, 0.1)]), // bone 0 — should be dropped
                bone(vec![(0, 0.2)]),
                bone(vec![(0, 0.3)]),
                bone(vec![(0, 0.4)]),
                bone(vec![(0, 0.5)]),
            ],
        };
        let (indices, weights) = densify_sparse_weights(1, &data);

        let total: f32 = weights[0].iter().sum();
        assert!((total - 1.0).abs() < 1e-5, "weights should sum to 1");

        let mut present: Vec<(u8, f32)> = indices[0]
            .iter()
            .zip(weights[0].iter())
            .filter(|(_, w)| **w > 0.0)
            .map(|(b, w)| (*b, *w))
            .collect();
        assert_eq!(present.len(), 4, "should keep exactly 4 bones");
        present.sort_by_key(|(b, _)| *b);

        // Dropped bone 0 (weight 0.1); kept bones 1..=4.
        let bones: Vec<u8> = present.iter().map(|(b, _)| *b).collect();
        assert_eq!(bones, vec![1, 2, 3, 4]);

        // Original sum = 0.2 + 0.3 + 0.4 + 0.5 = 1.4; after normalizing
        // each weight becomes w / 1.4.
        assert!((present[0].1 - 0.2 / 1.4).abs() < 1e-5);
        assert!((present[3].1 - 0.5 / 1.4).abs() < 1e-5);
    }

    #[test]
    fn ni_transform_to_yup_matrix_identity() {
        let t = identity_transform();
        let m = ni_transform_to_yup_matrix(&t);
        // Identity rotation through C * I * C^T = I, identity translation, scale 1.
        // Column 0 = (1,0,0,0), col 1 = (0,1,0,0), col 2 = (0,0,1,0), col 3 = (0,0,0,1)
        assert!((m[0][0] - 1.0).abs() < 1e-6);
        assert!((m[1][1] - 1.0).abs() < 1e-6);
        assert!((m[2][2] - 1.0).abs() < 1e-6);
        assert!((m[3][3] - 1.0).abs() < 1e-6);
        // Off-diagonals zero.
        assert!(m[0][1].abs() < 1e-6);
        assert!(m[1][0].abs() < 1e-6);
    }

    #[test]
    fn ni_transform_to_yup_matrix_translation_only() {
        // Gamebryo Z-up translation (1, 2, 3) → Y-up (1, 3, -2).
        let t = NiTransform {
            rotation: NiMatrix3 {
                rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            },
            translation: NiPoint3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            scale: 1.0,
        };
        let m = ni_transform_to_yup_matrix(&t);
        // Column 3 holds the translation in column-major storage.
        assert!((m[3][0] - 1.0).abs() < 1e-6);
        assert!((m[3][1] - 3.0).abs() < 1e-6);
        assert!((m[3][2] + 2.0).abs() < 1e-6);
    }

    #[test]
    fn ni_transform_to_yup_matrix_scale_baked_in() {
        let mut t = identity_transform();
        t.scale = 2.5;
        let m = ni_transform_to_yup_matrix(&t);
        // Diagonal should be scale.
        assert!((m[0][0] - 2.5).abs() < 1e-6);
        assert!((m[1][1] - 2.5).abs() < 1e-6);
        assert!((m[2][2] - 2.5).abs() < 1e-6);
        // W column still identity.
        assert!((m[3][3] - 1.0).abs() < 1e-6);
    }
}
