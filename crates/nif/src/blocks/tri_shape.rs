//! NiTriShape and NiTriShapeData — indexed triangle geometry.
//!
//! NiTriShape is an NiAVObject leaf node that references a NiTriShapeData
//! block containing vertex positions, normals, UV coordinates, and triangle
//! index lists.

use super::base::NiAVObjectData;
use super::{traits, NiObject};
use crate::impl_ni_object;
use crate::stream::NifStream;
use crate::types::{BlockRef, NiPoint3, NiTransform};
use crate::version::NifVersion;
use std::any::Any;
use std::io;

/// Geometry leaf node referencing NiTriShapeData or NiTriStripsData.
///
/// This struct is used for both NiTriShape and NiTriStrips — they have
/// identical serialization (both inherit NiGeometry).
#[derive(Debug)]
pub struct NiTriShape {
    /// NiObjectNET + NiAVObject base fields.
    pub av: NiAVObjectData,
    // NiGeometry fields
    pub data_ref: BlockRef,
    pub skin_instance_ref: BlockRef,
    /// Skyrim+ (BSVER > 34): dedicated shader property ref.
    pub shader_property_ref: BlockRef,
    /// Skyrim+ (BSVER > 34): dedicated alpha property ref.
    pub alpha_property_ref: BlockRef,
    pub num_materials: u32,
    pub active_material_index: u32,
}

// Backward-compatible field access.
impl NiTriShape {
    pub fn name_str(&self) -> Option<&str> {
        self.av.net.name.as_deref()
    }
}

impl NiObject for NiTriShape {
    fn block_type_name(&self) -> &'static str {
        "NiTriShape"
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

impl traits::HasObjectNET for NiTriShape {
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

impl traits::HasAVObject for NiTriShape {
    fn flags(&self) -> u32 {
        self.av.flags
    }
    fn transform(&self) -> &NiTransform {
        &self.av.transform
    }
    fn properties(&self) -> &[BlockRef] {
        &self.av.properties
    }
    fn collision_ref(&self) -> BlockRef {
        self.av.collision_ref
    }
}

impl traits::HasShaderRefs for NiTriShape {
    fn shader_property_ref(&self) -> BlockRef {
        self.shader_property_ref
    }
    fn alpha_property_ref(&self) -> BlockRef {
        self.alpha_property_ref
    }
}

impl NiTriShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let av = NiAVObjectData::parse(stream)?;

        // NiGeometry fields
        let data_ref = stream.read_block_ref()?;
        let skin_instance_ref = stream.read_block_ref()?;

        let mut shader_property_ref = BlockRef::NULL;
        let mut alpha_property_ref = BlockRef::NULL;
        let mut num_materials = 0u32;
        let mut active_material_index = 0u32;

        if stream.version() >= NifVersion(0x14020005) {
            num_materials = stream.read_u32_le()?;
            for _ in 0..num_materials {
                let _mat_name_idx = stream.read_u32_le()?;
                let _mat_extra_data = stream.read_u32_le()?;
            }
            active_material_index = stream.read_u32_le()?;

            if stream.version() >= NifVersion::V20_2_0_7 {
                let _dirty_flag = stream.read_u8()?;
            }

            // Query the file's actual bsver rather than the routed
            // game variant — `variant().has_shader_alpha_refs()` returns
            // false for BSVER in 35..=82 (the `Unknown` corner) even
            // though nif.xml's `#BS_GT_FO3#` gate is `BSVER > 34` and
            // the field IS authored there. Mirrors the
            // `has_properties_list` site at `base.rs:103`. See
            // NIF-D2-NEW-07 (audit 2026-05-12).
            if stream.bsver() > 34 {
                shader_property_ref = stream.read_block_ref()?;
                alpha_property_ref = stream.read_block_ref()?;
            }
        } else if stream.version() >= NifVersion(0x0A000100)
            && stream.version() <= NifVersion(0x14010003)
        {
            // MaterialData Has Shader + Shader Name + Shader Extra Data
            // (since 10.0.1.0, until 20.1.0.3 — both boundaries inclusive
            // per the version.rs doctrine). Present in Oblivion v20.0.0.4/5
            // through v20.1.0.3.
            let has_shader = stream.read_bool()?;
            if has_shader {
                let _shader_name = stream.read_sized_string()?;
                let _implementation = stream.read_i32_le()?;
            }
        }

        Ok(Self {
            av,
            data_ref,
            skin_instance_ref,
            shader_property_ref,
            alpha_property_ref,
            num_materials,
            active_material_index,
        })
    }

