//! Items extracted from ../mod.rs (refactor stage C).
//!
//! Lead types: NiMultiTargetTransformController, NiControllerManager, ControlledBlock, NiControllerSequence, BsRefractionFirePeriodController.

use super::*;
use crate::impl_ni_object;
use crate::version::bsver;

#[derive(Debug)]
pub struct NiMultiTargetTransformController {
    pub base: NiTimeControllerBase,
    pub extra_targets: Vec<BlockRef>,
}

impl NiMultiTargetTransformController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiInterpController layer (base + Manager Controlled bool, #1506).
        let base = parse_interp_controller_base(stream)?;
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

impl NiControllerSequence {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // ── Inherited NiSequence fields ──────────────────────────────
        // nif.xml `<niobject name="NiSequence">`:
        //   Name              string
        //   Accum Root Name   string  until="10.1.0.103"
        //   Text Keys         Ref     until="10.1.0.103"
        //   Num Controlled Blocks  uint
        //   Array Grow By     uint    since="10.1.0.106"
        let name = stream.read_string()?;

        // #2345 — the `until=10.1.0.103` prologue pair. Absent before this
        // fix, so any NiSequence/NiControllerSequence at or below 10.1.0.103
        // under-read by a string plus a ref and mis-advanced the stream into
        // `Num Controlled Blocks`, in a version band with no size anchor to
        // recover from.
        //
        // NOTE these are NiSequence's OWN `Accum Root Name` / `Text Keys`,
        // NOT the same-named NiControllerSequence fields read after the
        // block array below. The derived class re-declares both with the
        // opposite gate (`since="10.1.0.106"`), so the two never coexist:
        // <= 10.1.0.103 reads them here, >= 10.1.0.106 reads them there, and
        // the 10.1.0.104/105 gap reads neither. Conflating them is easy and
        // wrong — the issue that prompted this fix did exactly that.
        let seq_accum_root_name = if stream.version() <= NifVersion::V10_1_0_103 {
            stream.read_string()?
        } else {
            None
        };
        // #3468 — bound, not discarded. `text_keys_ref` has exactly one
        // consumer (`anim/sequence.rs`, which feeds `collect_text_key_events`
        // and thence the ECS footstep / hit / sound channel), so leaving this
        // on the floor silently yielded zero text events for every sequence on
        // this band — the exact asymmetry #2345 avoided one line above for the
        // accum root name.
        let seq_text_keys_ref = if stream.version() <= NifVersion::V10_1_0_103 {
            stream.read_block_ref()?
        } else {
            BlockRef::NULL
        };

        let num_controlled_blocks = stream.read_u32_le()?;

        // Array Grow By (since 10.1.0.106)
        let array_grow_by = if stream.version() >= NifVersion::V10_1_0_106 {
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
        let uses_string_palette = stream.version() >= NifVersion::V10_2_0_0
            && stream.version() < NifVersion::STRING_TABLE_THRESHOLD;
        let mut controlled_blocks = stream.allocate_vec(num_controlled_blocks)?;
        for _ in 0..num_controlled_blocks {
            // Target Name — nif.xml `SizedString until="10.1.0.103"`. #2345:
            // never read at all before this fix, so every pre-10.1.0.104
            // ControlledBlock started one length-prefixed string too early.
            //
            // #3468 SIBLING — and, like the Text Keys ref above, it was then
            // read and discarded. `Target Name` is the pre-10.1.0.104
            // declaration of the same "which node does this block drive"
            // concept `Node Name` (`since="10.1.0.104"`) carries above that
            // band, so it feeds the same `node_name` slot. Dropping it left
            // `node_name == None` for the whole band, which short-circuits
            // `anim/controlled_block.rs`'s target resolution — every channel
            // in the sequence failed to bind, not just its text events.
            let target_name = if stream.version() <= NifVersion::V10_1_0_103 {
                Some(Arc::from(stream.read_sized_string()?.as_str()))
            } else {
                None
            };
            // Interpolator — nif.xml `since="10.1.0.106"`. #2345: read
            // unconditionally before this fix, a 4-byte over-read on
            // anything below that version.
            let interpolator_ref = if stream.version() >= NifVersion::V10_1_0_106 {
                stream.read_block_ref()?
            } else {
                BlockRef::NULL
            };
            let controller_ref = stream.read_block_ref()?;
            // Blend Interpolator (Ref) + Blend Index (ushort): nif.xml
            // gates both `since=10.1.0.104 until=10.1.0.110`, between
            // Controller and Priority. Missing them under-read every
            // v10.1.0.x ControlledBlock by 6 bytes, cascading truncation
            // through the sizeless format (#1508). Only the discarded
            // refs/index live here — downstream consumes neither — so the
            // read exists purely for byte-correct advancement. Every
            // retail Bethesda title is 20.x (> 10.1.0.110) and skips this.
            if stream.version() >= NifVersion::V10_1_0_104
                && stream.version() <= NifVersion::V10_1_0_110
            {
                let _blend_interpolator = stream.read_block_ref()?;
                let _blend_index = stream.read_u16_le()?;
            }
            // Priority — nif.xml `since="10.1.0.106" vercond="#BSSTREAM#"`.
            // #2345: only the `#BSSTREAM#` half (bsver > 0) was applied, so a
            // Bethesda file BELOW 10.1.0.106 read a phantom priority byte
            // that isn't in the layout. Both halves are required.
            let priority = if stream.version() >= NifVersion::V10_1_0_106
                && bsver > crate::version::bsver::PRE_BETHESDA
            {
                stream.read_u8()?
            } else {
                0
            };

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
                // Modern string-table (or 10.1.0.104-113 inline) format.
                //
                // #2345 — nif.xml declares the five IDTag strings twice, in
                // two disjoint bands: `since="10.1.0.104" until="10.1.0.113"`
                // (inline) and `since="20.1.0.1"` (string-table). Below
                // 10.1.0.104 they DO NOT EXIST — the ControlledBlock ends
                // after Controller/Priority. Reading them there consumed five
                // phantom length-prefixed strings and destroyed the stream.
                // The palette band (10.2.0.0-20.1.0.0) is handled above and
                // does not overlap either — 10.2.0.0 sorts after 10.1.0.113.
                let has_id_tag_strings = (stream.version() >= NifVersion::V10_1_0_104
                    && stream.version() <= NifVersion::V10_1_0_113)
                    || stream.version() >= NifVersion::STRING_TABLE_THRESHOLD;
                let (node_name, property_type, controller_type, controller_id, interpolator_id) =
                    if has_id_tag_strings {
                        (
                            stream.read_string()?,
                            stream.read_string()?,
                            stream.read_string()?,
                            stream.read_string()?,
                            stream.read_string()?,
                        )
                    } else {
                        // #3468 SIBLING — below 10.1.0.104 the node this
                        // block drives is named by `Target Name`, read in
                        // the prologue above. Same slot, disjoint gate.
                        (target_name.clone(), None, None, None, None)
                    };
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

        // ── NiControllerSequence's own fields ────────────────────────
        // #2345 — nif.xml gates EVERY field of the derived class
        // `since="10.1.0.106"`: Weight, Text Keys, Cycle Type, Frequency,
        // Phase, Start/Stop Time, Play Backwards, Manager, Accum Root Name.
        // Below that version a NiControllerSequence is structurally just its
        // NiSequence base — the block ends after the ControlledBlock array.
        // Reading this group unconditionally consumed ~30 phantom bytes on
        // any sub-10.1.0.106 file. `Phase` and `Play Backwards` already
        // carried their own gates, which is what made the surrounding gap
        // easy to miss: the two rarest fields were guarded and the eight
        // ordinary ones were not.
        //
        // Defaults are nif.xml's own (`weight` 1.0, `frequency` 1.0,
        // `cycle_type` CYCLE_CLAMP = 0, `start_time` FLT_MAX,
        // `stop_time` FLT_MIN) so a pre-10.1.0.106 sequence presents the
        // same neutral values the format itself specifies.
        let has_ctlr_seq_fields = stream.version() >= NifVersion::V10_1_0_106;

        let weight = if has_ctlr_seq_fields {
            stream.read_f32_le()?
        } else {
            1.0
        };
        // The derived class's own Text Keys ref (`since="10.1.0.106"`).
        // Below that band it comes from the NiSequence base field read in
        // the prologue instead — same concept declared twice with disjoint
        // gates, exactly like `accum_root_name` below (#3468).
        let text_keys_ref = if has_ctlr_seq_fields {
            stream.read_block_ref()?
        } else {
            seq_text_keys_ref
        };
        let cycle_type = if has_ctlr_seq_fields {
            stream.read_u32_le()?
        } else {
            0
        };
        let frequency = if has_ctlr_seq_fields {
            stream.read_f32_le()?
        } else {
            1.0
        };

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
        // Both boundaries are inclusive per the version.rs doctrine —
        // field present at v in [10.1.0.106, 10.4.0.1].
        let phase = if stream.version() >= NifVersion::V10_1_0_106
            && stream.version() <= NifVersion::V10_4_0_1
        {
            stream.read_f32_le()?
        } else {
            0.0
        };

        let start_time = if has_ctlr_seq_fields {
            stream.read_f32_le()?
        } else {
            f32::MAX
        };
        let stop_time = if has_ctlr_seq_fields {
            stream.read_f32_le()?
        } else {
            f32::MIN
        };

        // Play Backwards — exactly v=10.1.0.106. None of our targets
        // ship content at that exact version (Oblivion is 20.0.0.x,
        // pre-Oblivion sample files we've seen are 10.2.0.0), so this
        // is a no-op today; left in for completeness against nif.xml.
        if stream.version() == NifVersion::V10_1_0_106 {
            let _play_backwards = stream.read_u8()?;
        }

        let manager_ref = if has_ctlr_seq_fields {
            stream.read_block_ref()?
        } else {
            BlockRef::NULL
        };
        // The derived class's own Accum Root Name (`since="10.1.0.106"`).
        // Below that band the accumulation root comes from the NiSequence
        // base field read in the prologue instead — the two are the same
        // concept declared twice with disjoint gates, so exactly one is
        // present for any given version and the component sees one value.
        let accum_root_name = if has_ctlr_seq_fields {
            stream.read_string()?
        } else {
            seq_accum_root_name
        };

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
        if stream.version() >= NifVersion::V10_1_0_113
            && stream.version() < NifVersion::STRING_TABLE_THRESHOLD
        {
            let _deprecated_string_palette_ref = stream.read_block_ref()?;
        }

        // Anim notes — layout diverges by BSVER (#432):
        //   FO3-era (BSVER 24–28): single Ref<BSAnimNotes>
        //   Skyrim+ (BSVER > 28):   u16 count + Vec<Ref<BSAnimNotes>>
        // Normalise both into the same Vec so downstream consumers only
        // see one shape. Older BSVERs (< 24) carry no anim notes at all.
        let anim_note_refs = if bsver > bsver::ANIM_NOTES_THRESHOLD {
            let num = stream.read_u16_le()? as u32;
            let mut refs = stream.allocate_vec(num)?;
            for _ in 0..num {
                refs.push(stream.read_block_ref()?);
            }
            refs
        } else if (bsver::FO3_ANIM_NOTES_LOWER..=bsver::ANIM_NOTES_THRESHOLD).contains(&bsver) {
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

impl_ni_object!(
    NiMultiTargetTransformController,
    NiControllerManager,
    NiControllerSequence,
    BsRefractionFirePeriodController => "BSRefractionFirePeriodController",
);
