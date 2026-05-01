//! Items extracted from ../mod.rs (refactor stage C).
//!
//! Lead types: NiMultiTargetTransformController, NiControllerManager, ControlledBlock, NiControllerSequence, BsRefractionFirePeriodController.

use super::*;

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
    /// Phase offset within the cycle (radians). Present on
    /// v ∈ [10.1.0.106, 10.4.0.1]; defaults to 0 on later content.
    pub phase: f32,
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

        // Phase — only present in v ∈ [10.1.0.106, 10.4.0.1]. nif.xml:
        //   <field name="Phase" type="float" since="10.1.0.106"
        //          until="10.4.0.1" />
        // Skipping it on pre-Oblivion content (e.g. Oblivion's
        // v=10.2.0.0 / bsver=9 ships in `meshes/dungeons/ayleidruins/
        // interior/traps/artrapchannelspikes01.nif`) misaligned
        // start_time/stop_time/manager_ref by 4 bytes, then read
        // `accum_root_name`'s u32 length from the stop_time slot.
        // The downstream block read mid-string and the file truncated
        // after kept block 8 with 233 dropped (audit O5-2 / #687).
        // NiSequence Phase: nif.xml `since="10.1.0.106" until="10.4.0.1"`.
        // `until=` is exclusive — see #765 sweep. Field absent at
        // v10.4.0.1 exactly.
        let phase = if stream.version() >= NifVersion(0x0A01006A)
            && stream.version() < NifVersion(0x0A040001)
        {
            stream.read_f32_le()?
        } else {
            0.0
        };

        let start_time = stream.read_f32_le()?;
        let stop_time = stream.read_f32_le()?;

        // Play Backwards — exactly v=10.1.0.106. None of our targets
        // ship content at that exact version (Oblivion is 20.0.0.x,
        // pre-Oblivion sample files we've seen are 10.2.0.0), so this
        // is a no-op today; left in for completeness against nif.xml.
        if stream.version() == NifVersion(0x0A01006A) {
            let _play_backwards = stream.read_u8()?;
        }

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
        if stream.version() >= NifVersion(0x0A010071) && stream.version() < NifVersion(0x14010001) {
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
            phase,
            start_time,
            stop_time,
            manager_ref,
            accum_root_name,
            anim_note_refs,
        })
    }
}

/// Animates the fire-period of refraction shader effects (FO3).
#[derive(Debug)]
pub struct BsRefractionFirePeriodController {
    pub base: NiTimeControllerBase,
    pub interpolator_ref: BlockRef,
}

impl NiObject for BsRefractionFirePeriodController {
    fn block_type_name(&self) -> &'static str {
        "BSRefractionFirePeriodController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsRefractionFirePeriodController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let interpolator_ref = stream.read_block_ref()?;
        Ok(Self {
            base,
            interpolator_ref,
        })
    }
}
