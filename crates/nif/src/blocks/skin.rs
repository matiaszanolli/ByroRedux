//! Skinning block parsers: NiSkinInstance, NiSkinData, NiSkinPartition.
//!
//! These blocks define skeletal deformation for character meshes.
//! NiTriShape.skin_instance_ref → NiSkinInstance → NiSkinData + NiSkinPartition.

use super::NiObject;
use crate::stream::NifStream;
use crate::types::{BlockRef, NiTransform};
use std::any::Any;
use std::io;

// ── NiSkinInstance ───────────────────────────────────────────────────

/// Skinning instance — links a mesh to its skeleton and skin data.
#[derive(Debug)]
pub struct NiSkinInstance {
    /// Reference to NiSkinData (bind-pose bone transforms + vertex weights).
    pub data_ref: BlockRef,
    /// Reference to NiSkinPartition (hardware-optimized vertex groups).
    pub skin_partition_ref: BlockRef,
    /// Reference to the skeleton root NiNode.
    pub skeleton_root_ref: BlockRef,
    /// Bone block references (NiNode pointers).
    pub bone_refs: Vec<BlockRef>,
}

impl NiObject for NiSkinInstance {
    fn block_type_name(&self) -> &'static str {
        "NiSkinInstance"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiSkinInstance {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let data_ref = stream.read_block_ref()?;
        let skin_partition_ref = stream.read_block_ref()?;
        let skeleton_root_ref = stream.read_block_ref()?;
        let num_bones = stream.read_u32_le()? as usize;
        let mut bone_refs = Vec::with_capacity(num_bones);
        for _ in 0..num_bones {
            bone_refs.push(stream.read_block_ref()?);
        }
        Ok(Self {
            data_ref,
            skin_partition_ref,
            skeleton_root_ref,
            bone_refs,
        })
    }
}

// ── NiSkinData ──────────────────────────────────────────────────────

/// Per-bone vertex weight entry.
#[derive(Debug, Clone)]
pub struct BoneVertWeight {
    pub vertex_index: u16,
    pub weight: f32,
}

/// Per-bone skinning data: bind-pose transform, bounding sphere, vertex weights.
#[derive(Debug)]
pub struct BoneData {
    /// Offset from the bone to the skin in bind pose.
    pub skin_transform: NiTransform,
    /// Bounding sphere: [center_x, center_y, center_z, radius].
    pub bounding_sphere: [f32; 4],
    /// Vertex weights for this bone.
    pub vertex_weights: Vec<BoneVertWeight>,
}

/// Skinning data — per-bone transforms and vertex weights.
#[derive(Debug)]
pub struct NiSkinData {
    /// Overall skin transform (offset from mesh to skeleton root).
    pub skin_transform: NiTransform,
    /// Per-bone data: transform, bounds, weights.
    pub bones: Vec<BoneData>,
}

impl NiObject for NiSkinData {
    fn block_type_name(&self) -> &'static str {
        "NiSkinData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiSkinData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let skin_transform = stream.read_ni_transform()?;
        let num_bones = stream.read_u32_le()? as usize;

        // has_vertex_weights (version >= 4.2.1.0, always true for Bethesda games)
        let has_vertex_weights = stream.read_u8()? != 0;

        let mut bones = Vec::with_capacity(num_bones);
        for _ in 0..num_bones {
            let bone_transform = stream.read_ni_transform()?;

            // Bounding sphere: center (3 floats) + radius (1 float)
            let cx = stream.read_f32_le()?;
            let cy = stream.read_f32_le()?;
            let cz = stream.read_f32_le()?;
            let radius = stream.read_f32_le()?;

            let num_vertices = stream.read_u16_le()? as usize;

            let vertex_weights = if has_vertex_weights {
                let mut weights = Vec::with_capacity(num_vertices);
                for _ in 0..num_vertices {
                    let vertex_index = stream.read_u16_le()?;
                    let weight = stream.read_f32_le()?;
                    weights.push(BoneVertWeight {
                        vertex_index,
                        weight,
                    });
                }
                weights
            } else {
                Vec::new()
            };

            bones.push(BoneData {
                skin_transform: bone_transform,
                bounding_sphere: [cx, cy, cz, radius],
                vertex_weights,
            });
        }

