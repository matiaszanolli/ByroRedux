//! AI / dialogue / quest / combat-style records.

use super::super::common::{read_lstring_or_zstring, read_u32_at, read_zstring};
use crate::esm::reader::SubRecord;

/// `PACK` AI package record. 30-procedure scheduling system
/// (guard patrols, merchant behavior, dialogue triggers, ambient
/// idles). `NpcRecord.ai_packages` carries PKID form refs; pre-#446
/// those dangled.
///
/// Only the PKDT header (package flags + procedure type) is captured
/// here — PSDT / PLDT / PKTG / PKCU / PKPA decoding lands with the
/// AI runtime per the `ai_packages_procedures.md` memo.
#[derive(Debug, Clone, Default)]
pub struct PackRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Flags bitfield from PKDT (schedule / location repeat / weapon
    /// draw / etc.). Low 16 bits on FO3/FNV, u32 on Skyrim+.
    pub package_flags: u32,
    /// Procedure type — index into the 30-procedure catalog
    /// (`Travel`, `Wander`, `Sandbox`, `Find`, `Escort`, `Follow`,
    /// `Patrol`, `Use Item At`, ...). Read from PKDT offset 4.
    pub procedure_type: u32,
}

pub fn parse_pack(form_id: u32, subs: &[SubRecord]) -> PackRecord {
    let mut out = PackRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"PKDT" if sub.data.len() >= 8 => {
                out.package_flags = read_u32_at(&sub.data, 0).unwrap_or(0);
                out.procedure_type = read_u32_at(&sub.data, 4).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// `QUST` quest record. Lifecycle container for the Story Manager and
/// Radiant Story systems. Stages (QSDT), objectives (QOBJ), aliases
/// (ALST), conditions (CTDA), scripts (SCRI) are deferred; this stub
/// surfaces the quest's identity + a handful of scalar fields so the
/// `quest_alias_system.md` / `quest_story_manager.md` memos can start
/// tracking real counts.
#[derive(Debug, Clone, Default)]
pub struct QustRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Optional FO3/FNV quest script reference (pre-Papyrus bytecode).
    pub script_ref: u32,
    /// Quest flags from DATA byte 0 (`Start Game Enabled`, `Allow
    /// Repeated Stages`, `Event Based`, ...).
    pub quest_flags: u8,
    /// Priority from DATA byte 1. Higher = displayed first in pip-boy.
    pub priority: u8,
}

pub fn parse_qust(form_id: u32, subs: &[SubRecord]) -> QustRecord {
    let mut out = QustRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"SCRI" if sub.data.len() >= 4 => {
                out.script_ref = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            b"DATA" if sub.data.len() >= 2 => {
                out.quest_flags = sub.data[0];
                out.priority = sub.data[1];
            }
            _ => {}
        }
    }
    out
}

/// `DIAL` dialogue topic record. Parent of INFO dialogue lines (which
/// live in a nested GRUP tree — tracked as a follow-up; the current
/// `extract_records` walker takes a single record type and can't
/// simultaneously emit DIAL + INFO). This stub captures the topic's
/// quest owners (QSTI refs, 4 bytes each) so NPC / quest systems can
/// enumerate topics without re-parsing.
#[derive(Debug, Clone, Default)]
pub struct DialRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Quest form IDs that own this dialogue topic (one per QSTI
    /// sub-record). FO3/FNV topics often list multiple owners.
    pub quest_refs: Vec<u32>,
    /// INFO topic responses parsed from the DIAL's `Topic Children`
    /// sub-GRUP (group_type == 7). Pre-#631 the children were silently
    /// skipped because `extract_records` filters on a single record
    /// type; this field is now populated by the dedicated
    /// `extract_dial_with_info` walker. Each entry is one branch of the
    /// dialogue (a single NPC response + its conditions / triggers).
    pub infos: Vec<InfoRecord>,
}

