//! NIF animation controller blocks.
//!
//! Covers the NiTimeController hierarchy and NiControllerSequence.
//! Parsed enough to advance the stream correctly; actual animation
//! interpretation comes later.

use super::base::NiObjectNETData;
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

// ── BSShaderController family ──────────────────────────────────────────
//
// The four (+1) Bethesda shader property controllers each wrap
// `NiSingleInterpController` with a trailing `uint` enum identifying
// which shader slot the animation drives. Pre-#407 the trailing u32
// was unconsumed and block_size recovery seeked past; #407 added the
// read but dropped the value on the floor. This block preserves the
// value on a typed `BsShaderController` so the animation importer
// can route key streams to the correct shader uniform once the
// animated-shader pipeline lands. See #350 / audit S5-02.

/// Which shader-property controller kind and its enum payload.
///
/// The enum value decodes differently per block type (per nif.xml
/// `EffectShaderControlledVariable` / `EffectShaderControlledColor` /
/// `LightingShaderControlledFloat` / `LightingShaderControlledColor`),
/// so keep each variant as its own newtype until the importer grows a
/// real dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderControllerKind {
    /// `BSEffectShaderPropertyFloatController.Controlled Variable` — drives
    /// `EmissiveMultiple`, `Falloff Start Angle`, `Alpha`, `U Offset`, etc.
    EffectFloat(u32),
    /// `BSEffectShaderPropertyColorController.Controlled Color` — drives
    /// the base-color tint slot (alpha component ignored).
    EffectColor(u32),
    /// `BSLightingShaderPropertyFloatController.Controlled Variable` —
    /// drives `RefractionStrength`, `GlossinessMultiple`, shader-specific
    /// slots (skin tint, parallax, multi-layer, etc.).
    LightingFloat(u32),
    /// `BSLightingShaderPropertyColorController.Controlled Color` — drives
    /// emissive / skin tint / hair tint / sparkle colors per shader type.
    LightingColor(u32),
    /// `BSLightingShaderPropertyUShortController.Controlled Variable` —
    /// short-valued slot (wetness index, snow-material index).
    LightingUShort(u32),
}

/// Skyrim+ shader-property controller — `NiSingleInterpController` plus
/// a 4-byte controlled-variable enum.
#[derive(Debug)]
pub struct BsShaderController {
    /// Original block type name (e.g. `"BSEffectShaderPropertyFloatController"`)
    /// so telemetry and downstream dispatch can match the RTTI. One of 5
    /// values: `BSEffectShaderPropertyFloatController`,
    /// `BSEffectShaderPropertyColorController`,
    /// `BSLightingShaderPropertyFloatController`,
    /// `BSLightingShaderPropertyColorController`,
    /// `BSLightingShaderPropertyUShortController`.
    pub type_name: &'static str,
    pub base: NiSingleInterpController,
    pub kind: ShaderControllerKind,
}

