//! NiTriShape and NiTriShapeData — indexed triangle geometry.
//!
//! NiTriShape is an NiAVObject leaf node that references a NiTriShapeData
//! block containing vertex positions, normals, UV coordinates, and triangle
//! index lists.

use super::base::NiAVObjectData;
use super::{traits, NiObject};
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

            if stream.variant().has_shader_alpha_refs() {
                shader_property_ref = stream.read_block_ref()?;
                alpha_property_ref = stream.read_block_ref()?;
            }
        } else if stream.version() >= NifVersion(0x0A000100)
            && stream.version() <= NifVersion(0x14010003)
        {
            // NiGeometry Has Shader + Shader Name + Shader Extra Data
            // (since 10.0.1.0, until 20.1.0.3 — present in Oblivion v20.0.0.4).
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

/// Discriminator for the five wire-distinct types that share the
/// [`BsTriShape`] Rust struct. Pre-#560 every variant reported
/// `"BSTriShape"` and downstream consumers (facegen head detection,
/// distant-LOD import, dismember segmentation) couldn't tell them apart.
///
/// Mirrors the [`super::node::BsRangeKind`] pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// meshes for dismemberment segmentation. Trailing segmentation
    /// table is currently skipped via `block_size`; the `kind` is what
    /// tells the importer the body is segmented. See #404.
    SubIndex,
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
    /// Wire-type discriminator. Set by each parser arm; the dispatcher
    /// in [`super::mod`] uses [`Self::with_kind`] to override for types
    /// that share a parser (BSMeshLODTriShape / BSSubIndexTriShape). #560.
    pub kind: BsTriShapeKind,
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
            BsTriShapeKind::SubIndex => "BSSubIndexTriShape",
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
/// See nif.xml `VertexAttribute` (lines 2077-2090).
const VF_VERTEX: u16 = 0x001;
const VF_UVS: u16 = 0x002;
const VF_NORMALS: u16 = 0x008;
const VF_TANGENTS: u16 = 0x010;
const VF_VERTEX_COLORS: u16 = 0x020;
const VF_SKINNED: u16 = 0x040;
const VF_EYE_DATA: u16 = 0x100;
/// GPU-instancing flag (bit 9). Defined for completeness with the
/// nif.xml schema; no shipped FO4 / FO76 / Starfield content sets this
/// today, but modded geometry can. The trailing skip at the end of the
/// per-vertex parse loop already absorbs whatever bytes the bit asks
/// for via `vertex_size_quads * 4`, so flagging the constant doesn't
/// change runtime behavior — it just closes a defense-in-depth gap
/// flagged by audit S1-02 and makes the constant set match the schema
/// for code-review clarity. Field-level extraction (when the bit
/// becomes load-bearing) is tracked separately at #336 alongside
/// VF_UVS_2 / VF_LAND_DATA. See #358.
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
        if data_size != 0 {
            let expected_data_size =
                (vertex_size_quads * num_vertices as usize * 4) + (num_triangles as usize * 6);
            if (data_size as usize) != expected_data_size {
                log::warn!(
                    "BSTriShape data_size mismatch: stored {} vs derived {} \
                     (vertex_size_quads={}, num_vertices={}, num_triangles={}) — \
                     vertex_desc / num_vertices / num_triangles may be misparsed",
                    data_size,
                    expected_data_size,
                    vertex_size_quads,
                    num_vertices,
                    num_triangles,
                );
            }
        }

        let nv_u32 = num_vertices as u32;
        // #388/#408 — bounds-check every file-driven count before allocation.
        let mut vertices = stream.allocate_vec(nv_u32)?;
        let mut uvs = stream.allocate_vec(nv_u32)?;
        let mut normals = stream.allocate_vec(nv_u32)?;
        let mut vertex_colors = stream.allocate_vec(nv_u32)?;
        let mut triangles: Vec<[u16; 3]> = stream.allocate_vec(num_triangles)?;
        let is_skinned = vertex_attrs & VF_SKINNED != 0;
        let mut bone_weights: Vec<[f32; 4]> = if is_skinned {
            stream.allocate_vec(nv_u32)?
        } else {
            Vec::new()
        };
        let mut bone_indices: Vec<[u8; 4]> = if is_skinned {
            stream.allocate_vec(nv_u32)?
        } else {
            Vec::new()
        };

        if data_size > 0 {
            let vertex_size_bytes = vertex_size_quads * 4;

            // Parse each vertex from the packed format.
            for _ in 0..num_vertices {
                let vert_start = stream.position();

                // Position: full-precision (3×f32 + f32) or half-precision (3×f16 + u16).
                // SSE (BSVER < 130): always full-precision.
                // FO4+ (BSVER >= 130): bit VF_FULL_PRECISION selects precision.
                if vertex_attrs & VF_VERTEX != 0 {
                    let full_precision =
                        stream.bsver() < 130 || vertex_attrs & VF_FULL_PRECISION != 0;
                    if full_precision {
                        let pos = stream.read_ni_point3()?;
                        vertices.push(pos);
                        let _bitangent_x_or_w = stream.read_f32_le()?;
                    } else {
                        // Half-float positions (FO4 default)
                        let x = half_to_f32(stream.read_u16_le()?);
                        let y = half_to_f32(stream.read_u16_le()?);
                        let z = half_to_f32(stream.read_u16_le()?);
                        vertices.push(NiPoint3 { x, y, z });
                        let _bitangent_x_or_w = stream.read_u16_le()?;
                    }
                }

                // UV (HalfTexCoord = 2 × f16)
                if vertex_attrs & VF_UVS != 0 {
                    let u = half_to_f32(stream.read_u16_le()?);
                    let v = half_to_f32(stream.read_u16_le()?);
                    uvs.push([u, v]);
                }

                // Normal (ByteVector3 = 3 × u8 + bitangent Y as i8)
                if vertex_attrs & VF_NORMALS != 0 {
                    let nx = byte_to_normal(stream.read_u8()?);
                    let ny = byte_to_normal(stream.read_u8()?);
                    let nz = byte_to_normal(stream.read_u8()?);
                    let _bitangent_y = stream.read_u8()?;
                    normals.push(NiPoint3 {
                        x: nx,
                        y: ny,
                        z: nz,
                    });
                }

                // Tangent (ByteVector3 + bitangent Z)
                if vertex_attrs & VF_TANGENTS != 0 && vertex_attrs & VF_NORMALS != 0 {
                    stream.skip(4)?; // 3 bytes tangent + 1 byte bitangent Z
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

            // Triangle indices — bulk read 3 u16s per triangle.
            {
                let flat = stream.read_u16_array(num_triangles as usize * 3)?;
                for tri in flat.chunks_exact(3) {
                    triangles.push([tri[0], tri[1], tri[2]]);
                }
            }
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
            kind: BsTriShapeKind::Plain,
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
            let mut dynamic_vertices: Vec<NiPoint3> =
                stream.allocate_vec(dynamic_count as u32)?;
            for _ in 0..dynamic_count {
                let x = stream.read_f32_le()?;
                let y = stream.read_f32_le()?;
                let z = stream.read_f32_le()?;
                let _w = stream.read_f32_le()?; // bitangent-x or unused
                dynamic_vertices.push(NiPoint3 { x, y, z });
            }
            if !dynamic_vertices.is_empty() {
                shape.vertices = dynamic_vertices;
            }
        }
        shape.kind = BsTriShapeKind::Dynamic;
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
}

/// Read the 12-byte VF_SKINNED vertex extras: 4 × half-float weights
/// followed by 4 × u8 bone indices. Weights are stored as IEEE-754
/// half-floats per nif.xml BSVertexData_F / BSVertexDataSSE.
///
/// Exposed as a standalone helper so the skinning read can be unit-tested
/// without having to construct a full BSTriShape byte stream.
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
    Ok(([w0, w1, w2, w3], [i0, i1, i2, i3]))
}

/// Convert IEEE 754 half-precision float (u16) to f32.
fn half_to_f32(h: u16) -> f32 {
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

impl NiObject for NiTriShapeData {
    fn block_type_name(&self) -> &'static str {
        "NiTriShapeData"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
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
    if stream.version() >= NifVersion(0x0A010000) {
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
        if stream.variant().has_material_crc() {
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
    // explicit bool is only serialized through 4.0.0.2. For 4.0.0.3 onward
    // (Morrowind hybrid content, Oblivion, Gamebryo+) the presence of UV
    // data is derived from `num_uv_sets`: in the pre-Gamebryo branch this
    // came from the inline u16 at line 701-702, otherwise from
    // `data_flags & 0x3F`. See #325.
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
        let triangles = if has_triangles {
            let flat = stream.read_u16_array(num_triangles * 3)?;
            flat.chunks_exact(3)
                .map(|tri| [tri[0], tri[1], tri[2]])
                .collect()
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

impl NiObject for NiTriStripsData {
    fn block_type_name(&self) -> &'static str {
        "NiTriStripsData"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiTriStripsData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let (vertices, _data_flags, normals, center, radius, vertex_colors, uv_sets) =
            parse_geometry_data_base(stream)?;

        // NiTriBasedGeomData: num_triangles
        let num_triangles = stream.read_u16_le()?;

        // NiTriStripsData specific
        let num_strips = stream.read_u16_le()? as u32;
        stream.allocate_vec::<u16>(num_strips)?;
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
                // #388: `len` is a u16 read from the stream; the
                // `allocate_vec` budget check bounds the total.
                stream.allocate_vec::<u16>(len as u32)?;
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

#[cfg(test)]
mod skin_vertex_tests {
    use super::*;
    use crate::blocks::parse_block;
    use crate::header::NifHeader;
    use crate::version::NifVersion;

    fn test_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 12,
            user_version_2: 100, // Skyrim SE
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    /// Build a minimal valid Skyrim SE BSTriShape body with zero vertices
    /// and zero triangles. Used by the BSDynamicTriShape / BSLODTriShape
    /// dispatch regression tests (issue #157).
    fn minimal_bs_tri_shape_bytes() -> Vec<u8> {
        let mut d = Vec::new();
        // NiObjectNET: name=-1, extra_data count=0, controller=-1
        d.extend_from_slice(&(-1i32).to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&(-1i32).to_le_bytes());
        // NiAVObject (SSE, no properties): flags u32, transform, collision_ref
        d.extend_from_slice(&0u32.to_le_bytes()); // flags
                                                  // NiTransform: translation (3 f32) + rotation (9 f32) + scale (f32)
        for _ in 0..3 {
            d.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // Identity rotation
        for row in 0..3 {
            for col in 0..3 {
                let v: f32 = if row == col { 1.0 } else { 0.0 };
                d.extend_from_slice(&v.to_le_bytes());
            }
        }
        d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
        d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
                                                     // BSTriShape: center (3 f32) + radius + 3 refs + vertex_desc u64
        for _ in 0..3 {
            d.extend_from_slice(&0.0f32.to_le_bytes());
        }
        d.extend_from_slice(&0.0f32.to_le_bytes()); // radius
        d.extend_from_slice(&(-1i32).to_le_bytes()); // skin_ref
        d.extend_from_slice(&(-1i32).to_le_bytes()); // shader_property_ref
        d.extend_from_slice(&(-1i32).to_le_bytes()); // alpha_property_ref
        d.extend_from_slice(&0u64.to_le_bytes()); // vertex_desc (no attrs, stride 0)
                                                  // SSE (bsver<130): num_triangles as u16
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes()); // num_vertices
        d.extend_from_slice(&0u32.to_le_bytes()); // data_size — skip the vertex/tri loops
                                                  // SSE (bsver<130): particle_data_size is unconditional (#341).
        d.extend_from_slice(&0u32.to_le_bytes());
        d
    }

    /// Regression: #359 — a BSTriShape whose stored `data_size`
    /// disagrees with the value derived from `vertex_size_quads ·
    /// num_vertices · 4 + num_triangles · 6` must still parse
    /// successfully (no hard fail). The mismatch fires a `log::warn!`
    /// that's visible in `nif_stats` runs and would have caught audit
    /// findings S1-01 (FO76 Bound Min Max slip) and S5-01
    /// (BSDynamicTriShape misalignment) before manual inspection.
    /// Don't hard-fail — some shipped FO4 content has non-standard
    /// padding in this field.
    #[test]
    fn bs_tri_shape_with_mismatched_data_size_still_parses() {
        let header = test_header();
        // Patch the minimal-helper bytes: replace data_size = 0 with
        // a deliberately wrong non-zero value. With num_vertices = 0
        // and num_triangles = 0 the derived value is 0, so any
        // nonzero stored value triggers the mismatch warning.
        // Helper layout (see minimal_bs_tri_shape_bytes): NiObjectNET(12)
        // + flags(4) + transform(52) + collision_ref(4) + center(12)
        // + radius(4) + 3 refs(12) + vertex_desc(8) + num_triangles(2)
        // + num_vertices(2) = 112 bytes before data_size.
        let mut bytes = minimal_bs_tri_shape_bytes();
        let data_size_offset = 112;
        bytes[data_size_offset..data_size_offset + 4]
            .copy_from_slice(&999u32.to_le_bytes());
        // Length unchanged, no trailing data needed because
        // num_vertices == num_triangles == 0 → no vertex/triangle
        // arrays are read regardless of `data_size` value.

        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let shape = parse_block("BSTriShape", &mut stream, Some(bytes.len() as u32))
            .expect("data_size mismatch must NOT hard-fail the parse");
        assert!(shape.as_any().downcast_ref::<BsTriShape>().is_some());
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "trailing bytes should still be consumed cleanly even when \
             data_size disagrees with the derived value"
        );
    }

    /// Regression: #341 — when `data_size == 0` (the BSDynamicTriShape case
    /// for facegen heads — real positions live in the trailing dynamic
    /// Vector4 array), the SSE `particle_data_size` u32 must still be
    /// consumed unconditionally. Previously the read was nested inside
    /// `if data_size > 0`, misaligning `parse_dynamic` by 4 bytes so it
    /// read `vertex_data_size`/`unknown` from the wrong offsets, dropped
    /// every NPC head, and spammed 21,140 "expected N consumed 124"
    /// warnings on a Skyrim - Meshes0.bsa scan.
    #[test]
    fn bs_dynamic_tri_shape_with_zero_data_size_imports_dynamic_vertices() {
        let header = test_header();
        let mut bytes = minimal_bs_tri_shape_bytes();
        // BSDynamicTriShape trailing for 2 dynamic vertices:
        //   dynamic_data_size = 2 * 16 = 32, then 2 × Vector4 (x, y, z, w).
        // Per nif.xml the dynamic-vertex count is `dynamic_data_size / 16`
        // — independent of the base BSTriShape `num_vertices` — so we
        // don't need to patch that field here.
        let dyn_verts: [[f32; 4]; 2] = [[1.0, 2.0, 3.0, 0.0], [4.0, 5.0, 6.0, 0.0]];
        bytes.extend_from_slice(&32u32.to_le_bytes()); // dynamic_data_size
        for v in &dyn_verts {
            for f in v {
                bytes.extend_from_slice(&f.to_le_bytes());
            }
        }

        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let block = parse_block("BSDynamicTriShape", &mut stream, Some(bytes.len() as u32))
            .expect("BSDynamicTriShape with data_size==0 should parse");
        let shape = block
            .as_any()
            .downcast_ref::<BsTriShape>()
            .expect("BSDynamicTriShape did not downcast to BsTriShape");
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "BSDynamicTriShape (#341): trailing bytes not fully consumed — \
             SSE particle_data_size was probably misaligned again"
        );
        assert_eq!(
            shape.vertices.len(),
            2,
            "dynamic_vertices override should populate shape.vertices"
        );
        assert!((shape.vertices[0].x - 1.0).abs() < 1e-6);
        assert!((shape.vertices[1].x - 4.0).abs() < 1e-6);
    }

    /// Regression: #157 — BSDynamicTriShape must dispatch to the Dynamic
    /// parser and consume its trailing `vertex_data_size` + `unknown`
    /// header (even when zero-sized). Previously routed to NiUnknown,
    /// making every Skyrim NPC face invisible.
    #[test]
    fn bs_dynamic_tri_shape_dispatches_and_consumes_trailing_bytes() {
        let header = test_header();
        let mut bytes = minimal_bs_tri_shape_bytes();
        // BSDynamicTriShape trailing: dynamic_data_size=0 (#341 — the
        // bogus `_unknown` u32 was removed; nif.xml only specifies one
        // u32 between the BSTriShape body and the Vector4 array).
        bytes.extend_from_slice(&0u32.to_le_bytes());

        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let block = parse_block("BSDynamicTriShape", &mut stream, Some(bytes.len() as u32))
            .expect("BSDynamicTriShape should dispatch through BsTriShape::parse_dynamic");
        assert!(
            block.as_any().downcast_ref::<BsTriShape>().is_some(),
            "BSDynamicTriShape did not downcast to BsTriShape"
        );
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "BSDynamicTriShape trailing extras not fully consumed"
        );
    }

    /// FO76 header — BSVER 155. `BS_F76` condition in nif.xml gates the
    /// 24-byte `Bound Min Max` AABB between the bounding sphere and the
    /// skin ref on BSTriShape. See #342.
    fn fo76_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 12,
            user_version_2: 155, // Fallout 76 — BS_F76
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    /// Build a minimal valid FO76 BSTriShape body with a non-zero
    /// `Bound Min Max` payload. Reads `num_triangles` as u32 (BSVER
    /// >= 130) and omits `particle_data_size` (BS_SSE only). Used by
    /// the S1-01 / #342 regression test.
    fn minimal_fo76_bs_tri_shape_bytes() -> Vec<u8> {
        let mut d = Vec::new();
        // NiObjectNET: name=-1, extra_data count=0, controller=-1
        d.extend_from_slice(&(-1i32).to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&(-1i32).to_le_bytes());
        // NiAVObject (no properties): flags u32, transform, collision_ref
        d.extend_from_slice(&0u32.to_le_bytes()); // flags
        for _ in 0..3 {
            d.extend_from_slice(&0.0f32.to_le_bytes());
        }
        for row in 0..3 {
            for col in 0..3 {
                let v: f32 = if row == col { 1.0 } else { 0.0 };
                d.extend_from_slice(&v.to_le_bytes());
            }
        }
        d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
        d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
        // BSTriShape: center (3 f32) + radius + Bound Min Max (6 f32, F76)
        for _ in 0..3 {
            d.extend_from_slice(&0.0f32.to_le_bytes()); // center
        }
        d.extend_from_slice(&0.0f32.to_le_bytes()); // radius
        // #342 — Bound Min Max payload. Non-zero so a regression that
        // skipped past it (or still consumed it as skin_ref) would
        // produce a wildly wrong BlockRef index and fail the test's
        // skin_ref / shader_ref / alpha_ref assertions.
        for v in [-1.0f32, -2.0, -3.0, 4.0, 5.0, 6.0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        // Refs — distinct sentinel values so a byte-slip shows up
        // immediately in the assertions.
        d.extend_from_slice(&7i32.to_le_bytes()); // skin_ref
        d.extend_from_slice(&8i32.to_le_bytes()); // shader_property_ref
        d.extend_from_slice(&9i32.to_le_bytes()); // alpha_property_ref
        d.extend_from_slice(&0u64.to_le_bytes()); // vertex_desc
        // FO76 (BSVER >= 130): num_triangles as u32
        d.extend_from_slice(&0u32.to_le_bytes()); // num_triangles
        d.extend_from_slice(&0u16.to_le_bytes()); // num_vertices
        d.extend_from_slice(&0u32.to_le_bytes()); // data_size
        // BS_SSE-only particle_data_size is NOT present on FO76.
        d
    }

    /// Regression: #342 (S1-01) — FO76 BSTriShape must skip the 24-byte
    /// `Bound Min Max` AABB between the bounding sphere and the skin
    /// ref. Pre-fix every FO76 BSTriShape mis-parsed skin_ref,
    /// shader_property_ref, alpha_property_ref, and vertex_desc by 24
    /// bytes; per-block `block_size` realignment hid the slip from
    /// parse-rate metrics but every block's *contents* were wrong.
    #[test]
    fn bs_tri_shape_fo76_consumes_bound_min_max() {
        let header = fo76_header();
        let bytes = minimal_fo76_bs_tri_shape_bytes();

        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let block = parse_block("BSTriShape", &mut stream, Some(bytes.len() as u32))
            .expect("BSTriShape on FO76 header should parse");
        let shape = block
            .as_any()
            .downcast_ref::<BsTriShape>()
            .expect("BSTriShape did not downcast");

        // The refs must resolve to the sentinel values we wrote into
        // the bytes. A 24-byte slip would shift skin_ref to
        // (-1.0f32 reinterpreted as u32) ≈ 0xBF800000, blowing past
        // any reasonable block index.
        assert_eq!(
            shape.skin_ref.index(),
            Some(7),
            "skin_ref misaligned — Bound Min Max was not consumed"
        );
        assert_eq!(
            shape.shader_property_ref.index(),
            Some(8),
            "shader_property_ref misaligned (#342 cascade)"
        );
        assert_eq!(
            shape.alpha_property_ref.index(),
            Some(9),
            "alpha_property_ref misaligned (#342 cascade)"
        );
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "FO76 BSTriShape must consume exactly the body (no trailing bytes)"
        );
    }

    /// Sibling — Skyrim SE (BSVER 100) must NOT consume the
    /// Bound Min Max bytes. The condition is strict equality on 155,
    /// so SkyrimSE / SkyrimLE / FO4 / Starfield stay at the pre-#342
    /// layout. Regression guard against a future `>= 155` or
    /// `>= 130` typo.
    #[test]
    fn bs_tri_shape_skyrim_sse_skips_no_bound_min_max() {
        let header = test_header(); // BSVER 100 (SSE)
        let bytes = minimal_bs_tri_shape_bytes();
        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        parse_block("BSTriShape", &mut stream, Some(bytes.len() as u32))
            .expect("SSE BSTriShape must still parse after the FO76 gate lands");
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "SSE body length unchanged — BSVER != 155 must not skip Bound Min Max"
        );
    }

    /// Sibling — Starfield (BSVER 172) also NOT affected. The pre-fix
    /// issue description called this out explicitly; test pins the
    /// boundary. Reuses `minimal_fo76_bs_tri_shape_bytes` (same FO4+
    /// layout: num_triangles u32, no particle_data_size) but patches
    /// BSVER to 172 and removes the 24-byte Bound Min Max payload —
    /// a strict-equality `BSVER == 155` gate must NOT fire here.
    #[test]
    fn bs_tri_shape_starfield_skips_no_bound_min_max() {
        let mut header = fo76_header();
        header.user_version_2 = 172;
        // Starfield body is identical to FO76 minus the Bound Min Max.
        // Build from the FO76 bytes and splice out the 24 bytes at the
        // Bound Min Max offset: NiObjectNET(12) + flags(4) + transform(52)
        // + collision_ref(4) + center(12) + radius(4) = 88 → Bound Min Max
        // occupies offsets 88..112.
        let mut sf = minimal_fo76_bs_tri_shape_bytes();
        sf.drain(88..112);
        let mut stream = crate::stream::NifStream::new(&sf, &header);
        parse_block("BSTriShape", &mut stream, Some(sf.len() as u32))
            .expect("Starfield BSTriShape must still parse after the FO76 gate lands");
        assert_eq!(
            stream.position() as usize,
            sf.len(),
            "Starfield body length unchanged — BSVER 172 != 155 must not skip Bound Min Max"
        );
    }

    /// FO3/FNV header — has_properties_list=true, no shader_alpha_refs.
    /// Used by the BSSegmentedTriShape regression test.
    fn fo3_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 11,
            user_version_2: 34, // Fallout 3 / NV
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    /// Build a minimal valid FO3/FNV NiTriShape body: zero materials,
    /// null data refs, identity transform. Used as the base for the
    /// BSSegmentedTriShape regression test.
    fn minimal_fo3_ni_tri_shape_bytes() -> Vec<u8> {
        let mut d = Vec::new();
        // NiObjectNET: name=-1, extra_data count=0, controller=-1
        d.extend_from_slice(&(-1i32).to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&(-1i32).to_le_bytes());
        // NiAVObject (FO3/FNV, bsver=34): flags u32, transform,
        // properties list (count=0, no entries), collision_ref
        d.extend_from_slice(&0u32.to_le_bytes()); // flags
        for _ in 0..3 {
            d.extend_from_slice(&0.0f32.to_le_bytes()); // translation
        }
        for row in 0..3 {
            for col in 0..3 {
                let v: f32 = if row == col { 1.0 } else { 0.0 };
                d.extend_from_slice(&v.to_le_bytes());
            }
        }
        d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
        d.extend_from_slice(&0u32.to_le_bytes()); // properties count
        d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
                                                     // NiTriShape: data_ref, skin_instance_ref, num_materials,
                                                     // active_material_index, dirty_flag (v >= 20.2.0.7).
        d.extend_from_slice(&(-1i32).to_le_bytes()); // data_ref
        d.extend_from_slice(&(-1i32).to_le_bytes()); // skin_instance_ref
        d.extend_from_slice(&0u32.to_le_bytes()); // num_materials
        d.extend_from_slice(&0u32.to_le_bytes()); // active_material_index
        d.push(0u8); // dirty_flag
                     // FO3/FNV has no shader_alpha_refs branch.
        d
    }

    /// Regression: #146 — BSSegmentedTriShape must dispatch to the
    /// segmented parser and consume its trailing `num_segments` (u32)
    /// + 9-byte segment records. Previously aliased to plain NiTriShape,
    /// dropping segment metadata and causing block-loop realignment
    /// warnings on every FO3/FNV/SkyrimLE body-part mesh.
    #[test]
    fn bs_segmented_tri_shape_dispatches_and_consumes_segment_table() {
        let header = fo3_header();
        let mut bytes = minimal_fo3_ni_tri_shape_bytes();
        // num_segments = 2 + two 9-byte segment records.
        bytes.extend_from_slice(&2u32.to_le_bytes());
        // Segment 0: flags=0x1, index=0, num_tris=10
        bytes.push(0x1);
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&10u32.to_le_bytes());
        // Segment 1: flags=0x2, index=10, num_tris=5
        bytes.push(0x2);
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(&5u32.to_le_bytes());

        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let block = parse_block("BSSegmentedTriShape", &mut stream, Some(bytes.len() as u32))
            .expect("BSSegmentedTriShape should dispatch through NiTriShape::parse_segmented");
        assert!(
            block.as_any().downcast_ref::<NiTriShape>().is_some(),
            "BSSegmentedTriShape did not downcast to NiTriShape"
        );
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "BSSegmentedTriShape segment table not fully consumed"
        );
    }

    /// Regression: #147 — BSMeshLODTriShape shares BSLODTriShape's
    /// 3-u32 LOD-size trailing layout. Previously dispatched to the
    /// plain BSTriShape arm, leaving 12 bytes unread and spamming the
    /// block-loop realignment warning.
    #[test]
    fn bs_mesh_lod_tri_shape_dispatches_and_consumes_trailing_bytes() {
        let header = test_header();
        let mut bytes = minimal_bs_tri_shape_bytes();
        // BSMeshLODTriShape trailing: 3 × u32 LOD sizes.
        bytes.extend_from_slice(&20u32.to_le_bytes());
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());

        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let block = parse_block("BSMeshLODTriShape", &mut stream, Some(bytes.len() as u32))
            .expect("BSMeshLODTriShape should dispatch through BsTriShape::parse_lod");
        assert!(
            block.as_any().downcast_ref::<BsTriShape>().is_some(),
            "BSMeshLODTriShape did not downcast to BsTriShape"
        );
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "BSMeshLODTriShape trailing LOD sizes not fully consumed"
        );
    }

    /// Regression: #147 — BSSubIndexTriShape carries a variable-size
    /// FO4+ segmentation block (num primitives + segment table + optional
    /// shared sub-segment data with SSF filename). The dispatch uses
    /// `block_size` to bound the skip rather than reimplementing the
    /// full variable layout, since segmentation is gameplay-only data.
    #[test]
    fn bs_sub_index_tri_shape_consumes_segmentation_via_block_size() {
        let header = test_header();
        let mut bytes = minimal_bs_tri_shape_bytes();
        // Simulate a 24-byte segmentation payload — doesn't need to be
        // semantically valid, only that block_size bounds the skip.
        let segmentation_bytes = [0xAAu8; 24];
        bytes.extend_from_slice(&segmentation_bytes);

        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let block = parse_block("BSSubIndexTriShape", &mut stream, Some(bytes.len() as u32))
            .expect("BSSubIndexTriShape should parse with block_size skip");
        assert!(
            block.as_any().downcast_ref::<BsTriShape>().is_some(),
            "BSSubIndexTriShape did not downcast to BsTriShape"
        );
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "BSSubIndexTriShape segmentation payload not fully consumed"
        );
    }

    /// Regression: #157 — BSLODTriShape must dispatch to the LOD parser
    /// and consume its 3 trailing LOD-size u32s. Previously routed to
    /// NiUnknown, breaking FO4 distant LOD.
    #[test]
    fn bs_lod_tri_shape_dispatches_and_consumes_trailing_bytes() {
        let header = test_header();
        let mut bytes = minimal_bs_tri_shape_bytes();
        // BSLODTriShape trailing: 3 × u32 LOD sizes.
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(&5u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());

        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let block = parse_block("BSLODTriShape", &mut stream, Some(bytes.len() as u32))
            .expect("BSLODTriShape should dispatch through BsTriShape::parse_lod");
        assert!(
            block.as_any().downcast_ref::<BsTriShape>().is_some(),
            "BSLODTriShape did not downcast to BsTriShape"
        );
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "BSLODTriShape trailing LOD sizes not fully consumed"
        );
    }

    /// Regression: #560 — each wire-distinct BsTriShape subtype must stamp
    /// the matching `kind` discriminator and report its original type name
    /// via `block_type_name()`. Pre-fix every variant returned
    /// `"BSTriShape"` and downstream consumers (facegen head detection,
    /// distant-LOD batch importer, dismember segmentation) could not tell
    /// a head from a static prop from a segmented body from a LOD shell.
    #[test]
    fn bs_tri_shape_variants_stamp_their_kind() {
        let header = test_header();

        // 1. Plain BSTriShape → Plain.
        {
            let bytes = minimal_bs_tri_shape_bytes();
            let mut stream = crate::stream::NifStream::new(&bytes, &header);
            let block = parse_block("BSTriShape", &mut stream, Some(bytes.len() as u32)).unwrap();
            let shape = block.as_any().downcast_ref::<BsTriShape>().unwrap();
            assert_eq!(shape.kind, BsTriShapeKind::Plain);
            assert_eq!(block.block_type_name(), "BSTriShape");
        }

        // 2. BSLODTriShape → LOD { lod0, lod1, lod2 } (values preserved).
        {
            let mut bytes = minimal_bs_tri_shape_bytes();
            bytes.extend_from_slice(&10u32.to_le_bytes());
            bytes.extend_from_slice(&5u32.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            let mut stream = crate::stream::NifStream::new(&bytes, &header);
            let block =
                parse_block("BSLODTriShape", &mut stream, Some(bytes.len() as u32)).unwrap();
            let shape = block.as_any().downcast_ref::<BsTriShape>().unwrap();
            assert_eq!(
                shape.kind,
                BsTriShapeKind::LOD {
                    lod0: 10,
                    lod1: 5,
                    lod2: 1,
                }
            );
            assert_eq!(block.block_type_name(), "BSLODTriShape");
        }

        // 3. BSMeshLODTriShape → MeshLOD (same wire format as LOD but
        //    different kind so importers can branch — Skyrim SE DLC
        //    doesn't consult the cutoffs).
        {
            let mut bytes = minimal_bs_tri_shape_bytes();
            bytes.extend_from_slice(&10u32.to_le_bytes());
            bytes.extend_from_slice(&5u32.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            let mut stream = crate::stream::NifStream::new(&bytes, &header);
            let block =
                parse_block("BSMeshLODTriShape", &mut stream, Some(bytes.len() as u32)).unwrap();
            let shape = block.as_any().downcast_ref::<BsTriShape>().unwrap();
            assert_eq!(shape.kind, BsTriShapeKind::MeshLOD);
            assert_eq!(block.block_type_name(), "BSMeshLODTriShape");
        }

        // 4. BSSubIndexTriShape → SubIndex. Trailing segmentation bytes
        //    are bounded by block_size; we append 16 zero-padding bytes
        //    to simulate a minimal FO4 segmentation table.
        {
            let mut bytes = minimal_bs_tri_shape_bytes();
            bytes.extend_from_slice(&[0u8; 16]);
            let mut stream = crate::stream::NifStream::new(&bytes, &header);
            let block =
                parse_block("BSSubIndexTriShape", &mut stream, Some(bytes.len() as u32)).unwrap();
            let shape = block.as_any().downcast_ref::<BsTriShape>().unwrap();
            assert_eq!(shape.kind, BsTriShapeKind::SubIndex);
            assert_eq!(block.block_type_name(), "BSSubIndexTriShape");
        }

        // 5. BSDynamicTriShape → Dynamic. Append dynamic_data_size=0 so
        //    the facegen-vertex loop runs zero iterations.
        {
            let mut bytes = minimal_bs_tri_shape_bytes();
            bytes.extend_from_slice(&0u32.to_le_bytes());
            let mut stream = crate::stream::NifStream::new(&bytes, &header);
            let block =
                parse_block("BSDynamicTriShape", &mut stream, Some(bytes.len() as u32)).unwrap();
            let shape = block.as_any().downcast_ref::<BsTriShape>().unwrap();
            assert_eq!(shape.kind, BsTriShapeKind::Dynamic);
            assert_eq!(block.block_type_name(), "BSDynamicTriShape");
        }
    }

    /// IEEE-754 half-float for 1.0 is 0x3C00; for 0.5 is 0x3800; for 0.0 is 0x0000.
    /// These are the constants the read_vertex_skin_data helper will decode.
    #[test]
    fn read_vertex_skin_data_weights_and_indices() {
        let header = test_header();
        let mut data = Vec::new();
        // Weights: 1.0, 0.5, 0.0, 0.0 as half-floats.
        data.extend_from_slice(&0x3C00u16.to_le_bytes()); // 1.0
        data.extend_from_slice(&0x3800u16.to_le_bytes()); // 0.5
        data.extend_from_slice(&0x0000u16.to_le_bytes()); // 0.0
        data.extend_from_slice(&0x0000u16.to_le_bytes()); // 0.0
                                                          // Indices: 0, 1, 0, 0
        data.extend_from_slice(&[0u8, 1, 0, 0]);

        let mut stream = NifStream::new(&data, &header);
        let (weights, indices) = read_vertex_skin_data(&mut stream).unwrap();

        assert!((weights[0] - 1.0).abs() < 1e-4);
        assert!((weights[1] - 0.5).abs() < 1e-4);
        assert_eq!(weights[2], 0.0);
        assert_eq!(weights[3], 0.0);
        assert_eq!(indices, [0, 1, 0, 0]);
        // All 12 bytes consumed.
        assert_eq!(stream.position() as usize, data.len());
    }

    #[test]
    fn read_vertex_skin_data_four_bones_normalized() {
        let header = test_header();
        let mut data = Vec::new();
        // Four equal weights of 0.25 as half-floats (0x3400).
        for _ in 0..4 {
            data.extend_from_slice(&0x3400u16.to_le_bytes());
        }
        // Four distinct bone indices.
        data.extend_from_slice(&[3u8, 7, 12, 42]);

        let mut stream = NifStream::new(&data, &header);
        let (weights, indices) = read_vertex_skin_data(&mut stream).unwrap();

        let sum: f32 = weights.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-3,
            "weights should sum to 1, got {}",
            sum
        );
        for w in &weights {
            assert!((w - 0.25).abs() < 1e-3);
        }
        assert_eq!(indices, [3, 7, 12, 42]);
    }
}

