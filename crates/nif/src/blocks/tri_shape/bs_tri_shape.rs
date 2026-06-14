//! BsTriShape and its wire-distinct subclasses (LOD / MeshLOD / SubIndex / Dynamic) +
//! the BSSubIndexTriShape segmentation payload.
//!
//! Skyrim SE+ packed-vertex geometry. Unlike NiTriShape which references separate
//! `NiTriShapeData`, BSTriShape packs positions / UVs / normals / tangents / colors
//! directly into the block using a `vertex_desc` bitfield. The five wire-distinct
//! subclasses share this Rust struct, disambiguated by the [`BsTriShapeKind`]
//! discriminator (#560 / #404).
//!
//! Split out of the prior monolithic `blocks/tri_shape.rs` (TD9-005 / #1118).

use super::super::base::NiAVObjectData;
use super::super::{traits, NiObject};
use super::{half_to_f32, renormalize_skin_weights};
use crate::stream::NifStream;
use crate::types::{BlockRef, NiPoint3, NiTransform};
use std::any::Any;
use std::io;

// Re-export `NiTriShape` into this module's scope so the `#[path]`-mounted
// `tri_shape_skin_vertex_tests.rs` (which calls `use super::*;`) can dispatch
// the BSSegmentedTriShape downcast assertion. Pre-split the test inherited
// `NiTriShape` from `tri_shape.rs`'s module head.
#[cfg(test)]
use super::NiTriShape;

/// Discriminator for the five wire-distinct types that share the
/// [`BsTriShape`] Rust struct. Pre-#560 every variant reported
/// `"BSTriShape"` and downstream consumers (facegen head detection,
/// distant-LOD import, dismember segmentation) couldn't tell them apart.
///
/// Mirrors the [`super::node::BsRangeKind`] pattern.
///
/// Was `#[derive(Copy, Eq)]` pre-#404. `SubIndex(Box<...>)` makes the
/// enum heap-owning so `Copy` is gone, and the boxed payload contains
/// `Vec<f32>` cut-offsets (no `Eq` for `f32`). Downstream consumers
/// only ever clone or `PartialEq`-compare the discriminant, so the
/// loss is non-load-bearing.
#[derive(Debug, Clone, PartialEq)]
pub enum BsTriShapeKind {
    /// Plain `BSTriShape` — the baseline Skyrim SE+ geometry block.
    Plain,
    /// `BSLODTriShape` — FO4 distant-LOD variant. Trailing three u32
    /// triangle-count cutoffs drive which LOD level is selected at
    /// render time for distant terrain / buildings.
    LOD { lod0: u32, lod1: u32, lod2: u32 },
    /// `BSMeshLODTriShape` — Skyrim SE DLC LOD variant. Same wire
    /// format as `BSLODTriShape` (three trailing u32s), but the engine
    /// doesn't consult them — the LOD selection is driven elsewhere.
    /// Preserved as a distinct `kind` so importers can differentiate.
    MeshLOD,
    /// `BSSubIndexTriShape` — ubiquitous in Skyrim SE DLC / FO4 actor
    /// meshes for dismemberment segmentation. Carries the parsed
    /// segmentation payload (segment table + optional shared sub-segment
    /// data with SSF filename). Boxed because `BsSubIndexTriShapeData`
    /// is large and only one variant carries it. See #404.
    SubIndex(Box<BsSubIndexTriShapeData>),
    /// `BSDynamicTriShape` — Skyrim facegen head meshes. The trailing
    /// `Vector4` array carries the CPU-morph-updated vertex positions
    /// that overwrite the base `vertices`. See #341.
    Dynamic,
}

/// BSTriShape — Skyrim SE+ geometry with embedded vertex data.
///
/// Unlike NiTriShape which references separate NiTriShapeData, BSTriShape
/// packs vertex positions, UVs, normals, tangents, and colors directly
/// into the block using a vertex descriptor bitfield.
///
/// Skyrim SE uses BSVertexDataSSE format (full-precision f32 positions).
/// FO4+ uses BSVertexData (half-float positions by default).
#[derive(Debug)]
pub struct BsTriShape {
    /// NiObjectNET + NiAVObject base (no properties list).
    pub av: NiAVObjectData,
    pub center: NiPoint3,
    pub radius: f32,
    pub skin_ref: BlockRef,
    pub shader_property_ref: BlockRef,
    pub alpha_property_ref: BlockRef,
    pub vertex_desc: u64,
    pub num_triangles: u32,
    pub num_vertices: u16,
    pub vertices: Vec<NiPoint3>,
    pub uvs: Vec<[f32; 2]>,
    pub normals: Vec<NiPoint3>,
    pub vertex_colors: Vec<[f32; 4]>,
    pub triangles: Vec<[u16; 3]>,
    /// Per-vertex bone weights (4 per vertex, half-float decoded to f32).
    /// Empty when the vertex descriptor lacks VF_SKINNED.
    pub bone_weights: Vec<[f32; 4]>,
    /// Per-vertex bone indices (4 per vertex). Parallel to `bone_weights`.
    /// Empty when the vertex descriptor lacks VF_SKINNED.
    pub bone_indices: Vec<[u8; 4]>,
    /// Per-vertex tangent (xyz raw Z-up, w bitangent sign). Populated only
    /// when `VF_TANGENTS | VF_NORMALS` are set on the vertex descriptor;
    /// empty otherwise. The xyz components hold **Bethesda's bitangent
    /// triplet** (bitangent_x at end of position vec4, bitangent_y after
    /// the normal triplet, bitangent_z after the tangent triplet) — per
    /// nifly's `CalcTangentSpace` (`Geometry.cpp:1014-1034`) the
    /// on-disk-named "bitangent" actually stores ∂P/∂U which is what the
    /// fragment shader's `vertexTangent.xyz` contract wants. The on-disk
    /// "tangent" stores ∂P/∂V and is used here only to derive the
    /// bitangent sign (`sign(dot(B, cross(N, T)))`); axis swap to Y-up
    /// happens at import time (`extract_bs_tri_shape`) on .xyz only —
    /// the sign is invariant under proper rotation. See #795 / SK-D1-03.
    pub tangents: Vec<[f32; 4]>,
    /// Wire-type discriminator. Set by each parser arm; the dispatcher
    /// in [`super::mod`] uses [`Self::with_kind`] to override for types
    /// that share a parser (BSMeshLODTriShape / BSSubIndexTriShape). #560.
    pub kind: BsTriShapeKind,
    /// `Data Size` from the BSTriShape body (vertex pool bytes + triangle
    /// pool bytes). Stored verbatim for downstream consumers — primarily
    /// `BSSubIndexTriShape` which gates its segmentation block on this
    /// value being non-zero (see nif.xml `cond="Data Size #GT# 0"`).
    /// #359 derives the expected value as a sanity check;
    /// #404 needs the stored value to gate the segmentation parse path.
    pub data_size: u32,
}