impl NiObject for BsShaderController {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsShaderController {
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        let base = NiSingleInterpController::parse(stream)?;
        let controlled_variable = stream.read_u32_le()?;
        let kind = match type_name {
            "BSEffectShaderPropertyFloatController" => {
                ShaderControllerKind::EffectFloat(controlled_variable)
            }
            "BSEffectShaderPropertyColorController" => {
                ShaderControllerKind::EffectColor(controlled_variable)
            }
            "BSLightingShaderPropertyFloatController" => {
                ShaderControllerKind::LightingFloat(controlled_variable)
            }
            "BSLightingShaderPropertyColorController" => {
                ShaderControllerKind::LightingColor(controlled_variable)
            }
            "BSLightingShaderPropertyUShortController" => {
                ShaderControllerKind::LightingUShort(controlled_variable)
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown BsShaderController type name: {other}"),
                ));
            }
        };
        Ok(Self {
            type_name,
            base,
            kind,
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
        let num_extra_targets = stream.read_u16_le()? as u32;
        let mut extra_targets = stream.allocate_vec(num_extra_targets)?;
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
        let num_sequences = stream.read_u32_le()?;
        let mut sequence_refs = stream.allocate_vec(num_sequences)?;
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
///
/// There are two disjoint on-disk layouts for the string fields, and
/// which one a file uses depends on its NIF version:
///
/// - **v ≥ 20.1.0.1** (FNV, Skyrim, FO4+): each string is an index into
///   the file's global string table. The importer resolves them to the
///   `node_name` / `property_type` / `controller_type` / `controller_id`
///   / `interpolator_id` `Option<Arc<str>>` fields during parse.
///
/// - **10.2.0.0 ≤ v ≤ 20.1.0.0** (Oblivion, Morrowind BBBB-era content):
///   the block has no strings inline; instead it carries a
///   `string_palette_ref` pointing at an `NiStringPalette` block plus
///   five `u32` byte offsets into that palette. The palette itself
///   stores the concatenated UTF-8 names; a downstream importer pass
///   slices them out (see [`NiStringPalette::get_string`]). The
///   `Option<Arc<str>>` name fields stay `None` on this path — the
///   parser does not cross-link blocks.
///
/// Both layouts are present in the struct to keep the type simple;
/// callers pick whichever set is populated based on
/// `string_palette_ref.is_null()`. See issue #107.
#[derive(Debug)]
pub struct ControlledBlock {
    pub interpolator_ref: BlockRef,
    pub controller_ref: BlockRef,
    pub priority: u8,
    /// Resolved string (modern format) or `None` (palette format or
    /// unresolved).
    pub node_name: Option<Arc<str>>,
    pub property_type: Option<Arc<str>>,
    pub controller_type: Option<Arc<str>>,
    pub controller_id: Option<Arc<str>>,
    pub interpolator_id: Option<Arc<str>>,
    /// Palette-format fields (Oblivion / Morrowind BBBB era). Null ref
    /// on the modern string-table path.
    pub string_palette_ref: BlockRef,
    pub node_name_offset: u32,
    pub property_type_offset: u32,
    pub controller_type_offset: u32,
    pub controller_id_offset: u32,
    pub interpolator_id_offset: u32,
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
        let num_controlled_blocks = stream.read_u32_le()?;

        // Array Grow By (since 10.1.0.106)
        let array_grow_by = if stream.version() >= NifVersion(0x0A01006A) {
            stream.read_u32_le()?
        } else {
            0
        };

        // ControlledBlock array. The layout of the per-block string
        // fields switches twice across the version range:
        //
        //   v >= 20.1.0.1              → modern string-table format
        //                                (FNV, Skyrim, FO4+)
        //   10.2.0.0 <= v <= 20.1.0.0  → string-palette format
        //                                (Oblivion, pre-FNV Bethesda)
        //                                BlockRef + 5 × u32 offsets
        //   v < 10.2.0.0               → inline strings (Morrowind
        //                                BBBB era, handled by
        //                                read_string's pre-20.1 branch)
        //
        // The old code unconditionally called read_string() even on the
        // Oblivion path, where that helper reads a u32 length prefix
        // followed by bytes. Against real Oblivion .kf files, the first
        // u32 is actually a palette offset (typically a small value like
        // 0x00000006), which read_string happily treated as a 6-byte
        // inline string and then went 5 more bytes past the descriptor,
        // corrupting the stream for every subsequent block. See #107.
        let bsver = stream.bsver();
        let uses_string_palette =
            stream.version() >= NifVersion(0x0A020000) && stream.version() < NifVersion(0x14010001);
        let mut controlled_blocks = stream.allocate_vec(num_controlled_blocks)?;
        for _ in 0..num_controlled_blocks {
            let interpolator_ref = stream.read_block_ref()?;
            let controller_ref = stream.read_block_ref()?;
            // Priority byte (BSVER > 0, i.e. any Bethesda game)
            let priority = if bsver > 0 { stream.read_u8()? } else { 0 };

            if uses_string_palette {
                // Oblivion-era: palette ref + 5 byte offsets.
                let string_palette_ref = stream.read_block_ref()?;
                let node_name_offset = stream.read_u32_le()?;
                let property_type_offset = stream.read_u32_le()?;
                let controller_type_offset = stream.read_u32_le()?;
                let controller_id_offset = stream.read_u32_le()?;
                let interpolator_id_offset = stream.read_u32_le()?;
                controlled_blocks.push(ControlledBlock {
                    interpolator_ref,
                    controller_ref,
                    priority,
                    node_name: None,
                    property_type: None,
                    controller_type: None,
                    controller_id: None,
                    interpolator_id: None,
                    string_palette_ref,
                    node_name_offset,
                    property_type_offset,
                    controller_type_offset,
                    controller_id_offset,
                    interpolator_id_offset,
                });
            } else {
                // Modern string-table (or pre-10.2 inline) format.
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
                    string_palette_ref: BlockRef::NULL,
                    node_name_offset: 0,
                    property_type_offset: 0,
                    controller_type_offset: 0,
                    controller_id_offset: 0,
                    interpolator_id_offset: 0,
                });
            }
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

        // Deprecated string-palette link (Gamebryo 2.3
        // `NiControllerSequence::LoadBinary`, v ∈ [10.1.0.113, 20.1.0.1)):
        // a trailing Ref<NiStringPalette> that was kept so the conversion
        // code could resolve the IDTag handle offsets into real strings
        // when loading older content. Oblivion (20.0.0.4 / 20.0.0.5) sits
        // in that window; skipping this field left a 4-byte drift that
        // mis-started every block after block 0 in every Oblivion KF —
        // `NiTransformInterpolator` and `NiStringPalette` then read
        // garbage counts and aborted the parse, so `import_kf` returned
        // zero clips on all 1843 Oblivion KF files. FO3/FNV (v20.0.0.5+
        // with BSVER >= 24) use the modern string-table layout and
        // skip this field. See #402 (audit premise was wrong — Oblivion
        // uses NiControllerSequence, not NiSequenceStreamHelper).
        if stream.version() >= NifVersion(0x0A010071)
            && stream.version() < NifVersion(0x14010001)
        {
            let _deprecated_string_palette_ref = stream.read_block_ref()?;
        }

        // Anim notes — layout diverges by BSVER (#432):
        //   FO3/FNV (BSVER 24–28):  single Ref<BSAnimNotes>
        //   Skyrim+ (BSVER > 28):   u16 count + Vec<Ref<BSAnimNotes>>
        // Normalise both into the same Vec so downstream consumers only
        // see one shape. Older BSVERs (< 24) carry no anim notes at all.
        let anim_note_refs = if bsver > 28 {
            let num = stream.read_u16_le()? as u32;
            let mut refs = stream.allocate_vec(num)?;
            for _ in 0..num {
                refs.push(stream.read_block_ref()?);
            }
            refs
        } else if (24..=28).contains(&bsver) {
            vec![stream.read_block_ref()?]
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

    pub(super) fn make_header_fnv() -> NifHeader {
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

    pub(super) fn write_time_controller_base(data: &mut Vec<u8>) {
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

    /// Regression: #350 / S5-02. Every BSShaderProperty*Controller
    /// block carries a trailing u32 enum identifying the driven slot.
    /// Pre-fix the dispatch discarded the value (`_controlled_variable`)
    /// and emitted `Box<NiSingleInterpController>`, so the animation
    /// importer had no way to learn which shader uniform to drive. The
    /// typed `BsShaderController` now preserves the enum in
    /// `ShaderControllerKind` and reports its original RTTI name.
    #[test]
    fn parse_bs_shader_controller_preserves_controlled_variable() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data); // 26 bytes
                                                 // NiSingleInterpController: interpolator_ref (since 10.1.0.104,
                                                 // FNV v=20.2.0.7 is above that).
        data.extend_from_slice(&5i32.to_le_bytes()); // interpolator_ref
                                                      // BSShaderController trailing enum.
        data.extend_from_slice(&3u32.to_le_bytes()); // controlled_variable = 3
        assert_eq!(data.len(), 34);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = BsShaderController::parse(&mut stream, "BSEffectShaderPropertyFloatController")
            .expect("shader controller with 4-byte enum tail must parse");
        assert_eq!(stream.position() as usize, data.len());
        assert_eq!(ctrl.type_name, "BSEffectShaderPropertyFloatController");
        assert_eq!(ctrl.base.interpolator_ref.index(), Some(5));
        assert_eq!(ctrl.kind, ShaderControllerKind::EffectFloat(3));
    }

    /// Each of the five controller type names must map to its own
    /// `ShaderControllerKind` variant so downstream dispatch can match
    /// on the kind rather than re-parsing the type string. Verifies the
    /// u32 payload rides through identically on all five.
    #[test]
    fn parse_bs_shader_controller_dispatches_all_five_kinds() {
        let header = make_header_fnv();
        for (type_name, expected) in [
            (
                "BSEffectShaderPropertyFloatController",
                ShaderControllerKind::EffectFloat(7),
            ),
            (
                "BSEffectShaderPropertyColorController",
                ShaderControllerKind::EffectColor(7),
            ),
            (
                "BSLightingShaderPropertyFloatController",
                ShaderControllerKind::LightingFloat(7),
            ),
            (
                "BSLightingShaderPropertyColorController",
                ShaderControllerKind::LightingColor(7),
            ),
            (
                "BSLightingShaderPropertyUShortController",
                ShaderControllerKind::LightingUShort(7),
            ),
        ] {
            let mut data = Vec::new();
            write_time_controller_base(&mut data);
            data.extend_from_slice(&0i32.to_le_bytes()); // interpolator_ref
            data.extend_from_slice(&7u32.to_le_bytes()); // controlled_variable

            let mut stream = NifStream::new(&data, &header);
            let ctrl = BsShaderController::parse(&mut stream, type_name).unwrap_or_else(|e| {
                panic!("{type_name} should parse: {e}");
            });
            assert_eq!(
                stream.position() as usize,
                data.len(),
                "{type_name} must consume all 34 bytes"
            );
            assert_eq!(ctrl.kind, expected, "{type_name} dispatched to wrong kind");
            assert_eq!(ctrl.type_name, type_name);
        }
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

    /// Build an Oblivion-era header (v20.0.0.5, user_version=11, uv2=11).
    /// String table is empty — Oblivion doesn't use it, and per-block
    /// strings go through the NiStringPalette format instead.
    fn make_header_oblivion() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_0_0_5,
            little_endian: true,
            user_version: 11,
            user_version_2: 11,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    /// Regression test for issue #107: Oblivion .kf files encode the
    /// ControlledBlock string fields via a NiStringPalette block ref +
    /// five byte offsets (since 10.2.0.0, until 20.1.0.0). The old
    /// parser called `read_string` unconditionally and mis-parsed the
    /// first u32 offset as a string length, shifting the stream and
    /// cascading into corrupted downstream blocks. The fix switches to
    /// a version branch; this test pins the Oblivion path.
    #[test]
    fn parse_controller_sequence_oblivion_string_palette_format() {
        let header = make_header_oblivion();
        let mut data = Vec::new();

        // NiSequence pre-10.1 string encoding: `read_string` returns
        // Ok(None) on len=0, so a 4-byte zero-length acts as an empty
        // "name" header field.
        data.extend_from_slice(&0u32.to_le_bytes()); // name: empty inline string
        data.extend_from_slice(&1u32.to_le_bytes()); // num_controlled_blocks
        data.extend_from_slice(&0u32.to_le_bytes()); // array_grow_by

        // One ControlledBlock in Oblivion palette format:
        //   interpolator_ref (i32)
        //   controller_ref   (i32)
        //   priority         (u8)          — bsver=11 > 0, so present
        //   string_palette_ref (i32)
        //   node_name_offset        (u32)
        //   property_type_offset    (u32)
        //   controller_type_offset  (u32)
        //   controller_id_offset    (u32)
        //   interpolator_id_offset  (u32)
        data.extend_from_slice(&12i32.to_le_bytes()); // interpolator_ref
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
        data.push(42); // priority
        data.extend_from_slice(&9i32.to_le_bytes()); // string_palette_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // node_name_offset
        data.extend_from_slice(&6u32.to_le_bytes()); // property_type_offset
        data.extend_from_slice(&11u32.to_le_bytes()); // controller_type_offset
        data.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // controller_id_offset (unset sentinel)
        data.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // interpolator_id_offset

        // NiControllerSequence trailer (same on all post-10.1 paths).
        data.extend_from_slice(&1.0f32.to_le_bytes()); // weight
        data.extend_from_slice(&(-1i32).to_le_bytes()); // text_keys_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // cycle_type
        data.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
        data.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
        data.extend_from_slice(&1.0f32.to_le_bytes()); // stop_time
        data.extend_from_slice(&(-1i32).to_le_bytes()); // manager_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // accum_root_name: empty inline
        // #402 — Oblivion (v ∈ [10.1.0.113, 20.1.0.1)) trails a
        // Ref<NiStringPalette>. Gamebryo 2.3's LoadBinary reads this so
        // the legacy IDTag palette offsets can be converted to
        // NiFixedStrings during link; on-disk it sits between
        // accum_root_name and the anim-note block.
        data.extend_from_slice(&9i32.to_le_bytes()); // deprecated string palette ref

        // Oblivion bsver=11, 11 <= 28 → no anim note list, so don't
        // append anything here.

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let seq = NiControllerSequence::parse(&mut stream)
            .expect("Oblivion NiControllerSequence must parse the palette format");
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "Oblivion parse consumed {} bytes, expected {}",
            stream.position(),
            expected_len,
        );

        assert_eq!(seq.controlled_blocks.len(), 1);
        let cb = &seq.controlled_blocks[0];
        assert_eq!(cb.interpolator_ref.index(), Some(12));
        assert!(cb.controller_ref.is_null());
        assert_eq!(cb.priority, 42);
        // Palette fields must be populated, name fields left None.
        assert_eq!(cb.string_palette_ref.index(), Some(9));
        assert_eq!(cb.node_name_offset, 0);
        assert_eq!(cb.property_type_offset, 6);
        assert_eq!(cb.controller_type_offset, 11);
        assert_eq!(cb.controller_id_offset, 0xFFFF_FFFF);
        assert_eq!(cb.interpolator_id_offset, 0xFFFF_FFFF);
        assert!(cb.node_name.is_none());
        assert!(cb.property_type.is_none());
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
        let num_interpolators = stream.read_u32_le()?;

        let mut interpolator_weights = stream.allocate_vec(num_interpolators)?;
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

        // Sanity cap: a real NIF never has more than a few thousand
        // vertices per morph target (the Oblivion face morph data tops
        // out around 1k verts). If we see something absurd, the block
        // has drifted — bail out rather than allocate several GB. The
        // caller's per-block recovery path will seek past the block.
        if num_morphs > 65_536 || num_vertices > 65_536 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "NiMorphData: implausible num_morphs={num_morphs} \
                     num_vertices={num_vertices} — block drifted"
                ),
            ));
        }

        // Morph element layout per nif.xml (see <struct name="Morph">):
        //
        //   since 10.1.0.106:          frame_name: string
        //   until 10.1.0.0:            num_keys: u32
        //                              interpolation: KeyType (u32)
        //                              keys: Key<float>[num_keys]
        //   since 10.1.0.104
        //     until 20.1.0.2
        //     && BSVER < 10:           legacy_weight: f32
        //   (always):                  vectors: Vec3[num_vertices]
        //
        // The "until 10.1.0.0" branch is pre-NetImmerse legacy content
        // — NONE of the games Redux targets (Morrowind 4.0.0.0 included)
        // fall into it, because the type was deprecated well before
        // 10.1. The previous implementation read those fields
        // unconditionally, which walked off the end of a valid Oblivion
        // morph and allocated a ~118 GB vector when a garbage num_keys
        // happened to be a huge number.
        //
        // Oblivion (v20.0.0.5, BSVER in 0..=11) hits the legacy_weight
        // window. FNV / FO3 (BSVER 34) and everything later do not.
        let version = stream.version();
        let bsver = stream.bsver();
        let has_keys = version <= NifVersion(0x0A010000);
        let has_legacy_weight =
            version >= NifVersion(0x0A010068) && version <= NifVersion(0x14010002) && bsver < 10;

        // Already bounded by the 65_536 sanity check above; route
        // through allocate_vec for consistency with #408 sweep.
        let mut morphs = stream.allocate_vec(num_morphs as u32)?;
        for _ in 0..num_morphs {
            // Frame name (string table indexed from 10.1.0.106).
            let name = if version >= NifVersion(0x0A01006A) {
                stream.read_string()?
            } else {
                None
            };

            if has_keys {
                let num_keys = stream.read_u32_le()? as u64;
                let interpolation = stream.read_u32_le()?;
                let key_size: u64 = match interpolation {
                    1 | 5 => 8, // LINEAR / CONSTANT: time(f32) + value(f32)
                    2 => 16,    // QUADRATIC: time + value + fwd + bwd
                    3 => 20,    // TBC: time + value + tension + bias + continuity
                    other => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "NiMorphData: unknown float key interpolation {other} \
                                 with {num_keys} keys — stream position unreliable"
                            ),
                        ));
                    }
                };
                stream.skip(key_size * num_keys)?;
            }

            if has_legacy_weight {
                let _legacy_weight = stream.read_f32_le()?;
            }

            // Vertex deltas — guarded against an absurd num_vertices
            // that would otherwise OOM the process on a corrupt block.
            // The hard cap stays as defensive belt; allocate_vec also
            // bounds against remaining stream bytes (#408).
            stream.allocate_vec::<[f32; 3]>((num_vertices as u32).min(1_000_000))?;
            let points = stream.read_ni_point3_array(num_vertices as usize)?;
            let vectors: Vec<[f32; 3]> = points.into_iter().map(|p| [p.x, p.y, p.z]).collect();

            morphs.push(MorphTarget { name, vectors });
        }

        Ok(Self {
            num_vertices,
            relative_targets,
            morphs,
        })
    }
}

