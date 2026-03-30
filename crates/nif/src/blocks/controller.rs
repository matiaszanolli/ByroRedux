//! NIF animation controller blocks.
//!
//! Covers the NiTimeController hierarchy and NiControllerSequence.
//! Parsed enough to advance the stream correctly; actual animation
//! interpretation comes later.

use crate::stream::NifStream;
use crate::types::BlockRef;
use crate::version::NifVersion;
use super::NiObject;
use std::any::Any;
use std::io;

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
            next_controller_ref, flags, frequency, phase,
            start_time, stop_time, target_ref,
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
    fn as_any(&self) -> &dyn Any { self }
}

impl NiTimeController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Ok(Self { base: NiTimeControllerBase::parse(stream)? })
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
    fn as_any(&self) -> &dyn Any { self }
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
        Ok(Self { base, interpolator_ref })
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
    fn as_any(&self) -> &dyn Any { self }
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
        Ok(Self { base, interpolator_ref, target_color })
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
    fn as_any(&self) -> &dyn Any { self }
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
        Ok(Self { base, interpolator_ref, shader_map, texture_slot, operation })
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
    fn as_any(&self) -> &dyn Any { self }
}

impl NiMultiTargetTransformController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let num_extra_targets = stream.read_u16_le()? as usize;
        let mut extra_targets = Vec::with_capacity(num_extra_targets);
        for _ in 0..num_extra_targets {
            extra_targets.push(stream.read_block_ref()?);
        }
        Ok(Self { base, extra_targets })
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
    fn as_any(&self) -> &dyn Any { self }
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
        Ok(Self { base, cumulative, sequence_refs, object_palette_ref })
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
    pub node_name: Option<String>,
    pub property_type: Option<String>,
    pub controller_type: Option<String>,
    pub controller_id: Option<String>,
    pub interpolator_id: Option<String>,
}

#[derive(Debug)]
pub struct NiControllerSequence {
    // NiSequence fields
    pub name: Option<String>,
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
    pub accum_root_name: Option<String>,
    pub anim_note_refs: Vec<BlockRef>,
}

impl NiObject for NiControllerSequence {
    fn block_type_name(&self) -> &'static str {
        "NiControllerSequence"
    }
    fn as_any(&self) -> &dyn Any { self }
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
        let bsver = stream.variant().bsver();
        let mut controlled_blocks = Vec::with_capacity(num_controlled_blocks);
        for _ in 0..num_controlled_blocks {
            let interpolator_ref = stream.read_block_ref()?;
            let controller_ref = stream.read_block_ref()?;
            // Priority byte (BSVER > 0, i.e. any Bethesda game)
            let priority = if bsver > 0 {
                stream.read_u8()?
            } else {
                0
            };
            let node_name = stream.read_string()?;
            let property_type = stream.read_string()?;
            let controller_type = stream.read_string()?;
            let controller_id = stream.read_string()?;
            let interpolator_id = stream.read_string()?;
            controlled_blocks.push(ControlledBlock {
                interpolator_ref, controller_ref, priority,
                node_name, property_type, controller_type,
                controller_id, interpolator_id,
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
            name, controlled_blocks, array_grow_by,
            weight, text_keys_ref, cycle_type, frequency,
            start_time, stop_time, manager_ref, accum_root_name,
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
            strings: vec!["TestName".to_string()],
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
        assert_eq!(seq.name, Some("TestName".to_string()));
        assert_eq!(seq.controlled_blocks.len(), 0);
        assert!(seq.text_keys_ref.is_null());
    }
}
