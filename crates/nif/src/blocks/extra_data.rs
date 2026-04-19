//! NiExtraData — generic extra data blocks.
//!
//! These carry metadata (BSXFlags, names, integers, binary blobs).
//! We parse the most common ones and skip unknown subtypes.

use super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use crate::version::NifVersion;
use std::any::Any;
use std::io;
use std::sync::Arc;

/// Generic extra data — covers NiStringExtraData, NiIntegerExtraData, etc.
#[derive(Debug)]
pub struct NiExtraData {
    pub type_name: String,
    pub name: Option<Arc<str>>,
    pub string_value: Option<Arc<str>>,
    pub integer_value: Option<u32>,
    pub binary_data: Option<Vec<u8>>,
    /// Populated for `NiStringsExtraData` — array of string table entries
    /// carrying e.g. material override lists.
    pub strings_array: Option<Vec<Option<Arc<str>>>>,
    /// Populated for `NiIntegersExtraData` — array of 32-bit integers.
    pub integers_array: Option<Vec<u32>>,
}

impl NiObject for NiExtraData {
    fn block_type_name(&self) -> &'static str {
        "NiExtraData"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiExtraData {
    pub fn parse(stream: &mut NifStream, type_name: &str) -> io::Result<Self> {
        // Three branches per nif.xml:
        //  - v <= 4.2.2.0: linked-list format with `Next Extra Data`
        //    (until=4.2.2.0) + `Num Bytes` (since=4.0.0.0, until=4.2.2.0)
        //    + subclass body. No `Name` field.
        //  - v in (4.2.2.0, 10.0.1.0): neither `Next Extra Data` nor
        //    `Num Bytes` is serialized; `Name` arrives at 10.0.1.0.
        //    Just read the subclass body. Fixes N1-06 / #330 — pre-fix
        //    `parse_legacy` claimed this entire window and consumed
        //    phantom ref + length bytes on every extra-data block.
        //  - v >= 10.0.1.0: inherits NiObjectNET's Name field
        //    (string-table at 20.1.0.1+, inline length-prefixed earlier).
        if stream.version() <= NifVersion(0x04020200) {
            return Self::parse_legacy(stream, type_name);
        }
        if stream.version() < NifVersion(0x0A000100) {
            return Self::parse_gap(stream, type_name);
        }

        let name = stream.read_string()?;

        let mut string_value = None;
        let mut integer_value = None;
        let mut binary_data = None;
        let mut strings_array = None;
        let mut integers_array = None;

        match type_name {
            "NiStringExtraData" => {
                string_value = stream.read_string()?;
            }
            "NiIntegerExtraData" | "BSXFlags" => {
                integer_value = Some(stream.read_u32_le()?);
            }
            "NiBooleanExtraData" => {
                // nif.xml: Boolean Data is type "byte" (1 byte), NOT u32.
                integer_value = Some(stream.read_u8()? as u32);
            }
            "NiBinaryExtraData" => {
                let size = stream.read_u32_le()? as usize;
                binary_data = Some(stream.read_bytes(size)?);
            }
            // Array variants — count (u32) followed by N items. See #164.
            "NiStringsExtraData" => {
                let count = stream.read_u32_le()?;
                let mut arr = stream.allocate_vec(count)?;
                for _ in 0..count {
                    arr.push(stream.read_string()?);
                }
                strings_array = Some(arr);
            }
            "NiIntegersExtraData" => {
                let count = stream.read_u32_le()?;
                let mut arr = stream.allocate_vec(count)?;
                for _ in 0..count {
                    arr.push(stream.read_u32_le()?);
                }
                integers_array = Some(arr);
            }
            _ => {
                // Unknown extra data subtype — can't skip without size
            }
        }

        Ok(Self {
            type_name: type_name.to_string(),
            name,
            string_value,
            integer_value,
            binary_data,
            strings_array,
            integers_array,
        })
    }

    /// Parse the (4.2.2.0, 10.0.1.0) gap-window variant: no linked-list
    /// ref, no bytes-remaining, no Name. Only the subclass body is on
    /// disk. See N1-06 / #330. Mirrors [`Self::parse_legacy`] modulo the
    /// two header fields the legacy branch pre-reads.
    fn parse_gap(stream: &mut NifStream, type_name: &str) -> io::Result<Self> {
        let mut string_value = None;
        let mut integer_value = None;

        match type_name {
            "NiStringExtraData" => {
                // Pre-10.0.1.0 variant drops the `bytes_remaining`
                // prefix, but the subclass still serializes its payload
                // as a sized string (inline u32 length + bytes).
                let s = stream.read_sized_string()?;
                string_value = Some(Arc::from(s.as_str()));
            }
            "NiIntegerExtraData" => {
                integer_value = Some(stream.read_u32_le()?);
            }
            _ => {
                // Unknown subtype in the gap window — we have no way to
                // advance past an arbitrary body because `Num Bytes`
                // only exists until 4.2.2.0. Leave the stream untouched
                // and let the outer parse loop reconcile via block_size
                // (or fall through to NiUnknown on pre-block_size
                // content). Same policy the modern branch applies.
            }
        }

        Ok(Self {
            type_name: type_name.to_string(),
            name: None,
            string_value,
            integer_value,
            binary_data: None,
            strings_array: None,
            integers_array: None,
        })
    }

    /// Parse pre-Gamebryo NiExtraData (v <= 4.2.2.0, Morrowind / early
    /// NetImmerse). Old format: next_extra_data_ref + bytes_remaining +
    /// subclass data. No NiObjectNET inheritance (no name field).
    fn parse_legacy(stream: &mut NifStream, type_name: &str) -> io::Result<Self> {
        let _next_extra_data_ref = stream.read_block_ref()?;
        let bytes_remaining = stream.read_u32_le()?;

        let mut string_value = None;
        let mut integer_value = None;

        match type_name {
            "NiStringExtraData" => {
                // Old NiStringExtraData: bytes_remaining includes the u32 length prefix.
                let s = stream.read_sized_string()?;
                string_value = Some(Arc::from(s.as_str()));
            }
            "NiIntegerExtraData" => {
                integer_value = Some(stream.read_u32_le()?);
            }
            _ => {
                // Unknown old extra data — skip bytes_remaining to stay aligned.
                if bytes_remaining > 0 {
                    stream.skip(bytes_remaining as u64)?;
                }
            }
        }

        Ok(Self {
            type_name: type_name.to_string(),
            name: None,
            string_value,
            integer_value,
            binary_data: None,
            strings_array: None,
            integers_array: None,
        })
    }
}

// ── BSBound ────────────────────────────────────────────────────────

/// BSBound — bounding box extra data (center + half-extents).
///
/// Attached to root nodes for object-level bounding volume queries.
#[derive(Debug)]
pub struct BsBound {
    pub name: Option<Arc<str>>,
    pub center: [f32; 3],
    pub dimensions: [f32; 3],
}

impl NiObject for BsBound {
    fn block_type_name(&self) -> &'static str {
        "BSBound"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsBound {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base: name — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        let center = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let dimensions = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        Ok(Self {
            name,
            center,
            dimensions,
        })
    }
}

// ── BSDecalPlacementVectorExtraData ────────────────────────────────

/// A block of decal placement vectors (points + normals).
#[derive(Debug)]
pub struct DecalVectorBlock {
    pub points: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
}

/// BSDecalPlacementVectorExtraData — decal projection data for placed decals.
///
/// Inherits NiFloatExtraData (NiExtraData + f32). Contains arrays of
/// point/normal pairs defining where decals are projected onto geometry.
#[derive(Debug)]
pub struct BsDecalPlacementVectorExtraData {
    pub name: Option<Arc<str>>,
    pub float_value: f32,
    pub vector_blocks: Vec<DecalVectorBlock>,
}

impl NiObject for BsDecalPlacementVectorExtraData {
    fn block_type_name(&self) -> &'static str {
        "BSDecalPlacementVectorExtraData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsDecalPlacementVectorExtraData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base: name — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        // NiFloatExtraData: float value
        let float_value = stream.read_f32_le()?;
        // BSDecalPlacementVectorExtraData: vector blocks
        let num_blocks = stream.read_u16_le()? as u32;
        let mut vector_blocks: Vec<DecalVectorBlock> = stream.allocate_vec(num_blocks)?;
        for _ in 0..num_blocks {
            let num_vectors = stream.read_u16_le()? as u32;
            let mut points: Vec<[f32; 3]> = stream.allocate_vec(num_vectors)?;
            for _ in 0..num_vectors {
                points.push([
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                ]);
            }
            let mut normals: Vec<[f32; 3]> = stream.allocate_vec(num_vectors)?;
            for _ in 0..num_vectors {
                normals.push([
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                ]);
            }
            vector_blocks.push(DecalVectorBlock { points, normals });
        }
        Ok(Self {
            name,
            float_value,
            vector_blocks,
        })
    }
}