        Ok(Self {
            skin_transform,
            bones,
        })
    }
}

// ── NiSkinPartition ─────────────────────────────────────────────────

/// A single hardware skin partition — bone subset, vertex map, weights, triangles.
#[derive(Debug)]
pub struct SkinPartitionEntry {
    pub num_vertices: u16,
    pub num_triangles: u16,
    pub bones: Vec<u16>,
    pub num_weights_per_vertex: u16,
    /// Maps partition-local vertex index → mesh-global vertex index.
    pub vertex_map: Vec<u16>,
    /// Per-vertex bone weights [num_vertices * num_weights_per_vertex].
    pub vertex_weights: Vec<f32>,
    /// Triangle indices (into partition-local vertex space).
    pub triangles: Vec<[u16; 3]>,
    /// Per-vertex bone indices [num_vertices * num_weights_per_vertex].
    pub bone_indices: Vec<u8>,
}

/// Hardware-optimized skin partitioning data.
///
/// Each partition is a subset of the mesh vertices that can be processed
/// by the GPU with a limited bone palette (typically 4 bones per vertex).
#[derive(Debug)]
pub struct NiSkinPartition {
    pub partitions: Vec<SkinPartitionEntry>,
}

impl NiObject for NiSkinPartition {
    fn block_type_name(&self) -> &'static str {
        "NiSkinPartition"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiSkinPartition {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_partitions = stream.read_u32_le()? as usize;

        // SSE (bsver==100): global vertex data before partitions.
        let bsver = stream.bsver();
        if bsver == 100 {
            let data_size = stream.read_u32_le()?;
            let _vertex_size = stream.read_u32_le()?;
            let _vertex_desc = stream.read_u64_le()?;
            if data_size > 0 {
                stream.skip(data_size as u64)?;
            }
        }

        let has_conditionals = stream.version() >= crate::version::NifVersion(0x0A010000);

        let mut partitions = Vec::with_capacity(num_partitions);
        for _ in 0..num_partitions {
            let num_vertices = stream.read_u16_le()?;
            let num_triangles = stream.read_u16_le()?;
            let num_bones = stream.read_u16_le()?;
            let num_strips = stream.read_u16_le()?;
            let num_weights_per_vertex = stream.read_u16_le()?;

            // Bones array.
            let mut bones = Vec::with_capacity(num_bones as usize);
            for _ in 0..num_bones {
                bones.push(stream.read_u16_le()?);
            }

            // Vertex map (conditional on has_vertex_map for v >= 10.1.0.0).
            let vertex_map = if has_conditionals {
                let has = stream.read_byte_bool()?;
                if has {
                    let mut map = Vec::with_capacity(num_vertices as usize);
                    for _ in 0..num_vertices {
                        map.push(stream.read_u16_le()?);
                    }
                    map
                } else {
                    Vec::new()
                }
            } else {
                let mut map = Vec::with_capacity(num_vertices as usize);
                for _ in 0..num_vertices {
                    map.push(stream.read_u16_le()?);
                }
                map
            };

            // Vertex weights (conditional).
            let vertex_weights = if has_conditionals {
                let has = stream.read_byte_bool()?;
                if has {
                    let count = num_vertices as usize * num_weights_per_vertex as usize;
                    let mut weights = Vec::with_capacity(count);
                    for _ in 0..count {
                        weights.push(stream.read_f32_le()?);
                    }
                    weights
                } else {
                    Vec::new()
                }
            } else {
                let count = num_vertices as usize * num_weights_per_vertex as usize;
                let mut weights = Vec::with_capacity(count);
                for _ in 0..count {
                    weights.push(stream.read_f32_le()?);
                }
                weights
            };

            // Strip lengths.
            let mut strip_lengths = Vec::with_capacity(num_strips as usize);
            for _ in 0..num_strips {
                strip_lengths.push(stream.read_u16_le()?);
            }

            // Has faces (conditional) — gates both strips and triangles.
            let has_faces = if has_conditionals {
                stream.read_byte_bool()?
            } else {
                true
            };

            // Strips or triangles.
            let mut triangles = Vec::new();
            if has_faces {
                if num_strips > 0 {
                    // Jagged strip arrays — skip strip data (not converted to triangles here).
                    for &len in &strip_lengths {
                        stream.skip(len as u64 * 2)?;
                    }
                } else {
                    triangles = Vec::with_capacity(num_triangles as usize);
                    for _ in 0..num_triangles {
                        let a = stream.read_u16_le()?;
                        let b = stream.read_u16_le()?;
                        let c = stream.read_u16_le()?;
                        triangles.push([a, b, c]);
                    }
                }
            }

            // Bone indices (conditional).
            let bone_indices = {
                let has = stream.read_byte_bool()?;
                if has {
                    let count = num_vertices as usize * num_weights_per_vertex as usize;
                    stream.read_bytes(count)?
                } else {
                    Vec::new()
                }
            };

            // Skyrim+ trailing fields (bsver > 34).
            if bsver > 34 {
                let _lod_level = stream.read_u8()?;
                let _global_vb = stream.read_byte_bool()?;
            }

            // SSE (bsver==100): per-partition vertex desc + triangles copy.
            if bsver == 100 {
                let _vertex_desc = stream.read_u64_le()?;
                // Triangles copy — same data as above, skip.
                stream.skip(num_triangles as u64 * 6)?;
            }

            partitions.push(SkinPartitionEntry {
                num_vertices,
                num_triangles,
                bones,
                num_weights_per_vertex,
                vertex_map,
                vertex_weights,
                triangles,
                bone_indices,
            });
        }

        Ok(Self { partitions })
    }
}

