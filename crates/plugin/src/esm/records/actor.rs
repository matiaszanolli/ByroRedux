//! Actor-related record parsers — NPC_, RACE, CLAS, FACT.
//!
//! NPC parsing pulls the essentials needed to spawn the NPC into the world:
//! base race/class form IDs, faction memberships, inventory list, and a
//! pointer to the head/body model. Combat stats and AI packages are stored
//! as raw form IDs for now; full evaluation lands when the AI/combat
//! systems come online.

use super::common::{read_lstring_or_zstring, read_u32_at, read_zstring};
use crate::esm::reader::SubRecord;

/// One faction the NPC belongs to, with their rank within it.
#[derive(Debug, Clone, Copy)]
pub struct FactionMembership {
    pub faction_form_id: u32,
    pub rank: i8,
}

/// One inventory entry on an NPC (`CNTO` sub-record).
#[derive(Debug, Clone, Copy)]
pub struct NpcInventoryEntry {
    pub item_form_id: u32,
    pub count: i32,
}

#[derive(Debug, Clone)]
pub struct NpcRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Model path (typically from MODL — head/body mesh, optional).
    pub model_path: String,
    /// Race form ID (RNAM).
    pub race_form_id: u32,
    /// Class form ID (CNAM).
    pub class_form_id: u32,
    /// Voice type form ID.
    pub voice_form_id: u32,
    /// Faction memberships (`SNAM` sub-records).
    pub factions: Vec<FactionMembership>,
    /// Inventory list (`CNTO` sub-records).
    pub inventory: Vec<NpcInventoryEntry>,
    /// AI packages (`PKID` sub-records, in priority order).
    pub ai_packages: Vec<u32>,
    /// Death item leveled list (DEST in some games, INAM in others).
    pub death_item_form_id: u32,
    /// Base level (from DATA).
    pub level: i16,
    /// Disposition base (from DATA).
    pub disposition_base: u8,
    /// Flags (from ACBS).
    pub acbs_flags: u32,
    /// True when the NPC record carries a `VMAD` sub-record (Skyrim+
    /// Papyrus VM attached-script blob). Presence flag only; full
    /// decoding deferred to scripting-as-ECS work. See #369.
    pub has_script: bool,
}

#[derive(Debug, Clone)]
pub struct RaceRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Skill bonuses: pairs of (skill AVIF form, bonus value).
    pub skill_bonuses: Vec<(u32, i8)>,
    /// Body part model paths (head, body, hand, foot).
    pub body_models: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ClassRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// 7 attribute weights (Strength, Perception, Endurance, Charisma,
    /// Intelligence, Agility, Luck) — order varies per game.
    pub attribute_weights: [u8; 7],
    /// Tag skill form IDs from DATA.
    pub tag_skills: Vec<u32>,
}

/// Faction-to-faction relation.
#[derive(Debug, Clone, Copy)]
pub struct FactionRelation {
    pub other_faction: u32,
    /// Modifier (-100..100, larger means more friendly).
    pub modifier: i32,
    /// Combat reaction (0=neutral, 1=enemy, 2=ally, 3=friend).
    pub combat_reaction: u8,
}

#[derive(Debug, Clone)]
pub struct FactionRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Hidden flag etc. (from DATA).
    pub flags: u32,
    pub relations: Vec<FactionRelation>,
    /// Rank index → label.
    pub ranks: Vec<String>,
}

// ── Parsers ───────────────────────────────────────────────────────────