#[cfg(test)]
mod nigeometry_data_version_tests {
    use super::*;
    use crate::header::NifHeader;

    fn header_at(version: NifVersion) -> NifHeader {
        NifHeader {
            version,
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    /// Minimal NiGeometryData body with zero vertices/normals/UVs/colors.
    /// Each optional pair matches one nif.xml version gate:
    ///  - `include_group_id`      — `Group ID` (since 10.1.0.114, 4 B)
    ///  - `include_keep_compress` — `Keep/Compress Flags` (since 10.1.0.0, 2 B)
    ///  - `include_consistency`   — `Consistency Flags` (since 10.0.1.0, 2 B)
    fn nigeometry_data_bytes(
        include_group_id: bool,
        include_keep_compress: bool,
        include_consistency: bool,
    ) -> Vec<u8> {
        let mut d = Vec::new();
        if include_group_id {
            d.extend_from_slice(&0i32.to_le_bytes()); // group_id
        }
        // num_vertices = 0
        d.extend_from_slice(&0u16.to_le_bytes());
        if include_keep_compress {
            d.push(0u8); // keep_flags
            d.push(0u8); // compress_flags
        }
        d.push(0u8); // has_vertices = false
                     // data_flags (u16) — version >= 10.0.1.0 branch.
        d.extend_from_slice(&0u16.to_le_bytes());
        d.push(0u8); // has_normals = false
                     // bounding sphere: center(3 f32) + radius(f32)
        for _ in 0..3 {
            d.extend_from_slice(&0.0f32.to_le_bytes());
        }
        d.extend_from_slice(&0.0f32.to_le_bytes());
        d.push(0u8); // has_vertex_colors = false
                     // (data_flags=0 ⇒ num_uv_sets=0 ⇒ has_uv=false, no UV arrays)
        if include_consistency {
            d.extend_from_slice(&0u16.to_le_bytes()); // consistency_flags
        }
        d
    }

    /// Regression: #327 / audit N1-02 — at NIF 10.0.1.0 the parser must
    /// NOT consume `keep_flags` / `compress_flags`. Those fields
    /// appear only from 10.1.0.0 per nif.xml. Previously this branch
    /// stole 2 bytes from `has_vertices` + `data_flags`, corrupting
    /// every downstream field. With #326 applied, `Group ID` is also
    /// absent (since 10.1.0.114).
    #[test]
    fn nigeometry_data_at_10_0_1_0_skips_keep_compress_flags() {
        let header = header_at(NifVersion(0x0A000100)); // 10.0.1.0 — in the gap.
        let bytes = nigeometry_data_bytes(
            /*include_group_id=*/ false, /*include_keep_compress=*/ false,
            /*include_consistency=*/ true,
        );
        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let _ = parse_geometry_data_base(&mut stream)
            .expect("NiGeometryData base should parse at 10.0.1.0");
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "at 10.0.1.0 NiGeometryData must NOT consume group_id or keep/compress"
        );
    }

    /// At NIF 10.1.0.0 (the corrected keep/compress threshold) the 2
    /// flags bytes ARE consumed. `Group ID` is still absent — it only
    /// appears from 10.1.0.114.
    #[test]
    fn nigeometry_data_at_10_1_0_0_reads_keep_compress_flags() {
        let header = header_at(NifVersion(0x0A010000)); // 10.1.0.0 — threshold.
        let bytes = nigeometry_data_bytes(
            /*include_group_id=*/ false, /*include_keep_compress=*/ true,
            /*include_consistency=*/ true,
        );
        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let _ = parse_geometry_data_base(&mut stream)
            .expect("NiGeometryData base should parse at 10.1.0.0");
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "at 10.1.0.0 NiGeometryData MUST consume keep/compress flags"
        );
    }