// ── BSBehaviorGraphExtraData ───────────────────────────────────────

/// Behavior graph reference for Havok animation behavior files.
/// Present on characters and animated objects (Skyrim+).
#[derive(Debug)]
pub struct BsBehaviorGraphExtraData {
    pub name: Option<Arc<str>>,
    pub behaviour_graph_file: Option<Arc<str>>,
    pub controls_base_skeleton: bool,
}

impl NiObject for BsBehaviorGraphExtraData {
    fn block_type_name(&self) -> &'static str {
        "BSBehaviorGraphExtraData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsBehaviorGraphExtraData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        let behaviour_graph_file = stream.read_string()?;
        // nif.xml line 8192: `Controls Base Skeleton: bool`. Pre-#106 we
        // read 4 bytes (u32-as-bool), desyncing every Skyrim skeleton
        // NIF with a behavior-graph reference by 3 bytes. The version-
        // aware `read_bool` helper does the right thing for both pre-
        // and post-4.1.0.1 (= 1 byte everywhere Skyrim+ cares about).
        let controls_base_skeleton = stream.read_bool()?;
        Ok(Self {
            name,
            behaviour_graph_file,
            controls_base_skeleton,
        })
    }
}

// ── BSInvMarker ────────────────────────────────────────────────────

/// Inventory display marker — rotation and zoom for in-menu 3D preview.
/// Rotation values are radians × 1000 stored as u16.
#[derive(Debug)]
pub struct BsInvMarker {
    pub name: Option<Arc<str>>,
    pub rotation_x: u16,
    pub rotation_y: u16,
    pub rotation_z: u16,
    pub zoom: f32,
}

impl NiObject for BsInvMarker {
    fn block_type_name(&self) -> &'static str {
        "BSInvMarker"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsInvMarker {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        let rotation_x = stream.read_u16_le()?;
        let rotation_y = stream.read_u16_le()?;
        let rotation_z = stream.read_u16_le()?;
        let zoom = stream.read_f32_le()?;
        Ok(Self {
            name,
            rotation_x,
            rotation_y,
            rotation_z,
            zoom,
        })
    }
}

// ── BSClothExtraData ───────────────────────────────────────────────

/// Havok cloth simulation data (opaque binary blob). FO4+.
#[derive(Debug)]
pub struct BsClothExtraData {
    pub name: Option<Arc<str>>,
    pub data: Vec<u8>,
}