impl NiObject for BsTriShape {
    fn block_type_name(&self) -> &'static str {
        // Static-string contract on the trait — dispatch on the wire
        // discriminator so downstream `block_type_name()` callers see
        // the original subclass. Consumers that need the LOD cutoffs
        // should match on `self.kind` instead.
        match self.kind {
            BsTriShapeKind::Plain => "BSTriShape",
            BsTriShapeKind::LOD { .. } => "BSLODTriShape",
            BsTriShapeKind::MeshLOD => "BSMeshLODTriShape",
            BsTriShapeKind::SubIndex(_) => "BSSubIndexTriShape",
            BsTriShapeKind::Dynamic => "BSDynamicTriShape",
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn traits::HasObjectNET> {
        Some(self)
    }
    fn as_av_object(&self) -> Option<&dyn traits::HasAVObject> {
        Some(self)
    }
    fn as_shader_refs(&self) -> Option<&dyn traits::HasShaderRefs> {
        Some(self)
    }
}

impl traits::HasObjectNET for BsTriShape {
    fn name(&self) -> Option<&str> {
        self.av.net.name.as_deref()
    }
    fn name_arc(&self) -> Option<&std::sync::Arc<str>> {
        self.av.net.name.as_ref()
    }
    fn extra_data_refs(&self) -> &[BlockRef] {
        &self.av.net.extra_data_refs
    }
    fn controller_ref(&self) -> BlockRef {
        self.av.net.controller_ref
    }
}

impl traits::HasAVObject for BsTriShape {
    fn flags(&self) -> u32 {
        self.av.flags
    }
    fn transform(&self) -> &NiTransform {
        &self.av.transform
    }
    fn properties(&self) -> &[BlockRef] {
        &[]
    } // BSTriShape never has properties
    fn collision_ref(&self) -> BlockRef {
        self.av.collision_ref
    }
}

impl traits::HasShaderRefs for BsTriShape {
    fn shader_property_ref(&self) -> BlockRef {
        self.shader_property_ref
    }
    fn alpha_property_ref(&self) -> BlockRef {
        self.alpha_property_ref
    }
}

/// Vertex attribute flags from BSVertexDesc bits [44:55].
/// See nif.xml `VertexAttribute` (lines 2077-2090). Every bit in the
/// 12-bit attribute field has a constant here, whether or not the
/// per-vertex parse loop below decodes it — keeps the schema/code
/// mapping auditable and lets the trailing `consumed < vertex_size_bytes`
/// skip absorb bytes for any bit whose decoder is deferred.
const VF_VERTEX: u16 = 0x001;
const VF_UVS: u16 = 0x002;
/// Bit 2 — second UV set. Authored by meshes that carry two UV
/// channels (detail maps, lightmaps). The BSVertexDesc `UV2 Offset`
/// nibble (bits 12..16) tells the runtime where inside each vertex
/// the second UV starts. Neither nif.xml's `BSVertexData` struct
/// (line 2107) nor nifly's authoritative C++ parser lists a named
/// field for this — community decoders typically treat the extra
/// 4 bytes as opaque and rely on the UV2 offset at sample time.
/// The sequential parser here does the same: the trailing skip at
/// the end of the per-vertex loop absorbs the 4 bytes (2 × f16)
/// reserved by this flag, so no downstream corruption.
///
/// Field-level extraction is deferred until a consumer (detail-map
/// shader, lightmap renderer) materializes to validate the wire
/// layout against real geometry — per the no-guessing policy on
/// schema-ambiguous fields. See audit N2-01 / #336.
#[allow(dead_code)]
const VF_UVS_2: u16 = 0x004;
const VF_NORMALS: u16 = 0x008;
const VF_TANGENTS: u16 = 0x010;
const VF_VERTEX_COLORS: u16 = 0x020;
const VF_SKINNED: u16 = 0x040;
/// Bit 7 — landscape per-vertex blend data (FO4+ terrain `BSTriShape`
/// meshes). `Landscape Data Offset` in BSVertexDesc (bits 32..36)
/// locates the field inside each vertex. Same story as `VF_UVS_2`:
/// nif.xml's `BSVertexData` struct does not enumerate a landscape
/// field, so community tooling treats the bytes as opaque.
/// Consumer-side decoding lives with the FO4 terrain wiring (ROADMAP
/// M40 worldspace streaming / FO4 cell loader); until then the
/// trailing skip at the end of the per-vertex loop absorbs the
/// reserved bytes and no parse corruption occurs. See audit N2-01 /
/// #336.
#[allow(dead_code)]
const VF_LAND_DATA: u16 = 0x080;
const VF_EYE_DATA: u16 = 0x100;
/// GPU-instancing flag (bit 9). Defined for completeness with the
/// nif.xml schema; no shipped FO4 / FO76 / Starfield content sets this
/// today, but modded geometry can. The trailing skip at the end of the
/// per-vertex parse loop already absorbs whatever bytes the bit asks
/// for via `vertex_size_quads * 4`, so flagging the constant doesn't
/// change runtime behavior — it just closes a defense-in-depth gap
/// flagged by audit S1-02 and makes the constant set match the schema
/// for code-review clarity. Field-level extraction (when the bit
/// becomes load-bearing) is tracked alongside VF_UVS_2 / VF_LAND_DATA
/// under #336. See #358.
#[allow(dead_code)]
const VF_INSTANCE: u16 = 0x200;
/// FO4+: full-precision vertex positions (bit 10). When clear, positions are half-float.
const VF_FULL_PRECISION: u16 = 0x400;

impl BsTriShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let av = NiAVObjectData::parse_no_properties(stream)?;

        // BSTriShape-specific: bounding sphere
        let center = stream.read_ni_point3()?;
        let radius = stream.read_f32_le()?;

