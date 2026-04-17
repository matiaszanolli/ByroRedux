//! NiExtraData — generic extra data blocks.
//!
//! These carry metadata (BSXFlags, names, integers, binary blobs).
//! We parse the most common ones and skip unknown subtypes.

use super::NiObject;
use crate::stream::NifStream;
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
        // Pre-Gamebryo (v < 5.0.0.1): NiExtraData does NOT inherit NiObjectNET.
        // Format is: next_extra_data_ref (Ref) + bytes_remaining (u32) + subclass data.
        // Gamebryo+ (v >= 10.0.1.0): inherits NiObjectNET (name + extra_data_refs + controller).
        // We only need the name; the rest comes from the subclass match below.
        if stream.version() < NifVersion(0x0A000100) {
            return Self::parse_legacy(stream, type_name);
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
                let count = stream.read_u32_le()? as usize;
                let mut arr = Vec::with_capacity(count);
                for _ in 0..count {
                    arr.push(stream.read_string()?);
                }
                strings_array = Some(arr);
            }
            "NiIntegersExtraData" => {
                let count = stream.read_u32_le()? as usize;
                let mut arr = Vec::with_capacity(count);
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

    /// Parse pre-Gamebryo NiExtraData (v < 5.0.0.1, e.g. Morrowind).
    /// Old format: next_extra_data_ref + bytes_remaining + subclass data.
    /// No NiObjectNET inheritance (no name field).
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
        // NiExtraData base: name (string table or inline)
        let name = stream.read_string()?;
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
        // NiExtraData base: name
        let name = stream.read_string()?;
        // NiFloatExtraData: float value
        let float_value = stream.read_f32_le()?;
        // BSDecalPlacementVectorExtraData: vector blocks
        let num_blocks = stream.read_u16_le()? as usize;
        let mut vector_blocks = Vec::with_capacity(num_blocks);
        for _ in 0..num_blocks {
            let num_vectors = stream.read_u16_le()? as usize;
            let mut points = Vec::with_capacity(num_vectors);
            for _ in 0..num_vectors {
                points.push([
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                ]);
            }
            let mut normals = Vec::with_capacity(num_vectors);
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
        let name = stream.read_string()?;
        let behaviour_graph_file = stream.read_string()?;
        let controls_base_skeleton = stream.read_u32_le()? != 0;
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
        let name = stream.read_string()?;
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
        let name = stream.read_string()?;
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
        let name = stream.read_string()?;
        let count = stream.read_u32_le()? as usize;
        let mut connect_points = Vec::with_capacity(count);
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

/// Shared fields for `BSPackedCombinedGeomDataExtra` and
/// `BSPackedCombinedSharedGeomDataExtra` — FO4+ distant-LOD merged
/// geometry batches attached to BSMultiBoundNode roots in cell LOD NIFs.
///
/// Wire layout (niflib nif.xml):
/// ```text
/// NiExtraData base (name)
/// uint64  vertex_desc       ; BSVertexDesc bitfield
/// uint    num_vertices
/// uint    num_triangles
/// uint    unknown_flags_1
/// uint    unknown_flags_2
/// uint    num_data
/// <variable-size per-object data + vertex/triangle pools>
/// ```
///
/// The renderer does not yet reconstruct merged LOD meshes from this
/// data (distant terrain decoration is a downstream importer task that
/// lands with the terrain streaming milestone). This struct classifies
/// the block and captures the fixed-layout header so scene walkers can
/// identify it by type and name; the variable-size tail is skipped via
/// the caller-supplied `block_size`. See issue #158.
#[derive(Debug)]
pub struct BsPackedCombinedGeomDataExtra {
    /// Discriminator: `"BSPackedCombinedGeomDataExtra"` or
    /// `"BSPackedCombinedSharedGeomDataExtra"`. The two variants differ
    /// only in how the downstream engine assembles the object pool —
    /// on the wire they share the identical header layout.
    pub type_name: &'static str,
    pub name: Option<Arc<str>>,
    pub vertex_desc: u64,
    pub num_vertices: u32,
    pub num_triangles: u32,
    pub unknown_flags_1: u32,
    pub unknown_flags_2: u32,
    pub num_data: u32,
}

impl NiObject for BsPackedCombinedGeomDataExtra {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsPackedCombinedGeomDataExtra {
    /// Parse the NiExtraData name + fixed-layout geometry header.
    /// The variable-size tail (per-object records + vertex/triangle
    /// pools) is left on the stream — the dispatcher uses `block_size`
    /// to bound the skip, same pattern as BSSubIndexTriShape (#147).
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        let name = stream.read_string()?;
        let vertex_desc = stream.read_u64_le()?;
        let num_vertices = stream.read_u32_le()?;
        let num_triangles = stream.read_u32_le()?;
        let unknown_flags_1 = stream.read_u32_le()?;
        let unknown_flags_2 = stream.read_u32_le()?;
        let num_data = stream.read_u32_le()?;
        Ok(Self {
            type_name,
            name,
            vertex_desc,
            num_vertices,
            num_triangles,
            unknown_flags_1,
            unknown_flags_2,
            num_data,
        })
    }
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
        let name = stream.read_string()?;
        let count = stream.read_u32_le()? as usize;
        let legacy = stream.bsver() <= 34;
        let mut positions = Vec::with_capacity(count);
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
        let name = stream.read_string()?;
        // nif.xml: `Skinned` is type `byte`, not `uint` — reading it as
        // a u32 over-consumes 3 bytes of the following `Num Connect
        // Points` count. See issue #108.
        let skinned = stream.read_u8()? != 0;
        let count = stream.read_u32_le()? as usize;
        let mut point_names = Vec::with_capacity(count);
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
}
