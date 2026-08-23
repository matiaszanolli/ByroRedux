//! Classic `NiTriShape` mesh extraction.
//!
//! `GeomData<'a>` SoA + `extract_mesh` / `extract_mesh_local` + local-bound
//! helper.

use crate::blocks::tri_shape::{NiTriShape, NiTriShapeData, NiTriStripsData};
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3, NiTransform};

use super::super::coord::{zup_matrix_to_yup_quat, zup_point_to_yup};
use super::super::material::{extract_material_info, extract_vertex_colors};
use super::super::ImportedMesh;
use super::*;
use byroredux_core::string::StringPool;

pub struct GeomData<'a> {
    pub vertices: &'a [NiPoint3],
    pub normals: &'a [NiPoint3],
    /// Per-vertex tangents in the renderer's
    /// [`crate::import::ImportedMesh::tangents`] format
    /// (`[Tx, Ty, Tz, bitangent_sign]` — Y-up world space). For
    /// FO3/FNV/Oblivion, decoded from the NIF's
    /// `NiBinaryExtraData("Tangent space (binormal & tangent vectors)")`
    /// blob. Empty when the source mesh has no authored tangents;
    /// the renderer's perturbNormal falls back to screen-space
    /// derivative TBN reconstruction in that case. See #783.
    pub tangents: Vec<[f32; 4]>,
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
pub fn extract_mesh(
    scene: &NifScene,
    shape: &NiTriShape,
    world_transform: &NiTransform,
    inherited_props: &[BlockRef],
    pool: &mut StringPool,
) -> Option<ImportedMesh> {
    let data_idx = shape.data_ref.index()?;

    // Try NiTriShapeData first, then NiTriStripsData. Tangents path
    // (#783 / M-NORMALS):
    //   1. Authored: walk `shape.av.net.extra_data_refs` for a
    //      `NiBinaryExtraData("Tangent space ...")` blob. Most modern
    //      Bethesda content (Skyrim+/FO4) ships this; the SE
    //      Cathedral patch and many Oblivion exterior meshes do too.
    //   2. Synthesized: when no authored blob, run nifly's
    //      `CalcTangentSpace` algorithm at import time to produce
    //      per-vertex tangents from positions + normals + UVs +
    //      triangles. This is what FNV / FO3 / most Oblivion interior
    //      content needs (the original D3D9 runtime computed them at
    //      load time too). Without this fallback the renderer falls
    //      back to screen-space derivative TBN — which produces the
    //      chrome regression on every mesh boundary.
    // `mut` for the `mem::take(&mut geom.tangents)` below — `tangents`
    // is the only owned-Vec field on `GeomData` (the rest are borrowed
    // slices) and the function doesn't read it after the move. #1265.
    let mut geom = if let Some(data) = scene.get_as::<NiTriShapeData>(data_idx) {
        let mut tangents = extract_tangents_from_extra_data(
            scene,
            &shape.av.net.extra_data_refs,
            &data.normals,
            data.vertices.len(),
        );
        if tangents.is_empty() && !data.uv_sets.is_empty() {
            tangents = synthesize_tangents(
                &data.vertices,
                &data.normals,
                &data.uv_sets[0],
                &data.triangles,
            );
        }
        GeomData {
            vertices: &data.vertices,
            normals: &data.normals,
            tangents,
            vertex_colors: &data.vertex_colors,
            uv_sets: &data.uv_sets,
            triangles: std::borrow::Cow::Borrowed(&data.triangles),
            bound_center: data.center,
            bound_radius: data.radius,
        }
    } else {
        let data = scene.get_as::<NiTriStripsData>(data_idx)?;
        let mut tangents = extract_tangents_from_extra_data(
            scene,
            &shape.av.net.extra_data_refs,
            &data.normals,
            data.vertices.len(),
        );
        let triangles_owned = data.to_triangles();
        if tangents.is_empty() && !data.uv_sets.is_empty() {
            tangents = synthesize_tangents(
                &data.vertices,
                &data.normals,
                &data.uv_sets[0],
                &triangles_owned,
            );
        }
        GeomData {
            vertices: &data.vertices,
            normals: &data.normals,
            tangents,
            vertex_colors: &data.vertex_colors,
            uv_sets: &data.uv_sets,
            triangles: std::borrow::Cow::Owned(triangles_owned),
            bound_center: data.center,
            bound_radius: data.radius,
        }
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
    let mat = extract_material_info(scene, shape, inherited_props, pool);

    // Determine vertex colors: prefer per-vertex colors, then material diffuse, then white.
    let colors = extract_vertex_colors(scene, shape, &geom, inherited_props, &mat);

    // Apply Z-up → Y-up to the entity transform.
    let t = &world_transform.translation;
    let r = &world_transform.rotation;

    // Convert the Z-up rotation matrix to Y-up, then extract a robust quaternion.
    let quat = zup_matrix_to_yup_quat(r);

    // Skinning data (issue #151). Populated when the shape has a
    // NiSkinInstance / BSDismemberSkinInstance backing it.
    let skin = extract_skin_ni_tri_shape(scene, shape, positions.len(), &indices);

    // Local bounding sphere in Y-up renderer space. Prefer the NIF-provided
    // NiBound on NiGeometryData; fall back to a fresh centroid+max-distance
    // sphere computed from the positions when the NIF omits one (radius 0).
    // See #217.
    let (local_bound_center, local_bound_radius) =
        extract_local_bound(geom.bound_center, geom.bound_radius, &positions);

    let material = mat.into_imported_material(pool, shape.av.net.name.as_deref());

    // #783 / M-NORMALS — pre-decoded tangents from the NIF's
    // `NiBinaryExtraData("Tangent space (binormal & tangent vectors)")`.
    // Empty when the source mesh has no authored tangents; the
    // renderer falls back to screen-space derivative TBN in that case.
    //
    // #1265 / NIF-D5-NEW-05 — `mem::take` instead of `.clone()`. `geom`
    // is the import-local one-shot `GeomData` (NOT the scene-retained
    // NiTriShapeData) and `tangents` is the only owned-Vec field on it.
    // After this move the function constructs ImportedMesh and returns;
    // `geom.tangents` is never read again. Saves a per-vertex `Vec<[f32; 3]>`
    // memcpy on every FNV/FO3/Oblivion NiTriShape that ships authored
    // tangents (~16-40 KB per mesh).
    let tangents_yup = std::mem::take(&mut geom.tangents);

    // #3231 — morph-target vertex deltas, when the shape carries a
    // NiGeomMorpherController. Must run after `positions` is final (the
    // vertex-count guard checks against it), before `positions` moves
    // into the struct literal below.
    let morph_targets = extract_morph_targets(
        scene,
        shape.av.net.controller_ref,
        positions.len(),
        shape.av.net.name.as_deref(),
    );

    Some(ImportedMesh {
        positions,
        colors,
        normals,
        tangents: tangents_yup,
        uvs,
        indices,
        translation: zup_point_to_yup(t),
        rotation: quat,
        scale: world_transform.scale,
        material,
        name: shape.av.net.name.clone(),
        parent_node: None,
        skin,
        local_bound_center,
        local_bound_radius,
        flags: shape.av.flags,
        bs_lod_cutoffs: None,
        bs_sub_index: None,
        bs_geometry_lod_slot: None,
        billboard_mode: None,
        morph_targets,
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
pub fn extract_local_bound(
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
pub fn extract_mesh_local(
    scene: &NifScene,
    shape: &NiTriShape,
    inherited_props: &[BlockRef],
    pool: &mut StringPool,
) -> Option<ImportedMesh> {
    extract_mesh(scene, shape, &shape.av.transform, inherited_props, pool)
}