        // FO76 only (`BS_F76` = BSVER == 155): 6 × f32 `Bound Min Max`
        // AABB between the bounding sphere and the skin ref. Pre-#342
        // the parser jumped straight from `radius` to `skin_ref`,
        // eating the first 4 bytes of the AABB into the skin_ref
        // field and cascading 24 bytes of slip through every
        // subsequent field (shader_ref, alpha_ref, vertex_desc). The
        // per-block `block_size` realignment swallowed the slip
        // silently — parse rate stayed at 100% while block contents
        // were wrong. Starfield (BSVER 172) is NOT affected because
        // `BS_F76` is strict equality. See nif.xml:8231.
        if stream.bsver() == 155 {
            stream.skip(24)?;
        }

        // Refs
        let skin_ref = stream.read_block_ref()?;
        let shader_property_ref = stream.read_block_ref()?;
        let alpha_property_ref = stream.read_block_ref()?;

        // Vertex descriptor bitfield
        let vertex_desc = stream.read_u64_le()?;
        let vertex_attrs = ((vertex_desc >> 44) & 0xFFF) as u16;
        let vertex_size_quads = (vertex_desc & 0xF) as usize; // size in units of 4 bytes

        // Triangle and vertex counts
        let num_triangles = if stream.bsver() >= crate::version::bsver::FALLOUT4 {
            stream.read_u32_le()?
        } else {
            stream.read_u16_le()? as u32
        };
        let num_vertices = stream.read_u16_le()?;
        let data_size = stream.read_u32_le()?;

        // #359 — Defense-in-depth structural assertion: nif.xml's
        // `Data Size` is derived from `(vertex_size_quads * 4) *
        // num_vertices + num_triangles * 6` (line 8239). A mismatch
        // proves that one of `vertex_desc`, `num_vertices`, or
        // `num_triangles` was misparsed upstream — exactly the kind
        // of cheap check that would have caught audit findings S1-01
        // (FO76 Bound Min Max slip) and S5-01 (BSDynamicTriShape
        // mis-aligned by 4 bytes) before manual inspection.
        //
        // Don't hard-fail — some shipped FO4 content uses non-standard
        // padding and we don't want to break parse-rate on those. Log
        // at WARN so the regression is visible in `nif_stats` runs.
        // Skip the warning when `data_size == 0`, since #341's
        // BSDynamicTriShape facegen path legitimately ships a zero
        // here (real positions live in the trailing dynamic Vector4
        // array) — flagging that case would mean 21k false positives
        // per Skyrim - Meshes0.bsa scan.
        //
        // #621 / SK-D1-05: when the assertion fails AND
        // (data_size − num_triangles*6) is a clean multiple of
        // num_vertices, prefer the data_size-derived stride for the
        // per-vertex loop. data_size is the on-disk authority — pre-fix
        // the parser logged the mismatch but plowed ahead with the
        // suspect `vertex_size_quads * 4` stride, silently misaligning
        // every vertex past the first. block_size recovery hid the
        // slip from the parse-rate metric. The non-standard FO4 padding
        // mentioned above is exactly the case where data_size > expected
        // — and routing through the derived stride aligns the loop
        // correctly across all such content.
        let mut vertex_size_bytes = vertex_size_quads * 4;
        // #836 / SK-D5-NEW-02: gate the warning on `num_vertices != 0`
        // too. SSE skinned bodies legitimately ship `num_vertices == 0`
        // here because the packed vertex buffer lives on a sister
        // `NiSkinPartition` (consumed via `try_reconstruct_sse_geometry`,
        // fix #559). The `data_size` field still carries the persisted
        // size of the data on the sister block, so without this gate
        // every skinned body in Skyrim Meshes0/1 (~67 / Whiterun cell
        // load) fired a false-positive "irrational" warning. The
        // per-vertex loop below is already bounded by `num_vertices`,
        // so the parse output is identical with or without the
        // warning fire — only the log noise changes.
        if data_size != 0 && num_vertices != 0 {
            let expected_data_size =
                (vertex_size_bytes * num_vertices as usize) + (num_triangles as usize * 6);
            if (data_size as usize) != expected_data_size {
                let derived_stride = if num_vertices > 0 {
                    let payload = (data_size as usize).saturating_sub(num_triangles as usize * 6);
                    if payload.is_multiple_of(num_vertices as usize) {
                        Some(payload / num_vertices as usize)
                    } else {
                        None
                    }
                } else {
                    None
                };
                log::warn!(
                    "BSTriShape data_size mismatch: stored {} vs derived {} \
                     (vertex_size_quads={}, num_vertices={}, num_triangles={}) — \
                     trusting data_size-derived stride{}",
                    data_size,
                    expected_data_size,
                    vertex_size_quads,
                    num_vertices,
                    num_triangles,
                    match derived_stride {
                        Some(s) => format!(" ({s} bytes/vertex)"),
                        None => " (irrational; falling back to descriptor stride)".into(),
                    },
                );
                if let Some(s) = derived_stride {
                    vertex_size_bytes = s;
                }
            }
        }

        let is_skinned = vertex_attrs & VF_SKINNED != 0;
        // #1216 / D2 FIND-2 — surface "no inline geometry + not a known
        // sister-block carrier" cases at debug level. The parser can't
        // see the walker-time context (was this shape under a
        // BSPackedCombinedGeomDataExtra-bearing NiNode? a sister
        // NiSkinPartition?) so the log records the AMBIGUOUS case and
        // leaves disambiguation to whoever's reading the logs:
        //  * FO4 precombined Shared variant (CSG-deferred, #1188) — expected.
        //  * SSE skinned body (data on `NiSkinPartition`, #559) —
        //    expected, filtered by `is_skinned`.
        //  * Genuinely malformed shape — needs investigation.
        // Surfaced via `RUST_LOG=byroredux_nif=debug`; `nif_stats` can
        // grep the resulting log for per-archive counts. See audit
        // memory: not raised to warn because vanilla FO4 ships 124,871
        // legitimate zero-vertex shapes that would flood the default log.
        if num_vertices == 0 && data_size == 0 && !is_skinned {
            log::debug!(
                "BSTriShape zero-vertex non-skinned shape \
                 (num_triangles={}, vertex_desc=0x{:016x}) — either CSG-deferred \
                 (`_oc.nif` Shared variant, #1188) or malformed",
                num_triangles,
                vertex_desc,
            );
        }
        // FO4 precombined LOD chunks (the dominant pattern in
        // `Fallout4 - MeshesExtra.ba2`) ship with `data_size == 0` and
        // non-trivial `num_vertices` / `num_triangles` — the actual
        // vertex / index payload lives in a sidecar precombined buffer
        // and the on-disk BSTriShape body is just metadata. Pre-#711
        // the file-driven `allocate_vec(num_vertices)` precheck fired
        // before the `data_size > 0` gate, comparing a 15k-vertex claim
        // against the few bytes left in the block — causing 45,521
        // `BSMeshLODTriShape` and 18,073 `BSTriShape` blocks to error
        // out and fall to NiUnknown. Move the file-driven allocations
        // inside the gate so they only run when there's actual data
        // to populate. Vectors stay empty otherwise — same convention
        // as the SSE skin-reconstruction path (#178) and BSDynamicTriShape
        // facegen (#341), both of which already pass `BsTriShape` with
        // empty inline arrays through to downstream consumers.
        let mut vertices: Vec<NiPoint3> = Vec::new();
        let mut uvs: Vec<[f32; 2]> = Vec::new();
        let mut normals: Vec<NiPoint3> = Vec::new();
        let mut vertex_colors: Vec<[f32; 4]> = Vec::new();
        let mut triangles: Vec<[u16; 3]> = Vec::new();
        let mut bone_weights: Vec<[f32; 4]> = Vec::new();
        let mut bone_indices: Vec<[u8; 4]> = Vec::new();
        let mut tangents: Vec<[f32; 4]> = Vec::new();