impl NiObject for BsClothExtraData {
    fn block_type_name(&self) -> &'static str {
        "BSClothExtraData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsClothExtraData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        let length = stream.read_u32_le()? as usize;
        let data = stream.read_bytes(length)?;
        Ok(Self { name, data })
    }
}

// ── BSConnectPoint::Parents ────────────────────────────────────────

/// Workshop connection point definition. FO4+.
#[derive(Debug)]
pub struct ConnectPointData {
    pub parent: String,
    pub name: String,
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
    pub scale: f32,
}

#[derive(Debug)]
pub struct BsConnectPointParents {
    pub name: Option<Arc<str>>,
    pub connect_points: Vec<ConnectPointData>,
}

impl NiObject for BsConnectPointParents {
    fn block_type_name(&self) -> &'static str {
        "BSConnectPoint::Parents"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsConnectPointParents {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        let count = stream.read_u32_le()?;
        let mut connect_points = stream.allocate_vec(count)?;
        for _ in 0..count {
            let parent = stream.read_sized_string()?;
            let cp_name = stream.read_sized_string()?;
            let rotation = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let translation = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let scale = stream.read_f32_le()?;
            connect_points.push(ConnectPointData {
                parent,
                name: cp_name,
                rotation,
                translation,
                scale,
            });
        }
        Ok(Self {
            name,
            connect_points,
        })
    }
}

// ── BSPackedCombined[Shared]GeomDataExtra ──────────────────────────

/// One placed instance inside a packed-combined batch — grayscale
/// tint, engine-space transform, and a bounding sphere. 72 bytes.
/// `size="72"` per nif.xml `BSPackedGeomDataCombined`.
#[derive(Debug, Clone)]
pub struct BsPackedGeomDataCombined {
    /// Per-instance tint / palette index (f32). Drives the
    /// grayscale_to_palette shader input for merged LOD dressing.
    pub grayscale_to_palette_scale: f32,
    pub transform: crate::types::NiTransform,
    /// Bounding sphere: `[cx, cy, cz, radius]`.
    pub bounding_sphere: [f32; 4],
}

impl BsPackedGeomDataCombined {
    fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let grayscale_to_palette_scale = stream.read_f32_le()?;
        let transform = stream.read_ni_transform()?;
        let cx = stream.read_f32_le()?;
        let cy = stream.read_f32_le()?;
        let cz = stream.read_f32_le()?;
        let radius = stream.read_f32_le()?;
        Ok(Self {
            grayscale_to_palette_scale,
            transform,
            bounding_sphere: [cx, cy, cz, radius],
        })
    }
}

/// One baked-geometry object inside a `BSPackedCombinedGeomDataExtra`.
/// Carries metadata (LOD counts/offsets, per-instance transforms) plus
/// the raw vertex and triangle bytes — they are retained as
/// `Vec<u8>` / `Vec<[u16; 3]>` so a downstream LOD importer can decode
/// them once the terrain-streaming milestone needs them. See #158 and
/// #365.
#[derive(Debug, Clone)]
pub struct BsPackedGeomData {
    pub num_verts: u32,
    pub lod_levels: u32,
    pub tri_count_lod0: u32,
    pub tri_offset_lod0: u32,
    pub tri_count_lod1: u32,
    pub tri_offset_lod1: u32,
    pub tri_count_lod2: u32,
    pub tri_offset_lod2: u32,
    pub combined: Vec<BsPackedGeomDataCombined>,
    pub vertex_desc: u64,
    /// Raw vertex bytes — `num_verts * vertex_stride(vertex_desc)`.
    /// Stored verbatim; the downstream importer decodes per-vertex via
    /// the same `vertex_desc` machinery `BsTriShape` already uses.
    pub vertex_data: Vec<u8>,
    /// Triangle indices for all LODs concatenated, in order
    /// LOD0 → LOD1 → LOD2. Each triangle is 3 u16s.
    pub triangles: Vec<[u16; 3]>,
}

/// One shared-geometry reference inside a
/// `BSPackedCombinedSharedGeomDataExtra`. The actual vertex/triangle
/// data lives in an external PSG/CSG file addressed by filename hash
/// + byte offset. 8 bytes.
#[derive(Debug, Clone, Copy)]
pub struct BsPackedGeomObject {
    /// BSCRC32 of the `.psg`/`.csg` filename (without extension).
    pub filename_hash: u32,
    /// Byte offset into the PSG/CSG blob where this object's geometry
    /// starts.
    pub data_offset: u32,
}

/// Shared-geometry metadata — identical header layout to
/// `BsPackedGeomData` but with the vertex and triangle arrays elided
/// (they live in the external PSG/CSG file).
#[derive(Debug, Clone)]
pub struct BsPackedSharedGeomData {
    pub num_verts: u32,
    pub lod_levels: u32,
    pub tri_count_lod0: u32,
    pub tri_offset_lod0: u32,
    pub tri_count_lod1: u32,
    pub tri_offset_lod1: u32,
    pub tri_count_lod2: u32,
    pub tri_offset_lod2: u32,
    pub combined: Vec<BsPackedGeomDataCombined>,
    pub vertex_desc: u64,
}

/// Two-variant payload: baked geometry is self-contained; shared
/// geometry defers to an external PSG/CSG file.
#[derive(Debug, Clone)]
pub enum BsPackedCombinedPayload {
    /// `BSPackedCombinedGeomDataExtra` — vertex/triangle data is
    /// baked into this NIF.
    Baked(Vec<BsPackedGeomData>),
    /// `BSPackedCombinedSharedGeomDataExtra` — vertex/triangle data
    /// is in a companion `.psg`/`.csg` file, addressed by hash +
    /// offset in each `BsPackedGeomObject`.
    Shared {
        objects: Vec<BsPackedGeomObject>,
        data: Vec<BsPackedSharedGeomData>,
    },
}

