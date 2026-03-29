//! NiTriShape and NiTriShapeData — indexed triangle geometry.
//!
//! NiTriShape is an NiAVObject leaf node that references a NiTriShapeData
//! block containing vertex positions, normals, UV coordinates, and triangle
//! index lists.

use crate::stream::NifStream;
use crate::types::{BlockRef, NiPoint3, NiTransform};
use crate::version::NifVersion;
use super::NiObject;
use std::any::Any;
use std::io;

/// Geometry leaf node referencing NiTriShapeData or NiTriStripsData.
///
/// This struct is used for both NiTriShape and NiTriStrips — they have
/// identical serialization (both inherit NiGeometry).
#[derive(Debug)]
pub struct NiTriShape {
    pub name: Option<String>,
    pub extra_data_refs: Vec<BlockRef>,
    pub controller_ref: BlockRef,
    pub flags: u32,
    pub transform: NiTransform,
    pub properties: Vec<BlockRef>,
    pub collision_ref: BlockRef,
    pub data_ref: BlockRef,
    pub skin_instance_ref: BlockRef,
    /// Skyrim+ (user_version_2 >= 130): dedicated shader property ref.
    pub shader_property_ref: BlockRef,
    /// Skyrim+ (user_version_2 >= 130): dedicated alpha property ref.
    pub alpha_property_ref: BlockRef,
    /// Material names from NiGeometry material array (pre-Skyrim SSE).
    pub num_materials: u32,
    pub active_material_index: u32,
}

impl NiObject for NiTriShape {
    fn block_type_name(&self) -> &'static str {
        "NiTriShape"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiTriShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiObjectNET fields
        let name = stream.read_string()?;
        let extra_data_refs = stream.read_block_ref_list()?;
        let controller_ref = stream.read_block_ref()?;

        // NiAVObject fields
        let flags = if stream.version() >= NifVersion::V20_2_0_7 {
            stream.read_u32_le()?
        } else {
            stream.read_u16_le()? as u32
        };
        let transform = stream.read_ni_transform()?;
        let properties = stream.read_block_ref_list()?;
        let collision_ref = stream.read_block_ref()?;

        // NiGeometry fields
        let data_ref = stream.read_block_ref()?;
        let skin_instance_ref = stream.read_block_ref()?;

        let mut shader_property_ref = BlockRef::NULL;
        let mut alpha_property_ref = BlockRef::NULL;
        let mut num_materials = 0u32;
        let mut active_material_index = 0u32;

        if stream.version() >= NifVersion(0x14020005) {
            // v20.2.0.5+ : material array format
            num_materials = stream.read_u32_le()?;
            for _ in 0..num_materials {
                let _mat_name_idx = stream.read_u32_le()?;  // string table index
                let _mat_extra_data = stream.read_u32_le()?;
            }
            active_material_index = stream.read_u32_le()?;

            if stream.version() >= NifVersion::V20_2_0_7 {
                // Material needs update default flag.
                // Bethesda serializes this as u8 (not NiBool u32).
                let _dirty_flag = stream.read_u8()?;
            }

            // Skyrim SSE+ (user_version_2 >= 130): dedicated shader/alpha property refs
            if stream.user_version_2() >= 130 {
                shader_property_ref = stream.read_block_ref()?;
                alpha_property_ref = stream.read_block_ref()?;
            }
        } else {
            // Pre-20.2.0.5: hasShader format
            let has_shader = stream.read_bool()?;
            if has_shader {
                let _shader_name = stream.read_sized_string()?;
                let _implementation = stream.read_u32_le()?;
            }
        }

        Ok(Self {
            name,
            extra_data_refs,
            controller_ref,
            flags,
            transform,
            properties,
            collision_ref,
            data_ref,
            skin_instance_ref,
            shader_property_ref,
            alpha_property_ref,
            num_materials,
            active_material_index,
        })
    }
}