        if data_size > 0 {
            // #388/#408 — bounds-check every file-driven count before
            // allocation. Now safely below the `data_size > 0` gate so
            // empty-payload LOD blocks aren't measured against impossible
            // capacity targets.
            // FO4 BSVertexData / SSE BSVertexDataSSE packed stream.
            // `full_precision` is computed here (not per-vertex inside the
            // loop) so the FO4 precombined CSG path can override it — on
            // disk a precombine stores half positions even when the
            // descriptor sets VF_FULL_PRECISION (M49). Shared decode lives
            // in `decode_bs_vertex_stream`.
            let full_precision = stream.bsver() < crate::version::bsver::FALLOUT4
                || vertex_attrs & VF_FULL_PRECISION != 0;
            let decoded = decode_bs_vertex_stream(
                stream,
                num_vertices as usize,
                vertex_attrs,
                vertex_size_bytes,
                full_precision,
                is_skinned,
            )?;
            vertices = decoded.vertices;
            uvs = decoded.uvs;
            normals = decoded.normals;
            vertex_colors = decoded.vertex_colors;
            tangents = decoded.tangents;
            bone_weights = decoded.bone_weights;
            bone_indices = decoded.bone_indices;

            // Triangle indices — single bulk read into `Vec<[u16; 3]>`.
            // Replaces the prior `read_u16_array(N*3)` +
            // `chunks_exact(3).map(...).collect()` rebuild with one
            // `read_exact` and zero per-element allocations. The
            // `allocate_vec(num_triangles)` reservation above stays as
            // the pre-allocation cap-check; the bulk read overwrites
            // the empty `triangles` Vec it produced. #874.
            triangles = stream.read_u16_triple_array(num_triangles as usize)?;
        }

        // Skyrim SE: particle data size is **unconditionally** present per
        // nif.xml (`#BS_SSE#` vercond is version-only, not data-size-gated).
        // Only the trailing particle arrays are gated by `particle_data_size > 0`.
        // Issue #341: previously this read was inside `if data_size > 0`, so for
        // every BSDynamicTriShape (data_size==0, real vertex data lives in the
        // trailing dynamic Vector4 array) the 4-byte size was never consumed,
        // misaligning `parse_dynamic` and dropping every Skyrim NPC face mesh.
        if stream.bsver() < crate::version::bsver::FALLOUT4 {
            let particle_data_size = stream.read_u32_le()?;
            if particle_data_size > 0 {
                // particle vertices (num_vertices × 6 bytes) + particle normals + particle triangles
                let skip_bytes = (num_vertices as u64) * 6 // half-float positions
                    + (num_vertices as u64) * 6 // half-float normals
                    + (num_triangles as u64) * 6; // triangle indices
                stream.skip(skip_bytes)?;
            }
        }