    /// Parse `BSSegmentedTriShape` — an NiTriShape subclass used by
    /// FO3/FNV/SkyrimLE for biped body-part LOD chunking. Adds a
    /// trailing segment table (niflib nif.xml):
    ///
    /// ```text
    /// NiTriShape body
    /// uint num_segments
    /// for each segment:
    ///     byte flags
    ///     uint index
    ///     uint num_tris_in_segment
    /// ```
    ///
    /// The segment metadata is used for runtime dismemberment / armour
    /// body-part toggles. The renderer doesn't need it, so we consume
    /// the bytes and discard — but doing so properly (fixed layout,
    /// 9 bytes per segment) is cheaper than relying on `block_size`
    /// realignment and avoids warning spam. See issue #146.
    pub fn parse_segmented(stream: &mut NifStream) -> io::Result<Self> {
        let shape = Self::parse(stream)?;
        let num_segments = stream.read_u32_le()?;
        for _ in 0..num_segments {
            let _flags = stream.read_u8()?;
            let _index = stream.read_u32_le()?;
            let _num_tris = stream.read_u32_le()?;
        }
        Ok(shape)
    }
}

/// NiTriStrips — identical serialization to NiTriShape (both are NiGeometry).
pub type NiTriStrips = NiTriShape;

/// `BSLODTriShape` — Skyrim/SSE distant-LOD shape for visibility
/// control over vertex groups. Per niftools nif.xml inherits from
/// `NiTriBasedGeom` (the `NiTriShape` lineage), **not** `BSTriShape`.
/// Pre-#838 the dispatcher routed it through [`BsTriShape::parse_lod`]
/// which over-consumed by 23 bytes per block on real Skyrim tree LODs
/// (`BSLODTriShape: expected 109 bytes, consumed 132`) — `block_size`
/// recovery silently realigned the stream and the LOD-size triplet
/// was decoded from BSTriShape vertex bytes that happened to land in
/// the right position.
///
/// Wire layout (Skyrim SE, 109 B):
/// ```text
/// NiTriShape body (97 B — NiAVObjectData no-properties + 2 BlockRefs
///                       + num_materials u32 + active_material_index u32
///                       + dirty_flag u8 + 2 shader BlockRefs)
/// uint lod0_size
/// uint lod1_size
/// uint lod2_size
/// ```
///
/// Note: FO4's `BSMeshLODTriShape` IS a true `BSTriShape` subclass
/// (per nif.xml `inherit="BSTriShape" versions="#FO4#"`) so it stays
/// on `BsTriShape::parse_lod`. The two LOD-style block types share
/// the trailing 3-u32 layout but live on different bodies.
#[derive(Debug)]
pub struct NiLodTriShape {
    pub base: NiTriShape,
    pub lod0_size: u32,
    pub lod1_size: u32,
    pub lod2_size: u32,
}

impl NiObject for NiLodTriShape {
    fn block_type_name(&self) -> &'static str {
        "BSLODTriShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn traits::HasObjectNET> {
        Some(&self.base)
    }
    fn as_av_object(&self) -> Option<&dyn traits::HasAVObject> {
        Some(&self.base)
    }
    fn as_shader_refs(&self) -> Option<&dyn traits::HasShaderRefs> {
        Some(&self.base)
    }
}

