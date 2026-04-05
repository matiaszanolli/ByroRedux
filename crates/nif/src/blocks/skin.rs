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

/// Hardware-optimized skin partitioning data.
///
/// The partition detail is complex and version-dependent (SSE adds
/// embedded vertex data). For now, this is a minimal parser that
/// stores the partition count. The block is consumed correctly via
/// block_size skip for any unparsed detail.
#[derive(Debug)]
pub struct NiSkinPartition {
    pub num_partitions: u32,
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
    /// Minimal parse — reads partition count only.
    /// The rest is consumed by block_size adjustment in the main parser loop.
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_partitions = stream.read_u32_le()?;
        Ok(Self { num_partitions })
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