pub fn parse_npc(form_id: u32, subs: &[SubRecord]) -> NpcRecord {
    let mut record = NpcRecord {
        form_id,
        editor_id: String::new(),
        full_name: String::new(),
        model_path: String::new(),
        race_form_id: 0,
        class_form_id: 0,
        voice_form_id: 0,
        factions: Vec::new(),
        inventory: Vec::new(),
        ai_packages: Vec::new(),
        death_item_form_id: 0,
        level: 1,
        disposition_base: 50,
        acbs_flags: 0,
        has_script: false,
    };

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),
            b"FULL" => record.full_name = read_lstring_or_zstring(&sub.data),
            b"MODL" => record.model_path = read_zstring(&sub.data),
            b"RNAM" if sub.data.len() >= 4 => {
                record.race_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            b"CNAM" if sub.data.len() >= 4 => {
                record.class_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            b"VTCK" if sub.data.len() >= 4 => {
                record.voice_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            // SNAM (FNV NPC_): faction form ID (u32) + rank (i8) + pad x3
            b"SNAM" if sub.data.len() >= 8 => {
                let faction = read_u32_at(&sub.data, 0).unwrap_or(0);
                let rank = sub.data[4] as i8;
                record.factions.push(FactionMembership {
                    faction_form_id: faction,
                    rank,
                });
            }
            // CNTO: shared with CONT
            b"CNTO" if sub.data.len() >= 8 => {
                record.inventory.push(NpcInventoryEntry {
                    item_form_id: read_u32_at(&sub.data, 0).unwrap_or(0),
                    count: i32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]),
                });
            }
            b"PKID" if sub.data.len() >= 4 => {
                record
                    .ai_packages
                    .push(read_u32_at(&sub.data, 0).unwrap_or(0));
            }
            b"INAM" if sub.data.len() >= 4 => {
                record.death_item_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            // ACBS (FNV NPC_): flags(u32), fatigue(u16), barter(u16), level(i16),
            // calc_min(u16), calc_max(u16), speed_mult(u16), karma(f32),
            // disposition_base(i16), template_flags(u16)
            b"ACBS" if sub.data.len() >= 24 => {
                record.acbs_flags = read_u32_at(&sub.data, 0).unwrap_or(0);
                record.level = i16::from_le_bytes([sub.data[8], sub.data[9]]);
                if sub.data.len() >= 22 {
                    record.disposition_base = sub.data[20];
                }
            }
            // VMAD presence-only flag — see `has_script` field doc.
            b"VMAD" => record.has_script = true,
            _ => {}
        }
    }

    record
}

pub fn parse_race(form_id: u32, subs: &[SubRecord]) -> RaceRecord {
    let mut record = RaceRecord {
        form_id,
        editor_id: String::new(),
        full_name: String::new(),
        description: String::new(),
        skill_bonuses: Vec::new(),
        body_models: Vec::new(),
    };

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),
            b"FULL" => record.full_name = read_lstring_or_zstring(&sub.data),
            b"DESC" => record.description = read_lstring_or_zstring(&sub.data),
            // DATA (FNV RACE): skill bonus pairs (u32 form + i8) ×7, then more.
            // We pull the first 7 pairs.
            b"DATA" => {
                let pair_size = 5; // u32 form_id + i8 bonus
                for i in 0..7 {
                    let off = i * pair_size;
                    if sub.data.len() < off + pair_size {
                        break;
                    }
                    let f = read_u32_at(&sub.data, off).unwrap_or(0);
                    let bonus = sub.data[off + 4] as i8;
                    if f != 0 {
                        record.skill_bonuses.push((f, bonus));
                    }
                }
            }
            // MODL appears multiple times in RACE for body parts. Collect them all.
            b"MODL" => record.body_models.push(read_zstring(&sub.data)),
            _ => {}
        }
    }

    record
}

pub fn parse_clas(form_id: u32, subs: &[SubRecord]) -> ClassRecord {
    let mut record = ClassRecord {
        form_id,
        editor_id: String::new(),
        full_name: String::new(),
        description: String::new(),
        attribute_weights: [0u8; 7],
        tag_skills: Vec::new(),
    };

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),
            b"FULL" => record.full_name = read_lstring_or_zstring(&sub.data),
            b"DESC" => record.description = read_lstring_or_zstring(&sub.data),
            // DATA layout (FNV CLAS): tag1..tag4 (4 × u32 form), flags (u32),
            // services (u32), trainer skill (i8), trainer level (u8),
            // teaches level (u8), teaches max (u8), then 7 attribute weights (u8).
            // 4*4 + 4 + 4 + 4 + 7 = 35 bytes.
            b"DATA" if sub.data.len() >= 35 => {
                for i in 0..4 {
                    let off = i * 4;
                    if let Some(f) = read_u32_at(&sub.data, off) {
                        if f != 0 {
                            record.tag_skills.push(f);
                        }
                    }
                }
                // Skip flags + services + skill/level/teaches bytes (16 + 4 = 20).
                // Attribute weights start at offset 28.
                for i in 0..7 {
                    record.attribute_weights[i] = sub.data.get(28 + i).copied().unwrap_or(0);
                }
            }
            _ => {}
        }
    }

    record
}