impl NiLodTriShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTriShape::parse(stream)?;
        let lod0_size = stream.read_u32_le()?;
        let lod1_size = stream.read_u32_le()?;
        let lod2_size = stream.read_u32_le()?;
        Ok(Self {
            base,
            lod0_size,
            lod1_size,
            lod2_size,
        })
    }
}

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
        let num_triangles = if stream.bsver() >= 130 {
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
                    if payload % num_vertices as usize == 0 {
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

        let nv_u32 = num_vertices as u32;
        let is_skinned = vertex_attrs & VF_SKINNED != 0;
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
            vertices = stream.allocate_vec(nv_u32)?;
            uvs = stream.allocate_vec(nv_u32)?;
            normals = stream.allocate_vec(nv_u32)?;
            vertex_colors = stream.allocate_vec(nv_u32)?;
            // No pre-allocation for `triangles` — the bulk
            // `read_u16_triple_array` below does its own #408
            // check_alloc, and the prior `allocate_vec` here was a
            // dead store under that path. #874.
            if is_skinned {
                bone_weights = stream.allocate_vec(nv_u32)?;
                bone_indices = stream.allocate_vec(nv_u32)?;
            }

            // `vertex_size_bytes` was computed above the `data_size > 0`
            // gate so the #621 / SK-D1-05 mismatch path can override
            // the descriptor's quad count with the data_size-derived
            // stride before the per-vertex loop runs.

            // Parse each vertex from the packed format.
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
                    let full_precision =
                        stream.bsver() < 130 || vertex_attrs & VF_FULL_PRECISION != 0;
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
                    // sign(dot(B, cross(N, T))) — disambiguates left/
                    // right-handed TBN. Operates on raw Z-up values;
                    // determinant is preserved across the proper
                    // rotation Z-up → Y-up so the sign is correct
                    // post-conversion. Mirrors `extract_tangents_from_extra_data`.
                    let cnx = n[1] * t_xyz[2] - n[2] * t_xyz[1];
                    let cny = n[2] * t_xyz[0] - n[0] * t_xyz[2];
                    let cnz = n[0] * t_xyz[1] - n[1] * t_xyz[0];
                    let dot_b_cross = bx * cnx + by * cny + bz * cnz;
                    let sign = if dot_b_cross >= 0.0 { 1.0 } else { -1.0 };
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
        if stream.bsver() < 130 {
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
        // #571 / SK-D1-02: surface the silent-import path. Vanilla
        // Skyrim SE facegen ships `data_size > 0` (real triangles
        // packed alongside the placeholder positions), so this
        // branch is dormant on shipped content. A malformed or
        // aggressively stripped-down mod facegen NIF that ships
        // `data_size == 0` would land here — `parse()` skipped both
        // the vertex and the triangle reads, `parse_dynamic`
        // populated `vertices` from the trailing Vector4 array but
        // `triangles` is still empty, and `extract_bs_tri_shape`
        // would then bail at the `triangles.is_empty()` early
        // return with zero log signal. Match the existing
        // "verbose at import boundary" idiom and warn so the
        // failure is audible.
        if !shape.vertices.is_empty() && shape.triangles.is_empty() {
            log::warn!(
                "BSDynamicTriShape produced {} vertices but 0 triangles \
                 (data_size==0 on the BSTriShape body skipped the \
                  triangle read; on shipped vanilla content this never \
                  fires) — mesh will silently fail to render at the \
                  import boundary",
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
    /// - **SSE (`bsver < 130`)** — always present (no `data_size > 0` gate):
    ///   `uint num_segments` followed by `BSGeometrySegmentData[num_segments]`
    ///   where each entry is `byte flags + uint start_index + uint num_primitives`
    ///   (no parent_array_index / sub_segments).
    /// - **FO4+ (`bsver >= 130`)** — gated on `BsTriShape::data_size > 0`:
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
                shape.kind = BsTriShapeKind::SubIndex(Box::new(BsSubIndexTriShapeData::default()));
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
        if bsver >= 130 {
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
            // SSE (`bsver == 100`). Always present; pre-FO4 segments use
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

/// Renormalize a 4-influence weight tuple to unit sum so half-float
/// quantization drift can't accumulate per-frame jitter on the GPU
/// skinning path.
///
/// `triangle.vert` computes the matrix-palette result as a straight
/// weighted sum without dividing by `wsum` (it only uses `wsum` to
/// detect the rigid-fallback case `wsum < 0.001`). Half-float
/// quantization produces ~1-part-in-1024 error per component, so a
/// 4-influence vertex can drift up to ~0.4% off unit sum and the
/// rendered skin position drifts the same fraction.
///
/// Skip the renormalization when the sum is already within `1e-4`
/// of `1.0` (well-formed content) or below `1e-6` (the rigid-fallback
/// path the vertex shader detects). See #889.
#[inline]
pub(crate) fn renormalize_skin_weights(w: [f32; 4]) -> [f32; 4] {
    let sum = w[0] + w[1] + w[2] + w[3];
    if (sum - 1.0).abs() <= 1e-4 || sum <= 1e-6 {
        return w;
    }
    let inv = 1.0 / sum;
    [w[0] * inv, w[1] * inv, w[2] * inv, w[3] * inv]
}

/// Convert IEEE 754 half-precision float (u16) to f32.
pub(crate) fn half_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as u32;
    let mantissa = (h & 0x3FF) as u32;

    if exp == 0 {
        if mantissa == 0 {
            return f32::from_bits(sign << 31);
        }
        // Subnormal: normalize
        let mut m = mantissa;
        let mut e = 0i32;
        while m & 0x400 == 0 {
            m <<= 1;
            e -= 1;
        }
        m &= 0x3FF;
        let f_exp = (127 - 15 + 1 + e) as u32;
        return f32::from_bits((sign << 31) | (f_exp << 23) | (m << 13));
    }
    if exp == 31 {
        // Inf/NaN
        return f32::from_bits((sign << 31) | (0xFF << 23) | (mantissa << 13));
    }
    let f_exp = exp + (127 - 15);
    f32::from_bits((sign << 31) | (f_exp << 23) | (mantissa << 13))
}

/// Convert a byte-normalized value [0, 255] to [-1.0, 1.0].
fn byte_to_normal(b: u8) -> f32 {
    (b as f32 / 127.5) - 1.0
}

/// The actual geometry data: vertices, normals, UVs, and triangle indices.
#[derive(Debug)]
pub struct NiTriShapeData {
    pub vertices: Vec<NiPoint3>,
    pub normals: Vec<NiPoint3>,
    pub center: NiPoint3,
    pub radius: f32,
    pub vertex_colors: Vec<[f32; 4]>,
    pub uv_sets: Vec<Vec<[f32; 2]>>,
    pub triangles: Vec<[u16; 3]>,
}

/// Parse the NiGeometryData base class fields shared by NiTriShapeData and NiTriStripsData.
/// Returns (vertices, data_flags, normals, center, radius, vertex_colors, uv_sets).
pub(crate) fn parse_geometry_data_base(
    stream: &mut NifStream,
) -> io::Result<(
    Vec<NiPoint3>,      // vertices
    u16,                // data_flags
    Vec<NiPoint3>,      // normals
    NiPoint3,           // center
    f32,                // radius
    Vec<[f32; 4]>,      // vertex_colors
    Vec<Vec<[f32; 2]>>, // uv_sets
)> {
    parse_geometry_data_base_inner(stream, false)
}

/// Variant that treats the per-vertex arrays (positions, normals, tangents,
/// colors, UVs) as zero-length regardless of the `Has*` bools. Used by
/// NiPSysData on BS_GTE_FO3 streams where nif.xml (line 3880) says:
/// "Vertices, Normals, Tangents, Colors, and UV arrays do not have length
/// for NiPSysData regardless of 'Num' or booleans." See #322.
pub(crate) fn parse_psys_geometry_data_base(
    stream: &mut NifStream,
) -> io::Result<(
    Vec<NiPoint3>,
    u16,
    Vec<NiPoint3>,
    NiPoint3,
    f32,
    Vec<[f32; 4]>,
    Vec<Vec<[f32; 2]>>,
)> {
    parse_geometry_data_base_inner(stream, true)
}

fn parse_geometry_data_base_inner(
    stream: &mut NifStream,
    zero_arrays: bool,
) -> io::Result<(
    Vec<NiPoint3>,
    u16,
    Vec<NiPoint3>,
    NiPoint3,
    f32,
    Vec<[f32; 4]>,
    Vec<Vec<[f32; 2]>>,
)> {
    // Group ID: nif.xml says `since="10.1.0.114"` (0x0A010072), not
    // 10.0.1.0. Files in the [10.0.1.0, 10.1.0.114) range (non-Bethesda
    // Gamebryo, pre-Civ IV era) read 4 phantom bytes, misaligning every
    // NiGeometryData afterward. See #326 / audit N1-01.
    if stream.version() >= NifVersion(0x0A010072) {
        let _group_id = stream.read_i32_le()?; // usually 0
    }
    let num_vertices_raw = stream.read_u16_le()? as usize;
    // For NiPSysData on BS202, `num_vertices_raw` is BS Max Vertices — an
    // upper bound on runtime particle count, not a serialized array length.
    let array_count = if zero_arrays { 0 } else { num_vertices_raw };
    // Keep / Compress Flags: nif.xml says `since="10.1.0.0"` (0x0A010000),
    // not 10.0.1.0. Files in the [10.0.1.0, 10.1.0.0) gap window (non-Bethesda
    // Gamebryo) had 2 phantom bytes consumed before `has_vertices`, corrupting
    // every NiGeometryData downstream. See #327 / audit N1-02.
    if stream.version() >= NifVersion::V10_1_0_0 {
        let _keep_flags = stream.read_u8()?;
        let _compress_flags = stream.read_u8()?;
    }

    let has_vertices = stream.read_byte_bool()?;
    let vertices = if has_vertices {
        stream.read_ni_point3_array(array_count)?
    } else {
        Vec::new()
    };

    // Data flags: present from v >= 10.0.1.0. Pre-Gamebryo stores UV set count
    // as a separate u16 field after normals + bounding sphere.
    let data_flags = if stream.version() >= NifVersion(0x0A000100) {
        let df = stream.read_u16_le()?;
        // Query the file's bsver directly — `variant().has_material_crc()`
        // would return false for the BSVER 35..=82 `Unknown` gap. The
        // material CRC is authored from Skyrim onward per nif.xml's
        // `BSVER > 34` rule. See NIF-D2-NEW-07 (audit 2026-05-12).
        if stream.bsver() > 34 {
            let _material_crc = stream.read_u32_le()?;
        }
        df
    } else {
        0
    };

    let has_normals = stream.read_byte_bool()?;
    let normals = if has_normals {
        stream.read_ni_point3_array(array_count)?
    } else {
        Vec::new()
    };

    // Tangents + bitangents. Per nif.xml `NiGeometryData.Tangents`,
    // the NBT vectors are present only when bit 12 (`NBT_METHOD = 0x1000`)
    // is set. Bits 13–15 carry unrelated payload pointers (FO3 FaceGen
    // heads set bit 13/14 for VAS_MATERIAL_DATA / VAS_MORPH_DATA) and
    // must NOT trigger the 24-bytes-per-vertex skip.
    //
    // Pre-fix the mask was `0xF000`, which mis-triggered on every FO3
    // FaceGen head — the resulting 24 * num_vertices over-read ran the
    // parser past the end of the block and `block_sizes` recovery
    // demoted the NiTriShapeData to `NiUnknown`, leaving the NPC face
    // with no geometry. See #440 / audit FO3-5-01.
    if has_normals && data_flags & 0x1000 != 0 {
        // Skip tangents (array_count * 3 floats)
        stream.skip(array_count as u64 * 12)?;
        // Skip bitangents (array_count * 3 floats)
        stream.skip(array_count as u64 * 12)?;
    }

    // Bounding sphere
    let center = stream.read_ni_point3()?;
    let radius = stream.read_f32_le()?;

    // Vertex colors
    let has_vertex_colors = stream.read_byte_bool()?;
    let vertex_colors = if has_vertex_colors {
        stream.read_ni_color4_array(array_count)?
    } else {
        Vec::new()
    };

    // UV sets. Two disjoint encodings share these bits depending on the
    // stream:
    //   - `NiGeometryDataFlags` (non-Bethesda Gamebryo v3.x+): bits 0..5
    //     encode `Num UV Sets` as a 6-bit count (0..63).
    //   - `BSGeometryDataFlags` (Bethesda #BS202# — NIF 20.2.0.7 with
    //     `bsver > 0`): bit 0 is `Has UV` (bool — 0 or 1 UV sets), bits
    //     1..5 are unused, bits 6..11 are Havok Material index.
    // nif.xml (line 3914) reconciles them with `UV_count =
    // (DataFlags & 63) | (BSDataFlags & 1)` — exactly one side is zero
    // per the vercond gating.
    //
    // Pre-fix every Bethesda stream used the NiGeometry decode, so a
    // FO3 FaceGen head with `data_flags = 0x1003` asked for 3 UV sets
    // when only 1 was serialized; the resulting 20,912-byte over-read
    // chained into a garbage `num_match_groups` u16 whose skip blew
    // past EOF and demoted the NiTriShapeData to NiUnknown → every
    // FO3 NPC face rendered as empty geometry. See #440 / audit
    // FO3-5-01.
    //
    // Pre-Gamebryo (v < 10.0.1.0) has a separate `num_uv_sets` u16 field.
    let num_uv_sets = if stream.version() < NifVersion(0x0A000100) {
        stream.read_u16_le()? as usize
    } else if stream.bsver() > 0 && stream.version() == NifVersion(0x14020007) {
        // BSGeometryDataFlags path.
        (data_flags & 0x0001) as usize
    } else {
        (data_flags & 0x003F) as usize
    };

    // nif.xml: `<field name="Has UV" type="bool" until="4.0.0.2">` — the
    // explicit bool is serialized at v <= 4.0.0.2 (`until=` is inclusive
    // per the version.rs doctrine). At v4.0.0.2 (Morrowind canonical) the
    // bool IS still read; from v4.0.0.3 onward UV presence is derived from
    // `num_uv_sets`: in the pre-Gamebryo branch this came from the inline
    // u16 at line 701-702, otherwise from `data_flags & 0x3F`. See #325.
    let has_uv = if stream.version() <= NifVersion(0x04000002) {
        stream.read_byte_bool()?
    } else {
        num_uv_sets > 0
    };

    // #408 — file-driven count via allocate_vec.
    let uv_set_capacity = if has_uv { num_uv_sets.max(1) } else { 0 };
    let mut uv_sets = stream.allocate_vec(uv_set_capacity as u32)?;
    if has_uv {
        // Ensure at least 1 UV set if has_uv is true but num_uv_sets is 0 (legacy)
        let count = num_uv_sets.max(1);
        for _ in 0..count {
            uv_sets.push(stream.read_uv_array(array_count)?);
        }
    }

    // Consistency flags (version >= 10.0.1.0)
    if stream.version() >= NifVersion(0x0A000100) {
        let _consistency_flags = stream.read_u16_le()?;
    }

    // Additional data (version >= 20.0.0.4)
    if stream.version() >= NifVersion(0x14000004) {
        let _additional_data_ref = stream.read_block_ref()?;
    }

    Ok((
        vertices,
        data_flags,
        normals,
        center,
        radius,
        vertex_colors,
        uv_sets,
    ))
}

impl NiTriShapeData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let (vertices, _data_flags, normals, center, radius, vertex_colors, uv_sets) =
            parse_geometry_data_base(stream)?;

        // NiTriShapeData specific: triangles
        let num_triangles = stream.read_u16_le()? as usize;
        let _num_triangle_points = stream.read_u32_le()?; // num_triangles * 3

        // has_triangles bool: only present from v >= 10.0.1.0. In Morrowind-era
        // NIFs, triangles are always present when num_triangles > 0.
        let has_triangles = if stream.version() >= NifVersion(0x0A000100) {
            stream.read_byte_bool()?
        } else {
            num_triangles > 0
        };
        // Single bulk read — `[u16; 3]` is POD so the prior
        // `read_u16_array(N*3)` + `chunks_exact(3).map(...).collect()`
        // is replaced by one `read_pod_vec::<[u16; 3]>` cast. #874.
        let triangles = if has_triangles {
            stream.read_u16_triple_array(num_triangles)?
        } else {
            Vec::new()
        };

        // Match groups (skip)
        let num_match_groups = stream.read_u16_le()? as usize;
        for _ in 0..num_match_groups {
            let count = stream.read_u16_le()? as usize;
            stream.skip(count as u64 * 2)?; // u16 per entry
        }

        Ok(Self {
            vertices,
            normals,
            center,
            radius,
            vertex_colors,
            uv_sets,
            triangles,
        })
    }
}

/// Triangle strip geometry data (NiTriStripsData).
///
/// Same NiGeometryData base as NiTriShapeData, but stores triangle strips
/// instead of a flat triangle index list.
#[derive(Debug)]
pub struct NiTriStripsData {
    pub vertices: Vec<NiPoint3>,
    pub normals: Vec<NiPoint3>,
    pub center: NiPoint3,
    pub radius: f32,
    pub vertex_colors: Vec<[f32; 4]>,
    pub uv_sets: Vec<Vec<[f32; 2]>>,
    pub num_triangles: u16,
    pub strips: Vec<Vec<u16>>,
}

impl NiTriStripsData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let (vertices, _data_flags, normals, center, radius, vertex_colors, uv_sets) =
            parse_geometry_data_base(stream)?;

        // NiTriBasedGeomData: num_triangles
        let num_triangles = stream.read_u16_le()?;

        // NiTriStripsData specific
        let num_strips = stream.read_u16_le()? as u32;
        let strip_lengths = stream.read_u16_array(num_strips as usize)?;

        // has_strips: only from v >= 10.0.1.0. In Morrowind NIFs, strips always present.
        let has_strips = if stream.version() >= NifVersion(0x0A000100) {
            stream.read_byte_bool()?
        } else {
            num_strips > 0
        };
        let mut strips: Vec<Vec<u16>> = stream.allocate_vec(num_strips)?;
        if has_strips {
            for &len in &strip_lengths {
                // `read_u16_array` validates `len * 2 <= remaining`
                // via its internal `check_alloc` (#831).
                strips.push(stream.read_u16_array(len as usize)?);
            }
        }

        Ok(Self {
            vertices,
            normals,
            center,
            radius,
            vertex_colors,
            uv_sets,
            num_triangles,
            strips,
        })
    }

    /// Convert triangle strips to a flat triangle list.
    ///
    /// Handles winding order alternation and skips degenerate triangles
    /// (used for strip stitching).
    pub fn to_triangles(&self) -> Vec<[u16; 3]> {
        let mut triangles = Vec::with_capacity(self.num_triangles as usize);
        for strip in &self.strips {
            for i in 2..strip.len() {
                // OpenGL/Vulkan strip convention (CCW front face):
                // Even triangles: standard order. Odd: swap last two to maintain CCW.
                // D3D convention swaps first two on odd — produces CW instead.
                let (a, b, c) = if i % 2 == 0 {
                    (strip[i - 2], strip[i - 1], strip[i])
                } else {
                    (strip[i - 2], strip[i], strip[i - 1])
                };
                // Skip degenerate triangles (strip stitching)
                if a != b && b != c && a != c {
                    triangles.push([a, b, c]);
                }
            }
        }
        triangles
    }
}

// ── NiAdditionalGeometryData ──────────────────────────────────────────
//
// Per-vertex auxiliary channels (tangents / bitangents / blend weights /
// optional skin bone IDs) referenced by NiGeometryData.additional_data_ref.
// Replaced by BSTriShape's embedded vertex-attribute blob at 20.2.0.7+.
// Oblivion predates it; FO3 + FNV ship 4,039 vanilla blocks. See #547.
//
// Wire layout (nif.xml lines 6996-7011):
//
//   NiAdditionalGeometryData (arg=0)                  BSPackedAdditionalGeometryData (arg=1)
//     num_vertices: u16                                 (identical)
//     num_block_infos: u32                              ...
//     block_infos[num_block_infos]: NiAGDDataStream     ...
//     num_blocks: u32                                   ...
//     blocks[num_blocks]: NiAGDDataBlocks(arg)          blocks[num_blocks]: NiAGDDataBlocks(arg=1)
//
//   NiAGDDataStream (25 bytes):
//     type, unit_size, total_size, stride, block_index,
//     block_offset: u32 × 6
//     flags: u8
//
//   NiAGDDataBlocks:
//     has_data: bool
//     if has_data: NiAGDDataBlock(arg)
//
//   NiAGDDataBlock:
//     block_size: u32
//     num_blocks: u32
//     block_offsets[num_blocks]: u32
//     num_data: u32
//     data_sizes[num_data]: u32
//     data[num_data][block_size]: u8           (flat num_data * block_size byte blob)
//     if arg == 1: shader_index: u32            (BSPackedAdditionalGeometryData only)
//     if arg == 1: total_size: u32              ...

/// Per-channel descriptor: which vertex attribute this stream carries,
/// its byte layout within the packed vertex, and mutability flags.
/// nif.xml `NiAGDDataStream` (line 6969).
#[derive(Debug, Clone)]
pub struct NiAgdDataStream {
    pub ty: u32,
    pub unit_size: u32,
    pub total_size: u32,
    pub stride: u32,
    pub block_index: u32,
    pub block_offset: u32,
    pub flags: u8,
}

impl NiAgdDataStream {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Ok(Self {
            ty: stream.read_u32_le()?,
            unit_size: stream.read_u32_le()?,
            total_size: stream.read_u32_le()?,
            stride: stream.read_u32_le()?,
            block_index: stream.read_u32_le()?,
            block_offset: stream.read_u32_le()?,
            flags: stream.read_u8()?,
        })
    }
}