// ── NiSequenceStreamHelper ─────────────────────────────────────────────
//
// Pre-Skyrim animation root used by Oblivion / Morrowind / FO3 / FNV KF
// files. Inherits from NiObjectNET with no extra fields: the per-bone
// drivers hang off the controller chain (NiKeyframeController instances)
// and the text keys hang off the extra_data list.
//
// We don't currently consume this from the animation importer — that
// work remains as a follow-up — but parsing it here lets Oblivion KF
// files load without hard-failing on unknown block types (v20.0.0.5 has
// no block_sizes fallback).

#[derive(Debug)]
pub struct NiSequenceStreamHelper {
    pub net: NiObjectNETData,
}

impl NiObject for NiSequenceStreamHelper {
    fn block_type_name(&self) -> &'static str {
        "NiSequenceStreamHelper"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiSequenceStreamHelper {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Ok(Self {
            net: NiObjectNETData::parse(stream)?,
        })
    }
}

// ── NiUVController ────────────────────────────────────────────────────
//
// DEPRECATED (pre-10.1), REMOVED (20.3). The last Bethesda game that
// ships with NiUVController is Oblivion (v20.0.0.5) — water, fire, and
// banner meshes rely on it to scroll texture coordinates. Inherits
// from NiTimeController with two trailing fields: target_attribute (u16)
// and data ref (NiUVData). See issue #156... wait #154.
//
// The parser is stateless beyond the NiTimeController base; the actual
// keyframe data lives in the referenced NiUVData block. The UV channel
// extractor in anim.rs can pick it up later — parsing is the blocker.

