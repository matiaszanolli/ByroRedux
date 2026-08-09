//! Skinning data extraction (issue #151).
//!
//! `extract_skin_ni_tri_shape` / `extract_skin_bs_tri_shape`, partition /
//! palette remap, bone-pose flattening, sparse-weight densification.

use std::sync::Arc;

use crate::blocks::bs_geometry::{BSGeometry, BSGeometryMeshData, BoneWeight};
use crate::blocks::node::NiNode;
use crate::blocks::skin::{
    BsDismemberSkinInstance, BsSkinBoneData, BsSkinInstance, NiSkinData, NiSkinInstance,
};
use crate::blocks::tri_shape::{BsTriShape, NiTriShape};
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3, NiTransform};

use super::super::{ImportedBone, ImportedSkin};

pub fn extract_skin_ni_tri_shape(
    scene: &NifScene,
    shape: &NiTriShape,
    num_vertices: usize,
) -> Option<ImportedSkin> {
    let skin_idx = shape.skin_instance_ref.index()?;

    // Accept either NiSkinInstance or BSDismemberSkinInstance (the
    // Bethesda extension with body-part flags). #1659 — capture the
    // dismemberment partitions (empty for plain NiSkinInstance) so
    // ImportedSkin::body_part_flags isn't silently dropped.
    let (bone_refs, skeleton_root_ref, data_ref, body_part_flags) =
        if let Some(inst) = scene.get_as::<NiSkinInstance>(skin_idx) {
            (
                inst.bone_refs.as_slice(),
                inst.skeleton_root_ref,
                inst.data_ref,
                Vec::new(),
            )
        } else {
            let inst = scene.get_as::<BsDismemberSkinInstance>(skin_idx)?;
            (
                inst.base.bone_refs.as_slice(),
                inst.base.skeleton_root_ref,
                inst.base.data_ref,
                inst.partitions.clone(),
            )
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

    // M41.0 Phase 1b.x — surface NiSkinData::skinTransform (the global
    // per-skin transform). Bethesda body NIFs ship this with a non-
    // identity cyclic-permutation rotation that the runtime palette
    // composes as the outermost factor. See OpenMW
    // `riggeometry.cpp:175-208`.
    let global_skin_transform = ni_transform_to_yup_matrix(&data.skin_transform);

    Some(ImportedSkin {
        bones,
        skeleton_root,
        vertex_bone_indices,
        vertex_bone_weights,
        global_skin_transform,
        body_part_flags,
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
pub fn extract_skin_bs_tri_shape(scene: &NifScene, shape: &BsTriShape) -> Option<ImportedSkin> {
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
    // #1659 — capture BSDismemberSkinInstance's partitions alongside.
    let (bone_refs_slice, skeleton_root_ref, data_ref, body_part_flags) =
        if let Some(inst) = scene.get_as::<NiSkinInstance>(skin_idx) {
            (
                inst.bone_refs.as_slice(),
                inst.skeleton_root_ref,
                inst.data_ref,
                Vec::new(),
            )
        } else if let Some(inst) = scene.get_as::<BsDismemberSkinInstance>(skin_idx) {
            (
                inst.base.bone_refs.as_slice(),
                inst.base.skeleton_root_ref,
                inst.base.data_ref,
                inst.partitions.clone(),
            )
        } else {
            (&[] as &[_], BlockRef::NULL, BlockRef::NULL, Vec::new())
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
        let global_skin_transform = ni_transform_to_yup_matrix(&data.skin_transform);
        return Some(ImportedSkin {
            bones,
            skeleton_root,
            vertex_bone_indices,
            vertex_bone_weights,
            global_skin_transform,
            body_part_flags,
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
        // BSSkin (FO4+/Skyrim SE) doesn't carry a per-skin global
        // transform; identity is the right default per OpenMW's
        // FO4-mesh fallback comment at NifFile.cpp:2600-2602
        // ("FO4 meshes do not have this transform").
        return Some(ImportedSkin {
            bones,
            skeleton_root,
            vertex_bone_indices,
            vertex_bone_weights,
            global_skin_transform: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            // BSSkin::Instance (FO4+/Skyrim SE) carries no dismemberment
            // partitions of its own — those live on NiSkinInstance /
            // BsDismemberSkinInstance, handled above. See #1659.
            body_part_flags: Vec::new(),
        });
    }

    None
}

/// Convert Starfield `BSGeometry`'s per-vertex `skin_weights` (already
/// grouped by vertex — outer index is the vertex, inner list holds up to
/// `weights_per_vert` [`BoneWeight`] entries) into the engine's fixed
/// `[u16; 4]` bone-index / `[f32; 4]` weight per-vertex arrays. Mirrors
/// `extract_skin_bs_tri_shape`'s FO4 `BsTriShape` contract: keeps the top
/// 4 contributions by weight (zero-padding when fewer), decodes the u16
/// NORM weight (`/ 65535.0`), and renormalizes via
/// [`crate::blocks::tri_shape::renormalize_skin_weights`] so NORM
/// quantization drift doesn't accumulate on the GPU skinning path.
///
/// Returns empty vecs — the engine's existing "no per-vertex skin data"
/// sentinel (see `nif_loader.rs`'s `vertex_bone_indices.is_empty()`
/// rigid-fallback gate) — when `skin_weights.len()` doesn't match
/// `num_vertices`: a mismatch means the mesh-data table and the geometry
/// it's paired with disagree on vertex count, and guessing which vertex
/// maps to which weight row would silently mis-skin the mesh rather than
/// fail safe. See #2613.
fn convert_bs_geometry_skin_weights(
    mesh_data: &BSGeometryMeshData,
    num_vertices: usize,
) -> (Vec<[u16; 4]>, Vec<[f32; 4]>) {
    if mesh_data.weights_per_vert == 0
        || mesh_data.skin_weights.is_empty()
        || mesh_data.skin_weights.len() != num_vertices
    {
        return (Vec::new(), Vec::new());
    }

    let mut indices = Vec::with_capacity(num_vertices);
    let mut weights = Vec::with_capacity(num_vertices);
    for vertex_weights in &mesh_data.skin_weights {
        let mut sorted: Vec<&BoneWeight> = vertex_weights.iter().collect();
        // Descending by raw u16 weight — exact integer order, no NaN
        // concerns from comparing the decoded f32.
        sorted.sort_by(|a, b| b.weight.cmp(&a.weight));

        let mut idx = [0u16; 4];
        let mut w = [0.0f32; 4];
        for (slot, bw) in sorted.into_iter().take(4).enumerate() {
            idx[slot] = bw.bone_index;
            w[slot] = bw.weight as f32 / 65535.0;
        }
        weights.push(crate::blocks::tri_shape::renormalize_skin_weights(w));
        indices.push(idx);
    }
    (indices, weights)
}

/// Extract `ImportedSkin` for a Starfield `BSGeometry` via `skin_instance_ref`.
/// Resolves the linked `BSSkin::Instance` + `BSSkin::BoneData` and builds the
/// engine-side bone list with bind-inverse transforms, then decodes
/// `mesh_data.skin_weights` into per-vertex bone indices/weights via
/// [`convert_bs_geometry_skin_weights`]. See #2613 — pre-fix these were
/// hardcoded empty on the stale premise that the BSGeometry parser didn't
/// surface per-vertex skin data yet; `skin_weights` has been fully decoded
/// since #873, just never plumbed through here (#1827 closed on that same
/// stale premise).
///
/// Mirrors the FO4+ BSSkin path in `extract_skin_bs_tri_shape` — only
/// the geometry container differs.
pub fn extract_skin_bs_geometry(
    scene: &NifScene,
    shape: &BSGeometry,
    mesh_data: &BSGeometryMeshData,
) -> Option<ImportedSkin> {
    let skin_idx = shape.skin_instance_ref.index()?;

    let inst = scene.get_as::<BsSkinInstance>(skin_idx)?;
    let bone_data = scene.get_as::<BsSkinBoneData>(inst.bone_data_ref.index()?)?;
    if bone_data.bones.len() != inst.bone_refs.len() {
        log::debug!(
            "BSGeometry skin: BsSkinBoneData bone count ({}) != BsSkinInstance bone_refs count ({})",
            bone_data.bones.len(),
            inst.bone_refs.len(),
        );
        return None;
    }
    let mut bones = Vec::with_capacity(inst.bone_refs.len());
    for (i, bone_ref) in inst.bone_refs.iter().enumerate() {
        let name =
            resolve_node_name(scene, *bone_ref).unwrap_or_else(|| Arc::from(format!("Bone{}", i)));
        let bt = &bone_data.bones[i];
        bones.push(ImportedBone {
            name,
            bind_inverse: bs_bone_to_inverse_matrix(bt),
            bounding_sphere: bt.bounding_sphere,
        });
    }
    let skeleton_root = resolve_node_name(scene, inst.skeleton_root_ref);
    let (vertex_bone_indices, vertex_bone_weights) =
        convert_bs_geometry_skin_weights(mesh_data, mesh_data.vertices.len());
    // BSSkin (FO4+/Skyrim SE/Starfield) doesn't carry a per-skin global
    // transform; identity matches the OpenMW FO4-mesh fallback.
    Some(ImportedSkin {
        bones,
        skeleton_root,
        vertex_bone_indices,
        vertex_bone_weights,
        global_skin_transform: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        // BSGeometry's BsSkinInstance carries no dismemberment
        // partitions — see #1659.
        body_part_flags: Vec::new(),
    })
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
/// raw u8 to u16. Even a single partition must still consult its
/// palette: vanilla SSE FaceGen meshes can use a non-identity subset
/// of the global bone list (#2577).
pub fn remap_bs_tri_shape_bone_indices(
    scene: &NifScene,
    shape: &BsTriShape,
    bone_indices: &[[u8; 4]],
) -> Vec<[u16; 4]> {
    if bone_indices.is_empty() {
        return Vec::new();
    }

    // Identity widen — the safe fallback used when no partition
    // table is available or a vertex is absent from every map.
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
            let part = vertex_to_partition[v].and_then(|p| partition.partitions.get(p as usize));
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
/// Decoded SSE skin payload: per-vertex `(weights, partition-local bone
/// indices)`, returned by [`decode_sse_skin_payload`].
pub type SseSkinPayload = (Vec<[f32; 4]>, Vec<[u8; 4]>);

pub fn decode_sse_skin_payload(scene: &NifScene, shape: &BsTriShape) -> Option<SseSkinPayload> {
    let skin_idx = shape.skin_ref.index()?;
    let partition_ref = if let Some(inst) = scene.get_as::<NiSkinInstance>(skin_idx) {
        inst.skin_partition_ref
    } else {
        let inst = scene.get_as::<BsDismemberSkinInstance>(skin_idx)?;
        inst.base.skin_partition_ref
    };
    let partition_idx = partition_ref.index()?;
    let partition = scene.get_as::<crate::blocks::skin::NiSkinPartition>(partition_idx)?;
    let buffer = partition.global_vertex_data.as_ref()?;
    // BSDynamicTriShape keeps positions outside the partition buffer,
    // so its descriptor legitimately has VF_VERTEX clear even though
    // the packed skin lanes are present (#2576). Use the same
    // shape-aware decoder as geometry reconstruction.
    let decoded = super::sse_recon::decode_sse_shape_buffer(buffer, shape)?;
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
pub fn remap_one(local_idx: u8, palette: &[u16]) -> u16 {
    palette.get(local_idx as usize).copied().unwrap_or(0)
}

/// Build `ImportedBone`s from a NiSkinInstance bone list and NiSkinData
/// bone entries. The two inputs must have matching lengths (checked by
/// the caller). Applies Z-up → Y-up conversion to each bind transform.
pub fn build_imported_bones(
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
pub fn resolve_node_name(scene: &NifScene, node_ref: BlockRef) -> Option<Arc<str>> {
    let idx = node_ref.index()?;
    let node = scene.get_as::<NiNode>(idx)?;
    node.av.net.name.clone()
}

/// Convert a NiTransform to a column-major 4x4 matrix with the Y-up
/// basis change applied. NiSkinData stores the bind-inverse already —
/// we just need to reorder rows/columns for glam's column-major layout
/// and convert Gamebryo Z-up to engine Y-up (90° rotation around X).
pub fn ni_transform_to_yup_matrix(t: &NiTransform) -> [[f32; 4]; 4] {
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
    // C * t — translation swap through the coord SoT (#1617, bit-identical).
    // The `cr`/`rr` matrix basis-change above is the rotation-similarity
    // analog (C·R·Cᵀ), a deliberate parallel — not the position swap.
    let tt = byroredux_core::math::coord::zup_to_yup_pos([tx, ty, tz]);

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
pub fn bs_bone_to_inverse_matrix(b: &crate::blocks::skin::BsSkinBoneTrans) -> [[f32; 4]; 4] {
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
pub fn densify_sparse_weights(
    num_vertices: usize,
    data: &NiSkinData,
) -> (Vec<[u16; 4]>, Vec<[f32; 4]>) {
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
