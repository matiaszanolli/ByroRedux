//! Geometry extraction from NiTriShape and BsTriShape blocks.

use std::sync::Arc;

use crate::blocks::node::NiNode;
use crate::blocks::skin::{
    BsDismemberSkinInstance, BsSkinBoneData, BsSkinInstance, NiSkinData, NiSkinInstance,
    NiSkinPartition, SseSkinGlobalBuffer,
};
use crate::blocks::tri_shape::{BsTriShape, NiTriShape, NiTriShapeData, NiTriStripsData};
#[cfg(test)]
use crate::blocks::tri_shape::BsTriShapeKind;
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3, NiTransform};

use super::coord::{zup_matrix_to_yup_quat, zup_point_to_yup};
use super::material::{extract_material_info, extract_vertex_colors};
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

    // Convert positions: Gamebryo Z-up → renderer Y-up (see `coord.rs`).
    let positions: Vec<[f32; 3]> = geom.vertices.iter().map(zup_point_to_yup).collect();

    // Convert indices (u16 → u32). Winding order preserved — the Z-up → Y-up
    // transform is a proper rotation (det=+1), not a reflection.
    let indices: Vec<u32> = geom
        .triangles
        .iter()
        .flat_map(|tri| [tri[0] as u32, tri[1] as u32, tri[2] as u32])
        .collect();

    // Convert normals with same axis swap (fall back to +Y up if none)
    let normals: Vec<[f32; 3]> = if !geom.normals.is_empty() {
        geom.normals.iter().map(zup_point_to_yup).collect()
    } else {
        vec![[0.0, 1.0, 0.0]; positions.len()]
    };

    // Get UVs from first UV set (if available)
    let uvs = geom.uv_sets.first().cloned().unwrap_or_default();

    // Single-pass material property extraction — called once and reused for
    // both vertex color resolution and material fields. Eliminates the double
    // extract_material_info that previously occurred via extract_material →
    // find_texture_path → extract_material_info + direct call. #279 D5-10.
    let mat = extract_material_info(scene, shape, inherited_props);

    // Determine vertex colors: prefer per-vertex colors, then material diffuse, then white.
    let colors = extract_vertex_colors(scene, shape, &geom, inherited_props, &mat);

    // Apply Z-up → Y-up to the entity transform.
    let t = &world_transform.translation;
    let r = &world_transform.rotation;

    // Convert the Z-up rotation matrix to Y-up, then extract a robust quaternion.
    let quat = zup_matrix_to_yup_quat(r);

    // Skinning data (issue #151). Populated when the shape has a
    // NiSkinInstance / BSDismemberSkinInstance backing it.
    let skin = extract_skin_ni_tri_shape(scene, shape, positions.len());

    // Local bounding sphere in Y-up renderer space. Prefer the NIF-provided
    // NiBound on NiGeometryData; fall back to a fresh centroid+max-distance
    // sphere computed from the positions when the NIF omits one (radius 0).
    // See #217.
    let (local_bound_center, local_bound_radius) =
        extract_local_bound(geom.bound_center, geom.bound_radius, &positions);

    // Capture the shader-type fields before moving other `mat` fields into
    // the `ImportedMesh` literal. See #430.
    let shader_type_fields = mat.shader_type_fields();

    Some(ImportedMesh {
        positions,
        colors,
        normals,
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
        skin,
        z_test: mat.z_test,
        z_write: mat.z_write,
        z_function: mat.z_function,
        local_bound_center,
        local_bound_radius,
        effect_shader: mat.effect_shader,
        material_kind: mat.material_kind,
        // #430 — surface SkinTint / HairTint / EyeEnvmap / ParallaxOcc /
        // MultiLayerParallax / SparkleSnow fields on the mesh.
        // `extract_material_info` already populated them on MaterialInfo
        // via `apply_shader_type_data`; before this fix they died here.
        shader_type_fields,
        // #451 — forward the BSShaderNoLightingProperty soft-falloff
        // cone (FO3/FNV HUD overlays). `None` for non-NoLighting meshes.
        no_lighting_falloff: mat.no_lighting_falloff,
        flags: shape.av.flags,
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
        return (zup_point_to_yup(&nif_center), nif_radius);
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
///
/// Material extraction delegates to [`extract_material_info_from_refs`]
/// — the same implementation the NiTriShape path uses — so every
/// shader-data capture (BSLightingShaderProperty fields, ShaderTypeData
/// variants, BSEffectShaderProperty effect data, NiAlphaProperty
/// flags, decal / two-sided / material_kind / BGSM path resolution)
/// stays in parity between the two geometry types. Pre-#129 this
/// function re-implemented ~130 lines of material extraction inline
/// and drifted (see NIF-403 for a concrete instance).
pub(super) fn extract_bs_tri_shape(
    scene: &NifScene,
    shape: &BsTriShape,
    world_transform: &NiTransform,
) -> Option<ImportedMesh> {
    // Skyrim SE / FO4 skinned meshes ship `data_size == 0` on the
    // `BsTriShape` itself — the real geometry lives on the linked
    // `NiSkinPartition` as a global packed-vertex buffer plus
    // per-partition `vertex_map` arrays. Reconstruct here before the
    // early-return so every NPC body and creature renders. See #559.
    let reconstructed = if shape.vertices.is_empty() && shape.triangles.is_empty() {
        try_reconstruct_sse_geometry(scene, shape)
    } else {
        None
    };

    if shape.vertices.is_empty() && reconstructed.is_none() {
        return None;
    }
    if shape.triangles.is_empty() && reconstructed.is_none() {
        return None;
    }

    let (positions, indices, sse_normals, sse_uvs, sse_colors) = if let Some(geom) = reconstructed {
        (
            geom.positions,
            geom.indices,
            Some(geom.normals),
            Some(geom.uvs),
            Some(geom.colors),
        )
    } else {
        let positions: Vec<[f32; 3]> = shape.vertices.iter().map(zup_point_to_yup).collect();
        let indices: Vec<u32> = shape
            .triangles
            .iter()
            .flat_map(|tri| [tri[0] as u32, tri[1] as u32, tri[2] as u32])
            .collect();
        (positions, indices, None, None, None)
    };

    let normals: Vec<[f32; 3]> = if let Some(n) = sse_normals {
        n
    } else if !shape.normals.is_empty() {
        shape.normals.iter().map(zup_point_to_yup).collect()
    } else {
        vec![[0.0, 1.0, 0.0]; positions.len()]
    };

    let uvs = if let Some(u) = sse_uvs {
        u
    } else {
        shape.uvs.clone()
    };

    // Keep all 4 components — alpha lane carries authored per-vertex
    // modulation (hair tips, eyelash strips, BSEffectShader meshes).
    // See #618.
    let colors: Vec<[f32; 4]> = if let Some(c) = sse_colors {
        c
    } else if !shape.vertex_colors.is_empty() {
        shape.vertex_colors.clone()
    } else {
        vec![[1.0, 1.0, 1.0, 1.0]; positions.len()]
    };

    // Unified material extraction — shared with the NiTriShape path.
    // BsTriShape has no legacy NiProperty chain, so direct / inherited
    // slices are empty. The shared implementation handles
    // BSLightingShaderProperty / BSEffectShaderProperty, the implicit
    // effect-shader alpha blend override (#354), Double_Sided from
    // shader_flags_2, decals from shader flags, BGSM/BGEM name
    // resolution, and the ShaderTypeData → ShaderTypeFields capture
    // (#430). See #129.
    let mat = super::material::extract_material_info_from_refs(
        scene,
        shape.shader_property_ref,
        shape.alpha_property_ref,
        &[],
        &[],
    );

    let t = &world_transform.translation;
    let quat = zup_matrix_to_yup_quat(&world_transform.rotation);

    // Skinning data. BSTriShape per-vertex weights live in the packed
    // vertex buffer (VF_SKINNED), decoded at parse time (#177).
    let skin = extract_skin_bs_tri_shape(scene, shape);

    // BSTriShape carries its own bounding sphere (center + radius) on the
    // block. See #217.
    let (local_bound_center, local_bound_radius) =
        extract_local_bound(shape.center, shape.radius, &positions);

    // #430 — capture ShaderTypeFields before the `mat` move.
    let shader_type_fields = mat.shader_type_fields();

    Some(ImportedMesh {
        positions,
        colors,
        normals,
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
        // BsTriShape (Skyrim+) routes all texture slots through
        // BSShaderTextureSet — the legacy NiTexturingProperty
        // glow/detail/gloss/dark slots don't apply. Skyrim+ glow is in
        // BSShaderTextureSet slot 2 (`mat.glow_map`), which the shared
        // extractor already reads.
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
        skin,
        // BSTriShape (Skyrim+) has no NiZBufferProperty binding; the
        // shared extractor preserves Gamebryo runtime defaults
        // (z_test+write on, LESSEQUAL) when no NiZBufferProperty is
        // found — which is always on this path.
        z_test: mat.z_test,
        z_write: mat.z_write,
        z_function: mat.z_function,
        local_bound_center,
        local_bound_radius,
        effect_shader: mat.effect_shader,
        material_kind: mat.material_kind,
        shader_type_fields,
        // BSShaderNoLightingProperty is an FO3/FNV-era property and
        // doesn't bind to BsTriShape (Skyrim+); the shared extractor
        // won't populate it here. See #451.
        no_lighting_falloff: mat.no_lighting_falloff,
        flags: shape.av.flags,
    })
}

/// Extract a BsTriShape with local transform (for hierarchical import).
pub(super) fn extract_bs_tri_shape_local(
    scene: &NifScene,
    shape: &BsTriShape,
) -> Option<ImportedMesh> {
    extract_bs_tri_shape(scene, shape, &shape.av.transform)
}

// ── #559: SSE skinned-geometry reconstruction ─────────────────────

/// Reassembled geometry sourced from a `NiSkinPartition` global vertex
/// buffer when the linked `BsTriShape` has empty inline arrays.
/// Positions and normals are already Z-up→Y-up converted; triangles
/// are flat u32 indices into the buffer's vertex space.
struct ReconstructedSseGeometry {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

/// `BSVertexDesc` flag bits — mirror the constants in
/// [`crate::blocks::tri_shape`]. Re-declared private here to keep the
/// SSE-skin reconstructor self-contained without bumping the visibility
/// of every parser-side flag. The values are part of the nif.xml
/// `BSVertexDesc.VertexAttribute` bitfield (line 8231) and stable
/// across the engine's lifetime.
const VF_VERTEX: u16 = 0x001;
const VF_UVS: u16 = 0x002;
const VF_NORMALS: u16 = 0x008;
const VF_TANGENTS: u16 = 0x010;
const VF_VERTEX_COLORS: u16 = 0x020;
const VF_SKINNED: u16 = 0x040;
const VF_EYE_DATA: u16 = 0x100;

/// Resolve `shape.skin_ref` → `NiSkinInstance` (or
/// `BsDismemberSkinInstance`) → `NiSkinPartition` and reconstruct
/// vertices + triangles when the partition's SSE global buffer is
/// populated. Returns `None` for non-SSE NIFs and for shapes whose
/// inline arrays already carry the geometry.
///
/// The global buffer holds every mesh vertex in the same packed format
/// `BsTriShape::parse` decodes inline (positions + uvs + normals +
/// colors + skin data + eye data, gated by `vertex_attrs`). Each
/// partition's `vertex_map` translates partition-local 0..N-1 indices
/// into global-buffer indices; partition triangles concatenate (after
/// remap) into the final index list.
fn try_reconstruct_sse_geometry(
    scene: &NifScene,
    shape: &BsTriShape,
) -> Option<ReconstructedSseGeometry> {
    let skin_idx = shape.skin_ref.index()?;

    // Resolve through either the legacy NiSkinInstance or the FO4+
    // BSDismemberSkinInstance — both expose `skin_partition_ref`.
    let partition_ref = if let Some(inst) = scene.get_as::<NiSkinInstance>(skin_idx) {
        inst.skin_partition_ref
    } else if let Some(inst) = scene.get_as::<BsDismemberSkinInstance>(skin_idx) {
        inst.base.skin_partition_ref
    } else {
        return None;
    };

    let partition_idx = partition_ref.index()?;
    let partition = scene.get_as::<NiSkinPartition>(partition_idx)?;
    let buffer = partition.global_vertex_data.as_ref()?;

    // Decode the global buffer into Y-up positions / normals / UVs /
    // colors. Per-vertex skin payload is also captured by the inline
    // parser at `tri_shape.rs`, but reconstructing the skin palette
    // from the partition's own bone_indices/vertex_weights is a
    // follow-up — see commit message.
    let decoded = decode_sse_packed_buffer(buffer)?;

    // Concatenate partition triangles, remapping each partition-local
    // index through the partition's vertex_map.
    let mut indices = Vec::new();
    for part in &partition.partitions {
        for tri in &part.triangles {
            for &local in tri {
                let local = local as usize;
                let global = part
                    .vertex_map
                    .get(local)
                    .copied()
                    .unwrap_or(local as u16);
                indices.push(global as u32);
            }
        }
    }
    if indices.is_empty() {
        return None;
    }

    Some(ReconstructedSseGeometry {
        positions: decoded.positions,
        normals: decoded.normals,
        uvs: decoded.uvs,
        colors: decoded.colors,
        indices,
    })
}

struct DecodedPackedBuffer {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    /// Per-vertex bone weights when the buffer carries `VF_SKINNED`.
    /// Empty when the flag is clear. 4 weights per vertex, decoded
    /// from packed half-floats. See #638.
    bone_weights: Vec<[f32; 4]>,
    /// Per-vertex bone indices when the buffer carries `VF_SKINNED`.
    /// Partition-local — the caller must remap through
    /// `NiSkinPartition.partitions[i].bones` to get global skin
    /// list indices. See #638 / #613.
    bone_indices: Vec<[u8; 4]>,
}

/// Decode a `SseSkinGlobalBuffer` into Y-up vertex arrays.
///
/// On Skyrim SE (bsver in `[100, 130)` — the only band where this
/// buffer is captured) positions are always full-precision per the
/// inline parser's `bsver < 130 || VF_FULL_PRECISION`. UVs are 2 ×
/// half-float, normals are 3 × normbyte + 1 byte bitangent_y, colors
/// are 4 × u8. Tangent / skin / eye data slots are skipped per the
/// `vertex_attrs` mask. Returns `None` when the buffer is malformed
/// (size mismatch, vertex_size == 0, or VF_VERTEX clear).
fn decode_sse_packed_buffer(buffer: &SseSkinGlobalBuffer) -> Option<DecodedPackedBuffer> {
    let vertex_size = buffer.vertex_size as usize;
    if vertex_size == 0 || buffer.raw_bytes.len() % vertex_size != 0 {
        return None;
    }
    let num_vertices = buffer.raw_bytes.len() / vertex_size;
    let vertex_attrs = ((buffer.vertex_desc >> 44) & 0xFFF) as u16;
    if vertex_attrs & VF_VERTEX == 0 {
        return None;
    }

    let mut positions = Vec::with_capacity(num_vertices);
    let mut normals = Vec::with_capacity(num_vertices);
    let mut uvs = Vec::with_capacity(num_vertices);
    let mut colors = Vec::with_capacity(num_vertices);
    let is_skinned = vertex_attrs & VF_SKINNED != 0;
    let mut bone_weights: Vec<[f32; 4]> = if is_skinned {
        Vec::with_capacity(num_vertices)
    } else {
        Vec::new()
    };
    let mut bone_indices: Vec<[u8; 4]> = if is_skinned {
        Vec::with_capacity(num_vertices)
    } else {
        Vec::new()
    };

    for i in 0..num_vertices {
        let base = i * vertex_size;
        let bytes = &buffer.raw_bytes[base..base + vertex_size];
        let mut off = 0usize;

        // Position: 3 × f32 + bitangent_x (f32) — 16 bytes total.
        // SSE always uses full-precision per inline-decoder's
        // `bsver < 130 || VF_FULL_PRECISION` rule.
        let x = read_f32_le(bytes, off)?;
        let y = read_f32_le(bytes, off + 4)?;
        let z = read_f32_le(bytes, off + 8)?;
        // Z-up → Y-up: (x, z, -y).
        positions.push([x, z, -y]);
        off += 16;

        // UV: 2 × f16.
        if vertex_attrs & VF_UVS != 0 {
            let u = half_to_f32(read_u16_le(bytes, off)?);
            let v = half_to_f32(read_u16_le(bytes, off + 2)?);
            uvs.push([u, v]);
            off += 4;
        }

        // Normal: 3 × normbyte + 1 byte bitangent_y.
        if vertex_attrs & VF_NORMALS != 0 {
            let nx = byte_to_normal(bytes[off]);
            let ny = byte_to_normal(bytes[off + 1]);
            let nz = byte_to_normal(bytes[off + 2]);
            // Z-up → Y-up: (x, z, -y).
            normals.push([nx, nz, -ny]);
            off += 4;
        }

        // Tangent: 3 × normbyte + bitangent_z. Discarded per #351.
        if vertex_attrs & VF_TANGENTS != 0 && vertex_attrs & VF_NORMALS != 0 {
            off += 4;
        }

        // Vertex colors: 4 × u8 → RGBA float. #618 keeps alpha.
        if vertex_attrs & VF_VERTEX_COLORS != 0 {
            let r = bytes[off] as f32 / 255.0;
            let g = bytes[off + 1] as f32 / 255.0;
            let b = bytes[off + 2] as f32 / 255.0;
            let a = bytes[off + 3] as f32 / 255.0;
            colors.push([r, g, b, a]);
            off += 4;
        }

        // Skin payload: 4 × half-float weights + 4 × u8 indices.
        // #638 — pre-fix this whole 12-byte run was skipped, and
        // `extract_skin_bs_tri_shape` then read `shape.bone_weights`
        // off the BSTriShape itself. That field is empty when geometry
        // lives in the global buffer (Skyrim SE NPC bodies have
        // `data_size == 0` on the BSTriShape and ship skin data only
        // in the partition's `SseSkinGlobalBuffer.raw_bytes`). The
        // fallback path now reads decoded values from
        // `bone_weights` / `bone_indices` here so every NPC body
        // animates correctly once M41 spawns them.
        if is_skinned {
            let w0 = half_to_f32(read_u16_le(bytes, off)?);
            let w1 = half_to_f32(read_u16_le(bytes, off + 2)?);
            let w2 = half_to_f32(read_u16_le(bytes, off + 4)?);
            let w3 = half_to_f32(read_u16_le(bytes, off + 6)?);
            bone_weights.push([w0, w1, w2, w3]);
            bone_indices.push([
                bytes[off + 8],
                bytes[off + 9],
                bytes[off + 10],
                bytes[off + 11],
            ]);
            off += 12;
        }

        // Eye data: 1 × f32. Discarded — no consumer today.
        if vertex_attrs & VF_EYE_DATA != 0 {
            off += 4;
        }

        // Trailing padding (vertex_size - off) bytes — silently absorbed.
        // Defensive guard: bail if we read past the declared stride.
        if off > vertex_size {
            return None;
        }
    }

    // Fall-back fills when a flag is clear so the parallel arrays stay
    // length-aligned with `positions`. The renderer's per-vertex
    // composition tolerates [0, 1, 0] / [0, 0] / opaque-white defaults.
    if normals.is_empty() {
        normals = vec![[0.0, 1.0, 0.0]; num_vertices];
    }
    if uvs.is_empty() {
        uvs = vec![[0.0, 0.0]; num_vertices];
    }
    if colors.is_empty() {
        colors = vec![[1.0, 1.0, 1.0, 1.0]; num_vertices];
    }

    Some(DecodedPackedBuffer {
        positions,
        normals,
        uvs,
        colors,
        bone_weights,
        bone_indices,
    })
}

#[inline]
fn read_f32_le(bytes: &[u8], offset: usize) -> Option<f32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(f32::from_le_bytes(slice.try_into().ok()?))
}

#[inline]
fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes(slice.try_into().ok()?))
}

