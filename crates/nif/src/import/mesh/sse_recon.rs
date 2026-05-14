//! Skyrim-SE skinned-geometry reconstruction (#559).
//!
//! `ReconstructedSseGeometry` + `DecodedPackedBuffer` — recover vertex /
//! index streams from `SseSkinGlobalBuffer` when the legacy reader couldn't.



use crate::blocks::skin::{
    BsDismemberSkinInstance, NiSkinInstance,
    NiSkinPartition, SseSkinGlobalBuffer,
};
use crate::blocks::tri_shape::BsTriShape;
use crate::scene::NifScene;

use super::*;

/// Reassembled geometry sourced from a `NiSkinPartition` global vertex
/// buffer when the linked `BsTriShape` has empty inline arrays.
/// Positions and normals are already Z-up→Y-up converted; triangles
/// are flat u32 indices into the buffer's vertex space.
pub struct ReconstructedSseGeometry {
    pub(super) positions: Vec<[f32; 3]>,
    pub(super) normals: Vec<[f32; 3]>,
    pub(super) uvs: Vec<[f32; 2]>,
    pub(super) colors: Vec<[f32; 4]>,
    pub(super) indices: Vec<u32>,
    /// Per-vertex tangent (xyz Y-up + bitangent sign). Populated when
    /// the global buffer's `vertex_attrs` carries `VF_TANGENTS`; empty
    /// otherwise. Mirrors `BsTriShape.tangents`'s contract — the on-
    /// disk-named "bitangent" triplet is what we route here as ∂P/∂U
    /// per the existing convention (#795 / SK-D1-04 sibling of SK-D1-03).
    pub(super) tangents: Vec<[f32; 4]>,
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
/// nif.xml `BSVertexDesc` flag: positions are 3 × f32 rather than
/// 3 × f16. SSE-era buffers are unconditionally full-precision
/// (the flag bit may or may not be set on the descriptor; the
/// schema-struct identity guarantees the layout). FO4 (bsver
/// 130+) gates full-precision on `(ARG & 0x401) == 0x401`. The
/// SSE-only packed-buffer decoder relies on the SSE-band invariant
/// — see `decode_sse_packed_buffer`'s "SSE-only contract" docstring
/// and #888. Constant kept for the future FO4-extension branch.
#[allow(dead_code)]
const VF_FULL_PRECISION: u16 = 0x400;

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
pub fn try_reconstruct_sse_geometry(
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
    //
    // #725 / NIF-D4-04 — when a partition-local index falls outside
    // its `vertex_map`'s range, drop the whole triangle rather than
    // alias to the raw cast `local as u16`. The aliased fallback
    // confines damage to malformed content (vanilla Bethesda BSAs
    // always supply complete vertex_maps) but produced collapsed
    // faces on truncated NIFs instead of clean drops. Mirrors the
    // partition-local index policy in
    // `remap_bs_tri_shape_bone_indices`.
    let mut indices = Vec::new();
    let mut dropped_triangles: u32 = 0;
    for part in &partition.partitions {
        for tri in &part.triangles {
            // Resolve all three indices first; commit none of them
            // unless every lookup landed inside vertex_map.
            let mut globals = [0u16; 3];
            let mut ok = true;
            for (i, &local) in tri.iter().enumerate() {
                match part.vertex_map.get(local as usize).copied() {
                    Some(g) => globals[i] = g,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                indices.push(globals[0] as u32);
                indices.push(globals[1] as u32);
                indices.push(globals[2] as u32);
            } else {
                dropped_triangles = dropped_triangles.saturating_add(1);
            }
        }
    }
    if dropped_triangles > 0 {
        log::debug!(
            "BSTriShape SSE reconstruct: dropped {} triangle(s) with \
             out-of-range vertex_map indices (truncated/malformed NIF)",
            dropped_triangles,
        );
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
        tangents: decoded.tangents,
    })
}

pub struct DecodedPackedBuffer {
    pub(super) positions: Vec<[f32; 3]>,
    pub(super) normals: Vec<[f32; 3]>,
    pub(super) uvs: Vec<[f32; 2]>,
    pub(super) colors: Vec<[f32; 4]>,
    /// Per-vertex bone weights when the buffer carries `VF_SKINNED`.
    /// Empty when the flag is clear. 4 weights per vertex, decoded
    /// from packed half-floats. See #638.
    pub(super) bone_weights: Vec<[f32; 4]>,
    /// Per-vertex bone indices when the buffer carries `VF_SKINNED`.
    /// Partition-local — the caller must remap through
    /// `NiSkinPartition.partitions[i].bones` to get global skin
    /// list indices. See #638 / #613.
    pub(super) bone_indices: Vec<[u8; 4]>,
    /// Per-vertex tangent (Y-up xyz + bitangent sign) when the buffer
    /// carries `VF_TANGENTS`. Empty otherwise. The xyz components are
    /// Bethesda's bitangent triplet (∂P/∂U per nifly's `CalcTangentSpace`
    /// swap) reassembled from `bitangent_x` (vec4 trailing slot of
    /// position), `bitangent_y` (after normal), and `bitangent_z`
    /// (after tangent). Sign derived from the on-disk tangent (∂P/∂V)
    /// per `sign(dot(B, cross(N, T)))`. See #796 / SK-D1-04.
    pub(super) tangents: Vec<[f32; 4]>,
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
///
/// **SSE-only contract (#888).** The position read at the head of
/// each vertex hard-codes the 16-byte layout `3 × f32 +
/// (Bitangent X / Unused W)` per nif.xml `BSVertexDataSSE`. This
/// is sound today: `try_reconstruct_sse_geometry` is gated on
/// bsver in `[100, 130)` (Skyrim SE) where `BSVertexDataSSE` is
/// unconditionally f32 by schema-struct identity. Extending the
/// reconstructor to FO4 (bsver 130+) requires either:
/// 1. mirroring the inline parser's `bsver < 130 ||
///    vertex_attrs & VF_FULL_PRECISION` rule and producing a
///    half-precision branch (FO4's `BSVertexData` is conditional
///    on `(ARG & 0x401) == 0x401`); or
/// 2. keeping the upstream `try_reconstruct_sse_geometry` gate
///    locked to the SSE band so this decoder never sees FO4 input.
/// Without either, FO4 meshes that ship without `VF_FULL_PRECISION`
/// (the common case) would silently mis-decode every vertex.
pub fn decode_sse_packed_buffer(buffer: &SseSkinGlobalBuffer) -> Option<DecodedPackedBuffer> {
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
    let has_tangents = vertex_attrs & VF_TANGENTS != 0 && vertex_attrs & VF_NORMALS != 0;
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
    let mut tangents: Vec<[f32; 4]> = if has_tangents {
        Vec::with_capacity(num_vertices)
    } else {
        Vec::new()
    };

    for i in 0..num_vertices {
        let base = i * vertex_size;
        let bytes = &buffer.raw_bytes[base..base + vertex_size];
        let mut off = 0usize;

        // Tangent reassembly state — see the matching block in
        // `tri_shape.rs::BsTriShape::parse`. SSE buffer layout is the
        // same packed format the inline parser walks, so the same
        // three-slot capture (bitangent_x, bitangent_y, tangent_xyz +
        // bitangent_z) applies. #796 / SK-D1-04 (sibling of SK-D1-03).
        // All four `Option`s stay `None` until their respective flag
        // gates fire — the SSE trailing slot (Bitangent X / Unused W)
        // is only `Some` when VF_TANGENTS is set, mirroring the inline
        // parser at `tri_shape.rs::BsTriShape::parse`. See #887.
        let mut bitangent_x: Option<f32> = None;
        let mut bitangent_y: Option<f32> = None;
        let mut tangent_xyz: Option<[f32; 3]> = None;
        let mut bitangent_z: Option<f32> = None;
        let mut normal_zup: Option<[f32; 3]> = None;

        // Position: 3 × f32 + trailing 4-byte slot — 16 bytes total.
        // SSE always uses full-precision per inline-decoder's
        // `bsver < 130 || VF_FULL_PRECISION` rule. Trailing slot per
        // nif.xml `BSVertexDataSSE`: `Bitangent X` (f32) when
        // VF_TANGENTS is set, else `Unused W` (uint, discarded). Same
        // 4 bytes either way so the +16 advance is unconditional.
        let x = read_f32_le(bytes, off)?;
        let y = read_f32_le(bytes, off + 4)?;
        let z = read_f32_le(bytes, off + 8)?;
        // Z-up → Y-up: (x, z, -y).
        positions.push([x, z, -y]);
        if has_tangents {
            bitangent_x = Some(read_f32_le(bytes, off + 12)?);
        }
        off += 16;

        // UV: 2 × f16.
        if vertex_attrs & VF_UVS != 0 {
            let u = half_to_f32(read_u16_le(bytes, off)?);
            let v = half_to_f32(read_u16_le(bytes, off + 2)?);
            uvs.push([u, v]);
            off += 4;
        }

        // Normal: 3 × normbyte + 1 byte bitangent_y normbyte.
        if vertex_attrs & VF_NORMALS != 0 {
            let nx = byte_to_normal(bytes[off]);
            let ny = byte_to_normal(bytes[off + 1]);
            let nz = byte_to_normal(bytes[off + 2]);
            // Z-up → Y-up: (x, z, -y).
            normals.push([nx, nz, -ny]);
            normal_zup = Some([nx, ny, nz]);
            bitangent_y = Some(byte_to_normal(bytes[off + 3]));
            off += 4;
        }

        // Tangent: 3 × normbyte + bitangent_z normbyte. Pre-#796 the
        // whole quad was discarded with `off += 4`; now we capture both
        // halves so the assembler below can stitch the bitangent
        // triplet (∂P/∂U → our tangent slot) and derive the sign from
        // the on-disk tangent triplet (∂P/∂V).
        if has_tangents {
            tangent_xyz = Some([
                byte_to_normal(bytes[off]),
                byte_to_normal(bytes[off + 1]),
                byte_to_normal(bytes[off + 2]),
            ]);
            bitangent_z = Some(byte_to_normal(bytes[off + 3]));
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
            // Renormalize to unit sum — the inline BSTriShape skin
            // path runs the same helper. `triangle.vert` does not
            // divide by `wsum`, so half-float quantization drift
            // (~0.4% on a 4-influence vertex) bleeds straight onto
            // the GPU as per-frame skin jitter. See #889.
            bone_weights.push(crate::blocks::tri_shape::renormalize_skin_weights([
                w0, w1, w2, w3,
            ]));
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

        // Assemble the per-vertex tangent record (Bethesda bitangent
        // triplet → our tangent.xyz; sign from on-disk tangent
        // (∂P/∂V) per `sign(dot(B, cross(N, T)))`). Operates on raw
        // Z-up values and applies the same `(x, y, z) → (x, z, -y)`
        // axis swap as the inline parser's importer-side helper. Sign
        // is rotation-invariant so the swap doesn't flip it. See
        // #796 / SK-D1-04.
        if let (Some(bx), Some(by), Some(bz), Some(t_xyz), Some(n)) = (
            bitangent_x,
            bitangent_y,
            bitangent_z,
            tangent_xyz,
            normal_zup,
        ) {
            let cnx = n[1] * t_xyz[2] - n[2] * t_xyz[1];
            let cny = n[2] * t_xyz[0] - n[0] * t_xyz[2];
            let cnz = n[0] * t_xyz[1] - n[1] * t_xyz[0];
            let dot_b_cross = bx * cnx + by * cny + bz * cnz;
            let sign = if dot_b_cross >= 0.0 { 1.0 } else { -1.0 };
            // Z-up → Y-up on the bitangent triplet (xyz). Sign passes
            // through unchanged.
            tangents.push([bx, bz, -by, sign]);
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
        tangents,
    })
}
