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
        } else if stream.version() <= NifVersion(0x14010003) {
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
}

/// NiTriStrips — identical serialization to NiTriShape (both are NiGeometry).
pub type NiTriStrips = NiTriShape;

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
}

impl NiObject for BsTriShape {
    fn block_type_name(&self) -> &'static str {
        "BSTriShape"
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
const VF_VERTEX: u16 = 0x001;
const VF_UVS: u16 = 0x002;
const VF_NORMALS: u16 = 0x008;
const VF_TANGENTS: u16 = 0x010;
const VF_VERTEX_COLORS: u16 = 0x020;
const VF_SKINNED: u16 = 0x040;
const VF_EYE_DATA: u16 = 0x100;
/// FO4+: full-precision vertex positions (bit 10). When clear, positions are half-float.
const VF_FULL_PRECISION: u16 = 0x400;

impl BsTriShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let av = NiAVObjectData::parse_no_properties(stream)?;

        // BSTriShape-specific: bounding sphere
        let center = stream.read_ni_point3()?;
        let radius = stream.read_f32_le()?;

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

        let nv = num_vertices as usize;
        let mut vertices = Vec::with_capacity(nv);
        let mut uvs = Vec::with_capacity(nv);
        let mut normals = Vec::with_capacity(nv);
        let mut vertex_colors = Vec::with_capacity(nv);
        let mut triangles = Vec::with_capacity(num_triangles as usize);
        let is_skinned = vertex_attrs & VF_SKINNED != 0;
        let mut bone_weights: Vec<[f32; 4]> = if is_skinned {
            Vec::with_capacity(nv)
        } else {
            Vec::new()
        };
        let mut bone_indices: Vec<[u8; 4]> = if is_skinned {
            Vec::with_capacity(nv)
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
                    stream.skip(4); // 3 bytes tangent + 1 byte bitangent Z
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
                    stream.skip(4);
                }

                // Ensure we consumed exactly vertex_size_bytes.
                // Guard against underflow: if consumed > vertex_size_bytes (malformed
                // vertex descriptor), report an error instead of wrapping to a huge skip.
                let consumed = (stream.position() - vert_start) as usize;
                if consumed < vertex_size_bytes {
                    stream.skip((vertex_size_bytes - consumed) as u64);
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

            // Triangle indices
            for _ in 0..num_triangles {
                let a = stream.read_u16_le()?;
                let b = stream.read_u16_le()?;
                let c = stream.read_u16_le()?;
                triangles.push([a, b, c]);
            }

            // Skyrim SE: particle data (skip)
            if stream.bsver() < 130 {
                let particle_data_size = stream.read_u32_le()?;
                if particle_data_size > 0 {
                    // particle vertices (num_vertices × 6 bytes) + particle normals + particle triangles
                    let skip_bytes = (num_vertices as u64) * 6 // half-float positions
                        + (num_vertices as u64) * 6 // half-float normals
                        + (num_triangles as u64) * 6; // triangle indices
                    stream.skip(skip_bytes);
                }
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
        })
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
    // Bethesda extensions (material CRC) only exist in Skyrim+.
    let data_flags = stream.read_u16_le()?;
    if stream.variant().has_material_crc() {
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
        assert!((sum - 1.0).abs() < 1e-3, "weights should sum to 1, got {}", sum);
        for w in &weights {
            assert!((w - 0.25).abs() < 1e-3);
        }
        assert_eq!(indices, [3, 7, 12, 42]);
    }
}