#[derive(Debug)]
pub struct NiUVController {
    pub base: NiTimeControllerBase,
    /// Texture slot index to animate. 0 = base, 1 = normal, etc. Rarely
    /// non-zero in Bethesda content.
    pub target_attribute: u16,
    /// Ref to the NiUVData block with the four KeyGroup channels.
    pub data_ref: BlockRef,
}

impl NiObject for NiUVController {
    fn block_type_name(&self) -> &'static str {
        "NiUVController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiUVController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let target_attribute = stream.read_u16_le()?;
        let data_ref = stream.read_block_ref()?;
        Ok(Self {
            base,
            target_attribute,
            data_ref,
        })
    }
}

// ── NiLookAtController ────────────────────────────────────────────────
// Inherits NiTimeController. DEPRECATED (10.2), REMOVED (20.5) — appears
// in Oblivion/FO3/FNV/Skyrim-LE but never in Skyrim-SE+. Orients a target
// NiNode at a follow target; the engine later replaced this with
// NiLookAtInterpolator on a plain NiTransformController. See #228.

/// Legacy look-at constraint controller.
///
/// Rotates its owning block so that a chosen axis points at the
/// `look_at_ref` target every frame. The `flags` bit layout is the
/// `LookAtFlags` from nif.xml:
///   - bit 0: LOOK_FLIP (invert the follow axis)
///   - bit 1: LOOK_Y_AXIS (follow axis = Y instead of X)
///   - bit 2: LOOK_Z_AXIS (follow axis = Z instead of X)
///
/// The `flags` field is only present from version 10.1.0.0 onwards; on
/// earlier files only `look_at_ref` follows the base.
#[derive(Debug)]
pub struct NiLookAtController {
    pub base: NiTimeControllerBase,
    pub look_at_flags: u16,
    pub look_at_ref: BlockRef,
}