        Ok(Self {
            av,
            center,
            radius,
            skin_ref,
            shader_property_ref,
            alpha_property_ref,
            vertex_desc,
            num_triangles,
            num_vertices,
            vertices,
            uvs,
            normals,
            vertex_colors,
            triangles,
            bone_weights,
            bone_indices,
            tangents,
            kind: BsTriShapeKind::Plain,
            data_size,
        })
    }

    /// Builder used by the block dispatcher to stamp the wire type
    /// discriminator for BSMeshLODTriShape and BSSubIndexTriShape, which
    /// share [`Self::parse`] / [`Self::parse_lod`] with the Plain and LOD
    /// variants. See #560.
    pub fn with_kind(mut self, kind: BsTriShapeKind) -> Self {
        self.kind = kind;
        self
    }

    /// Parse `BSDynamicTriShape` — a BSTriShape subclass used for Skyrim
    /// facegen head meshes. The block contains the full BSTriShape body
    /// (including the unconditional SSE `Particle Data Size` u32 — see
    /// `parse()` issue #341) followed by a CPU-mutable per-vertex
    /// `Vector4` array that the engine updates at runtime for morphs.
    ///
    /// Wire layout (niflib nif.xml `<niobject name="BSDynamicTriShape">`):
    /// ```text
    /// BSTriShape body
    /// uint dynamic_data_size  ; calc = num_vertices * 16
    /// Vector4[dynamic_data_size / 16] vertices
    /// ```
    ///
    /// When the dynamic-vertex array is present we overwrite the BSTriShape
    /// positions with it — on facegen meshes the base-packed positions are
    /// often zero placeholders, and the trailing float4 array carries the
    /// actual head shape. See issues #157 and #341.
    pub fn parse_dynamic(stream: &mut NifStream) -> io::Result<Self> {
        let mut shape = Self::parse(stream)?;
        let dynamic_data_size = stream.read_u32_le()?;
        let dynamic_count = (dynamic_data_size / 16) as usize;
        if dynamic_count > 0 {
            // #388: bound the file-driven count through allocate_vec.
            let mut dynamic_vertices: Vec<NiPoint3> = stream.allocate_vec(dynamic_count as u32)?;
            for _ in 0..dynamic_count {
                let x = stream.read_f32_le()?;
                let y = stream.read_f32_le()?;
                let z = stream.read_f32_le()?;
                let _w = stream.read_f32_le()?; // bitangent-x or unused
                dynamic_vertices.push(NiPoint3 { x, y, z });
            }
            if !dynamic_vertices.is_empty() {
                shape.vertices = dynamic_vertices;
                // #621 / SK-D1-04: the dynamic Vector4 array is full-
                // precision f32 — it overwrote the (typically packed
                // half-precision on FO4+ facegen) positions. Update
                // `vertex_desc` so downstream consumers reading
                // `vertex_attrs & VF_FULL_PRECISION` see the post-
                // overwrite reality. Latent today (no consumer cross-
                // checks), but a future GPU-skinning path that re-
                // uploads from the packed buffer needs this metadata
                // to match. The bit lives in the high u16 of the u64
                // vertex_desc per nif.xml `BSVertexDesc` (line 2092):
                // `vertex_attrs` is bits 44..56.
                shape.vertex_desc |= (VF_FULL_PRECISION as u64) << 44;
            }
        }
        shape.kind = BsTriShapeKind::Dynamic;
        // #571 / SK-D1-02: surface the silent-import path. Empirical
        // measurement (#946 / SK-D5-NEW-08): every single
        // BSDynamicTriShape in `Skyrim - Meshes0.bsa` (21 140 / 21 140)
        // ships `data_size == 0` on the parent BSTriShape body — the
        // pre-#946 doc claim that this branch "is dormant on shipped
        // content" was empirically false. Demote from `warn!` to
        // `debug!` so vanilla loads don't spam logs; the diagnostic
        // is still available for `RUST_LOG=debug` runs investigating
        // a specific mesh that silently fails to render.
        if !shape.vertices.is_empty() && shape.triangles.is_empty() {
            log::debug!(
                "BSDynamicTriShape produced {} vertices but 0 triangles \
                 (data_size==0 on the BSTriShape body skipped the \
                  triangle read; expected on vanilla Skyrim SE facegen \
                  meshes — see #946) — mesh will silently fail to render \
                  at the import boundary",
                shape.vertices.len()
            );
        }
        Ok(shape)
    }

    /// Parse `BSLODTriShape` — a BSTriShape subclass used for FO4 distant
    /// LOD geometry. Adds three trailing LOD triangle counts.
    ///
    /// Wire layout (niflib nif.xml):
    /// ```text
    /// BSTriShape body
    /// uint lod_0_size
    /// uint lod_1_size
    /// uint lod_2_size
    /// ```
    ///
    /// The sizes are preserved via [`BsTriShapeKind::LOD`] so the FO4
    /// distant-LOD batch importer can pick the correct triangle cutoff.
    /// The `BSMeshLODTriShape` dispatch arm calls this and then overwrites
    /// the kind via [`Self::with_kind`] — Skyrim SE DLC doesn't consult
    /// the cutoffs but we still want the wire-type discriminator. See #157, #560.
    pub fn parse_lod(stream: &mut NifStream) -> io::Result<Self> {
        let mut shape = Self::parse(stream)?;
        let lod0 = stream.read_u32_le()?;
        let lod1 = stream.read_u32_le()?;
        let lod2 = stream.read_u32_le()?;
        shape.kind = BsTriShapeKind::LOD { lod0, lod1, lod2 };
        Ok(shape)
    }

    /// Parse `BSSubIndexTriShape` — Skyrim SE DLC / FO4 / FO76 actor mesh
    /// segmentation. Pre-#404 the dispatcher ran the base `BsTriShape::parse`
    /// and then skipped the remaining bytes via `block_size`, so the
    /// per-segment bone-slot flags (used for dismemberment / locational
    /// damage) were never recovered. This implements the structured-decode
    /// path per nif.xml `<niobject name="BSSubIndexTriShape">`.
    ///
    /// Wire layout differs by stream variant:
    /// - **SSE (`bsver < crate::version::bsver::FALLOUT4`)** — always present (no `data_size > 0` gate):
    ///   `uint num_segments` followed by `BSGeometrySegmentData[num_segments]`
    ///   where each entry is `byte flags + uint start_index + uint num_primitives`
    ///   (no parent_array_index / sub_segments).
    /// - **FO4+ (`bsver >= crate::version::bsver::FALLOUT4`)** — gated on `BsTriShape::data_size > 0`:
    ///   `uint num_primitives + uint num_segments + uint total_segments`
    ///   then `BSGeometrySegmentData[num_segments]` (`start_index +
    ///   num_primitives + parent_array_index + num_sub_segments +
    ///   BSGeometrySubSegment[num_sub_segments]`). When
    ///   `num_segments < total_segments`, a trailing
    ///   `BSGeometrySegmentSharedData` follows: `num_segments + total_segments
    ///   + segment_starts[num_segments] + per_segment_data[total_segments]
    ///   + ssf_filename` (u16-prefixed string).
    pub fn parse_sub_index(stream: &mut NifStream, block_size: Option<u32>) -> io::Result<Self> {
        let block_start = stream.position();
        let mut shape = Self::parse(stream)?;
        let segmentation_start = stream.position();
        match BsSubIndexTriShapeData::parse(stream, &shape) {
            Ok(sub_data) => {
                shape.kind = BsTriShapeKind::SubIndex(Box::new(sub_data));
            }
            Err(e) => {
                // Segmentation parse failed — typically a misaligned
                // sub-segment / per-segment-data layout in some shipped
                // FO4 content. Don't take the BsTriShape body down with
                // it: degrade to the pre-#404 wholesale-skip behaviour
                // so the renderer still sees the geometry. Skip past
                // the segmentation block via `block_size` when known;
                // otherwise rewind to the segmentation start so the
                // outer block-loop's recovery can resync via the header
                // size table. Empty `Default` data signals "segmentation
                // payload not recovered" to downstream consumers.
                log::debug!(
                    "BSSubIndexTriShape segmentation decode failed at offset {}: \
                     {} — falling back to block_size skip; geometry preserved",
                    segmentation_start,
                    e,
                );
                if let Some(size) = block_size {
                    let target = block_start + size as u64;
                    let cur = stream.position();
                    if cur < target {
                        stream.skip(target - cur)?;
                    } else {
                        // Already past target — rewind and re-skip.
                        stream.set_position(segmentation_start);
                        let consumed = segmentation_start - block_start;
                        if (consumed as u32) < size {
                            stream.skip((size as u64) - consumed)?;
                        }
                    }
                } else {
                    stream.set_position(segmentation_start);
                }
                shape.kind = BsTriShapeKind::SubIndex(Box::default());
            }
        }
        Ok(shape)
    }
}