#[inline]
fn half_to_f32(h: u16) -> f32 {
    // Same IEEE 754 binary16 decode as `tri_shape::half_to_f32`.
    // Re-declared so `import/mesh.rs` doesn't depend on a
    // `pub(crate)` export in tri_shape that might churn.
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as i32;
    let mant = (h & 0x3FF) as u32;
    let bits = if exp == 0 {
        if mant == 0 {
            sign << 31
        } else {
            // Subnormal — normalise.
            let mut m = mant;
            let mut e = -14_i32;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            m &= 0x3FF;
            (sign << 31) | (((e + 127) as u32) << 23) | (m << 13)
        }
    } else if exp == 31 {
        // Inf / NaN — preserve mantissa for NaN payloads.
        (sign << 31) | (0xFFu32 << 23) | (mant << 13)
    } else {
        (sign << 31) | (((exp - 15 + 127) as u32) << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

#[inline]
fn byte_to_normal(b: u8) -> f32 {
    // Same `(b / 127.5) - 1.0` as `tri_shape::byte_to_normal`.
    (b as f32 / 127.5) - 1.0
}

/// Resolve the double-sided flag for a BsTriShape from either of the
/// two shader-property variants Skyrim+ binds. Both
/// `BSLightingShaderProperty` (the common case for static / clutter /
/// actor meshes) and `BSEffectShaderProperty` (Skyrim+ VFX surfaces:
/// force fields, magic auras, glow shells, Dwemer steam) use bit
/// `0x10` of `shader_flags_2` for the same double-sided semantics.
///
/// Pre-#128 only the BSLightingShaderProperty branch was checked, so
/// effect-shader-backed meshes silently dropped the flag and rendered
/// backface-culled glow geometry that should have been visible from
/// either side.
/// Return `Some(name)` when `name` is a `.bgsm`/`.bgem` material file
/// path, else `None`. Shared between the BsTriShape and NiTriShape
/// material-path extractors so both report material pointers consistently.
pub(super) fn material_path_from_name(name: Option<&str>) -> Option<String> {
    let name = name?;
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".bgsm") || lower.ends_with(".bgem") {
        Some(name.to_string())
    } else {
        None
    }
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
    let (vertex_bone_indices, vertex_bone_weights) = densify_sparse_weights(num_vertices, data);

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

    // Per-vertex weights come from the BSTriShape vertex buffer
    // (VF_SKINNED) — already decoded at parse time (#177). The
    // bone-INDEX side needs a partition-aware remap before it's
    // safe for downstream consumers — see #613 / SK-D1-01: the
    // inline `[u8; 4]` indices are partition-LOCAL (indices into
    // each `NiSkinPartition.partitions[i].bones` palette), not
    // global indices into the skin's bone list. The legacy clone
    // pre-#613 silently aliased every vertex past partition 0
    // when shapes split into > 1 partition.
    // #638 — Skyrim SE NPC bodies (and any BSTriShape whose `data_size
    // == 0`) ship per-vertex skin data only in the partition's
    // `SseSkinGlobalBuffer`, not on the inline arrays. Pre-fix
    // `shape.bone_weights.clone()` returned an empty Vec on those
    // meshes and every vertex hit the renderer's rigid fallback
    // (`wsum < 0.001` in `triangle.vert:151`), rendering NPCs in
    // bind pose. Fall back to the decoded global-buffer payload
    // when the inline arrays are empty.
    let (vertex_bone_weights, vertex_bone_indices) = if shape.bone_weights.is_empty() {
        match decode_sse_skin_payload(scene, shape) {
            Some((weights, raw_indices)) => {
                let remapped = remap_bs_tri_shape_bone_indices(scene, shape, &raw_indices);
                (weights, remapped)
            }
            None => (
                Vec::new(),
                remap_bs_tri_shape_bone_indices(scene, shape, &shape.bone_indices),
            ),
        }
    } else {
        (
            shape.bone_weights.clone(),
            remap_bs_tri_shape_bone_indices(scene, shape, &shape.bone_indices),
        )
    };

    // Skyrim LE path: NiSkinInstance + NiSkinData (bone list + bind transforms).
    // Borrow bone_refs instead of cloning — they're only iterated. #279 D5-11.
    let (bone_refs_slice, skeleton_root_ref, data_ref) =
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
            (&[] as &[_], BlockRef::NULL, BlockRef::NULL)
        };
    // #613 defensive: if the global skin bone list exceeds u16 range,
    // remap below truncates. Vanilla Bethesda content stays well under
    // this; warn if seen so the gap surfaces in test runs.
    if bone_refs_slice.len() > u16::MAX as usize {
        log::warn!(
            "BsTriShape skin has {} bones — exceeds u16 remap range; \
             indices past 65535 will truncate (see #613)",
            bone_refs_slice.len()
        );
    }
    if !bone_refs_slice.is_empty() {
        let data = scene.get_as::<NiSkinData>(data_ref.index()?)?;
        if data.bones.len() != bone_refs_slice.len() {
            return None;
        }
        let bones = build_imported_bones(scene, bone_refs_slice, data)?;
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
            let name = resolve_node_name(scene, *bone_ref)
                .unwrap_or_else(|| Arc::from(format!("Bone{}", i)));
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

/// Remap a `BsTriShape`'s inline `[u8; 4]` partition-local bone
/// indices to global `[u16; 4]` indices into the linked skin's bone
/// list. See #613 / SK-D1-01.
///
/// The wire format stores per-vertex bone indices as u8s indexing
/// into whichever `NiSkinPartition.partitions[i].bones` palette the
/// vertex belongs to — the partition splitter rebuilds a small bone
/// palette per partition so each vertex's 4 bones can fit in 1 byte
/// each. To recover the global bone list index we:
///
/// 1. Resolve `shape.skin_ref` → `NiSkinInstance` (or
///    `BsDismemberSkinInstance`) → `skin_partition_ref` →
///    `NiSkinPartition`.
/// 2. Build an inverse `vertex_map` lookup (global vertex idx →
///    partition idx) from each partition's `vertex_map`.
/// 3. For each vertex, find its partition's `bones` palette and
///    replace each u8 partition-local index with the global u16.
///
/// When the partition table is missing or the inverse map is
/// incomplete (synthetic / mod content), fall back to widening the
/// raw u8 to u16 — same behaviour as pre-#613 single-partition
/// shapes, which were correct because partition-local and global
/// indices coincide when there's only one partition with all bones.
fn remap_bs_tri_shape_bone_indices(
    scene: &NifScene,
    shape: &BsTriShape,
    bone_indices: &[[u8; 4]],
) -> Vec<[u16; 4]> {
    if bone_indices.is_empty() {
        return Vec::new();
    }

    // Identity widen — the safe fallback used when no partition
    // table is available. Single-partition shapes work fine here:
    // partition-local indices already match the global palette
    // because the partition's `bones` palette is the full bone list.
    let widen = |slot: u8| slot as u16;
    let identity_remap = || -> Vec<[u16; 4]> {
        bone_indices
            .iter()
            .map(|idx| [widen(idx[0]), widen(idx[1]), widen(idx[2]), widen(idx[3])])
            .collect()
    };

    let Some(skin_idx) = shape.skin_ref.index() else {
        return identity_remap();
    };
    let partition_ref = if let Some(inst) = scene.get_as::<NiSkinInstance>(skin_idx) {
        inst.skin_partition_ref
    } else if let Some(inst) = scene.get_as::<BsDismemberSkinInstance>(skin_idx) {
        inst.base.skin_partition_ref
    } else {
        return identity_remap();
    };
    let Some(partition_idx) = partition_ref.index() else {
        return identity_remap();
    };
    let Some(partition) = scene.get_as::<crate::blocks::skin::NiSkinPartition>(partition_idx)
    else {
        return identity_remap();
    };
    if partition.partitions.len() <= 1 {
        // Single-partition shapes don't need remapping: the
        // partition's bones palette covers the full skin list and
        // partition-local indices == global indices. Skip the work.
        return identity_remap();
    }

    // Build inverse map: global_vertex_idx → (partition_idx). Each
    // partition's `vertex_map[local_i] = global_v` describes which
    // BsTriShape vertex slot the partition-local position points at.
    // Multi-partition shapes split vertices across partitions; the
    // first vertex_map entry that mentions a global index wins (no
    // vanilla content overlaps partitions on the same vertex).
    let mut vertex_to_partition: Vec<Option<u32>> = vec![None; bone_indices.len()];
    for (p_idx, part) in partition.partitions.iter().enumerate() {
        for &gv in &part.vertex_map {
            let gv = gv as usize;
            if gv < vertex_to_partition.len() && vertex_to_partition[gv].is_none() {
                vertex_to_partition[gv] = Some(p_idx as u32);
            }
        }
    }

    bone_indices
        .iter()
        .enumerate()
        .map(|(v, idx)| {
            let part = vertex_to_partition[v]
                .and_then(|p| partition.partitions.get(p as usize));
            match part {
                Some(p) => [
                    remap_one(idx[0], &p.bones),
                    remap_one(idx[1], &p.bones),
                    remap_one(idx[2], &p.bones),
                    remap_one(idx[3], &p.bones),
                ],
                // Vertex outside every partition's vertex_map — rare
                // edge case (truncated NIF, mod malformation). Widen
                // with zero so the renderer falls back to bind pose
                // for that vertex rather than reading garbage.
                None => [widen(idx[0]), widen(idx[1]), widen(idx[2]), widen(idx[3])],
            }
        })
        .collect()
}

/// Resolve `shape.skin_ref` → `NiSkinPartition` → `SseSkinGlobalBuffer`
/// and decode the per-vertex skin payload (4 × half-float weights +
/// 4 × u8 partition-local bone indices). Returns `None` when the
/// shape doesn't go through the global-buffer path or the buffer is
/// missing / malformed.
///
/// Caller (`extract_skin_bs_tri_shape`) feeds the indices through
/// `remap_bs_tri_shape_bone_indices` for the partition-local → global
/// remap. The weights are pass-through — they're already partition-
/// agnostic. See #638.
fn decode_sse_skin_payload(
    scene: &NifScene,
    shape: &BsTriShape,
) -> Option<(Vec<[f32; 4]>, Vec<[u8; 4]>)> {
    let skin_idx = shape.skin_ref.index()?;
    let partition_ref = if let Some(inst) = scene.get_as::<NiSkinInstance>(skin_idx) {
        inst.skin_partition_ref
    } else if let Some(inst) = scene.get_as::<BsDismemberSkinInstance>(skin_idx) {
        inst.base.skin_partition_ref
    } else {
        return None;
    };
    let partition_idx = partition_ref.index()?;
    let partition = scene.get_as::<crate::blocks::skin::NiSkinPartition>(partition_idx)?;
    let buffer = partition.global_vertex_data.as_ref()?;
    let decoded = decode_sse_packed_buffer(buffer)?;
    if decoded.bone_weights.is_empty() {
        // Buffer was decoded but VF_SKINNED was clear — nothing to
        // hand back. The caller treats this the same as "no payload"
        // and falls through to the empty-arrays branch.
        return None;
    }
    Some((decoded.bone_weights, decoded.bone_indices))
}

/// Resolve one partition-local u8 bone index against a partition's
/// `bones` palette (a `Vec<u16>` of global skin bone list indices).
/// Returns 0 (root bone) when the local index is out of range — the
/// renderer's bind-pose fallback is the same behaviour the partition
/// splitter would emit for an unused slot.
#[inline]
fn remap_one(local_idx: u8, palette: &[u16]) -> u16 {
    palette.get(local_idx as usize).copied().unwrap_or(0)
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
        let name =
            resolve_node_name(scene, *bone_ref).unwrap_or_else(|| Arc::from(format!("Bone{}", i)));
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
fn resolve_node_name(scene: &NifScene, node_ref: BlockRef) -> Option<Arc<str>> {
    let idx = node_ref.index()?;
    let node = scene.get_as::<NiNode>(idx)?;
    node.av.net.name.clone()
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
fn densify_sparse_weights(num_vertices: usize, data: &NiSkinData) -> (Vec<[u16; 4]>, Vec<[f32; 4]>) {
    // Per-vertex sorted top-4 contributions. Initialized to
    // (u16::MAX, 0.0) so missing slots are obviously invalid until
    // we replace them. Pre-#613 the slot type was `u8` and any
    // NiSkinData with > 256 bones silently dropped every weight
    // past index 255 — same semantic gap as the BsTriShape side
    // that #613 fixes; widening the type covers both paths.
    const VACANT: u16 = u16::MAX;
    let mut per_vertex: Vec<[(u16, f32); 4]> = vec![[(VACANT, 0.0f32); 4]; num_vertices];

    for (bone_idx, bone) in data.bones.iter().enumerate() {
        // NiSkinData carries the global bone list directly — index
        // is a u16 with no partition splitting. Cap at u16::MAX so
        // the sentinel above stays distinguishable.
        let bone_u16 = if bone_idx < VACANT as usize {
            bone_idx as u16
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
                .min_by(|a, b| {
                    a.1 .1
                        .partial_cmp(&b.1 .1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, s)| (i, s.1))
                .unwrap_or((0, 0.0));

            if vw.weight > min_weight {
                slots[min_slot] = (bone_u16, vw.weight);
            }
        }
    }

    let mut vertex_bone_indices = Vec::with_capacity(num_vertices);
    let mut vertex_bone_weights = Vec::with_capacity(num_vertices);

    for slots in &per_vertex {
        let total: f32 = slots
            .iter()
            .filter(|(b, _)| *b != VACANT)
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
        let mut idx = [0u16; 4];
        let mut w = [0.0f32; 4];
        for (i, (b, weight)) in slots.iter().enumerate() {
            if *b != VACANT {
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
#[path = "mesh_skin_tests.rs"]
mod skin_tests;

#[cfg(test)]
#[path = "mesh_bs_tri_shape_shader_flag_tests.rs"]
mod bs_tri_shape_shader_flag_tests;

/// Regression tests for issue #430 — the BsTriShape import path must
/// capture `BSLightingShaderProperty.shader_type_data` payload onto
/// `ImportedMesh.shader_type_fields`. Pre-fix the match collapsed every
/// non-`EnvironmentMap` variant to `1.0` and silently dropped SkinTint /
/// HairTint / EyeEnvmap / ParallaxOcc / MultiLayerParallax / SparkleSnow
/// payloads on Skyrim+ / FO4 / FO76 / Starfield characters.
#[cfg(test)]
#[path = "mesh_shader_type_fields_tests.rs"]
mod shader_type_fields_tests;

/// Regression tests for issue #434 — `find_material_path_bs_tri_shape`
/// must pick up the `.bgem` path from a `BSEffectShaderProperty` bound to
/// the shape, not just from a `BSLightingShaderProperty`. FO4+/FO76/
/// Starfield weapon energy effects, magic surfaces, and steam vents all
/// bind the effect-shader variant with the material pointer in
/// `net.name`.
#[cfg(test)]
#[path = "mesh_material_path_capture_tests.rs"]
mod material_path_capture_tests;

#[cfg(test)]
#[path = "mesh_sse_skin_geometry_reconstruction_tests.rs"]
mod sse_skin_geometry_reconstruction_tests;

#[cfg(test)]
#[path = "mesh_bs_tri_shape_partition_remap_tests.rs"]
mod bs_tri_shape_partition_remap_tests;