// ── BSDismemberSkinInstance (Bethesda extension) ─────────────────────

/// Bethesda's extended skin instance with dismemberment body part flags.
/// Inherits NiSkinInstance, adds per-partition body part data.
#[derive(Debug)]
pub struct BsDismemberSkinInstance {
    pub base: NiSkinInstance,
    pub partitions: Vec<BodyPartInfo>,
}

/// Per-partition body part flag.
#[derive(Debug)]
pub struct BodyPartInfo {
    pub part_flag: u16,
    pub body_part: u16,
}

impl NiObject for BsDismemberSkinInstance {
    fn block_type_name(&self) -> &'static str {
        "BSDismemberSkinInstance"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsDismemberSkinInstance {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiSkinInstance::parse(stream)?;
        let num_partitions = stream.read_u32_le()? as usize;
        let mut partitions = Vec::with_capacity(num_partitions);
        for _ in 0..num_partitions {
            let part_flag = stream.read_u16_le()?;
            let body_part = stream.read_u16_le()?;
            partitions.push(BodyPartInfo {
                part_flag,
                body_part,
            });
        }
        Ok(Self { base, partitions })
    }
}

// ── BSSkin::Instance (FO4+ skinning) ────────────────────────────────

/// FO4+ skin instance — replaces NiSkinInstance for BSTriShape meshes.
///
/// Key differences: no skin partition ref (partition data is in BSTriShape),
/// adds per-bone non-uniform scales, skeleton root is first field.
#[derive(Debug)]
pub struct BsSkinInstance {
    pub skeleton_root_ref: BlockRef,
    pub bone_data_ref: BlockRef,
    pub bone_refs: Vec<BlockRef>,
    pub scales: Vec<[f32; 3]>,
}

impl NiObject for BsSkinInstance {
    fn block_type_name(&self) -> &'static str {
        "BSSkin::Instance"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsSkinInstance {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let skeleton_root_ref = stream.read_block_ref()?;
        let bone_data_ref = stream.read_block_ref()?;
        let num_bones = stream.read_u32_le()? as usize;
        let mut bone_refs = Vec::with_capacity(num_bones);
        for _ in 0..num_bones {
            bone_refs.push(stream.read_block_ref()?);
        }
        let num_scales = stream.read_u32_le()? as usize;
        let mut scales = Vec::with_capacity(num_scales);
        for _ in 0..num_scales {
            scales.push([
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ]);
        }
        Ok(Self {
            skeleton_root_ref,
            bone_data_ref,
            bone_refs,
            scales,
        })
    }
}

// ── BSSkin::BoneData (FO4+ bone transforms) ─────────────────────────