/// FO4+ sub-segment within a `BSGeometrySegmentData`. Each describes a
/// contiguous triangle range within the parent segment, and (via its
/// parent index in `BSGeometryPerSegmentSharedData`) carries the user
/// slot / bone hash needed for body-part / dismemberment routing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BsGeometrySubSegment {
    pub start_index: u32,
    pub num_primitives: u32,
    pub parent_array_index: u32,
    pub unused: u32,
}

/// One entry in the `BSSubIndexTriShape` segment table. SSE-era meshes
/// use the `flags` byte with no sub-segments; FO4+ replaces the byte
/// flags with `parent_array_index` + a (possibly empty) sub-segment list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BsGeometrySegmentData {
    /// Pre-FO4 (`NI_BS_LT_FO4`) byte flags. `None` on FO4+/FO76.
    pub flags: Option<u8>,
    pub start_index: u32,
    pub num_primitives: u32,
    /// FO4+ only. `None` on SSE.
    pub parent_array_index: Option<u32>,
    /// FO4+ only. Empty on SSE.
    pub sub_segments: Vec<BsGeometrySubSegment>,
}

/// Per-segment shared data attached to a sub-segment via its
/// `parent_array_index`. Carries the user slot (Biped Object) +
/// hashed bone name + cut-offset list used by the Creation Engine
/// dismemberment system. FO4 / FO76 only.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BsGeometryPerSegmentSharedData {
    pub user_index: u32,
    pub bone_id: u32,
    pub cut_offsets: Vec<f32>,
}

/// Trailing shared-data block on FO4+ `BSSubIndexTriShape` when
/// `num_segments < total_segments` (i.e., at least one segment has
/// sub-segments). Pairs the segment-start offsets with the
/// per-(sub)segment shared metadata and an SSF filename pointer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BsGeometrySegmentSharedData {
    pub num_segments: u32,
    pub total_segments: u32,
    pub segment_starts: Vec<u32>,
    pub per_segment_data: Vec<BsGeometryPerSegmentSharedData>,
    /// `.ssf` filename — the segment-shape file the CK uses to author
    /// dismemberment metadata. u16-prefixed string per nif.xml
    /// `SizedString16`.
    pub ssf_filename: String,
}

/// Parsed segmentation payload for `BSSubIndexTriShape`. Lives inside
/// [`BsTriShapeKind::SubIndex`] and exposes the bone-slot metadata
/// downstream consumers need for dismemberment / locational damage
/// (the renderer itself doesn't consult any of this — but the M-series
/// combat / damage roadmap needs the parse path landed first). See #404.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BsSubIndexTriShapeData {
    /// FO4+ total triangle count (calc'd from the BSTriShape body).
    /// Always 0 on SSE-era meshes.
    pub num_primitives: u32,
    /// Number of explicit segments in the segment table.
    pub num_segments: u32,
    /// FO4+ inclusive count (segments + sub-segments). 0 on SSE.
    pub total_segments: u32,
    pub segments: Vec<BsGeometrySegmentData>,
    /// FO4+ only, present when `num_segments < total_segments`.
    pub shared: Option<BsGeometrySegmentSharedData>,
}

impl BsSubIndexTriShapeData {
    /// Parse the trailing segmentation block following the BSTriShape body.
    /// `shape` is needed for `data_size` (the FO4+ gate) — SSE always
    /// reads the segment count regardless.
    fn parse(stream: &mut NifStream, shape: &BsTriShape) -> io::Result<Self> {
        let bsver = stream.bsver();
        if bsver >= crate::version::bsver::FALLOUT4 {
            // FO4+ / FO76. All fields gated on data_size > 0 — empty
            // template / stub meshes ship with no segmentation payload.
            if shape.data_size == 0 {
                return Ok(Self::default());
            }
            let num_primitives = stream.read_u32_le()?;
            let num_segments = stream.read_u32_le()?;
            let total_segments = stream.read_u32_le()?;
            // #388/#408 — bound the file-driven count before allocation.
            let mut segments: Vec<BsGeometrySegmentData> = stream.allocate_vec(num_segments)?;
            for _ in 0..num_segments {
                let start_index = stream.read_u32_le()?;
                let seg_num_primitives = stream.read_u32_le()?;
                let parent_array_index = stream.read_u32_le()?;
                let num_sub_segments = stream.read_u32_le()?;
                let mut sub_segments: Vec<BsGeometrySubSegment> =
                    stream.allocate_vec(num_sub_segments)?;
                for _ in 0..num_sub_segments {
                    sub_segments.push(BsGeometrySubSegment {
                        start_index: stream.read_u32_le()?,
                        num_primitives: stream.read_u32_le()?,
                        parent_array_index: stream.read_u32_le()?,
                        unused: stream.read_u32_le()?,
                    });
                }
                segments.push(BsGeometrySegmentData {
                    flags: None,
                    start_index,
                    num_primitives: seg_num_primitives,
                    parent_array_index: Some(parent_array_index),
                    sub_segments,
                });
            }
            // `BSGeometrySegmentSharedData` is only present when there's
            // at least one sub-segment (i.e., `total_segments` strictly
            // exceeds the explicit segment count).
            let shared = if num_segments < total_segments {
                let s_num_segments = stream.read_u32_le()?;
                let s_total_segments = stream.read_u32_le()?;
                // #981 — bulk-read segment offsets via `read_u32_array`.
                let segment_starts = stream.read_u32_array(s_num_segments as usize)?;
                let mut per_segment_data: Vec<BsGeometryPerSegmentSharedData> =
                    stream.allocate_vec(s_total_segments)?;
                for _ in 0..s_total_segments {
                    let user_index = stream.read_u32_le()?;
                    let bone_id = stream.read_u32_le()?;
                    let num_cut_offsets = stream.read_u32_le()? as usize;
                    // nif.xml documents `range="0:8"` on Num Cut Offsets,
                    // but nifly's `Geometry.cpp:1230` doesn't enforce the
                    // cap and shipped FO4 content carries values above 8
                    // (verified empirically against `Fallout4 - Meshes.ba2`
                    // — a strict cap dropped parse rate from 100% to
                    // 96.46%). Trust `read_pod_vec`'s byte-budget gate
                    // (inherited from `allocate_vec`'s #388 hard cap) to
                    // bound malicious inputs and let real content through.
                    let cut_offsets = stream.read_f32_array(num_cut_offsets)?;
                    per_segment_data.push(BsGeometryPerSegmentSharedData {
                        user_index,
                        bone_id,
                        cut_offsets,
                    });
                }
                // SizedString16 — u16 length prefix.
                let ssf_len = stream.read_u16_le()? as usize;
                let ssf_bytes = stream.read_bytes(ssf_len)?;
                let ssf_filename = match String::from_utf8(ssf_bytes) {
                    Ok(s) => s,
                    Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
                };
                Some(BsGeometrySegmentSharedData {
                    num_segments: s_num_segments,
                    total_segments: s_total_segments,
                    segment_starts,
                    per_segment_data,
                    ssf_filename,
                })
            } else {
                None
            };
            Ok(Self {
                num_primitives,
                num_segments,
                total_segments,
                segments,
                shared,
            })
        } else {
            // SSE (`bsver == crate::version::bsver::SKYRIM_SE`). Always present; pre-FO4 segments use
            // a single byte for flags and don't carry parent_array_index
            // or sub-segments.
            let num_segments = stream.read_u32_le()?;
            let mut segments: Vec<BsGeometrySegmentData> = stream.allocate_vec(num_segments)?;
            for _ in 0..num_segments {
                let flags = stream.read_u8()?;
                let start_index = stream.read_u32_le()?;
                let num_primitives = stream.read_u32_le()?;
                segments.push(BsGeometrySegmentData {
                    flags: Some(flags),
                    start_index,
                    num_primitives,
                    parent_array_index: None,
                    sub_segments: Vec::new(),
                });
            }
            Ok(Self {
                num_primitives: 0,
                num_segments,
                total_segments: 0,
                segments,
                shared: None,
            })
        }
    }
}
/// Decoded per-vertex arrays from a packed `BSVertexData` stream — the
/// output of [`decode_bs_vertex_stream`]. Arrays are parallel and either
/// length `num_vertices` (when the matching `VF_*` attribute bit is set)
/// or empty.
pub(crate) struct DecodedBsVertices {
    pub vertices: Vec<NiPoint3>,
    pub uvs: Vec<[f32; 2]>,
    pub normals: Vec<NiPoint3>,
    pub vertex_colors: Vec<[f32; 4]>,
    pub tangents: Vec<[f32; 4]>,
    pub bone_weights: Vec<[f32; 4]>,
    pub bone_indices: Vec<[u8; 4]>,
}