/// `BSPackedCombinedGeomDataExtra` and
/// `BSPackedCombinedSharedGeomDataExtra` — FO4+ distant-LOD merged
/// geometry batches attached to `BSMultiBoundNode` roots in cell LOD
/// NIFs. The two variants differ in whether vertex/triangle data is
/// baked into the NIF (`Baked`) or deferred to a PSG/CSG companion
/// file (`Shared`).
///
/// The full wire format is now parsed (issue #365 / regression of
/// #158). Downstream LOD rendering — reconstructing merged
/// BSTriShape-equivalent batches from the baked-geometry arrays — is
/// still future work (tied to the terrain-streaming milestone), but
/// the structural data is no longer silently skipped.
#[derive(Debug)]
pub struct BsPackedCombinedGeomDataExtra {
    /// Discriminator: `"BSPackedCombinedGeomDataExtra"` or
    /// `"BSPackedCombinedSharedGeomDataExtra"`.
    pub type_name: &'static str,
    pub name: Option<Arc<str>>,
    pub vertex_desc: u64,
    pub num_vertices: u32,
    pub num_triangles: u32,
    pub unknown_flags_1: u32,
    pub unknown_flags_2: u32,
    pub num_data: u32,
    pub payload: BsPackedCombinedPayload,
}

impl NiObject for BsPackedCombinedGeomDataExtra {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Extract the per-vertex stride in bytes from a BSVertexDesc bitfield.
///
/// The low nibble stores "size-in-quads" — multiply by 4 to get bytes.
/// Matches the formula used by `BsTriShape` in tri_shape.rs.
#[inline]
fn vertex_stride_from_desc(vertex_desc: u64) -> usize {
    ((vertex_desc & 0xF) as usize) * 4
}

impl BsPackedCombinedGeomDataExtra {
    /// Parse the full wire format. See the struct doc comment for
    /// variant differences.
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        // NiExtraData base — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        let vertex_desc = stream.read_u64_le()?;
        let num_vertices = stream.read_u32_le()?;
        let num_triangles = stream.read_u32_le()?;
        let unknown_flags_1 = stream.read_u32_le()?;
        let unknown_flags_2 = stream.read_u32_le()?;
        let num_data = stream.read_u32_le()?;

        let payload = if type_name == "BSPackedCombinedSharedGeomDataExtra" {
            // Shared variant: N × GeomObject (8 bytes each) then N ×
            // SharedGeomData (header-only, no vertex / triangle arrays).
            // #388: allocate_vec bounds the count against the stream
            // budget so a corrupt num_data can't OOM.
            let mut objects: Vec<BsPackedGeomObject> = stream.allocate_vec(num_data)?;
            for _ in 0..num_data {
                let filename_hash = stream.read_u32_le()?;
                let data_offset = stream.read_u32_le()?;
                objects.push(BsPackedGeomObject {
                    filename_hash,
                    data_offset,
                });
            }
            let mut data: Vec<BsPackedSharedGeomData> = stream.allocate_vec(num_data)?;
            for _ in 0..num_data {
                data.push(parse_shared_geom_data(stream)?);
            }
            BsPackedCombinedPayload::Shared { objects, data }
        } else {
            // Baked variant: N × BSPackedGeomData.
            let mut baked: Vec<BsPackedGeomData> = stream.allocate_vec(num_data)?;
            for _ in 0..num_data {
                baked.push(parse_baked_geom_data(stream)?);
            }
            BsPackedCombinedPayload::Baked(baked)
        };

        Ok(Self {
            type_name,
            name,
            vertex_desc,
            num_vertices,
            num_triangles,
            unknown_flags_1,
            unknown_flags_2,
            num_data,
            payload,
        })
    }
}

fn parse_common_geom_header(
    stream: &mut NifStream,
) -> io::Result<(
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    Vec<BsPackedGeomDataCombined>,
    u64,
)> {
    let num_verts = stream.read_u32_le()?;
    let lod_levels = stream.read_u32_le()?;
    let tri_count_lod0 = stream.read_u32_le()?;
    let tri_offset_lod0 = stream.read_u32_le()?;
    let tri_count_lod1 = stream.read_u32_le()?;
    let tri_offset_lod1 = stream.read_u32_le()?;
    let tri_count_lod2 = stream.read_u32_le()?;
    let tri_offset_lod2 = stream.read_u32_le()?;
    let num_combined = stream.read_u32_le()?;
    let mut combined: Vec<BsPackedGeomDataCombined> = stream.allocate_vec(num_combined)?;
    for _ in 0..num_combined {
        combined.push(BsPackedGeomDataCombined::parse(stream)?);
    }
    let vertex_desc = stream.read_u64_le()?;
    Ok((
        num_verts,
        lod_levels,
        tri_count_lod0,
        tri_offset_lod0,
        tri_count_lod1,
        tri_offset_lod1,
        tri_count_lod2,
        tri_offset_lod2,
        combined,
        vertex_desc,
    ))
}