/// One variable-length data block. The `data` field is a flat
/// `num_data × block_size` byte blob — the 2D `[Num Data][Block Size]`
/// layout from nif.xml is preserved row-major so consumers can slice
/// it by `block_size * row_index`.
#[derive(Debug)]
pub struct NiAgdDataBlock {
    pub block_size: u32,
    pub block_offsets: Vec<u32>,
    pub data_sizes: Vec<u32>,
    pub data: Vec<u8>,
    /// Only populated for `BSPackedAdditionalGeometryData` (nif.xml arg==1).
    pub shader_index: Option<u32>,
    /// Only populated for `BSPackedAdditionalGeometryData` (nif.xml arg==1).
    pub total_size: Option<u32>,
}

impl NiAgdDataBlock {
    fn parse(stream: &mut NifStream, packed: bool) -> io::Result<Self> {
        let block_size = stream.read_u32_le()?;
        // #981 — bulk-read both u32 arrays via `read_u32_array`.
        let num_blocks = stream.read_u32_le()? as usize;
        let block_offsets = stream.read_u32_array(num_blocks)?;
        let num_data = stream.read_u32_le()? as usize;
        let data_sizes = stream.read_u32_array(num_data)?;
        // Flat data blob: nif.xml `length="Num Data" width="Block Size"` is
        // a row-major 2D array. `read_bytes` already guards against a
        // corrupt multiplier via `check_alloc`.
        let total = (num_data as u64)
            .checked_mul(block_size as u64)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "NiAGDDataBlock: num_data * block_size overflowed u64",
                )
            })?;
        let data = stream.read_bytes(total as usize)?;
        let (shader_index, total_size) = if packed {
            (Some(stream.read_u32_le()?), Some(stream.read_u32_le()?))
        } else {
            (None, None)
        };
        Ok(Self {
            block_size,
            block_offsets,
            data_sizes,
            data,
            shader_index,
            total_size,
        })
    }
}