impl NiObject for NiLookAtController {
    fn block_type_name(&self) -> &'static str {
        "NiLookAtController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiLookAtController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let look_at_flags = if stream.version() >= NifVersion(0x0A010000) {
            stream.read_u16_le()?
        } else {
            0
        };
        let look_at_ref = stream.read_block_ref()?;
        Ok(Self {
            base,
            look_at_flags,
            look_at_ref,
        })
    }
}

// ── NiPathController ──────────────────────────────────────────────────
// Inherits NiTimeController. DEPRECATED (10.2), REMOVED (20.5) — cutscene
// and environmental animation spline follower. The engine later replaced
// this with NiPathInterpolator on a plain NiTransformController. See #228.

/// Legacy spline-path follower controller.
///
/// Walks the owning block along a 3D spline defined by `path_data_ref`
/// (NiPosData with XYZ keys) parameterized by `percent_data_ref`
/// (NiFloatData mapping time → [0, 1] along the path). `bank_dir` +
/// `max_bank_angle` drive roll around the motion axis, `smoothing`
/// dampens tangent changes, and `follow_axis` picks which local axis
/// tracks the tangent (0 = X, 1 = Y, 2 = Z).
///
/// The `path_flags` field is only present from version 10.1.0.0 onwards.
#[derive(Debug)]
pub struct NiPathController {
    pub base: NiTimeControllerBase,
    pub path_flags: u16,
    pub bank_dir: i32,
    pub max_bank_angle: f32,
    pub smoothing: f32,
    pub follow_axis: i16,
    pub path_data_ref: BlockRef,
    pub percent_data_ref: BlockRef,
}