fn parse_baked_geom_data(stream: &mut NifStream) -> io::Result<BsPackedGeomData> {
    let (
        num_verts,
        lod_levels,
        tri_count_lod0,
        tri_offset_lod0,
        tri_count_lod1,
        tri_offset_lod1,
        tri_count_lod2,
        tri_offset_lod2,
        combined,
        vertex_desc,
    ) = parse_common_geom_header(stream)?;

    let stride = vertex_stride_from_desc(vertex_desc);
    let vertex_bytes = (num_verts as usize).saturating_mul(stride);
    let vertex_data = stream.read_bytes(vertex_bytes)?;

    let total_triangles = tri_count_lod0
        .saturating_add(tri_count_lod1)
        .saturating_add(tri_count_lod2);
    let mut triangles: Vec<[u16; 3]> = stream.allocate_vec(total_triangles)?;
    for _ in 0..total_triangles {
        let a = stream.read_u16_le()?;
        let b = stream.read_u16_le()?;
        let c = stream.read_u16_le()?;
        triangles.push([a, b, c]);
    }

    Ok(BsPackedGeomData {
        num_verts,
        lod_levels,
        tri_count_lod0,
        tri_offset_lod0,
        tri_count_lod1,
        tri_offset_lod1,
        tri_count_lod2,
        tri_offset_lod2,
        combined,
        vertex_desc,
        vertex_data,
        triangles,
    })
}

fn parse_shared_geom_data(stream: &mut NifStream) -> io::Result<BsPackedSharedGeomData> {
    let (
        num_verts,
        lod_levels,
        tri_count_lod0,
        tri_offset_lod0,
        tri_count_lod1,
        tri_offset_lod1,
        tri_count_lod2,
        tri_offset_lod2,
        combined,
        vertex_desc,
    ) = parse_common_geom_header(stream)?;

    Ok(BsPackedSharedGeomData {
        num_verts,
        lod_levels,
        tri_count_lod0,
        tri_offset_lod0,
        tri_count_lod1,
        tri_offset_lod1,
        tri_count_lod2,
        tri_offset_lod2,
        combined,
        vertex_desc,
    })
}

// ── BSFurnitureMarker ──────────────────────────────────────────────

/// A single furniture marker position — where an actor sits, sleeps, or leans.
///
/// Wire layout is version-split: BSVER ≤ 34 (up to and including FO3/FNV)
/// uses orientation + 2 position refs; BSVER > 34 (Skyrim+) replaces them
/// with heading + animation type + entry properties. Per nif.xml FurniturePosition.
#[derive(Debug, Clone)]
pub struct FurniturePosition {
    pub offset: [f32; 3],
    /// Oblivion/FO3/FNV: orientation + ref1 + ref2. Skyrim+: heading + anim + entry.
    pub data: FurniturePositionData,
}

#[derive(Debug, Clone)]
pub enum FurniturePositionData {
    /// BSVER ≤ 34 (Oblivion, FO3, FNV).
    Legacy {
        orientation: u16,
        position_ref_1: u8,
        position_ref_2: u8,
    },
    /// BSVER > 34 (Skyrim, Skyrim SE, FO4).
    Modern {
        heading: f32,
        animation_type: u16,
        entry_properties: u16,
    },
}

/// BSFurnitureMarker — sitting/sleeping/leaning position list on furniture meshes.
/// Introduced in Oblivion (v20.0.0.5, BSVER=11).
#[derive(Debug)]
pub struct BsFurnitureMarker {
    pub type_name: &'static str,
    pub name: Option<Arc<str>>,
    pub positions: Vec<FurniturePosition>,
}

impl NiObject for BsFurnitureMarker {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsFurnitureMarker {
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        // NiExtraData base — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        let count = stream.read_u32_le()?;
        let legacy = stream.bsver() <= 34;
        let mut positions = stream.allocate_vec(count)?;
        for _ in 0..count {
            let offset = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let data = if legacy {
                FurniturePositionData::Legacy {
                    orientation: stream.read_u16_le()?,
                    position_ref_1: stream.read_u8()?,
                    position_ref_2: stream.read_u8()?,
                }
            } else {
                FurniturePositionData::Modern {
                    heading: stream.read_f32_le()?,
                    animation_type: stream.read_u16_le()?,
                    entry_properties: stream.read_u16_le()?,
                }
            };
            positions.push(FurniturePosition { offset, data });
        }
        Ok(Self {
            type_name,
            name,
            positions,
        })
    }
}

// ── BSConnectPoint::Children ───────────────────────────────────────

/// Workshop connection point child references. FO4+.
#[derive(Debug)]
pub struct BsConnectPointChildren {
    pub name: Option<Arc<str>>,
    pub skinned: bool,
    pub point_names: Vec<String>,
}

impl NiObject for BsConnectPointChildren {
    fn block_type_name(&self) -> &'static str {
        "BSConnectPoint::Children"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsConnectPointChildren {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        // nif.xml: `Skinned` is type `byte`, not `uint` — reading it as
        // a u32 over-consumes 3 bytes of the following `Num Connect
        // Points` count. See issue #108.
        let skinned = stream.read_u8()? != 0;
        let count = stream.read_u32_le()?;
        let mut point_names = stream.allocate_vec(count)?;
        for _ in 0..count {
            point_names.push(stream.read_sized_string()?);
        }
        Ok(Self {
            name,
            skinned,
            point_names,
        })
    }
}

// ── BSAnimNote / BSAnimNotes ──────────────────────────────────────────
//
// Bethesda IK hint blocks attached to `NiControllerSequence` via
// `anim_note_refs` / the singular `anim_notes` ref. Before #432 these
// blocks hit the `NiUnknown` fallback on every FO3/FNV/Skyrim/FO4 .kf
// file, which — combined with the per-block recovery seek — silently
// dropped the IK hints. Layout per `docs/legacy/nif.xml:6871-6891`:
//
// ```
// enum AnimNoteType : uint { 0 = INVALID, 1 = GRABIK, 2 = LOOKIK }
// BSAnimNote : NiObject {
//     Type  : AnimNoteType,
//     Time  : f32,
//     Arm   : u32   cond Type == 1  (GRABIK arm index)
//     Gain  : f32   cond Type == 2  (LOOKIK blend gain)
//     State : u32   cond Type == 2  (LOOKIK target state)
// }
// BSAnimNotes : NiObject {
//     Num Anim Notes : u16,
//     Anim Notes     : Vec<Ref<BSAnimNote>>,
// }
// ```
//
// Note: these are IK hints (grab-IK arm picking, look-IK target tracking),
// NOT the generic gameplay text events that `NiTextKeyExtraData` carries.
// Footsteps / weapon-impact / SFX triggers flow through `text_keys` as
// before.