/// Discriminator for the two wire types that share the
/// [`NiAdditionalGeometryData`] Rust struct. `BSPackedAdditionalGeometryData`
/// appears in older FNV DLC (`nvdlc01vaultposter01.nif`) and carries the
/// two extra `shader_index` + `total_size` fields per data block. Mirrors
/// the [`BsTriShapeKind`] pattern — #560.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NiAgdKind {
    /// Plain `NiAdditionalGeometryData` — FO3 + FNV architecture tangents.
    Plain,
    /// `BSPackedAdditionalGeometryData` — nif.xml arg=1 packed variant.
    Packed,
}

/// `NiAdditionalGeometryData` / `BSPackedAdditionalGeometryData` — per-vertex
/// auxiliary channels (tangents / bitangents / blend weights, etc.) attached
/// to a NiGeometryData via its `Additional Data` ref. 4,039 FO3+FNV blocks
/// in vanilla corpora were previously demoted to `NiUnknown`. See #547.
#[derive(Debug)]
pub struct NiAdditionalGeometryData {
    pub num_vertices: u16,
    pub block_infos: Vec<NiAgdDataStream>,
    /// One entry per `num_blocks`; `None` when the `has_data` bool is false.
    pub blocks: Vec<Option<NiAgdDataBlock>>,
    pub kind: NiAgdKind,
}

