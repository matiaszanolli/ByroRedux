//! Skyrim-SE skinned-geometry reconstruction (#559).
//!
//! `ReconstructedSseGeometry` + `DecodedPackedBuffer` — recover vertex /
//! index streams from `SseSkinGlobalBuffer` when the legacy reader couldn't.

use crate::blocks::skin::{
    BsDismemberSkinInstance, NiSkinInstance, NiSkinPartition, SseSkinGlobalBuffer,
};
use crate::blocks::tri_shape::{check_vertex_desc_offsets, BsTriShape, BsTriShapeKind};
use crate::scene::NifScene;
use crate::types::NiPoint3;

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
    /// Whether `normals` came from the buffer's `VF_NORMALS` lane rather
    /// than the renderer-safe `[0,1,0]` fallback fill at the bottom of
    /// [`decode_sse_packed_buffer_with_external_positions`]. `normals`
    /// itself is unconditionally populated either way, so callers that
    /// need to know whether the data is *real* (e.g. before feeding it to
    /// tangent synthesis) must consult this flag, not `normals.is_empty()`.
    /// See #2817.
    pub(super) normals_authored: bool,
    /// Same contract as [`Self::normals_authored`] for `uvs` / `VF_UVS`.
    pub(super) uvs_authored: bool,
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
// `VF_FULL_PRECISION` (bit 0x400) is intentionally NOT re-declared
// here. The SSE-only packed-buffer decoder doesn't consult it (the
// schema-struct identity guarantees full-precision layout for SSE);
// any future FO4-extension branch should import the canonical const
// from `crates/nif/src/blocks/tri_shape.rs::VF_FULL_PRECISION` (where
// the inline parser keys off it) rather than re-rolling. TD2-204 /
// #1120.

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
    } else {
        let inst = scene.get_as::<BsDismemberSkinInstance>(skin_idx)?;
        inst.base.skin_partition_ref
    };

    let partition_idx = partition_ref.index()?;
    let partition = scene.get_as::<NiSkinPartition>(partition_idx)?;
    let buffer = partition.global_vertex_data.as_ref()?;

    // Decode the global buffer into Y-up positions / normals / UVs /
    // colors, combining BSDynamicTriShape's external position lanes
    // with packed attributes when needed.
    let decoded = decode_sse_shape_buffer(buffer, shape)?;

    // Concatenate partition triangles. On SSE these are ALREADY global
    // indices into the packed buffer — see #3355.
    //
    // This function only runs when `partition.global_vertex_data` is `Some`,
    // which `NiSkinPartition::parse` populates only for `bsver` in
    // `SKYRIM_SE..FALLOUT4`. nifly forces `bMappedIndices = false` for
    // `Stream() == 100` (`Skin.cpp`) and documents the flag as "if true, the
    // vertex indices in triangles and strips are indices into vertexMap, not
    // the shape's vertices" (`Skin.hpp`) — so on SSE the `Triangles` field is
    // nifly's `trueTriangles`, in the shape's own vertex space.
    //
    // Pushing them through `vertex_map` was therefore inverted, and measured
    // on both vanilla SSE mesh archives it cost:
    //   * 3,297,664 of 18,753,141 triangles (17.6%) silently DROPPED, because
    //     a global index >= vertex_map.len() looked malformed under the
    //     #725/NIF-D4-04 policy when it was simply past this partition's own
    //     vertex count;
    //   * 6,681,098 more (35.6%) silently REPOINTED at unrelated vertices.
    // 10,501 of 26,940 skinned shapes (39%) came out mangled — Skyrim's
    // facegen heads and skinned bodies. The 61% that looked fine were the
    // single-partition shapes whose vertex_map is the identity permutation,
    // where the wrong remap is accidentally a no-op.
    //
    // The decisive measurement: all 56,259,423 triangle indices in the corpus
    // are vertex_map *values* (the definition of a global index belonging to
    // that partition), while only 48,042,230 are within vertex_map's
    // *length*. Read as global, zero are out of range.
    //
    // The #725 drop policy is kept, retargeted at the real bound — the
    // decoded buffer's vertex count. The corpus says it never fires here
    // (`raw_oob_global=0`); it remains the guard for a truncated NIF.
    // `remap_bs_tri_shape_bone_indices` keeps its `vertex_map` reads: that
    // one uses the map correctly, as a global -> partition-local inverse.
    let vertex_count = decoded.positions.len();
    let mut indices = Vec::new();
    let mut dropped_triangles: u32 = 0;
    for part in &partition.partitions {
        for tri in &part.triangles {
            // Resolve all three first; commit none unless every index is
            // inside the decoded buffer.
            let mut globals = [0u32; 3];
            let mut ok = true;
            for (i, &global) in tri.iter().enumerate() {
                if (global as usize) < vertex_count {
                    globals[i] = global as u32;
                } else {
                    ok = false;
                    break;
                }
            }
            if ok {
                indices.push(globals[0]);
                indices.push(globals[1]);
                indices.push(globals[2]);
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
        normals_authored: decoded.normals_authored,
        uvs_authored: decoded.uvs_authored,
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
    /// `vertex_attrs & VF_NORMALS != 0` — whether `normals` holds real
    /// decoded data rather than the `[0,1,0]` fallback fill applied
    /// below when the flag is clear. See #2817.
    pub(super) normals_authored: bool,
    /// `vertex_attrs & VF_UVS != 0` — same contract as
    /// [`Self::normals_authored`] for `uvs`.
    pub(super) uvs_authored: bool,
}

/// Decode a `SseSkinGlobalBuffer` into Y-up vertex arrays.
///
/// On Skyrim SE (bsver in `[100, 130)` — the only band where this
/// buffer is captured) positions are always full-precision per the
/// inline parser's `bsver < 130 || VF_FULL_PRECISION`. UVs are 2 ×
/// half-float, normals are 3 × normbyte + 1 byte bitangent_y, colors
/// are 4 × u8. Tangent / skin / eye data slots are skipped per the
/// `vertex_attrs` mask. Returns `None` when the buffer is malformed
/// (size mismatch, vertex_size == 0, or no packed/external positions).
///
/// **SSE-only contract (#888).** A packed position at the head of
/// each vertex uses the 16-byte layout `3 × f32 +
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
///
/// Without either, FO4 meshes that ship without `VF_FULL_PRECISION`
/// (the common case) would silently mis-decode every packed position.
pub fn decode_sse_packed_buffer(buffer: &SseSkinGlobalBuffer) -> Option<DecodedPackedBuffer> {
    decode_sse_packed_buffer_with_external_positions(buffer, None, None)
}

/// Decode an SSE partition buffer using any external position and
/// bitangent-X lanes carried by its owning `BsTriShape`. Skyrim SE
/// `BSDynamicTriShape` clears `VF_VERTEX` in the packed descriptor and
/// stores those lanes on the shape, while skin/normal/UV attributes stay
/// in the partition buffer (#2318, #2576).
pub(super) fn decode_sse_shape_buffer(
    buffer: &SseSkinGlobalBuffer,
    shape: &BsTriShape,
) -> Option<DecodedPackedBuffer> {
    let external_positions = (!shape.vertices.is_empty()).then_some(shape.vertices.as_slice());
    let external_bitangent_x = match &shape.kind {
        BsTriShapeKind::Dynamic { bitangent_x } if !bitangent_x.is_empty() => {
            Some(bitangent_x.as_slice())
        }
        _ => None,
    };
    if external_positions.is_none() && external_bitangent_x.is_none() {
        return decode_sse_packed_buffer(buffer);
    }
    decode_sse_packed_buffer_with_external_positions(
        buffer,
        external_positions,
        external_bitangent_x,
    )
}

/// Decode an SSE partition buffer whose positions may live in a linked
/// `BSDynamicTriShape` Vector4 array rather than in the packed buffer.
/// External positions and W/bitangent-X lanes, when supplied, override the
/// corresponding packed lanes while the packed cursor still consumes any
/// authored `VF_VERTEX` slot.
fn decode_sse_packed_buffer_with_external_positions(
    buffer: &SseSkinGlobalBuffer,
    external_positions: Option<&[NiPoint3]>,
    external_bitangent_x: Option<&[f32]>,
) -> Option<DecodedPackedBuffer> {
    let vertex_size = buffer.vertex_size as usize;
    if vertex_size == 0 || !buffer.raw_bytes.len().is_multiple_of(vertex_size) {
        return None;
    }
    let num_vertices = buffer.raw_bytes.len() / vertex_size;
    let vertex_attrs = ((buffer.vertex_desc >> 44) & 0xFFF) as u16;
    let has_packed_positions = vertex_attrs & VF_VERTEX != 0;
    if !has_packed_positions && external_positions.is_none() {
        return None;
    }
    // #2578 — diagnostic-only; see `check_vertex_desc_offsets` doc comment.
    // This path is SSE-only (pre-FO4), which is always full-precision.
    check_vertex_desc_offsets(
        buffer.vertex_desc,
        vertex_attrs,
        /* full_precision = */ true,
        vertex_attrs & VF_SKINNED != 0,
    );
    if external_positions.is_some_and(|positions| positions.len() != num_vertices)
        || external_bitangent_x.is_some_and(|values| values.len() != num_vertices)
    {
        return None;
    }

    let mut positions = Vec::with_capacity(num_vertices);
    let mut normals = Vec::with_capacity(num_vertices);
    let mut uvs = Vec::with_capacity(num_vertices);
    let mut colors = Vec::with_capacity(num_vertices);
    let is_skinned = vertex_attrs & VF_SKINNED != 0;
    // Two distinct nif.xml `BSVertexDataSSE` predicates, matching the inline
    // `bs_tri_shape.rs` decoder (which gates them separately):
    //   - `Bitangent X` (position trailing slot): `(#ARG# #BITAND# 0x11) == 0x11`
    //     — VF_VERTEX && VF_TANGENTS. Gated by `has_tangents`.
    //   - `Tangent` + `Bitangent Z` quad: `(#ARG# #BITAND# 0x18) == 0x18`
    //     — VF_NORMALS && VF_TANGENTS. Gated by `has_tangent_quad`.
    // #1559 collapsed both onto `has_tangents`, dropping the `&& VF_NORMALS`
    // term for the quad; a VF_TANGENTS-without-VF_NORMALS descriptor then
    // over-read the 4-byte quad the layout never wrote, misaligning the
    // colors / skin / eye reads after it. No shipped Skyrim mesh hits this
    // (tangent space requires a normal, so both gates agree on real content) —
    // the split just keeps the stride exact for malformed/synthetic input.
    let has_tangents = vertex_attrs & VF_TANGENTS != 0;
    let has_tangent_quad = has_tangents && vertex_attrs & VF_NORMALS != 0;
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
    let mut tangents: Vec<[f32; 4]> = if has_tangent_quad {
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

        // Position: 3 × f32 + trailing 4-byte slot when VF_VERTEX is
        // authored. BSDynamicTriShape may instead keep positions and the
        // W/bitangent-X lane in its trailing Vector4 array (#2318).
        let mut packed_position = None;
        if has_packed_positions {
            let x = read_f32_le(bytes, off)?;
            let y = read_f32_le(bytes, off + 4)?;
            let z = read_f32_le(bytes, off + 8)?;
            packed_position = Some([x, y, z]);
            if has_tangents {
                bitangent_x = Some(read_f32_le(bytes, off + 12)?);
            }
            off += 16;
        }
        let position = external_positions
            .map(|values| {
                let value = values[i];
                [value.x, value.y, value.z]
            })
            .or(packed_position)?;
        positions.push(byroredux_core::math::coord::zup_to_yup_pos(position));
        if let Some(values) = external_bitangent_x {
            bitangent_x = Some(values[i]);
        }

        // UV: 2 × f16.
        if vertex_attrs & VF_UVS != 0 {
            let u = half_to_f32(read_u16_le(bytes, off)?);
            let v = half_to_f32(read_u16_le(bytes, off + 2)?);
            uvs.push([u, v]);
            off += 4;
        }

        // Normal: 3 × normbyte + 1 byte bitangent_y normbyte.
        if vertex_attrs & VF_NORMALS != 0 {
            // Bounds-checked: a malformed vertex_desc can declare VF_NORMALS
            // while vertex_size is too small to hold the quad. Fail-soft to
            // None (skip the shape) instead of a raw-index OOB panic (#1547).
            let nx = byte_to_normal(*bytes.get(off)?);
            let ny = byte_to_normal(*bytes.get(off + 1)?);
            let nz = byte_to_normal(*bytes.get(off + 2)?);
            // Z-up → Y-up: (x, z, -y) via the canonical helper (#1753).
            normals.push(byroredux_core::math::coord::zup_to_yup_pos([nx, ny, nz]));
            normal_zup = Some([nx, ny, nz]);
            bitangent_y = Some(byte_to_normal(*bytes.get(off + 3)?));
            off += 4;
        }

        // Tangent: 3 × normbyte + bitangent_z normbyte. Pre-#796 the
        // whole quad was discarded with `off += 4`; now we capture both
        // halves so the assembler below can stitch the bitangent
        // triplet (∂P/∂U → our tangent slot) and derive the sign from
        // the on-disk tangent triplet (∂P/∂V).
        if has_tangent_quad {
            tangent_xyz = Some([
                byte_to_normal(*bytes.get(off)?),
                byte_to_normal(*bytes.get(off + 1)?),
                byte_to_normal(*bytes.get(off + 2)?),
            ]);
            bitangent_z = Some(byte_to_normal(*bytes.get(off + 3)?));
            off += 4;
        }

        // Vertex colors: 4 × u8 → RGBA float. #618 keeps alpha.
        if vertex_attrs & VF_VERTEX_COLORS != 0 {
            let r = *bytes.get(off)? as f32 / 255.0;
            let g = *bytes.get(off + 1)? as f32 / 255.0;
            let b = *bytes.get(off + 2)? as f32 / 255.0;
            let a = *bytes.get(off + 3)? as f32 / 255.0;
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
                *bytes.get(off + 8)?,
                *bytes.get(off + 9)?,
                *bytes.get(off + 10)?,
                *bytes.get(off + 11)?,
            ]);
            off += 12;
        }

        // Eye data: 1 × f32. Discarded — no consumer today.
        if vertex_attrs & VF_EYE_DATA != 0 {
            off += 4;
        }

        // Assemble the per-vertex tangent record (Bethesda bitangent
        // triplet → our tangent.xyz; sign from on-disk tangent
        // (∂P/∂V) per `sign(dot(B, cross(N, T)))`). T is the stored
        // tangent (∂P/∂U = [bx,by,bz]); B is the on-disk tangent
        // (∂P/∂V = t_xyz). Operand order must match
        // `extract_tangents_from_extra_data` — cross(N, ∂P/∂U) dotted
        // with ∂P/∂V — since the triple product is antisymmetric.
        // Operates on raw Z-up values and applies the same
        // `(x, y, z) → (x, z, -y)` axis swap as the inline parser's
        // importer-side helper. Sign is rotation-invariant so the
        // swap doesn't flip it. See #796 / SK-D1-04 and #1516.
        if let (Some(bx), Some(by), Some(bz), Some(t_xyz), Some(n)) = (
            bitangent_x,
            bitangent_y,
            bitangent_z,
            tangent_xyz,
            normal_zup,
        ) {
            let sign = crate::types::bitangent_sign(n, [bx, by, bz], t_xyz);
            // Z-up → Y-up on the bitangent triplet (xyz) via the
            // canonical helper. Sign passes through unchanged (#1753).
            let [tx, ty, tz] = byroredux_core::math::coord::zup_to_yup_pos([bx, by, bz]);
            tangents.push([tx, ty, tz, sign]);
        }

        // Trailing padding (vertex_size - off) bytes — silently absorbed.
        // Defensive guard: bail if we read past the declared stride.
        if off > vertex_size {
            return None;
        }
    }

    // Capture authorship before the fallback fills below make `normals` /
    // `uvs` unconditionally non-empty — #2817 (sibling of #2363): a caller
    // gating tangent synthesis on `!normals.is_empty()` would otherwise
    // vacuously pass on a buffer that never carried `VF_NORMALS`.
    let normals_authored = vertex_attrs & VF_NORMALS != 0;
    let uvs_authored = vertex_attrs & VF_UVS != 0;

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
        normals_authored,
        uvs_authored,
    })
}