/// Type of a [`BsAnimNote`] — matches the `AnimNoteType` enum in nif.xml.
/// Unknown numeric values preserve the raw u32 so the importer can
/// diagnose corrupted content without losing information.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimNoteType {
    Invalid,
    GrabIk,
    LookIk,
    Unknown(u32),
}

impl AnimNoteType {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => AnimNoteType::Invalid,
            1 => AnimNoteType::GrabIk,
            2 => AnimNoteType::LookIk,
            other => AnimNoteType::Unknown(other),
        }
    }
}

/// Single IK hint attached to an animation sequence. See the module
/// comment above for the nif.xml layout.
#[derive(Debug, Clone)]
pub struct BsAnimNote {
    pub kind: AnimNoteType,
    pub time: f32,
    /// GRABIK — arm index (0 = left, 1 = right per Bethesda convention).
    /// Present only when `kind == GrabIk`.
    pub arm: Option<u32>,
    /// LOOKIK — blend-in gain. Present only when `kind == LookIk`.
    pub gain: Option<f32>,
    /// LOOKIK — target state. Present only when `kind == LookIk`.
    pub state: Option<u32>,
}

impl NiObject for BsAnimNote {
    fn block_type_name(&self) -> &'static str {
        "BSAnimNote"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsAnimNote {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let raw_type = stream.read_u32_le()?;
        let time = stream.read_f32_le()?;
        let kind = AnimNoteType::from_u32(raw_type);
        let (arm, gain, state) = match kind {
            AnimNoteType::GrabIk => (Some(stream.read_u32_le()?), None, None),
            AnimNoteType::LookIk => {
                let gain = stream.read_f32_le()?;
                let state = stream.read_u32_le()?;
                (None, Some(gain), Some(state))
            }
            // Invalid / Unknown — no conditional tail.
            AnimNoteType::Invalid | AnimNoteType::Unknown(_) => (None, None, None),
        };
        Ok(Self {
            kind,
            time,
            arm,
            gain,
            state,
        })
    }
}

/// Collection of [`BsAnimNote`] refs — one per IK event in the sequence.
#[derive(Debug, Clone)]
pub struct BsAnimNotes {
    pub notes: Vec<BlockRef>,
}