impl NiObject for NiAdditionalGeometryData {
    fn block_type_name(&self) -> &'static str {
        match self.kind {
            NiAgdKind::Plain => "NiAdditionalGeometryData",
            NiAgdKind::Packed => "BSPackedAdditionalGeometryData",
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiAdditionalGeometryData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Self::parse_with_kind(stream, NiAgdKind::Plain)
    }

    pub fn parse_packed(stream: &mut NifStream) -> io::Result<Self> {
        Self::parse_with_kind(stream, NiAgdKind::Packed)
    }

    fn parse_with_kind(stream: &mut NifStream, kind: NiAgdKind) -> io::Result<Self> {
        let num_vertices = stream.read_u16_le()?;
        let num_block_infos = stream.read_u32_le()?;
        let mut block_infos = stream.allocate_vec::<NiAgdDataStream>(num_block_infos)?;
        for _ in 0..num_block_infos {
            block_infos.push(NiAgdDataStream::parse(stream)?);
        }
        let num_blocks = stream.read_u32_le()?;
        let mut blocks = stream.allocate_vec::<Option<NiAgdDataBlock>>(num_blocks)?;
        let packed = matches!(kind, NiAgdKind::Packed);
        for _ in 0..num_blocks {
            let has_data = stream.read_byte_bool()?;
            if has_data {
                blocks.push(Some(NiAgdDataBlock::parse(stream, packed)?));
            } else {
                blocks.push(None);
            }
        }
        Ok(Self {
            num_vertices,
            block_infos,
            blocks,
            kind,
        })
    }
}