pub fn parse_fact(form_id: u32, subs: &[SubRecord]) -> FactionRecord {
    let mut record = FactionRecord {
        form_id,
        editor_id: String::new(),
        full_name: String::new(),
        flags: 0,
        relations: Vec::new(),
        ranks: Vec::new(),
    };

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),
            b"FULL" => record.full_name = read_lstring_or_zstring(&sub.data),
            // DATA (FNV FACT): flags is a single byte per UESP
            // `Mod_File_Format/FACT` (FO3 / FNV). The tail is a
            // variable-width payload (FNV adds `u8 unknown + f32 crime
            // gold multiplier`) that different vanilla records truncate
            // differently — reading 4 bytes pulled padding / neighbor
            // bytes into the high 24 bits, producing spurious bits 8+.
            // Only bits 0 (hidden from PC), 1 (evil), 2 (special
            // combat) are authoritative on FO3 / FNV.
            //
            // Skyrim and FO4 extend DATA to a full u32; if / when those
            // parse paths get added here, split per `GameKind`. See
            // #481 / FNV-2-L1.
            b"DATA" if !sub.data.is_empty() => {
                record.flags = sub.data[0] as u32;
            }
            // XNAM: relation entry — other faction (u32) + modifier (i32) + reaction (u32).
            // The reaction field is a full 4-byte u32 per UESP; pre-#482 the
            // parser read only the low byte via `sub.data[8]`, which happened
            // to be correct for vanilla values 0..=3 but would silently
            // truncate any future mod that extends the enum past 255.
            b"XNAM" if sub.data.len() >= 8 => {
                let other = read_u32_at(&sub.data, 0).unwrap_or(0);
                let modifier =
                    i32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]);
                let combat = if sub.data.len() >= 12 {
                    read_u32_at(&sub.data, 8).unwrap_or(0) as u8
                } else {
                    0
                };
                record.relations.push(FactionRelation {
                    other_faction: other,
                    modifier,
                    combat_reaction: combat,
                });
            }
            // MNAM: male rank label (string)
            b"MNAM" => record.ranks.push(read_zstring(&sub.data)),
            _ => {}
        }
    }

    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::reader::SubRecord;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    #[test]
    fn npc_extracts_race_class_factions_inventory() {
        let mut acbs = Vec::new();
        acbs.extend_from_slice(&0x100u32.to_le_bytes()); // flags
        acbs.extend_from_slice(&[0u8; 4]); // fatigue + barter
        acbs.extend_from_slice(&5i16.to_le_bytes()); // level
        acbs.extend_from_slice(&[0u8; 14]); // pad to 24 bytes total

        let mut snam = Vec::new();
        snam.extend_from_slice(&0xAAAAu32.to_le_bytes());
        snam.push(2u8);
        snam.extend_from_slice(&[0u8; 3]);

        let mut cnto = Vec::new();
        cnto.extend_from_slice(&0xBBBBu32.to_le_bytes());
        cnto.extend_from_slice(&3i32.to_le_bytes());

        let subs = vec![
            sub(b"EDID", b"NpcTest\0"),
            sub(b"FULL", b"Test NPC\0"),
            sub(b"RNAM", &0xCCCCu32.to_le_bytes()),
            sub(b"CNAM", &0xDDDDu32.to_le_bytes()),
            sub(b"ACBS", &acbs),
            sub(b"SNAM", &snam),
            sub(b"CNTO", &cnto),
            sub(b"PKID", &0xEEEEu32.to_le_bytes()),
        ];
        let n = parse_npc(0x500, &subs);
        assert_eq!(n.editor_id, "NpcTest");
        assert_eq!(n.race_form_id, 0xCCCC);
        assert_eq!(n.class_form_id, 0xDDDD);
        assert_eq!(n.factions.len(), 1);
        assert_eq!(n.factions[0].faction_form_id, 0xAAAA);
        assert_eq!(n.factions[0].rank, 2);
        assert_eq!(n.inventory.len(), 1);
        assert_eq!(n.inventory[0].item_form_id, 0xBBBB);
        assert_eq!(n.inventory[0].count, 3);
        assert_eq!(n.ai_packages, vec![0xEEEE]);
        assert_eq!(n.acbs_flags, 0x100);
        assert_eq!(n.level, 5);
    }

    #[test]
    fn npc_vmad_flips_has_script() {
        // Regression: #369 — Skyrim NPCs with attached Papyrus scripts
        // were not discoverable. The presence-only `has_script` flag
        // is the audit's minimum-viable signal.
        let subs = vec![
            sub(b"EDID", b"ScriptedActor\0"),
            sub(b"VMAD", b"\x05\x00\x02\x00\x00\x00"),
        ];
        let n = parse_npc(0x501, &subs);
        assert!(n.has_script);
    }

    #[test]
    fn npc_without_vmad_has_script_false() {
        // Sibling check — bare NPC must keep has_script at default.
        let subs = vec![sub(b"EDID", b"PlainActor\0")];
        let n = parse_npc(0x502, &subs);
        assert!(!n.has_script);
    }

    #[test]
    fn fact_extracts_relations_and_ranks() {
        let mut xnam = Vec::new();
        xnam.extend_from_slice(&0x123u32.to_le_bytes());
        xnam.extend_from_slice(&(-50i32).to_le_bytes());
        xnam.extend_from_slice(&1u32.to_le_bytes()); // combat reaction = enemy

        let subs = vec![
            sub(b"EDID", b"NCR\0"),
            sub(b"FULL", b"NCR\0"),
            sub(b"DATA", &0x01u32.to_le_bytes()),
            sub(b"XNAM", &xnam),
            sub(b"MNAM", b"Recruit\0"),
            sub(b"MNAM", b"Trooper\0"),
            sub(b"MNAM", b"Veteran\0"),
        ];
        let f = parse_fact(0x42, &subs);
        assert_eq!(f.editor_id, "NCR");
        assert_eq!(f.flags, 0x01);
        assert_eq!(f.relations.len(), 1);
        assert_eq!(f.relations[0].other_faction, 0x123);
        assert_eq!(f.relations[0].modifier, -50);
        assert_eq!(f.relations[0].combat_reaction, 1);
        assert_eq!(f.ranks, vec!["Recruit", "Trooper", "Veteran"]);
    }

    /// Regression for #482: the reaction field is a 4-byte u32 per
    /// UESP spec, not a single byte. A typical u32 like `0x00000002`
    /// (ally) must round-trip through the parser correctly — this is
    /// the minimal "parser reads the right field width" check.
    ///
    /// Pre-fix the parser read only `sub.data[8]` (the low byte). For
    /// vanilla values 0..=3 the low byte happens to equal the full
    /// value, so the test passes with the old code too — its job is
    /// to document the spec and catch a future regression that goes
    /// back to byte access.
    #[test]
    fn fact_xnam_combat_reaction_reads_full_u32() {
        let mut xnam = Vec::new();
        xnam.extend_from_slice(&0x999u32.to_le_bytes()); // other faction
        xnam.extend_from_slice(&0i32.to_le_bytes()); // modifier
        xnam.extend_from_slice(&2u32.to_le_bytes()); // combat reaction = ally (full 4 bytes)

        let subs = vec![
            sub(b"EDID", b"AllyFaction\0"),
            sub(b"DATA", &0x00u32.to_le_bytes()),
            sub(b"XNAM", &xnam),
        ];
        let f = parse_fact(0x77, &subs);
        assert_eq!(f.relations.len(), 1);
        assert_eq!(
            f.relations[0].combat_reaction, 2,
            "ally (combat_reaction=2) must round-trip — parser must read 4 bytes"
        );
    }

    /// Regression for #481 (FNV-2-L1): FACT DATA is a single-byte
    /// flags field on FO3 / FNV per UESP. Pre-fix the parser read 4
    /// bytes, so any garbage in bytes 1..=3 of the DATA payload
    /// (variable tail, neighbour padding) leaked into the high 24
    /// bits. Only bits 0–2 are authoritative; verify the fix rejects
    /// the high bytes.
    #[test]
    fn fact_data_reads_only_low_byte() {
        // Simulate a DATA sub-record whose first byte holds the real
        // flags (bit 0 = hidden) and whose remaining bytes are the
        // FNV tail (e.g. `unknown: u8 + crime_gold_multiplier: f32`)
        // or just padding. Pre-fix the parser treated all 4 bytes as
        // flags and reported `0x0EFF_FF01`; post-fix it reports `0x01`.
        let data = [
            0x01u8, // real flags — bit 0 = hidden
            0xFFu8, 0xFFu8, 0xEFu8, // tail / padding bytes; must NOT become flags
        ];
        let subs = vec![sub(b"EDID", b"SpookyFaction\0"), sub(b"DATA", &data)];
        let f = parse_fact(0x88, &subs);
        assert_eq!(
            f.flags, 0x01,
            "only byte 0 of DATA carries flag bits on FO3 / FNV (#481)"
        );
    }

    /// Edge case: a zero-length DATA sub-record must not crash and
    /// must leave flags at the default (0).
    #[test]
    fn fact_data_empty_leaves_flags_default() {
        let subs = vec![sub(b"EDID", b"PlaceholderFaction\0"), sub(b"DATA", &[])];
        let f = parse_fact(0x89, &subs);
        assert_eq!(
            f.flags, 0,
            "empty DATA must not override the FactionRecord default"
        );
    }
}
