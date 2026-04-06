//! NIF animation controller blocks.
//!
//! Covers the NiTimeController hierarchy and NiControllerSequence.
//! Parsed enough to advance the stream correctly; actual animation
//! interpretation comes later.

use super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use crate::version::NifVersion;
use std::any::Any;
use std::io;
use std::sync::Arc;

// ── NiTimeController base ──────────────────────────────────────────────

/// Base fields for all NiTimeController subclasses (26 bytes).
#[derive(Debug)]
pub struct NiTimeControllerBase {
    pub next_controller_ref: BlockRef,
    pub flags: u16,
    pub frequency: f32,
    pub phase: f32,
    pub start_time: f32,
    pub stop_time: f32,
    pub target_ref: BlockRef,
}

impl NiTimeControllerBase {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let next_controller_ref = stream.read_block_ref()?;
        let flags = stream.read_u16_le()?;
        let frequency = stream.read_f32_le()?;
        let phase = stream.read_f32_le()?;
        let start_time = stream.read_f32_le()?;
        let stop_time = stream.read_f32_le()?;
        let target_ref = stream.read_block_ref()?;
        Ok(Self {
            next_controller_ref,
            flags,
            frequency,
            phase,
            start_time,
            stop_time,
            target_ref,
        })
    }
}

// ── NiTimeController (fallback for unknown controller subtypes) ────────

/// Stub for unknown controller types. Reads only the base 26 bytes.
#[derive(Debug)]
pub struct NiTimeController {
    pub base: NiTimeControllerBase,
}

impl NiObject for NiTimeController {
    fn block_type_name(&self) -> &'static str {
        "NiTimeController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiTimeController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Ok(Self {
            base: NiTimeControllerBase::parse(stream)?,
        })
    }
}

// ── NiSingleInterpController ───────────────────────────────────────────
// Adds: interpolator_ref (Ref = i32 = 4 bytes) for version >= 10.1.0.104.
// Subclasses: NiTransformController, NiVisController, NiAlphaController,
//             NiTextureTransformController, NiKeyframeController, etc.

/// Controller with a single interpolator reference.
/// Used for NiTransformController, NiVisController, NiAlphaController,
/// NiTextureTransformController, and BSShader*Controller types.
#[derive(Debug)]
pub struct NiSingleInterpController {
    pub base: NiTimeControllerBase,
    pub interpolator_ref: BlockRef,
}

impl NiObject for NiSingleInterpController {
    fn block_type_name(&self) -> &'static str {
        "NiSingleInterpController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiSingleInterpController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        // NiSingleInterpController: interpolator ref (since 10.1.0.104)
        let interpolator_ref = if stream.version() >= NifVersion(0x0A010068) {
            stream.read_block_ref()?
        } else {
            BlockRef::NULL
        };
        Ok(Self {
            base,
            interpolator_ref,
        })
    }
}

// ── NiMaterialColorController ──────────────────────────────────────────
// Inherits NiSingleInterpController, adds: target_color (MaterialColor enum, u16).

#[derive(Debug)]
pub struct NiMaterialColorController {
    pub base: NiTimeControllerBase,
    pub interpolator_ref: BlockRef,
    pub target_color: u16,
}

impl NiObject for NiMaterialColorController {
    fn block_type_name(&self) -> &'static str {
        "NiMaterialColorController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiMaterialColorController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let interpolator_ref = if stream.version() >= NifVersion(0x0A010068) {
            stream.read_block_ref()?
        } else {
            BlockRef::NULL
        };
        // MaterialColor enum (ushort since 10.1.0.0)
        let target_color = stream.read_u16_le()?;
        Ok(Self {
            base,
            interpolator_ref,
            target_color,
        })
    }
}

// ── NiTextureTransformController ───────────────────────────────────────
// Inherits NiFloatInterpController → NiSingleInterpController, adds:
// shader_map (bool), texture_slot (u32 TexType), operation (u32 TransformMember).

#[derive(Debug)]
pub struct NiTextureTransformController {
    pub base: NiTimeControllerBase,
    pub interpolator_ref: BlockRef,
    pub shader_map: bool,
    pub texture_slot: u32,
    pub operation: u32,
}

impl NiObject for NiTextureTransformController {
    fn block_type_name(&self) -> &'static str {
        "NiTextureTransformController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiTextureTransformController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let interpolator_ref = if stream.version() >= NifVersion(0x0A010068) {
            stream.read_block_ref()?
        } else {
            BlockRef::NULL
        };
        let shader_map = stream.read_byte_bool()?;
        let texture_slot = stream.read_u32_le()?;
        let operation = stream.read_u32_le()?;
        Ok(Self {
            base,
            interpolator_ref,
            shader_map,
            texture_slot,
            operation,
        })
    }
}

