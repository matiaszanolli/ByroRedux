use super::super::common::{read_lstring_or_zstring, CommonNamedFields};
use crate::esm::reader::{GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;

#[derive(Debug, Clone, Default)]
pub struct ClassRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// 7 base SPECIAL attribute values (Strength, Perception, Endurance,
    /// Charisma, Intelligence, Agility, Luck), each 0–10.
    ///
    /// FNV/FO3 source: the **`ATTR` subrecord** (fopdoc `CLAS`) — one
    /// 7-byte struct on FNV, seven single-byte `ATTR` subrecords on FO3
    /// (same order). These are **absolute base attributes, not weights**:
    /// an auto-calc NPC adopts its class's base attributes as its SPECIAL,
    /// from which skills derive (#1663). The FNV `DATA` subrecord carries
    /// only the tag skills + flags/services (28 bytes, no attributes) —
    /// the pre-#1663 reader looked for them at `DATA[28..35]`, a layout
    /// that never matched real 28-byte FNV `DATA`. `[0; 7]` on Oblivion
    /// (which uses [`Self::primary_attributes`] + [`Self::specialization`])
    /// and whenever no `ATTR` is present.
    pub base_attributes: [u8; 7],
    /// Tag skill form IDs from FNV DATA. Empty on Oblivion (see
    /// [`Self::major_skills`] for the analogous field).
    pub tag_skills: Vec<u32>,
    /// Oblivion-only: 2 × u32 primary attribute indices (0=Strength
    /// .. 7=Luck per OpenMW's `SkillIndex` neighbour set). Read from
    /// bytes 0..8 of the 52-byte DATA. `None` outside Oblivion.
    pub primary_attributes: Option<(u32, u32)>,
    /// Oblivion-only: u32 specialization at DATA offset 8.
    /// `0 = Combat`, `1 = Magic`, `2 = Stealth`. `None` outside Oblivion.
    pub specialization: Option<u32>,
    /// Oblivion-only: 7 × u32 major skill indices (`SkillIndex` enum
    /// values 0x0C..=0x20) at DATA offset 12..40. Empty outside
    /// Oblivion.
    ///
    /// The audit description (#968) at filing time said "14 × u32",
    /// but its own test assertion said `len() == 7`. Empirical probe
    /// against vanilla `Oblivion.esm` confirms 7 majors: every CLAS
    /// DATA sub-record is exactly 52 bytes, and Knight (form 0x836)
    /// decodes as `[Block, Illusion, HeavyArmor, Blunt, Blade,
    /// Speechcraft, HandToHand]`.
    pub major_skills: Vec<u32>,
    /// Oblivion-only: u32 race-class flags at DATA offset 40. Bit 0
    /// = Playable. `None` outside Oblivion (the FNV 35-byte arm
    /// reads its own `flags` into a different position; not split
    /// out here because it's not a current consumer).
    ///
    /// Parsed and real-data-verified (`clas_oblivion_knight_against_vanilla`)
    /// but intentionally has **no production consumer yet** — this is
    /// forward-sequencing for CHARAL (the per-game character-rules
    /// abstraction layer, `docs/engine/charal.md`), whose Oblivion
    /// class-flag pass is the consumer (e.g. a playable/spellmaking-
    /// eligibility gate). Flagged here so it reads as deliberate, not
    /// as a rediscovered "surprise" gap. See #2089 (DIM3-OBL-02).
    pub flags_oblivion: Option<u32>,
}