impl_ni_object!(
    NiTriShapeData,
    NiTriStripsData,
);

#[cfg(test)]
#[path = "tri_shape_skin_vertex_tests.rs"]
mod skin_vertex_tests;

#[cfg(test)]
#[path = "tri_shape_nigeometry_data_version_tests.rs"]
mod nigeometry_data_version_tests;

#[cfg(test)]
#[path = "tri_shape_ni_additional_geometry_data_tests.rs"]
mod ni_additional_geometry_data_tests;

#[cfg(test)]
#[path = "tri_shape_bsvertex_flag_constant_tests.rs"]
mod bsvertex_flag_constant_tests;

#[cfg(test)]
mod renormalize_skin_weights_tests {
    use super::renormalize_skin_weights;

    /// Regression for #889: a 4-influence vertex with weights
    /// summing to 0.997 (typical sub-unit drift after half-float
    /// decode) must round-trip with a sum of 1.0 ± 1e-4.
    #[test]
    fn drifted_weights_renormalize_to_unit_sum() {
        let drift: [f32; 4] = [0.30, 0.30, 0.30, 0.097];
        let normed = renormalize_skin_weights(drift);
        let sum: f32 = normed.iter().sum();
        assert!(
            (sum - 1.0).abs() <= 1e-4,
            "post-renorm sum {sum} not within 1e-4 of 1.0"
        );
        // Ratios preserved.
        let ratio = normed[0] / normed[3];
        let original_ratio = drift[0] / drift[3];
        assert!((ratio - original_ratio).abs() < 1e-3);
    }

    /// Already-unit-sum weights pass through untouched (no float
    /// noise injected on well-formed content).
    #[test]
    fn unit_sum_weights_pass_through_unchanged() {
        let exact: [f32; 4] = [0.5, 0.25, 0.15, 0.10];
        let normed = renormalize_skin_weights(exact);
        assert_eq!(normed, exact);
    }

    /// Within-tolerance drift (0.99995) is treated as unit-sum and
    /// passes through unchanged — avoids touching content that's
    /// already within float error of well-formed.
    #[test]
    fn within_tolerance_drift_passes_through() {
        let near_unit: [f32; 4] = [0.49998, 0.24999, 0.15, 0.09998];
        let normed = renormalize_skin_weights(near_unit);
        assert_eq!(normed, near_unit);
    }

    /// Rigid-fallback weights (sum below 1e-6) pass through so the
    /// vertex shader's `wsum < 0.001` rigid-fallback branch still
    /// triggers. Renormalising would push them to spurious unit
    /// sum and break the fallback.
    #[test]
    fn near_zero_weights_preserve_rigid_fallback_path() {
        let zeroish: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
        let normed = renormalize_skin_weights(zeroish);
        assert_eq!(normed, zeroish);
    }
}