// ── NiMultiTargetTransformController ───────────────────────────────────
// Inherits NiInterpController (which adds nothing for FNV), adds:
// num_extra_targets (u16) + extra_targets (Ptr[]).

#[derive(Debug)]
pub struct NiMultiTargetTransformController {
    pub base: NiTimeControllerBase,
    pub extra_targets: Vec<BlockRef>,
}

impl NiObject for NiMultiTargetTransformController {
    fn block_type_name(&self) -> &'static str {
        "NiMultiTargetTransformController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiMultiTargetTransformController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let num_extra_targets = stream.read_u16_le()? as usize;
        let mut extra_targets = Vec::with_capacity(num_extra_targets);
        for _ in 0..num_extra_targets {
            extra_targets.push(stream.read_block_ref()?);
        }
        Ok(Self {
            base,
            extra_targets,
        })
    }
}

// ── NiControllerManager ────────────────────────────────────────────────
// Inherits NiTimeController, adds: cumulative (bool, 1 byte), sequences, palette.

#[derive(Debug)]
pub struct NiControllerManager {
    pub base: NiTimeControllerBase,
    pub cumulative: bool,
    pub sequence_refs: Vec<BlockRef>,
    pub object_palette_ref: BlockRef,
}

impl NiObject for NiControllerManager {
    fn block_type_name(&self) -> &'static str {
        "NiControllerManager"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiControllerManager {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        // cumulative is a byte bool based on observed block sizes
        let cumulative = stream.read_byte_bool()?;
        let num_sequences = stream.read_u32_le()? as usize;
        let mut sequence_refs = Vec::with_capacity(num_sequences);
        for _ in 0..num_sequences {
            sequence_refs.push(stream.read_block_ref()?);
        }
        let object_palette_ref = stream.read_block_ref()?;
        Ok(Self {
            base,
            cumulative,
            sequence_refs,
            object_palette_ref,
        })
    }
}

// ── NiControllerSequence ───────────────────────────────────────────────
// Does NOT inherit NiTimeController. Inherits NiSequence → NiObject.

/// A single controlled block entry within a NiControllerSequence.
#[derive(Debug)]
pub struct ControlledBlock {
    pub interpolator_ref: BlockRef,
    pub controller_ref: BlockRef,
    pub priority: u8,
    pub node_name: Option<Arc<str>>,
    pub property_type: Option<Arc<str>>,
    pub controller_type: Option<Arc<str>>,
    pub controller_id: Option<Arc<str>>,
    pub interpolator_id: Option<Arc<str>>,
}

#[derive(Debug)]
pub struct NiControllerSequence {
    // NiSequence fields
    pub name: Option<Arc<str>>,
    pub controlled_blocks: Vec<ControlledBlock>,
    pub array_grow_by: u32,
    // NiControllerSequence fields
    pub weight: f32,
    pub text_keys_ref: BlockRef,
    pub cycle_type: u32,
    pub frequency: f32,
    pub start_time: f32,
    pub stop_time: f32,
    pub manager_ref: BlockRef,
    pub accum_root_name: Option<Arc<str>>,
    pub anim_note_refs: Vec<BlockRef>,
}