pub fn parse_clas(form_id: u32, subs: &[SubRecord], game: GameKind) -> ClassRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    let mut record = ClassRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        description: String::new(),
        base_attributes: [0u8; 7],
        tag_skills: Vec::new(),
        primary_attributes: None,
        specialization: None,
        major_skills: Vec::new(),
        flags_oblivion: None,
    };

    let is_oblivion = matches!(game, GameKind::Oblivion);
    // FO3 splits the 7 base attributes across 7 single-byte `ATTR`
    // subrecords; this tracks the next slot to fill so they accumulate in
    // order. FNV's one 7-byte `ATTR` fills all 7 in a single pass.
    let mut attr_idx = 0usize;

    for sub in subs {
        match &sub.sub_type {
            b"DESC" => record.description = read_lstring_or_zstring(&sub.data),
            // DATA layout (Oblivion CLAS — 48 or 52 bytes per empirical
            // probe against vanilla Oblivion.esm, #968; histogram is
            // 79 × 52-byte + 31 × 48-byte):
            //   2 × u32 primary attribute indices         (offset 0..8)
            //   u32 specialization (0=Combat/1=Mag/2=Sth) (offset 8..12)
            //   7 × u32 major skill indices               (offset 12..40)
            //   u32 race-class flags (bit 0 = Playable)   (offset 40..44)
            //   u32 services                              (offset 44..48)
            //   i8 trainer skill + u8 trainer level + 2 B (offset 48..52, OPTIONAL)
            //
            // Knight (form 0x836) is a 52-byte record: primary=(0=Strength,
            // 6=Personality), spec=0 (Combat), majors=[0x0F Block, 0x17
            // Illusion, 0x12 HeavyArmor, 0x10 Blunt, 0x0E Blade, 0x20
            // Speechcraft, 0x11 HandToHand]. 31 vanilla classes
            // (Hunter, Priest, Noble, TGGrayFoxClass, etc.) ship the
            // 48-byte variant — same primary block, no trainer tail.
            //
            // The audit (#968) described the layout as 60 bytes / 14
            // major skills — wrong on both counts. Its own test
            // assertion said `len() == 7`, which matches the empirical
            // truth.
            b"DATA" if is_oblivion && sub.data.len() >= 48 => {
                let mut r = SubReader::new(&sub.data);
                let a0 = r.u32_or_default();
                let a1 = r.u32_or_default();
                record.primary_attributes = Some((a0, a1));
                record.specialization = r.u32().ok();
                for _ in 0..7 {
                    if let Ok(s) = r.u32() {
                        record.major_skills.push(s);
                    }
                }
                record.flags_oblivion = r.u32().ok();
            }
            // DATA layout (FNV/FO3 CLAS — 28 bytes, fopdoc `CLAS`):
            // tag1..tag4 (4 × i32 skill enum), flags (u32), buys/sells +
            // services (u32), teaches (i8), max training level (u8),
            // unused (2 B). Only the 4 tag skills are read here. The base
            // SPECIAL attributes are NOT in DATA — they're in the separate
            // `ATTR` subrecord (below). Pre-#1663 this arm gated on `>= 35`
            // and read 7 attribute bytes from `DATA[28..35]`, a layout that
            // never matched real 28-byte FNV `DATA` (so it silently no-op'd
            // on real content). Gate at `>= 16` — all we consume is the tag
            // block. Stays `!is_oblivion`-gated so the wider Oblivion DATA
            // routes to its own arm above.
            b"DATA" if !is_oblivion && sub.data.len() >= 16 => {
                let mut r = SubReader::new(&sub.data);
                for _ in 0..4 {
                    if let Ok(f) = r.u32() {
                        if f != 0 {
                            record.tag_skills.push(f);
                        }
                    }
                }
            }
            // ATTR subrecord (FNV/FO3 CLAS, fopdoc): the 7 base SPECIAL
            // attributes (Str, Per, End, Cha, Int, Agi, Luck), each a u8.
            // FNV ships one 7-byte struct; FO3 ships 7 single-byte `ATTR`
            // subrecords. Folding both: append every byte into the next
            // open slot until the 7 are filled (a 7-byte struct fills them
            // in one pass; seven 1-byte records fill one each). Oblivion's
            // race-style `ATTR` is handled in `parse_race`, not here.
            b"ATTR" if !is_oblivion => {
                for &byte in sub.data.iter() {
                    if attr_idx < record.base_attributes.len() {
                        record.base_attributes[attr_idx] = byte;
                        attr_idx += 1;
                    }
                }
            }
            _ => {}
        }
    }

    record
}