/// `INFO` dialogue topic response. One per branch of an `NPC says X
/// when Y` choice tree, owned by the parent `DIAL` topic via the
/// nested Topic Children GRUP. Stub captures the response text +
/// type byte + sibling links so quest / dialogue systems can
/// enumerate branches without re-parsing. Conditions (CTDA),
/// scripts (SCHR/SCDA), and edits (NAM3) are deferred until the
/// condition runtime lands. See #631.
#[derive(Debug, Clone, Default)]
pub struct InfoRecord {
    pub form_id: u32,
    /// Response text shown / spoken to the player (NAM1).
    pub response_text: String,
    /// Designer notes — usually direction for the voice actor (NAM2).
    pub designer_notes: String,
    /// `TRDT` response-data byte 0 — `Response_Type` enum (Custom /
    /// Force Greet / etc. on FO3/FNV; Combat / Death / Hello etc. on
    /// Skyrim). Captured raw; mapping to the per-game enum is
    /// downstream consumer work. 0 when TRDT is absent.
    pub response_type: u8,
    /// `TCLT` topic-link ref — IDs of other DIAL topics that this
    /// branch routes the conversation to. Multiple TCLTs are
    /// concatenated.
    pub topic_links: Vec<u32>,
    /// `PNAM` previous-info ref — the prior INFO in this branch. 0
    /// means "this is the first response in the chain".
    pub previous_info: u32,
}

pub fn parse_dial(form_id: u32, subs: &[SubRecord]) -> DialRecord {
    let mut out = DialRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"QSTI" if sub.data.len() >= 4 => {
                if let Some(q) = read_u32_at(&sub.data, 0) {
                    out.quest_refs.push(q);
                }
            }
            _ => {}
        }
    }
    out
}