impl NiObject for NiControllerSequence {
    fn block_type_name(&self) -> &'static str {
        "NiControllerSequence"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiControllerSequence {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiSequence fields (for v >= 20.1.0.1, string table format)
        let name = stream.read_string()?;
        let num_controlled_blocks = stream.read_u32_le()? as usize;

        // Array Grow By (since 10.1.0.106)
        let array_grow_by = if stream.version() >= NifVersion(0x0A01006A) {
            stream.read_u32_le()?
        } else {
            0
        };

        // ControlledBlock array
        let bsver = stream.bsver();
        let mut controlled_blocks = Vec::with_capacity(num_controlled_blocks);
        for _ in 0..num_controlled_blocks {
            let interpolator_ref = stream.read_block_ref()?;
            let controller_ref = stream.read_block_ref()?;
            // Priority byte (BSVER > 0, i.e. any Bethesda game)
            let priority = if bsver > 0 { stream.read_u8()? } else { 0 };
            let node_name = stream.read_string()?;
            let property_type = stream.read_string()?;
            let controller_type = stream.read_string()?;
            let controller_id = stream.read_string()?;
            let interpolator_id = stream.read_string()?;
            controlled_blocks.push(ControlledBlock {
                interpolator_ref,
                controller_ref,
                priority,
                node_name,
                property_type,
                controller_type,
                controller_id,
                interpolator_id,
            });
        }

        // NiControllerSequence fields
        let weight = stream.read_f32_le()?;
        let text_keys_ref = stream.read_block_ref()?;
        let cycle_type = stream.read_u32_le()?;
        let frequency = stream.read_f32_le()?;
        let start_time = stream.read_f32_le()?;
        let stop_time = stream.read_f32_le()?;
        let manager_ref = stream.read_block_ref()?;
        let accum_root_name = stream.read_string()?;

        // Anim note arrays (BSVER > 28)
        let anim_note_refs = if bsver > 28 {
            let num = stream.read_u16_le()? as usize;
            let mut refs = Vec::with_capacity(num);
            for _ in 0..num {
                refs.push(stream.read_block_ref()?);
            }
            refs
        } else {
            Vec::new()
        };

        Ok(Self {
            name,
            controlled_blocks,
            array_grow_by,
            weight,
            text_keys_ref,
            cycle_type,
            frequency,
            start_time,
            stop_time,
            manager_ref,
            accum_root_name,
            anim_note_refs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;
    use crate::version::NifVersion;

    fn make_header_fnv() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 11,
            user_version_2: 34,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("TestName")],
            max_string_length: 8,
            num_groups: 0,
        }
    }

    fn write_time_controller_base(data: &mut Vec<u8>) {
        // next_controller_ref: -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // flags: 0x000C
        data.extend_from_slice(&0x000Cu16.to_le_bytes());
        // frequency: 1.0
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // phase: 0.0
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // start_time: 0.0
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // stop_time: 1.0
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // target_ref: 0
        data.extend_from_slice(&0i32.to_le_bytes());
    }

    #[test]
    fn parse_ni_time_controller_base_26_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        assert_eq!(data.len(), 26);
        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiTimeController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 26);
        assert!(ctrl.base.next_controller_ref.is_null());
    }

    #[test]
    fn parse_single_interp_controller_30_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // interpolator_ref: 5
        data.extend_from_slice(&5i32.to_le_bytes());
        assert_eq!(data.len(), 30);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiSingleInterpController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 30);
        assert_eq!(ctrl.interpolator_ref.index(), Some(5));
    }

    #[test]
    fn parse_material_color_controller_32_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&3i32.to_le_bytes()); // interpolator_ref
        data.extend_from_slice(&1u16.to_le_bytes()); // target_color
        assert_eq!(data.len(), 32);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiMaterialColorController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 32);
        assert_eq!(ctrl.target_color, 1);
    }

    #[test]
    fn parse_multi_target_transform_controller() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // num_extra_targets: 4
        data.extend_from_slice(&4u16.to_le_bytes());
        // 4 target refs
        for i in 0..4 {
            data.extend_from_slice(&(i as i32).to_le_bytes());
        }
        assert_eq!(data.len(), 44);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiMultiTargetTransformController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 44);
        assert_eq!(ctrl.extra_targets.len(), 4);
    }

    #[test]
    fn parse_controller_manager_1_sequence() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.push(1); // cumulative = true (byte bool)
        data.extend_from_slice(&1u32.to_le_bytes()); // num_sequences
        data.extend_from_slice(&7i32.to_le_bytes()); // sequence_refs[0]
        data.extend_from_slice(&8i32.to_le_bytes()); // object_palette_ref
        assert_eq!(data.len(), 39);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiControllerManager::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 39);
        assert!(ctrl.cumulative);
        assert_eq!(ctrl.sequence_refs.len(), 1);
        assert_eq!(ctrl.sequence_refs[0].index(), Some(7));
        assert_eq!(ctrl.object_palette_ref.index(), Some(8));
    }

    #[test]
    fn parse_controller_sequence_no_blocks() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        // NiSequence: name (string table index 0)
        data.extend_from_slice(&0i32.to_le_bytes());
        // num_controlled_blocks: 0
        data.extend_from_slice(&0u32.to_le_bytes());
        // array_grow_by: 0
        data.extend_from_slice(&0u32.to_le_bytes());
        // NiControllerSequence fields:
        data.extend_from_slice(&1.0f32.to_le_bytes()); // weight
        data.extend_from_slice(&(-1i32).to_le_bytes()); // text_keys_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // cycle_type
        data.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
        data.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
        data.extend_from_slice(&1.0f32.to_le_bytes()); // stop_time
        data.extend_from_slice(&(-1i32).to_le_bytes()); // manager_ref
        data.extend_from_slice(&(-1i32).to_le_bytes()); // accum_root_name
                                                        // anim note arrays (BSVER > 28 = yes for FNV)
        data.extend_from_slice(&0u16.to_le_bytes()); // num_anim_note_arrays
        let expected_len = data.len();

        let mut stream = NifStream::new(&data, &header);
        let seq = NiControllerSequence::parse(&mut stream).unwrap();
        assert_eq!(stream.position() as usize, expected_len);
        assert_eq!(seq.name.as_deref(), Some("TestName"));
        assert_eq!(seq.controlled_blocks.len(), 0);
        assert!(seq.text_keys_ref.is_null());
    }
}

