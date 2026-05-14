//! Skyrim+ compressed mesh shape.
//!
//! BhkCompressedMeshShape + Data + chunk / big-tri / transform sub-types.

use super::super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::any::Any;
use std::io;


/// Thin wrapper pointing to bhkCompressedMeshShapeData.
#[derive(Debug)]
pub struct BhkCompressedMeshShape {
    pub target_ref: BlockRef,
    pub user_data: u32,
    pub radius: f32,
    pub scale: [f32; 4],
    pub data_ref: BlockRef,
}

impl NiObject for BhkCompressedMeshShape {
    fn block_type_name(&self) -> &'static str {
        "bhkCompressedMeshShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkCompressedMeshShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let target_ref = stream.read_block_ref()?;
        let user_data = stream.read_u32_le()?;
        let radius = stream.read_f32_le()?;
        let _unknown_float = stream.read_f32_le()?;
        let scale = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let _radius_copy = stream.read_f32_le()?;
        let _scale_copy = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let data_ref = stream.read_block_ref()?;
        Ok(Self {
            target_ref,
            user_data,
            radius,
            scale,
            data_ref,
        })
    }
}

/// A single quantized collision chunk within bhkCompressedMeshShapeData.
#[derive(Debug)]
pub struct CmsChunk {
    /// Chunk-local origin in Havok coordinates.
    pub translation: [f32; 4],
    pub material_index: u32,
    pub transform_index: u16,
    /// Quantized vertex positions: (x, y, z) as u16. Dequantize: translation + vertex/1000.
    pub vertices: Vec<[u16; 3]>,
    /// Triangle strip indices into the vertices array.
    pub indices: Vec<u16>,
    /// Strip lengths — if empty, indices are a plain triangle list.
    pub strips: Vec<u16>,
}

/// Full-precision triangle referencing BigVerts.
#[derive(Debug)]
pub struct CmsBigTri {
    pub v1: u16,
    pub v2: u16,
    pub v3: u16,
    pub material: u32,
}

/// Chunk transform (translation + rotation as quaternion).
#[derive(Debug)]
pub struct CmsTransform {
    pub translation: [f32; 4],
    pub rotation: [f32; 4],
}

/// Compressed mesh collision data — Skyrim's primary collision format.
#[derive(Debug)]
pub struct BhkCompressedMeshShapeData {
    pub bits_per_index: u32,
    pub bits_per_w_index: u32,
    pub mask_w_index: u32,
    pub mask_index: u32,
    pub error: f32,
    pub aabb_min: [f32; 4],
    pub aabb_max: [f32; 4],
    pub chunk_materials: Vec<[u32; 2]>,
    pub chunk_transforms: Vec<CmsTransform>,
    /// Full-precision vertices for oversized triangles.
    pub big_verts: Vec<[f32; 4]>,
    /// Oversized triangles indexing into big_verts.
    pub big_tris: Vec<CmsBigTri>,
    /// Quantized collision geometry chunks.
    pub chunks: Vec<CmsChunk>,
}

impl NiObject for BhkCompressedMeshShapeData {
    fn block_type_name(&self) -> &'static str {
        "bhkCompressedMeshShapeData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkCompressedMeshShapeData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let bits_per_index = stream.read_u32_le()?;
        let bits_per_w_index = stream.read_u32_le()?;
        let mask_w_index = stream.read_u32_le()?;
        let mask_index = stream.read_u32_le()?;
        let error = stream.read_f32_le()?;

        // AABB
        let aabb_min = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let aabb_max = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];

        let _welding_type = stream.read_u8()?;
        let _material_type = stream.read_u8()?;

        // Material arrays (unused but must be consumed)
        let num_mat32 = stream.read_u32_le()? as usize;
        stream.skip(num_mat32 as u64 * 4)?;
        let num_mat16 = stream.read_u32_le()? as usize;
        stream.skip(num_mat16 as u64 * 4)?;
        let num_mat8 = stream.read_u32_le()? as usize;
        stream.skip(num_mat8 as u64 * 4)?;

        // Chunk materials: (SkyrimHavokMaterial, HavokFilter) = 2×u32 = 8 bytes each
        let num_chunk_materials = stream.read_u32_le()?;
        let mut chunk_materials = stream.allocate_vec(num_chunk_materials)?;
        for _ in 0..num_chunk_materials {
            chunk_materials.push([stream.read_u32_le()?, stream.read_u32_le()?]);
        }

        let _num_named_materials = stream.read_u32_le()?;

        // Chunk transforms
        let num_transforms = stream.read_u32_le()?;
        let mut chunk_transforms = stream.allocate_vec(num_transforms)?;
        for _ in 0..num_transforms {
            let translation = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let rotation = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            chunk_transforms.push(CmsTransform {
                translation,
                rotation,
            });
        }

        // Big verts (full-precision)
        let num_big_verts = stream.read_u32_le()?;
        let mut big_verts = stream.allocate_vec(num_big_verts)?;
        for _ in 0..num_big_verts {
            big_verts.push([
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ]);
        }

        // Big tris
        let num_big_tris = stream.read_u32_le()?;
        let mut big_tris = stream.allocate_vec(num_big_tris)?;
        for _ in 0..num_big_tris {
            let v1 = stream.read_u16_le()?;
            let v2 = stream.read_u16_le()?;
            let v3 = stream.read_u16_le()?;
            let material = stream.read_u32_le()?;
            let _welding_info = stream.read_u16_le()?;
            big_tris.push(CmsBigTri {
                v1,
                v2,
                v3,
                material,
            });
        }

        // Chunks (variable-size)
        let num_chunks = stream.read_u32_le()?;
        let mut chunks = stream.allocate_vec(num_chunks)?;
        for _ in 0..num_chunks {
            let translation = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let material_index = stream.read_u32_le()?;
            let _reference = stream.read_u16_le()?;
            let transform_index = stream.read_u16_le()?;

            // Vertices: nif.xml Num Vertices is the count of u16 values (not triples).
            // Divide by 3 to get the number of (x, y, z) vertex positions.
            // Confirmed via Havok source: Chunk::m_vertices is hkArray<hkUint16>,
            // count = actual_vertices * 3. #981 — bulk reads via
            // `read_u16_triple_array` / `read_u16_array`.
            let num_vertex_components = stream.read_u32_le()?;
            let num_vertices = (num_vertex_components / 3) as usize;
            let vertices = stream.read_u16_triple_array(num_vertices)?;

            // Indices
            let num_indices = stream.read_u32_le()? as usize;
            let indices = stream.read_u16_array(num_indices)?;

            // Strips
            let num_strips = stream.read_u32_le()? as usize;
            let strips = stream.read_u16_array(num_strips)?;

            // Welding info
            let num_welding = stream.read_u32_le()? as usize;
            stream.skip(num_welding as u64 * 2)?;

            chunks.push(CmsChunk {
                translation,
                material_index,
                transform_index,
                vertices,
                indices,
                strips,
            });
        }

        // Num convex piece A (unused)
        let _num_convex_piece_a = stream.read_u32_le()?;

        Ok(Self {
            bits_per_index,
            bits_per_w_index,
            mask_w_index,
            mask_index,
            error,
            aabb_min,
            aabb_max,
            chunk_materials,
            chunk_transforms,
            big_verts,
            big_tris,
            chunks,
        })
    }
}