impl NiObject for BsAnimNotes {
    fn block_type_name(&self) -> &'static str {
        "BSAnimNotes"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsAnimNotes {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let count = stream.read_u16_le()? as u32;
        let mut notes = stream.allocate_vec(count)?;
        for _ in 0..count {
            notes.push(stream.read_block_ref()?);
        }
        Ok(Self { notes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;
    use crate::version::NifVersion;

    fn oblivion_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_0_0_5,
            little_endian: true,
            user_version: 0,
            user_version_2: 11, // BSVER=11 for Oblivion
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    fn skyrim_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 12,
            user_version_2: 83, // BSVER=83 for Skyrim
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    #[test]
    fn bs_furniture_marker_oblivion() {
        // Oblivion wire layout: inline name (len=0 → None), u32 count,
        // then each position: vec3 offset + u16 orientation + u8 ref1 + u8 ref2.
        let header = oblivion_header();
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_le_bytes()); // inline string: empty
        data.extend_from_slice(&2u32.to_le_bytes()); // 2 positions
                                                     // Position 0
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&2.0f32.to_le_bytes());
        data.extend_from_slice(&3.0f32.to_le_bytes());
        data.extend_from_slice(&0x1234u16.to_le_bytes()); // orientation
        data.push(0x56u8); // ref1
        data.push(0x78u8); // ref2
                           // Position 1
        data.extend_from_slice(&4.0f32.to_le_bytes());
        data.extend_from_slice(&5.0f32.to_le_bytes());
        data.extend_from_slice(&6.0f32.to_le_bytes());
        data.extend_from_slice(&0x9abcu16.to_le_bytes());
        data.push(0xdeu8);
        data.push(0xefu8);

        let mut stream = NifStream::new(&data, &header);
        let marker = BsFurnitureMarker::parse(&mut stream, "BSFurnitureMarker").unwrap();

        assert_eq!(marker.type_name, "BSFurnitureMarker");
        assert_eq!(marker.positions.len(), 2);
        assert_eq!(marker.positions[0].offset, [1.0, 2.0, 3.0]);
        match marker.positions[0].data {
            FurniturePositionData::Legacy {
                orientation,
                position_ref_1,
                position_ref_2,
            } => {
                assert_eq!(orientation, 0x1234);
                assert_eq!(position_ref_1, 0x56);
                assert_eq!(position_ref_2, 0x78);
            }
            _ => panic!("expected Legacy variant for Oblivion (BSVER=11)"),
        }
        assert_eq!(stream.position() as usize, data.len());
    }

    #[test]
    fn bs_furniture_marker_skyrim() {
        // Skyrim wire layout: string-table name (-1 = None), u32 count,
        // then each position: vec3 offset + f32 heading + u16 anim + u16 entry.
        let header = skyrim_header();
        let mut data = Vec::new();
        data.extend_from_slice(&(-1i32).to_le_bytes()); // string table: None
        data.extend_from_slice(&1u32.to_le_bytes()); // 1 position
        data.extend_from_slice(&10.0f32.to_le_bytes());
        data.extend_from_slice(&20.0f32.to_le_bytes());
        data.extend_from_slice(&30.0f32.to_le_bytes());
        data.extend_from_slice(&1.5707964f32.to_le_bytes()); // heading ≈ π/2
        data.extend_from_slice(&1u16.to_le_bytes()); // AnimationType::Sit
        data.extend_from_slice(&0x0003u16.to_le_bytes()); // Entry: Front|Behind

        let mut stream = NifStream::new(&data, &header);
        let marker = BsFurnitureMarker::parse(&mut stream, "BSFurnitureMarkerNode").unwrap();

        assert_eq!(marker.type_name, "BSFurnitureMarkerNode");
        assert_eq!(marker.positions.len(), 1);
        assert_eq!(marker.positions[0].offset, [10.0, 20.0, 30.0]);
        match marker.positions[0].data {
            FurniturePositionData::Modern {
                heading,
                animation_type,
                entry_properties,
            } => {
                assert!((heading - 1.5707964).abs() < 1e-6);
                assert_eq!(animation_type, 1);
                assert_eq!(entry_properties, 0x0003);
            }
            _ => panic!("expected Modern variant for Skyrim (BSVER=83)"),
        }
        assert_eq!(stream.position() as usize, data.len());
    }

    /// Regression: #106 — `BSBehaviorGraphExtraData.Controls Base
    /// Skeleton` is a 1-byte bool per nif.xml line 8192. Pre-fix the
    /// parser read a u32 (4 bytes), desyncing every Skyrim skeleton
    /// NIF with a behavior-graph reference by 3 bytes. Block-size
    /// recovery realigned the next block, but the tail of every
    /// behavior-graph block was silently misread.
    #[test]
    fn behavior_graph_extra_data_reads_bool_as_one_byte_on_skyrim() {
        let header = skyrim_header();
        let mut data = Vec::new();
        // name: string-table index = -1 (None).
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // behaviour_graph_file: string-table index = -1 (None).
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // controls_base_skeleton: 1 byte (true).
        data.push(0x01u8);
        // Block end. Total: 4 + 4 + 1 = 9 bytes.
        assert_eq!(data.len(), 9);

        let mut stream = NifStream::new(&data, &header);
        let block = BsBehaviorGraphExtraData::parse(&mut stream).unwrap();
        assert!(block.controls_base_skeleton);
        // Critical assertion — the parser must consume EXACTLY 9 bytes,
        // not 12. Pre-fix we'd consume 4 + 4 + 4 = 12 and trip a
        // truncation EOF or read 3 bytes from the next block.
        assert_eq!(
            stream.position() as usize,
            data.len(),
            "BSBehaviorGraphExtraData must consume the bool as 1 byte on Skyrim"
        );
    }

    /// Sibling — the `false` case must also consume exactly 1 byte.
    #[test]
    fn behavior_graph_extra_data_reads_false_as_one_byte() {
        let header = skyrim_header();
        let mut data = Vec::new();
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.push(0x00u8); // false

        let mut stream = NifStream::new(&data, &header);
        let block = BsBehaviorGraphExtraData::parse(&mut stream).unwrap();
        assert!(!block.controls_base_skeleton);
        assert_eq!(stream.position() as usize, data.len());
    }

    // ── BSAnimNote / BSAnimNotes regression tests (#432) ──────────────
    //
    // Each test asserts the parser consumes exactly the right number of
    // bytes and produces the right typed payload. Exact consumption is
    // load-bearing for Oblivion's block-sizes-less recovery path — if we
    // under-read, the next block's start offset is wrong and the whole
    // file cascades into NiUnknown.

    #[test]
    fn bs_anim_note_invalid_consumes_type_plus_time() {
        let header = skyrim_header();
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_le_bytes()); // type = INVALID
        data.extend_from_slice(&1.25f32.to_le_bytes()); // time
        let mut stream = NifStream::new(&data, &header);
        let note = BsAnimNote::parse(&mut stream).unwrap();
        assert_eq!(note.kind, AnimNoteType::Invalid);
        assert_eq!(note.time, 1.25);
        assert!(note.arm.is_none() && note.gain.is_none() && note.state.is_none());
        assert_eq!(stream.position() as usize, 8, "INVALID reads only 8 bytes");
    }

