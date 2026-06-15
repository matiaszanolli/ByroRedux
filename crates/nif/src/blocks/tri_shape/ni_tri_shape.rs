//! NiTriShape, NiTriStrips, NiLodTriShape, NiTriShapeData, NiTriStripsData.
//!
//! Classic Gamebryo indexed-triangle geometry: a leaf `NiAVObject` (NiTriShape /
//! NiTriStrips / BSLODTriShape) references a separate data block holding the per-vertex
//! and per-triangle arrays. The data-block parsers share a `parse_geometry_data_base*`
//! prelude (NiGeometryData fields) which is also consumed by `NiPSysData` in
//! `blocks::particle`.
//!
//! Split out of the prior monolithic `blocks/tri_shape.rs` (TD9-005 / #1118).

use super::super::base::NiAVObjectData;
use super::super::{traits, NiObject};
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

        if stream.version() >= NifVersion::V20_2_0_5 {
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
            if stream.variant().has_shader_alpha_refs() {
                shader_property_ref = stream.read_block_ref()?;
                alpha_property_ref = stream.read_block_ref()?;
            }
        } else if stream.version() >= NifVersion::V10_0_1_0
            && stream.version() <= NifVersion::V20_1_0_3
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

/// Decoded NiGeometryData base-class fields:
/// `(vertices, data_flags, normals, center, radius, vertex_colors, uv_sets)`.
pub(crate) type GeometryDataBase = (
    Vec<NiPoint3>,      // vertices
    u16,                // data_flags
    Vec<NiPoint3>,      // normals
    NiPoint3,           // center
    f32,                // radius
    Vec<[f32; 4]>,      // vertex_colors
    Vec<Vec<[f32; 2]>>, // uv_sets
);

/// Parse the NiGeometryData base class fields shared by NiTriShapeData and NiTriStripsData.
/// Returns (vertices, data_flags, normals, center, radius, vertex_colors, uv_sets).
pub(crate) fn parse_geometry_data_base(stream: &mut NifStream) -> io::Result<GeometryDataBase> {
    parse_geometry_data_base_inner(stream, false)
}

/// Variant that treats the per-vertex arrays (positions, normals, tangents,
/// colors, UVs) as zero-length regardless of the `Has*` bools. Used by
/// NiPSysData on BS_GTE_FO3 streams where nif.xml (line 3880) says:
/// "Vertices, Normals, Tangents, Colors, and UV arrays do not have length
/// for NiPSysData regardless of 'Num' or booleans." See #322.
pub(crate) fn parse_psys_geometry_data_base(
    stream: &mut NifStream,
) -> io::Result<GeometryDataBase> {
    parse_geometry_data_base_inner(stream, true)
}

fn parse_geometry_data_base_inner(
    stream: &mut NifStream,
    zero_arrays: bool,
) -> io::Result<GeometryDataBase> {
    // Group ID: nif.xml says `since="10.1.0.114"` (0x0A010072), not
    // 10.0.1.0. Files in the [10.0.1.0, 10.1.0.114) range (non-Bethesda
    // Gamebryo, pre-Civ IV era) read 4 phantom bytes, misaligning every
    // NiGeometryData afterward. See #326 / audit N1-01.
    if stream.version() >= NifVersion::V10_1_0_114 {
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
    let data_flags = if stream.version() >= NifVersion::V10_0_1_0 {
        let df = stream.read_u16_le()?;
        // Query the file's bsver directly — `variant().has_material_crc()`
        // would return false for the BSVER 35..=82 `Unknown` gap. The
        // material CRC is authored from Skyrim onward per nif.xml's
        // `BSVER > 34` rule. See NIF-D2-NEW-07 (audit 2026-05-12).
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
    let num_uv_sets = if stream.version() < NifVersion::V10_0_1_0 {
        stream.read_u16_le()? as usize
    } else if stream.bsver() > crate::version::bsver::PRE_BETHESDA && stream.version() == NifVersion::V20_2_0_7 {
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
    let has_uv = if stream.version() <= NifVersion::V4_0_0_2 {
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
    if stream.version() >= NifVersion::V10_0_1_0 {
        let _consistency_flags = stream.read_u16_le()?;
    }

    // Additional data (version >= 20.0.0.4)
    if stream.version() >= NifVersion::V20_0_0_4 {
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

        // has_triangles bool: only present from v >= 10.0.1.3 (nif.xml
        // `Triangles` is unconditional `until="10.0.1.2"`; the cond-gated form is
        // `since="10.0.1.3"`. OpenMW data.cpp:182 reads the bool only for version >
        // VER_OB_OLD (= 10.0.1.2), confirming the bool is absent at v10.0.1.0/10.0.1.2.
        // Using >= V10_0_1_0 consumed a phantom byte at those versions, misaligning the
        // triangle list with no recovery (Oblivion has no block-size table). (#1301)
        let has_triangles = if stream.version() >= NifVersion::V10_0_1_3 {
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

        // has_strips: only from v >= 10.0.1.3 (nif.xml `Has Points` since="10.0.1.3";
        // confirmed by nifly StripsInfo::Sync and OpenMW NiTriStripsData::read).
        // Pre-10.0.1.3 (Morrowind / early-Gamebryo, incl. Oblivion v10.0.1.0/10.0.1.2)
        // the bool is absent on disk — using >= V10_0_1_0 consumed a phantom byte, shifting
        // the stream 1 byte and cascading into tail truncation (Oblivion has no block-size
        // table, so there is no recovery). The old wrong boundary was V10_0_1_0; nif.xml and
        // both reference impls agree on V10_0_1_3. (#1310)
        let has_strips = if stream.version() >= NifVersion::V10_0_1_3 {
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

impl_ni_object!(NiTriShapeData, NiTriStripsData,);

#[cfg(test)]
#[path = "../tri_shape_nigeometry_data_version_tests.rs"]
mod nigeometry_data_version_tests;