/// Per-bone transform for FO4+ skinning.
#[derive(Debug)]
pub struct BsSkinBoneTrans {
    /// Bounding sphere: center (3 floats) + radius (1 float).
    pub bounding_sphere: [f32; 4],
    /// Bone-to-skin rotation matrix (3x3).
    pub rotation: [[f32; 3]; 3],
    /// Bone-to-skin translation.
    pub translation: [f32; 3],
    /// Bone scale.
    pub scale: f32,
}

/// FO4+ bone data — replaces NiSkinData for BSTriShape meshes.
///
/// Simpler than NiSkinData: no overall skin transform, no per-vertex weights
/// (weights are stored in BSTriShape vertex buffer as bone indices + weights).
#[derive(Debug)]
pub struct BsSkinBoneData {
    pub bones: Vec<BsSkinBoneTrans>,
}

impl NiObject for BsSkinBoneData {
    fn block_type_name(&self) -> &'static str {
        "BSSkin::BoneData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsSkinBoneData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_bones = stream.read_u32_le()? as usize;
        let mut bones = Vec::with_capacity(num_bones);
        for _ in 0..num_bones {
            let bounding_sphere = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let mut rotation = [[0.0f32; 3]; 3];
            for row in &mut rotation {
                for val in row.iter_mut() {
                    *val = stream.read_f32_le()?;
                }
            }
            let translation = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let scale = stream.read_f32_le()?;
            bones.push(BsSkinBoneTrans {
                bounding_sphere,
                rotation,
                translation,
                scale,
            });
        }
        Ok(Self { bones })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;
    use crate::version::NifVersion;

    fn make_fnv_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
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
        }
    }

    #[test]
    fn parse_skin_partition_fnv_one_partition() {
        let header = make_fnv_header();
        let mut data = Vec::new();

        // num_partitions = 1
        data.extend_from_slice(&1u32.to_le_bytes());
        // Partition 0:
        let num_verts: u16 = 3;
        let num_tris: u16 = 1;
        let num_bones: u16 = 2;
        let num_strips: u16 = 0;
        let num_wpv: u16 = 2; // weights per vertex
        data.extend_from_slice(&num_verts.to_le_bytes());
        data.extend_from_slice(&num_tris.to_le_bytes());
        data.extend_from_slice(&num_bones.to_le_bytes());
        data.extend_from_slice(&num_strips.to_le_bytes());
        data.extend_from_slice(&num_wpv.to_le_bytes());
        // Bones: [0, 1]
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        // Has vertex map = true
        data.push(1u8);
        // Vertex map: [0, 1, 2]
        for i in 0..3u16 {
            data.extend_from_slice(&i.to_le_bytes());
        }
        // Has vertex weights = true
        data.push(1u8);
        // Weights: 3 verts × 2 weights = 6 floats
        for w in [0.8f32, 0.2, 0.5, 0.5, 0.3, 0.7] {
            data.extend_from_slice(&w.to_le_bytes());
        }
        // Strip lengths: (none, num_strips=0)
        // Has faces = true
        data.push(1u8);
        // Triangles: 1 triangle [0, 1, 2]
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes());
        // Has bone indices = true
        data.push(1u8);
        // Bone indices: 3 verts × 2 = 6 bytes
        data.extend_from_slice(&[0u8, 1, 0, 1, 1, 0]);
        // FNV bsver=34: no trailing LOD/Global fields (bsver > 34 is false)

        let mut stream = NifStream::new(&data, &header);
        let part = NiSkinPartition::parse(&mut stream).unwrap();

        assert_eq!(part.partitions.len(), 1);
        let p = &part.partitions[0];
        assert_eq!(p.num_vertices, 3);
        assert_eq!(p.num_triangles, 1);
        assert_eq!(p.bones, vec![0, 1]);
        assert_eq!(p.vertex_map, vec![0, 1, 2]);
        assert_eq!(p.vertex_weights.len(), 6);
        assert!((p.vertex_weights[0] - 0.8).abs() < 1e-6);
        assert_eq!(p.triangles.len(), 1);
        assert_eq!(p.triangles[0], [0, 1, 2]);
        assert_eq!(p.bone_indices, vec![0, 1, 0, 1, 1, 0]);
        assert_eq!(stream.position() as usize, data.len());
    }
}
