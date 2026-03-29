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

/// Geometry leaf node referencing NiTriShapeData.
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
    pub shader_property_ref: BlockRef,
    pub alpha_property_ref: BlockRef,
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
        let flags = if stream.version() >= crate::version::NifVersion::V20_2_0_7 {
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

        // Shader property refs (version >= 20.2.0.7 with BS version)
        let shader_property_ref = if stream.version() >= crate::version::NifVersion::V20_2_0_7 {
            stream.read_block_ref()?
        } else {
            BlockRef::NULL
        };
        let alpha_property_ref = if stream.version() >= crate::version::NifVersion::V20_2_0_7 {
            stream.read_block_ref()?
        } else {
            BlockRef::NULL
        };

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

impl NiObject for NiTriShapeData {
    fn block_type_name(&self) -> &'static str {
        "NiTriShapeData"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiTriShapeData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiGeometryData base
        let _group_id = stream.read_i32_le()?; // usually 0
        let num_vertices = stream.read_u16_le()? as usize;
        let _keep_flags = stream.read_u8()?;
        let _compress_flags = stream.read_u8()?;

        let has_vertices = stream.read_bool()?;
        let vertices = if has_vertices {
            let mut verts = Vec::with_capacity(num_vertices);
            for _ in 0..num_vertices {
                verts.push(stream.read_ni_point3()?);
            }
            verts
        } else {
            Vec::new()
        };

        // BS-specific vector flags (version >= 20.2.0.7 with user_version >= 12)
        // We read them but don't use them yet
        let bs_vector_flags = if stream.version() >= crate::version::NifVersion::V20_2_0_7 {
            stream.read_u16_le()?
        } else {
            0
        };
        let _material_crc = if stream.version() >= crate::version::NifVersion::V20_2_0_7 {
            stream.read_u32_le()?
        } else {
            0
        };

        let has_normals = stream.read_bool()?;
        let normals = if has_normals {
            let mut norms = Vec::with_capacity(num_vertices);
            for _ in 0..num_vertices {
                norms.push(stream.read_ni_point3()?);
            }
            norms
        } else {
            Vec::new()
        };

        // Tangents + bitangents (if has_normals and BS vector flags indicate)
        if has_normals && bs_vector_flags & 0x1000 != 0 {
            // Skip tangents (num_vertices * 3 floats)
            stream.skip(num_vertices as u64 * 12);
            // Skip bitangents (num_vertices * 3 floats)
            stream.skip(num_vertices as u64 * 12);
        }

        // Bounding sphere
        let center = stream.read_ni_point3()?;
        let radius = stream.read_f32_le()?;

        // Vertex colors
        let has_vertex_colors = stream.read_bool()?;
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

        // UV sets
        let num_uv_sets = if stream.version() >= crate::version::NifVersion::V20_2_0_7 {
            (bs_vector_flags & 0x003F) as usize // packed in vector flags
        } else {
            // Older: read from data flags field (already skipped, default 1)
            if has_vertices { 1 } else { 0 }
        };
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

        // NiTriShapeData specific: triangles
        let num_triangles = stream.read_u16_le()? as usize;
        let _num_triangle_points = stream.read_u32_le()?; // num_triangles * 3

        let has_triangles = stream.read_bool()?;
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