    /// Regression: #326 / audit N1-01 — `Group ID` is only serialized
    /// from 10.1.0.114 onward per nif.xml. Previously read from 10.0.1.0,
    /// stealing 4 bytes in the [10.0.1.0, 10.1.0.114) window (non-Bethesda
    /// Gamebryo pre-Civ IV era).
    #[test]
    fn nigeometry_data_at_10_1_0_113_skips_group_id() {
        let header = header_at(NifVersion(0x0A010071)); // 10.1.0.113 — one below.
        let bytes = nigeometry_data_bytes(
            /*include_group_id=*/ false, /*include_keep_compress=*/ true,
            /*include_consistency=*/ true,
        );
        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let _ = parse_geometry_data_base(&mut stream)
            .expect("NiGeometryData base should parse at 10.1.0.113");
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "at 10.1.0.113 NiGeometryData must NOT consume group_id"
        );
    }

    /// Regression: #440 / FO3-5-01. Bethesda streams (BSVER > 0,
    /// version 20.2.0.7) interpret `dataFlags` as `BSGeometryDataFlags`,
    /// where bit 0 is a `Has UV` bool — exactly 0 or 1 UV sets. The
    /// non-Bethesda `NiGeometryDataFlags` decode (bits 0..5 = count)
    /// would read bits 1..5 as additional UV slots, over-reading N ×
    /// `num_vertices × 8` bytes. On a real FO3 FaceGen head
    /// (`headfemalefacegen.nif`, 1307 vertices, `data_flags = 0x1003`)
    /// the pre-fix decode asked for 3 UV sets and over-read enough to
    /// demote every FO3 NPC face to `NiUnknown`.
    #[test]
    fn bs_geometry_data_flags_decodes_has_uv_bit0_only() {
        // FO3/FNV header: NIF 20.2.0.7, user_version=11, bsver=34.
        let header = NifHeader {
            version: NifVersion(0x14020007),
            little_endian: true,
            user_version: 11,
            user_version_2: 34,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };
        // Build a minimal NiGeometryData body for 2 vertices, no normals,
        // no vcolor, 1 UV set, data_flags = 0x1003 (bits 0, 1, 12 set).
        let mut data = Vec::new();
        data.extend_from_slice(&0i32.to_le_bytes()); // group_id
        data.extend_from_slice(&2u16.to_le_bytes()); // num_vertices
        data.push(0u8); // keep_flags
        data.push(0u8); // compress_flags
        data.push(1u8); // has_vertices
                         // Two vertices, 12 bytes each.
        for _ in 0..2 {
            for _ in 0..3 {
                data.extend_from_slice(&0.0f32.to_le_bytes());
            }
        }
        // data_flags: bit 0 = HasUV, bit 1 = unused noise, bit 12 = tangents
        data.extend_from_slice(&0x1003u16.to_le_bytes());
        data.push(0u8); // has_normals = false (no NBT payload to read)
                         // bounding sphere
        for _ in 0..3 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.push(0u8); // has_vertex_colors = false
                         // Exactly 1 UV set (per BS decode) × 2 vertices × 8 bytes = 16 bytes
        for _ in 0..2 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        data.extend_from_slice(&0u16.to_le_bytes()); // consistency
        data.extend_from_slice(&(-1i32).to_le_bytes()); // additional_data_ref
        let expected_len = data.len();

        let mut stream = crate::stream::NifStream::new(&data, &header);
        let (verts, flags, _norms, _c, _r, _vc, uvs) =
            parse_geometry_data_base(&mut stream)
                .expect("FO3 NiGeometryData should parse with BS data flag decode");
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "BS decode must consume exactly 1 UV set; got position {} expected {}",
            stream.position(),
            expected_len
        );
        assert_eq!(flags, 0x1003);
        assert_eq!(verts.len(), 2);
        assert_eq!(uvs.len(), 1, "BS decode: bit 0 = 1 UV set, bit 1 is noise");
    }

    /// Non-Bethesda Gamebryo streams (bsver = 0) keep the
    /// `NiGeometryDataFlags` decode where bits 0..5 encode a 6-bit
    /// count. `data_flags = 0x0003` must still mean 3 UV sets on that
    /// path — the BS fix must not break vanilla Gamebryo content.
    #[test]
    fn ni_geometry_data_flags_decodes_count_on_non_bethesda() {
        let header = NifHeader {
            version: NifVersion(0x14020007),
            little_endian: true,
            user_version: 0,
            user_version_2: 0, // bsver=0 → NiGeometryDataFlags path
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };
        let mut data = Vec::new();
        data.extend_from_slice(&0i32.to_le_bytes()); // group_id
        data.extend_from_slice(&1u16.to_le_bytes()); // num_vertices = 1
        data.push(0u8); // keep
        data.push(0u8); // compress
        data.push(1u8); // has_vertices
        for _ in 0..3 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // data_flags = 3 → NiGeometryDataFlags count = 3 UV sets
        data.extend_from_slice(&0x0003u16.to_le_bytes());
        data.push(0u8); // has_normals
        for _ in 0..3 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.push(0u8); // has_vertex_colors
                         // 3 UV sets × 1 vertex × 8 bytes = 24
        for _ in 0..3 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        data.extend_from_slice(&0u16.to_le_bytes()); // consistency
        data.extend_from_slice(&(-1i32).to_le_bytes()); // additional_data

        let mut stream = crate::stream::NifStream::new(&data, &header);
        let (_v, _f, _n, _c, _r, _vc, uvs) = parse_geometry_data_base(&mut stream)
            .expect("non-Bethesda NiGeometryData should parse with count decode");
        assert_eq!(uvs.len(), 3, "non-Bethesda: bits 0..5 encode UV count");
    }

    /// Dual-side for #326: at 10.1.0.114 the `group_id` i32 IS consumed.
    #[test]
    fn nigeometry_data_at_10_1_0_114_reads_group_id() {
        let header = header_at(NifVersion(0x0A010072)); // 10.1.0.114 — threshold.
        let bytes = nigeometry_data_bytes(
            /*include_group_id=*/ true, /*include_keep_compress=*/ true,
            /*include_consistency=*/ true,
        );
        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let _ = parse_geometry_data_base(&mut stream)
            .expect("NiGeometryData base should parse at 10.1.0.114");
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "at 10.1.0.114 NiGeometryData MUST consume group_id"
        );
    }
}