/// Decode `num_vertices` packed vertices from a `BSVertexData` /
/// `BSVertexDataSSE` stream at the cursor.
///
/// `vertex_attrs` is the high-12-bit attribute mask (`vertex_desc >>
/// 44`); `vertex_size_bytes` the per-vertex stride the cursor is
/// realigned to after each vertex; `full_precision` selects f32 vs f16
/// positions (the caller decides — `BsTriShape::parse` derives it from
/// BSVER + `VF_FULL_PRECISION`, while the FO4 precombined CSG path forces
/// `false` because positions are stored as half on disk even when the
/// descriptor sets `VF_FULL_PRECISION`); `is_skinned` adds the trailing
/// 12-byte weight/index block. Shared by `BsTriShape::parse` (inline
/// geometry) and `crate::import::precombine` (M49 shared-geometry).
pub(crate) fn decode_bs_vertex_stream(
    stream: &mut NifStream,
    num_vertices: usize,
    vertex_attrs: u16,
    vertex_size_bytes: usize,
    full_precision: bool,
    is_skinned: bool,
) -> std::io::Result<DecodedBsVertices> {
    let nv_u32 = num_vertices as u32;
    // #388/#408 — bounds-check every count before allocation.
    let mut vertices: Vec<NiPoint3> = stream.allocate_vec(nv_u32)?;
    let mut uvs: Vec<[f32; 2]> = stream.allocate_vec(nv_u32)?;
    let mut normals: Vec<NiPoint3> = stream.allocate_vec(nv_u32)?;
    let mut vertex_colors: Vec<[f32; 4]> = stream.allocate_vec(nv_u32)?;
    let mut tangents: Vec<[f32; 4]> = Vec::new();
    let mut bone_weights: Vec<[f32; 4]> = Vec::new();
    let mut bone_indices: Vec<[u8; 4]> = Vec::new();
    if is_skinned {
        bone_weights = stream.allocate_vec(nv_u32)?;
        bone_indices = stream.allocate_vec(nv_u32)?;
    }

    for _ in 0..num_vertices {
        let vert_start = stream.position();

        // Per-vertex tangent reconstruction state. Bethesda's
        // bitangent is split across 3 non-contiguous slots in
        // the packed vertex (`bitangent_x` at end of position,
        // `bitangent_y` after normal, `bitangent_z` after
        // tangent). Capture each as it streams past so the
        // `[bx, by, bz, sign]` assembly at the end of the loop
        // body can reconstruct the full tangent record. See
        // #795 / SK-D1-03 + the on-disk-vs-shader convention
        // notes on the `tangents` field.
        let mut bitangent_x: Option<f32> = None;
        let mut bitangent_y: Option<f32> = None;
        let mut tangent_xyz: Option<[f32; 3]> = None;
        let mut bitangent_z: Option<f32> = None;
        let mut normal_xyz: Option<[f32; 3]> = None;

        // Position: full-precision (3×f32 + f32) or half-precision (3×f16 + u16).
        // SSE (BSVER < 130): always full-precision.
        // FO4+ (BSVER >= 130): bit VF_FULL_PRECISION selects precision.
        if vertex_attrs & VF_VERTEX != 0 {
            let has_tangents = vertex_attrs & VF_TANGENTS != 0;
            if full_precision {
                let pos = stream.read_ni_point3()?;
                vertices.push(pos);
                // Trailing 4-byte slot per nif.xml `BSVertexData`:
                // `Bitangent X` (f32) when VF_TANGENTS is set,
                // else `Unused W` (uint, discarded). Same byte
                // width either way so stream stays aligned.
                if has_tangents {
                    bitangent_x = Some(stream.read_f32_le()?);
                } else {
                    stream.skip(4)?;
                }
            } else {
                // Half-float positions (FO4 default)
                let x = half_to_f32(stream.read_u16_le()?);
                let y = half_to_f32(stream.read_u16_le()?);
                let z = half_to_f32(stream.read_u16_le()?);
                vertices.push(NiPoint3 { x, y, z });
                // Trailing 2-byte slot: `Bitangent X` (hfloat)
                // when VF_TANGENTS is set, else `Unused W`
                // (half, discarded).
                if has_tangents {
                    bitangent_x = Some(half_to_f32(stream.read_u16_le()?));
                } else {
                    stream.skip(2)?;
                }
            }
        }

        // UV (HalfTexCoord = 2 × f16)
        if vertex_attrs & VF_UVS != 0 {
            let u = half_to_f32(stream.read_u16_le()?);
            let v = half_to_f32(stream.read_u16_le()?);
            uvs.push([u, v]);
        }

        // Normal (ByteVector3 = 3 × u8 + bitangent Y as normbyte)
        if vertex_attrs & VF_NORMALS != 0 {
            let nx = byte_to_normal(stream.read_u8()?);
            let ny = byte_to_normal(stream.read_u8()?);
            let nz = byte_to_normal(stream.read_u8()?);
            bitangent_y = Some(byte_to_normal(stream.read_u8()?));
            normal_xyz = Some([nx, ny, nz]);
            normals.push(NiPoint3 {
                x: nx,
                y: ny,
                z: nz,
            });
        }

        // Tangent (ByteVector3 + bitangent Z normbyte). The
        // on-disk "tangent" is Bethesda's ∂P/∂V; we keep it
        // only to derive the bitangent sign — the value the
        // fragment shader actually consumes (∂P/∂U) is the
        // bitangent triplet captured above + below.
        if vertex_attrs & VF_TANGENTS != 0 && vertex_attrs & VF_NORMALS != 0 {
            let tx = byte_to_normal(stream.read_u8()?);
            let ty = byte_to_normal(stream.read_u8()?);
            let tz = byte_to_normal(stream.read_u8()?);
            tangent_xyz = Some([tx, ty, tz]);
            bitangent_z = Some(byte_to_normal(stream.read_u8()?));
        }

        // Assemble the per-vertex tangent record (Bethesda
        // bitangent triplet → our tangent slot, sign derived
        // from on-disk tangent). All in raw Z-up; importer
        // converts xyz → Y-up. Sign is rotation-invariant.
        if let (Some(bx), Some(by), Some(bz), Some(t_xyz), Some(n)) = (
            bitangent_x,
            bitangent_y,
            bitangent_z,
            tangent_xyz,
            normal_xyz,
        ) {
            // sign(dot(B, cross(N, T))) — disambiguates left/right-
            // handed TBN. T is the tangent we STORE (∂P/∂U = the
            // bitangent triplet [bx,by,bz]); B is the on-disk tangent
            // triplet (∂P/∂V = t_xyz). Shared with the authored + SSE
            // producers so the antisymmetric operand order can't drift
            // (see `bitangent_sign` / #1516). Raw Z-up values; the sign
            // is invariant under the Z-up → Y-up rotation.
            let sign = crate::types::bitangent_sign(n, [bx, by, bz], t_xyz);
            tangents.push([bx, by, bz, sign]);
        }

        // Vertex colors (RGBA as 4 × u8)
        if vertex_attrs & VF_VERTEX_COLORS != 0 {
            let r = stream.read_u8()? as f32 / 255.0;
            let g = stream.read_u8()? as f32 / 255.0;
            let b = stream.read_u8()? as f32 / 255.0;
            let a = stream.read_u8()? as f32 / 255.0;
            vertex_colors.push([r, g, b, a]);
        }

        // Skinning data — 4 × half-float weights + 4 × u8 bone indices
        // (12 bytes total). Present when the vertex descriptor has
        // VF_SKINNED set. Valid for both Skyrim LE/SE and FO4+ layouts.
        if is_skinned {
            let (weights, indices) = read_vertex_skin_data(stream)?;
            bone_weights.push(weights);
            bone_indices.push(indices);
        }

        // Eye data (f32)
        if vertex_attrs & VF_EYE_DATA != 0 {
            stream.skip(4)?;
        }

        // Ensure we consumed exactly vertex_size_bytes.
        // Guard against underflow: if consumed > vertex_size_bytes (malformed
        // vertex descriptor), report an error instead of wrapping to a huge skip.
        let consumed = (stream.position() - vert_start) as usize;
        if consumed < vertex_size_bytes {
            stream.skip((vertex_size_bytes - consumed) as u64)?;
        } else if consumed > vertex_size_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "BsTriShape vertex consumed {} bytes but descriptor says {}",
                    consumed, vertex_size_bytes
                ),
            ));
        }
    }

    Ok(DecodedBsVertices {
        vertices,
        uvs,
        normals,
        vertex_colors,
        tangents,
        bone_weights,
        bone_indices,
    })
}