impl NiObject for NiPathController {
    fn block_type_name(&self) -> &'static str {
        "NiPathController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiPathController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let path_flags = if stream.version() >= NifVersion(0x0A010000) {
            stream.read_u16_le()?
        } else {
            0
        };
        let bank_dir = stream.read_i32_le()?;
        let max_bank_angle = stream.read_f32_le()?;
        let smoothing = stream.read_f32_le()?;
        // follow_axis is nominally `short` in nif.xml but the defined
        // range is 0/1/2 (X/Y/Z); read as u16 and reinterpret.
        let follow_axis = stream.read_u16_le()? as i16;
        let path_data_ref = stream.read_block_ref()?;
        let percent_data_ref = stream.read_block_ref()?;
        Ok(Self {
            base,
            path_flags,
            bank_dir,
            max_bank_angle,
            smoothing,
            follow_axis,
            path_data_ref,
            percent_data_ref,
        })
    }
}

#[cfg(test)]
mod path_lookat_tests {
    use super::tests::*;
    use super::*;

    #[test]
    fn parse_look_at_controller_32_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // look_at_flags = LOOK_Y_AXIS (bit 1)
        data.extend_from_slice(&0x0002u16.to_le_bytes());
        // look_at_ref = 7
        data.extend_from_slice(&7i32.to_le_bytes());
        assert_eq!(data.len(), 32);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiLookAtController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 32);
        assert_eq!(ctrl.look_at_flags, 0x0002);
        assert_eq!(ctrl.look_at_ref.index(), Some(7));
        assert!(ctrl.base.next_controller_ref.is_null());
    }

    #[test]
    fn parse_path_controller_48_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // path_flags
        data.extend_from_slice(&0x0000u16.to_le_bytes());
        // bank_dir = 1 (positive)
        data.extend_from_slice(&1i32.to_le_bytes());
        // max_bank_angle = 0.5 rad
        data.extend_from_slice(&0.5f32.to_le_bytes());
        // smoothing = 0.25
        data.extend_from_slice(&0.25f32.to_le_bytes());
        // follow_axis = 1 (Y)
        data.extend_from_slice(&1i16.to_le_bytes());
        // path_data_ref = 11
        data.extend_from_slice(&11i32.to_le_bytes());
        // percent_data_ref = 12
        data.extend_from_slice(&12i32.to_le_bytes());
        // 26 (base) + 2 + 4 + 4 + 4 + 2 + 4 + 4 = 50
        assert_eq!(data.len(), 50);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiPathController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 50);
        assert_eq!(ctrl.path_flags, 0);
        assert_eq!(ctrl.bank_dir, 1);
        assert_eq!(ctrl.max_bank_angle, 0.5);
        assert_eq!(ctrl.smoothing, 0.25);
        assert_eq!(ctrl.follow_axis, 1);
        assert_eq!(ctrl.path_data_ref.index(), Some(11));
        assert_eq!(ctrl.percent_data_ref.index(), Some(12));
    }

    #[test]
    fn dispatch_routes_path_and_look_at_controllers() {
        use crate::blocks::parse_block;
        let header = make_header_fnv();

        // ── NiLookAtController ───────────
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&0x0004u16.to_le_bytes()); // LOOK_Z_AXIS
        data.extend_from_slice(&3i32.to_le_bytes());
        let size = data.len() as u32;
        let mut stream = NifStream::new(&data, &header);
        let block = parse_block("NiLookAtController", &mut stream, Some(size))
            .expect("NiLookAtController dispatch");
        let c = block.as_any().downcast_ref::<NiLookAtController>().unwrap();
        assert_eq!(c.look_at_flags, 0x0004);
        assert_eq!(c.look_at_ref.index(), Some(3));

        // ── NiPathController ─────────────
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&0x0000u16.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes()); // bank_dir = Negative
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.1f32.to_le_bytes());
        data.extend_from_slice(&2i16.to_le_bytes()); // Z
        data.extend_from_slice(&5i32.to_le_bytes());
        data.extend_from_slice(&6i32.to_le_bytes());
        let size = data.len() as u32;
        let mut stream = NifStream::new(&data, &header);
        let block = parse_block("NiPathController", &mut stream, Some(size))
            .expect("NiPathController dispatch");
        let c = block.as_any().downcast_ref::<NiPathController>().unwrap();
        assert_eq!(c.bank_dir, -1);
        assert_eq!(c.follow_axis, 2);
        assert_eq!(c.path_data_ref.index(), Some(5));
        assert_eq!(c.percent_data_ref.index(), Some(6));
    }
}