// ── NiGeomMorpherController ──────────────────────────────────────────

/// Morph target controller — drives facial animation and mesh deformation.
///
/// References NiMorphData (vertex deltas per morph target) and an array
/// of interpolators that control the blend weights over time.
#[derive(Debug)]
pub struct NiGeomMorpherController {
    pub base: NiTimeControllerBase,
    pub morpher_flags: u16,
    pub data_ref: BlockRef,
    pub always_update: u8,
    pub interpolator_weights: Vec<MorphWeight>,
}

/// An interpolator reference + weight for morph blending.
#[derive(Debug)]
pub struct MorphWeight {
    pub interpolator_ref: BlockRef,
    pub weight: f32,
}

impl NiObject for NiGeomMorpherController {
    fn block_type_name(&self) -> &'static str {
        "NiGeomMorpherController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiGeomMorpherController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let morpher_flags = stream.read_u16_le()?;
        let data_ref = stream.read_block_ref()?;
        let always_update = stream.read_u8()?;
        let num_interpolators = stream.read_u32_le()? as usize;

        let mut interpolator_weights = Vec::with_capacity(num_interpolators);
        for _ in 0..num_interpolators {
            let interpolator_ref = stream.read_block_ref()?;
            let weight = stream.read_f32_le()?;
            interpolator_weights.push(MorphWeight {
                interpolator_ref,
                weight,
            });
        }

        Ok(Self {
            base,
            morpher_flags,
            data_ref,
            always_update,
            interpolator_weights,
        })
    }
}

// ── NiMorphData ──────────────────────────────────────────────────────

/// A single morph target: name + vertex deltas.
#[derive(Debug)]
pub struct MorphTarget {
    /// Name of this morph frame (e.g., "Blink", "JawOpen").
    pub name: Option<Arc<str>>,
    /// Vertex position deltas (one per mesh vertex).
    pub vectors: Vec<[f32; 3]>,
}

/// Morph target data — vertex deltas for facial animation.
#[derive(Debug)]
pub struct NiMorphData {
    pub num_vertices: u32,
    pub relative_targets: u8,
    pub morphs: Vec<MorphTarget>,
}

impl NiObject for NiMorphData {
    fn block_type_name(&self) -> &'static str {
        "NiMorphData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiMorphData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_morphs = stream.read_u32_le()? as usize;
        let num_vertices = stream.read_u32_le()?;
        let relative_targets = stream.read_u8()?;

        let mut morphs = Vec::with_capacity(num_morphs);
        for _ in 0..num_morphs {
            // Frame name (string table indexed for version >= 10.1.0.106).
            let name = stream.read_string()?;

            // Legacy float key group — per nif.xml, each morph frame serializes
            // a KeyGroup<float> between the name and vertex deltas.
            let num_keys = stream.read_u32_le()?;
            if num_keys > 0 {
                let interpolation = stream.read_u32_le()?;
                // Key size depends on interpolation type:
                // 1 (LINEAR) = time(f32) + value(f32) = 8 bytes
                // 2 (QUADRATIC) = time + value + forward + backward = 16 bytes
                // 3 (TBC) = time + value + tension + bias + continuity = 20 bytes
                // 5 (CONSTANT) = time + value = 8 bytes
                let key_size: u64 = match interpolation {
                    1 | 5 => 8,
                    2 => 16,
                    3 => 20,
                    _ => 8, // fallback
                };
                stream.skip(key_size * num_keys as u64);
            }

            // Vertex position deltas.
            let mut vectors = Vec::with_capacity(num_vertices as usize);
            for _ in 0..num_vertices {
                let x = stream.read_f32_le()?;
                let y = stream.read_f32_le()?;
                let z = stream.read_f32_le()?;
                vectors.push([x, y, z]);
            }

            morphs.push(MorphTarget { name, vectors });
        }

        Ok(Self {
            num_vertices,
            relative_targets,
            morphs,
        })
    }
}