/// Read the 12-byte VF_SKINNED vertex extras: 4 × half-float weights
/// followed by 4 × u8 bone indices. Weights are stored as IEEE-754
/// half-floats per nif.xml BSVertexData_F / BSVertexDataSSE.
///
/// Exposed as a standalone helper so the skinning read can be unit-tested
/// without having to construct a full BSTriShape byte stream. The
/// returned weights are renormalized via [`renormalize_skin_weights`]
/// so the inline path matches the SSE-buffer twin in
/// `import/mesh.rs` and the NiSkinData `densify_sparse_weights`
/// path. See #889.
#[inline]
fn read_vertex_skin_data(stream: &mut NifStream) -> io::Result<([f32; 4], [u8; 4])> {
    let w0 = half_to_f32(stream.read_u16_le()?);
    let w1 = half_to_f32(stream.read_u16_le()?);
    let w2 = half_to_f32(stream.read_u16_le()?);
    let w3 = half_to_f32(stream.read_u16_le()?);
    let i0 = stream.read_u8()?;
    let i1 = stream.read_u8()?;
    let i2 = stream.read_u8()?;
    let i3 = stream.read_u8()?;
    Ok((renormalize_skin_weights([w0, w1, w2, w3]), [i0, i1, i2, i3]))
}
/// Convert a byte-normalized value [0, 255] to [-1.0, 1.0].
fn byte_to_normal(b: u8) -> f32 {
    (b as f32 / 127.5) - 1.0
}

#[cfg(test)]
#[path = "../tri_shape_skin_vertex_tests.rs"]
mod skin_vertex_tests;

#[cfg(test)]
#[path = "../tri_shape_bsvertex_flag_constant_tests.rs"]
mod bsvertex_flag_constant_tests;
