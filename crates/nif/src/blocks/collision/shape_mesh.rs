//! Mesh-backed collision shapes.
//!
//! NiTriStrips, PackedNiTriStrips and their per-strip data types.

use crate::impl_ni_object;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::io;

use super::{read_havok_material, read_vec4};

/// bhkNiTriStripsShape — collision mesh referencing NiTriStripsData blocks.
#[derive(Debug)]
pub struct BhkNiTriStripsShape {
    pub material: u32,
    pub radius: f32,
    pub data_refs: Vec<BlockRef>,
    pub filters: Vec<u32>,
}


impl BhkNiTriStripsShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let material = read_havok_material(stream)?;
        let radius = stream.read_f32_le()?;
        stream.skip(20)?; // unused
        let _grow_by = stream.read_u32_le()?;
        // Scale: since 10.1.0.0 (always present for Oblivion+)
        let _scale = read_vec4(stream)?;
        let num_data = stream.read_u32_le()?;
        let mut data_refs = stream.allocate_vec(num_data)?;
        for _ in 0..num_data {
            data_refs.push(stream.read_block_ref()?);
        }
        // #981 — bulk-read filter u32 array.
        let num_filters = stream.read_u32_le()? as usize;
        let filters = stream.read_u32_array(num_filters)?;
        Ok(Self {
            material,
            radius,
            data_refs,
            filters,
        })
    }
}

/// bhkPackedNiTriStripsShape — packed triangle mesh with sub-shapes.
#[derive(Debug)]
pub struct BhkPackedNiTriStripsShape {
    pub sub_shapes: Vec<HkSubPartData>,
    pub data_ref: BlockRef,
    pub scale: [f32; 4],
}

/// Sub-shape info for packed tri strips.
#[derive(Debug)]
pub struct HkSubPartData {
    pub havok_filter: u32,
    pub num_vertices: u32,
    pub material: u32,
}


impl BhkPackedNiTriStripsShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let version = stream.version();
        let sub_shapes = if version <= crate::version::NifVersion::V20_0_0_5 {
            // Oblivion: sub-shapes inline (until="20.0.0.5")
            let count = stream.read_u16_le()? as u32;
            let mut subs = stream.allocate_vec(count)?;
            for _ in 0..count {
                let havok_filter = stream.read_u32_le()?;
                let num_vertices = stream.read_u32_le()?;
                let material = stream.read_u32_le()?;
                subs.push(HkSubPartData {
                    havok_filter,
                    num_vertices,
                    material,
                });
            }
            subs
        } else {
            // FO3+: sub-shapes are in hkPackedNiTriStripsData
            Vec::new()
        };

        let _user_data = stream.read_u32_le()?;
        stream.skip(4)?; // unused
        let _radius = stream.read_f32_le()?;
        stream.skip(4)?; // unused
        let scale = read_vec4(stream)?;
        let _radius_copy = stream.read_f32_le()?;
        let _scale_copy = read_vec4(stream)?;
        let data_ref = stream.read_block_ref()?;

        Ok(Self {
            sub_shapes,
            data_ref,
            scale,
        })
    }
}

/// hkPackedNiTriStripsData — packed triangle and vertex arrays.
#[derive(Debug)]
pub struct HkPackedNiTriStripsData {
    pub triangles: Vec<PackedTriangle>,
    pub vertices: Vec<[f32; 3]>,
}

/// A single triangle in packed collision data.
#[derive(Debug)]
pub struct PackedTriangle {
    pub v0: u16,
    pub v1: u16,
    pub v2: u16,
    pub welding_info: u16,
    pub normal: Option<[f32; 3]>,
}


impl HkPackedNiTriStripsData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let version = stream.version();
        let num_triangles = stream.read_u32_le()?;
        let mut triangles = stream.allocate_vec(num_triangles)?;
        for _ in 0..num_triangles {
            let v0 = stream.read_u16_le()?;
            let v1 = stream.read_u16_le()?;
            let v2 = stream.read_u16_le()?;
            let welding_info = stream.read_u16_le()?;
            // Oblivion (until="20.0.0.5"): normal as 3 floats
            let normal = if version <= crate::version::NifVersion::V20_0_0_5 {
                let nx = stream.read_f32_le()?;
                let ny = stream.read_f32_le()?;
                let nz = stream.read_f32_le()?;
                Some([nx, ny, nz])
            } else {
                // FO3+ has i16 normal in the welding info (different encoding)
                None
            };
            triangles.push(PackedTriangle {
                v0,
                v1,
                v2,
                welding_info,
                normal,
            });
        }

        let num_vertices = stream.read_u32_le()?;
        // FO3+ (since 20.2.0.7) — nif.xml lines 3962-3967:
        //   `Compressed: bool` gates the trailing `Vertices` array
        //   between `Vector3[]` (12 B/vertex, IEEE f32) and
        //   `HalfVector3[]` (6 B/vertex, IEEE half-float). Vanilla
        //   Bethesda content ships `Compressed == 0`, but flipping
        //   the bit is legal — and if we always read f32 the per-
        //   vertex over-read scrambles the following `Num Sub Shapes`
        //   u16 and turns every collider vertex into NaN downstream.
        //   See issue #975 (NIF-D1-NEW-01).
        let compressed = if version >= crate::version::NifVersion::V20_2_0_7 {
            stream.read_byte_bool()?
        } else {
            false
        };
        let mut vertices: Vec<[f32; 3]> = stream.allocate_vec(num_vertices)?;
        for _ in 0..num_vertices {
            let (x, y, z) = if compressed {
                (
                    crate::blocks::tri_shape::half_to_f32(stream.read_u16_le()?),
                    crate::blocks::tri_shape::half_to_f32(stream.read_u16_le()?),
                    crate::blocks::tri_shape::half_to_f32(stream.read_u16_le()?),
                )
            } else {
                (
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                )
            };
            vertices.push([x, y, z]);
        }

        // FO3+ (since 20.2.0.7): sub-shapes at the end
        if version >= crate::version::NifVersion::V20_2_0_7 {
            let num_sub_shapes = stream.read_u16_le()? as usize;
            for _ in 0..num_sub_shapes {
                stream.skip(12)?; // HkSubPartData: filter(4) + numVerts(4) + material(4)
            }
        }

        Ok(Self {
            triangles,
            vertices,
        })
    }
}

impl_ni_object!(
    BhkNiTriStripsShape => "bhkNiTriStripsShape",
    BhkPackedNiTriStripsShape => "bhkPackedNiTriStripsShape",
    HkPackedNiTriStripsData => "hkPackedNiTriStripsData",
);