pub fn parse_info(form_id: u32, subs: &[SubRecord]) -> InfoRecord {
    let mut out = InfoRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"NAM1" => out.response_text = read_lstring_or_zstring(&sub.data),
            b"NAM2" => out.designer_notes = read_zstring(&sub.data),
            b"TRDT" if !sub.data.is_empty() => {
                out.response_type = sub.data[0];
            }
            b"TCLT" if sub.data.len() >= 4 => {
                if let Some(t) = read_u32_at(&sub.data, 0) {
                    out.topic_links.push(t);
                }
            }
            b"PNAM" if sub.data.len() >= 4 => {
                out.previous_info = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// `MESG` message / popup record. Quest-tutorial banners and
/// interaction prompts. `DESC` carries the text; `QNAM` optionally
/// ties the message to a quest for clean-up on quest completion.
#[derive(Debug, Clone, Default)]
pub struct MesgRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Owning quest form ID (optional) — message clears when quest
    /// completes.
    pub owner_quest: u32,
}

pub fn parse_mesg(form_id: u32, subs: &[SubRecord]) -> MesgRecord {
    let mut out = MesgRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            b"QNAM" if sub.data.len() >= 4 => {
                out.owner_quest = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// CSTY — combat style record. NPC combat AI behavior profile
/// (aggression, stealth preference, ranged vs melee). Per-NPC
/// reference via NPC.SPCT. `CSTD` carries the FO3/FNV 124-byte
/// payload; the stub captures only the first 4 bytes of CSTD as a
/// flags scalar so the dispatch is verifiable. Full CSTD decode
/// lands with the AI consumer. See audit `FNV-D2-NEW-02` / #809.
#[derive(Debug, Clone, Default)]
pub struct CstyRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `CSTD` offset 0..4 — combat-style flag bitfield (u32). Decoded
    /// lazily per-game; vanilla FNV uses ~12 bits.
    pub csty_flags: u32,
}

pub fn parse_csty(form_id: u32, subs: &[SubRecord]) -> CstyRecord {
    let mut out = CstyRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"CSTD" if sub.data.len() >= 4 => {
                out.csty_flags = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// IDLE — idle animation record. NPC behavior tree references —
/// "lean against wall", "smoke", "drink", etc. Each NPC's PACK
/// references IDLEs by form ID. Stub captures EDID + animation file
/// path (MODL). See audit `FNV-D2-NEW-02` / #809.
#[derive(Debug, Clone, Default)]
pub struct IdleRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `MODL` — animation file path (typically `.kf`).
    pub animation_path: String,
}

pub fn parse_idle(form_id: u32, subs: &[SubRecord]) -> IdleRecord {
    let mut out = IdleRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"MODL" => out.animation_path = read_zstring(&sub.data),
            _ => {}
        }
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    #[test]
    fn parse_pack_picks_pkdt_flags_and_procedure() {
        let mut pkdt = Vec::new();
        pkdt.extend_from_slice(&0x0000_0421u32.to_le_bytes()); // flags
        pkdt.extend_from_slice(&6u32.to_le_bytes()); // procedure 6 = Patrol
        let subs = vec![sub(b"EDID", b"GuardPatrolDay\0"), sub(b"PKDT", &pkdt)];
        let p = parse_pack(0xA1A1, &subs);
        assert_eq!(p.editor_id, "GuardPatrolDay");
        assert_eq!(p.package_flags, 0x0000_0421);
        assert_eq!(p.procedure_type, 6);
    }

    #[test]
    fn parse_qust_picks_scri_and_data_flags() {
        let subs = vec![
            sub(b"EDID", b"MQ01\0"),
            sub(b"FULL", b"Main Quest\0"),
            sub(b"SCRI", &0x0010_BEEFu32.to_le_bytes()),
            sub(b"DATA", &[0x05, 20]), // flags + priority
        ];
        let q = parse_qust(0xB2B2, &subs);
        assert_eq!(q.editor_id, "MQ01");
        assert_eq!(q.full_name, "Main Quest");
        assert_eq!(q.script_ref, 0x0010_BEEF);
        assert_eq!(q.quest_flags, 0x05);
        assert_eq!(q.priority, 20);
    }

    #[test]
    fn parse_dial_accumulates_multiple_quest_refs() {
        let subs = vec![
            sub(b"EDID", b"GREETING\0"),
            sub(b"FULL", b"Greeting\0"),
            sub(b"QSTI", &0x0100_0001u32.to_le_bytes()),
            sub(b"QSTI", &0x0100_0002u32.to_le_bytes()),
            sub(b"QSTI", &0x0100_0003u32.to_le_bytes()),
        ];
        let d = parse_dial(0xC3C3, &subs);
        assert_eq!(d.quest_refs.len(), 3);
        assert_eq!(d.quest_refs[1], 0x0100_0002);
    }

    #[test]
    fn parse_mesg_picks_desc_and_owner_quest() {
        let subs = vec![
            sub(b"EDID", b"FastTravelMessage\0"),
            sub(b"FULL", b"Fast Travel\0"),
            sub(b"DESC", b"You cannot fast travel right now.\0"),
            sub(b"QNAM", &0x0002_1234u32.to_le_bytes()),
        ];
        let m = parse_mesg(0xD4D4, &subs);
        assert_eq!(m.description, "You cannot fast travel right now.");
        assert_eq!(m.owner_quest, 0x0002_1234);
    }
    #[test]
    fn parse_csty_picks_edid_csty_flags() {
        // `csyAggressive` shape: CSTD with a flag byte at offset 0.
        let mut cstd = [0u8; 124];
        cstd[0..4].copy_from_slice(&0x0000_0042_u32.to_le_bytes());
        let subs = vec![sub(b"EDID", b"csyAggressive\0"), sub(b"CSTD", &cstd)];
        let c = parse_csty(0x0008_3122, &subs);
        assert_eq!(c.editor_id, "csyAggressive");
        assert_eq!(c.csty_flags, 0x42);
    }

    #[test]
    fn parse_idle_picks_edid_modl() {
        // `IdleStandSmokingCigarette` shape: EDID + MODL pointing at
        // a `.kf` animation file in `meshes\\actors\\character\\` etc.
        let subs = vec![
            sub(b"EDID", b"IdleStandSmokingCigarette\0"),
            sub(b"MODL", b"actors\\character\\idleanims\\smoke.kf\0"),
        ];
        let i = parse_idle(0x000A_FB31, &subs);
        assert_eq!(i.editor_id, "IdleStandSmokingCigarette");
        assert!(i.animation_path.contains("smoke.kf"));
    }
}