    #[test]
    fn bs_anim_note_grabik_reads_arm_only() {
        let header = skyrim_header();
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_le_bytes()); // type = GRABIK
        data.extend_from_slice(&0.5f32.to_le_bytes()); // time
        data.extend_from_slice(&1u32.to_le_bytes()); // arm (right hand)
        let mut stream = NifStream::new(&data, &header);
        let note = BsAnimNote::parse(&mut stream).unwrap();
        assert_eq!(note.kind, AnimNoteType::GrabIk);
        assert_eq!(note.time, 0.5);
        assert_eq!(note.arm, Some(1));
        assert!(note.gain.is_none());
        assert!(note.state.is_none());
        assert_eq!(stream.position() as usize, 12, "GRABIK reads 4+4+4");
    }

    #[test]
    fn bs_anim_note_lookik_reads_gain_and_state() {
        let header = skyrim_header();
        let mut data = Vec::new();
        data.extend_from_slice(&2u32.to_le_bytes()); // type = LOOKIK
        data.extend_from_slice(&2.0f32.to_le_bytes()); // time
        data.extend_from_slice(&0.75f32.to_le_bytes()); // gain
        data.extend_from_slice(&3u32.to_le_bytes()); // state
        let mut stream = NifStream::new(&data, &header);
        let note = BsAnimNote::parse(&mut stream).unwrap();
        assert_eq!(note.kind, AnimNoteType::LookIk);
        assert_eq!(note.time, 2.0);
        assert_eq!(note.gain, Some(0.75));
        assert_eq!(note.state, Some(3));
        assert!(note.arm.is_none());
        assert_eq!(stream.position() as usize, 16, "LOOKIK reads 4+4+4+4");
    }

    #[test]
    fn bs_anim_note_unknown_type_is_preserved_and_stops_at_time() {
        // Bethesda occasionally ships out-of-range AnimNoteType values
        // on older content. The parser preserves the raw value and
        // stops reading — the conditional tail is only present for the
        // known enum values, not for the unknown ones.
        let header = skyrim_header();
        let mut data = Vec::new();
        data.extend_from_slice(&42u32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        let mut stream = NifStream::new(&data, &header);
        let note = BsAnimNote::parse(&mut stream).unwrap();
        assert_eq!(note.kind, AnimNoteType::Unknown(42));
        assert_eq!(stream.position() as usize, 8);
    }

    #[test]
    fn bs_anim_notes_parses_array_of_refs() {
        let header = skyrim_header();
        let mut data = Vec::new();
        data.extend_from_slice(&3u16.to_le_bytes()); // count = 3
        data.extend_from_slice(&10i32.to_le_bytes());
        data.extend_from_slice(&11i32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes()); // NULL ref
        let mut stream = NifStream::new(&data, &header);
        let notes = BsAnimNotes::parse(&mut stream).unwrap();
        assert_eq!(notes.notes.len(), 3);
        assert_eq!(notes.notes[0].index(), Some(10));
        assert_eq!(notes.notes[1].index(), Some(11));
        assert_eq!(notes.notes[2].index(), None);
        assert_eq!(
            stream.position() as usize,
            14,
            "2 bytes for count + 3 × 4 bytes for refs = 14"
        );
    }

    #[test]
    fn bs_anim_notes_zero_count_reads_only_header() {
        let header = skyrim_header();
        let data = 0u16.to_le_bytes();
        let mut stream = NifStream::new(&data, &header);
        let notes = BsAnimNotes::parse(&mut stream).unwrap();
        assert!(notes.notes.is_empty());
        assert_eq!(stream.position() as usize, 2);
    }

    /// Regression: #329. Pre-10.0.1.0 streams have no `Name` field on
    /// `NiExtraData` per nif.xml (`since="10.0.1.0"`). BsBound is only
    /// ever parsed via the subclass dispatcher on Bethesda content
    /// (which is >= 10.0.1.0), but a fuzzed or non-Bethesda file can
    /// hit these subclass parsers directly — `read_extra_data_name`
    /// must NOT consume any bytes on those streams.
    #[test]
    fn read_extra_data_name_returns_none_pre_10_0_1_0() {
        let header = NifHeader {
            version: NifVersion(0x0A000006), // 10.0.0.6 — just below the gate
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
        };
        // Body is 24 bytes of BsBound (center + dimensions); no name
        // prefix on this version per the gate.
        let mut data = Vec::new();
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut stream = NifStream::new(&data, &header);
        let bound = BsBound::parse(&mut stream).expect("pre-10.0.1.0 BsBound should parse");
        assert!(bound.name.is_none(), "pre-10.0.1.0 has no Name field");
        assert_eq!(bound.center, [1.0, 2.0, 3.0]);
        assert_eq!(bound.dimensions, [4.0, 5.0, 6.0]);
        assert_eq!(stream.position() as usize, data.len());
    }

    /// Regression: #330. Files in the NetImmerse→Gamebryo gap window
    /// (v ∈ (4.2.2.0, 10.0.1.0)) have neither `Next Extra Data` /
    /// `Num Bytes` (until 4.2.2.0) nor `Name` (since 10.0.1.0). Before
    /// the fix, `NiExtraData::parse` treated this entire range as
    /// `parse_legacy` and consumed phantom 8 bytes (ref + u32 length),
    /// misaligning every subsequent block.
    #[test]
    fn ni_extra_data_gap_window_reads_only_subclass_body() {
        let header = NifHeader {
            version: NifVersion(0x0A000006), // 10.0.0.6 — in the gap
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
        };
        // NiStringExtraData body only — no name, no next_ref, no
        // bytes_remaining. Just a sized-string payload.
        let mut data = Vec::new();
        data.extend_from_slice(&5u32.to_le_bytes()); // payload length
        data.extend_from_slice(b"hello");

        let mut stream = NifStream::new(&data, &header);
        let extra = NiExtraData::parse(&mut stream, "NiStringExtraData")
            .expect("gap-window NiExtraData should parse");
        assert!(extra.name.is_none());
        assert_eq!(extra.string_value.as_deref(), Some("hello"));
        assert_eq!(stream.position() as usize, data.len());
    }

    #[test]
    fn bs_anim_notes_malicious_count_errors_without_panic() {
        // Regression test for #408: a corrupt/malicious count must not OOM
        // via Vec::with_capacity. allocate_vec bounds count against
        // remaining bytes and returns an io::Error instead of panicking.
        let header = skyrim_header();
        let data = u16::MAX.to_le_bytes(); // count = 65535, zero body bytes
        let mut stream = NifStream::new(&data, &header);
        let err = BsAnimNotes::parse(&mut stream).expect_err("expected bounds error");
        let msg = err.to_string();
        assert!(
            msg.contains("bytes remain") || msg.contains("only"),
            "expected allocate_vec bounds error, got: {msg}"
        );
    }
}