/// NiTriStrips — identical serialization to NiTriShape (both are NiGeometry).
pub type NiTriStrips = NiTriShape;

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
fn parse_geometry_data_base(stream: &mut NifStream) -> io::Result<(
    Vec<NiPoint3>,   // vertices
    u16,             // data_flags
    Vec<NiPoint3>,   // normals
    NiPoint3,        // center
    f32,             // radius
    Vec<[f32; 4]>,   // vertex_colors
    Vec<Vec<[f32; 2]>>, // uv_sets
)> {
    let _group_id = stream.read_i32_le()?; // usually 0
    let num_vertices = stream.read_u16_le()? as usize;
    let _keep_flags = stream.read_u8()?;
    let _compress_flags = stream.read_u8()?;

    let has_vertices = stream.read_byte_bool()?;
    let vertices = if has_vertices {
        let mut verts = Vec::with_capacity(num_vertices);
        for _ in 0..num_vertices {
            verts.push(stream.read_ni_point3()?);
        }
        verts
    } else {
        Vec::new()
    };

    // u16 dataFlags is always present in NiGeometryData.
    // Bethesda extensions (material CRC) only exist in Skyrim+ (user_version >= 12).
    let data_flags = stream.read_u16_le()?;
    if stream.user_version() >= 12 {
        let _material_crc = stream.read_u32_le()?;
    }

    let has_normals = stream.read_byte_bool()?;
    let normals = if has_normals {
        let mut norms = Vec::with_capacity(num_vertices);
        for _ in 0..num_vertices {
            norms.push(stream.read_ni_point3()?);
        }
        norms
    } else {
        Vec::new()
    };

    // Tangents + bitangents (if has_normals and dataFlags bit 12 set = NBT method)
    if has_normals && data_flags & 0xF000 != 0 {
        // Skip tangents (num_vertices * 3 floats)
        stream.skip(num_vertices as u64 * 12);
        // Skip bitangents (num_vertices * 3 floats)
        stream.skip(num_vertices as u64 * 12);
    }

    // Bounding sphere
    let center = stream.read_ni_point3()?;
    let radius = stream.read_f32_le()?;

    // Vertex colors
    let has_vertex_colors = stream.read_byte_bool()?;
    let vertex_colors = if has_vertex_colors {
        let mut colors = Vec::with_capacity(num_vertices);
        for _ in 0..num_vertices {
            let r = stream.read_f32_le()?;
            let g = stream.read_f32_le()?;
            let b = stream.read_f32_le()?;
            let a = stream.read_f32_le()?;
            colors.push([r, g, b, a]);
        }
        colors
    } else {
        Vec::new()
    };

    // UV sets: count is packed in dataFlags bits [0..5]
    let num_uv_sets = (data_flags & 0x003F) as usize;
    let mut uv_sets = Vec::with_capacity(num_uv_sets);
    for _ in 0..num_uv_sets {
        let mut uvs = Vec::with_capacity(num_vertices);
        for _ in 0..num_vertices {
            let u = stream.read_f32_le()?;
            let v = stream.read_f32_le()?;
            uvs.push([u, v]);
        }
        uv_sets.push(uvs);
    }

    // Consistency flags
    let _consistency_flags = stream.read_u16_le()?;

    // Additional data (version >= 20.0.0.4)
    if stream.version() >= NifVersion(0x14000004) {
        let _additional_data_ref = stream.read_block_ref()?;
    }

    Ok((vertices, data_flags, normals, center, radius, vertex_colors, uv_sets))
}

impl NiTriShapeData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let (vertices, _data_flags, normals, center, radius, vertex_colors, uv_sets) =
            parse_geometry_data_base(stream)?;

        // NiTriShapeData specific: triangles
        let num_triangles = stream.read_u16_le()? as usize;
        let _num_triangle_points = stream.read_u32_le()?; // num_triangles * 3

        let has_triangles = stream.read_byte_bool()?;
        let triangles = if has_triangles {
            let mut tris = Vec::with_capacity(num_triangles);
            for _ in 0..num_triangles {
                let a = stream.read_u16_le()?;
                let b = stream.read_u16_le()?;
                let c = stream.read_u16_le()?;
                tris.push([a, b, c]);
            }
            tris
        } else {
            Vec::new()
        };

        // Match groups (skip)
        let num_match_groups = stream.read_u16_le()? as usize;
        for _ in 0..num_match_groups {
            let count = stream.read_u16_le()? as usize;
            stream.skip(count as u64 * 2); // u16 per entry
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
        let num_strips = stream.read_u16_le()? as usize;
        let mut strip_lengths = Vec::with_capacity(num_strips);
        for _ in 0..num_strips {
            strip_lengths.push(stream.read_u16_le()?);
        }

        let has_strips = stream.read_byte_bool()?;
        let mut strips = Vec::with_capacity(num_strips);
        if has_strips {
            for &len in &strip_lengths {
                let mut strip = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    strip.push(stream.read_u16_le()?);
                }
                strips.push(strip);
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
        let mut triangles = Vec::new();
        for strip in &self.strips {
            for i in 2..strip.len() {
                let (a, b, c) = if i % 2 == 0 {
                    (strip[i - 2], strip[i - 1], strip[i])
                } else {
                    (strip[i - 1], strip[i - 2], strip[i]) // flip winding
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
