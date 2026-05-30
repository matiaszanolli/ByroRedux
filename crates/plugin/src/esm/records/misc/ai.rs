//! AI / dialogue / quest / combat-style records.

use super::super::common::{read_lstring_or_zstring, read_zstring};
use crate::esm::sub_reader::SubReader;
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
                let mut r = SubReader::new(&sub.data);
                out.package_flags = r.u32_or_default();
                out.procedure_type = r.u32_or_default();
            }
            _ => {}
        }
    }
    out
}

/// One stage of a quest, defined by an `INDX` / `QSDT` sub-record
/// pair. Stage data carried inside the block (CNAM log text, SCHR
/// script-on-advance) attaches to the most recently opened stage.
///
/// Stages are *defined* here; the *runtime* progress through them
/// (which stage the player has reached / completed) lives in
/// `byroredux_scripting::quest_stages::QuestStageState`. M47.1's
/// `GetStage` / `GetStageDone` condition functions read the runtime
/// state, not this list — but this list is what they validate against
/// (M47.2 consumer).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestStage {
    /// Stage index from INDX (u16). Skyrim's u32 form is truncated
    /// to u16 here — vanilla Skyrim stage indices stay well inside
    /// u16 range; if a mod ever ships a larger index this widens.
    pub index: u16,
    /// Stage flags from QSDT byte 0. Bit 0 = Start Up Stage (the
    /// stage the quest advances to when activated), bit 1 = Shut Down
    /// Stage (terminal stage), bit 2 = Keep Instance Data From Here On
    /// (Skyrim+ Radiant). Other bits per UESP.
    pub flags: u8,
    /// `CNAM` log entry text shown in the Pip-Boy / quest journal
    /// when this stage is reached. Empty when the stage carries no
    /// log entry (silent stages that only fire scripts).
    pub log_text: String,
    /// True when the stage carries an `SCHR` script (advance-time
    /// bytecode). The bytecode itself isn't decoded here — it goes
    /// through the same SCPT compiled-stream path as standalone
    /// scripts and is deferred to M47.2 (Papyrus transpiler / script
    /// runtime).
    pub has_script: bool,
}

/// One objective of a quest, defined by a `QOBJ` block. Objectives
/// surface in the Pip-Boy / quest journal; their targets drive the
/// map marker / compass indicator.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestObjective {
    /// Objective index from QOBJ (u16). Quest stages reference
    /// objectives by this index when they enable/disable markers.
    pub index: u16,
    /// Objective text (`NNAM` on Skyrim+, `CNAM` on FO3/FNV). Empty
    /// when the objective ships no display text — rare but
    /// permissible per the on-disk schema.
    pub text: String,
    /// `QSTA` target form-IDs (REFR / ACHR / objects the player
    /// should head toward). Multiple QSTA blocks → multiple targets.
    /// Stored as raw u32 form IDs; the consumer resolves to entities
    /// via the global form-ID map at quest-system runtime.
    pub target_refs: Vec<u32>,
}

/// `QUST` quest record. Lifecycle container for the Story Manager and
/// Radiant Story systems. Stages + objectives are decoded; aliases
/// (ALST) and CTDA conditions remain deferred — alias decoding is its
/// own multi-file follow-up (`quest_alias_system.md` lists 6 ref fill
/// types), and CTDA is already handled by M47.1's `ConditionList`
/// at the script-consumer side rather than per-stage here.
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
    /// All defined stages, in authoring order (which is also INDX
    /// order on every vanilla master sampled). Most quests ship 5-20
    /// stages; the longest vanilla FNV quest (Heartache by the
    /// Number) has 60+.
    pub stages: Vec<QuestStage>,
    /// `INDX` of the stage flagged Start Up Stage (QSDT bit 0). The
    /// Story Manager advances the quest to this index on activation;
    /// `None` when no stage has the bit set (rare — typically only
    /// scripted-start quests).
    pub start_up_stage: Option<u16>,
    /// All defined objectives, in authoring order. Objectives are
    /// usually a strict subset of stages (one objective per major
    /// player-visible step).
    pub objectives: Vec<QuestObjective>,
}

/// Which block-structured sub-record we're currently inside while
/// walking the QUST sub-record stream. Stage and objective blocks
/// are mutually exclusive at any point — `INDX` opens a stage block,
/// `QOBJ` opens an objective block, and either closes whatever was
/// open before.
enum QustBlock {
    None,
    Stage(QuestStage),
    Objective(QuestObjective),
}

pub fn parse_qust(form_id: u32, subs: &[SubRecord]) -> QustRecord {
    let mut out = QustRecord {
        form_id,
        ..Default::default()
    };
    let mut block = QustBlock::None;

    // Close whichever block is currently open and push it onto the
    // record. Called when a new block-opener appears or when the walk
    // finishes.
    fn flush_block(out: &mut QustRecord, block: QustBlock) {
        match block {
            QustBlock::Stage(stage) => {
                if stage.flags & 0x01 != 0 && out.start_up_stage.is_none() {
                    out.start_up_stage = Some(stage.index);
                }
                out.stages.push(stage);
            }
            QustBlock::Objective(obj) => out.objectives.push(obj),
            QustBlock::None => {}
        }
    }

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"SCRI" if sub.data.len() >= 4 => {
                out.script_ref = SubReader::new(&sub.data).u32_or_default();
            }
            b"DATA" if sub.data.len() >= 2 => {
                out.quest_flags = sub.data[0];
                out.priority = sub.data[1];
            }
            // INDX opens a stage block. Anything still open (a prior
            // stage or objective) is flushed first. Skyrim widened
            // INDX to u32; we truncate to u16 (vanilla content stays
            // well within u16 range — flag if a mod ever exceeds).
            b"INDX" if sub.data.len() >= 2 => {
                let prev = std::mem::replace(&mut block, QustBlock::None);
                flush_block(&mut out, prev);
                let mut r = SubReader::new(&sub.data);
                let index = r.u16_or_default();
                block = QustBlock::Stage(QuestStage {
                    index,
                    ..Default::default()
                });
            }
            b"QSDT" if !sub.data.is_empty() => {
                if let QustBlock::Stage(stage) = &mut block {
                    stage.flags = sub.data[0];
                }
            }
            // QOBJ opens an objective block. Same flush rule as INDX.
            b"QOBJ" if sub.data.len() >= 2 => {
                let prev = std::mem::replace(&mut block, QustBlock::None);
                flush_block(&mut out, prev);
                let mut r = SubReader::new(&sub.data);
                let index = r.u16_or_default();
                block = QustBlock::Objective(QuestObjective {
                    index,
                    ..Default::default()
                });
            }
            // CNAM is dual-purpose: stage log text inside a Stage
            // block, objective text inside an Objective block (FO3/
            // FNV authoring path; Skyrim+ moves objective text onto
            // NNAM). Dispatch on the open block.
            b"CNAM" => match &mut block {
                QustBlock::Stage(stage) => {
                    stage.log_text = read_lstring_or_zstring(&sub.data);
                }
                QustBlock::Objective(obj) => {
                    obj.text = read_lstring_or_zstring(&sub.data);
                }
                QustBlock::None => {}
            },
            // NNAM is Skyrim+ objective text. FO3/FNV objectives use
            // CNAM (handled above); both arms are defensive — an
            // older parser sniffing NNAM on FO3 just no-ops.
            b"NNAM" => {
                if let QustBlock::Objective(obj) = &mut block {
                    obj.text = read_lstring_or_zstring(&sub.data);
                }
            }
            // QSTA target reference inside an objective block. Pre-
            // Skyrim QSTA is 8 bytes (form-id u32 + flags u32);
            // Skyrim+ keeps the same prefix shape and extends the
            // tail. We only read the leading form-id — the trailing
            // flags / alias-ref are deferred until the consumer
            // needs them.
            b"QSTA" if sub.data.len() >= 4 => {
                if let QustBlock::Objective(obj) = &mut block {
                    let mut r = SubReader::new(&sub.data);
                    let target = r.u32_or_default();
                    if target != 0 {
                        obj.target_refs.push(target);
                    }
                }
            }
            // SCHR / SCDA inside a stage block mean the stage has an
            // advance-time bytecode block (Oblivion / FO3 / FNV).
            // The bytecode itself isn't decoded here — flagged for
            // M47.2's consumer.
            b"SCHR" | b"SCDA" => {
                if let QustBlock::Stage(stage) = &mut block {
                    stage.has_script = true;
                }
            }
            _ => {}
        }
    }

    // Flush whichever block was open at the end of the stream.
    flush_block(&mut out, block);

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
    /// `DATA` dialogue-type byte 0 — Topic / Conversation / Combat /
    /// Persuasion / Detection / Service / Miscellaneous (Oblivion enum).
    /// Oblivion's DATA is a single byte; FO3+ widen it (type byte +
    /// flags) but byte 0 is the type in every game, so the byte-0 read is
    /// cross-game safe. 0 (Topic) when DATA is absent. Captured raw;
    /// per-game enum mapping is downstream consumer work.
    pub dial_type: u8,
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
    /// `TRDT` Emotion Type — the low byte of the `EmotionType` `u32` at
    /// TRDT offset 0: 0=Neutral, 1=Anger, 2=Disgust, 3=Fear, 4=Sad,
    /// 5=Happy, 6=Surprise (Oblivion / FO3 / FNV; Skyrim keeps the
    /// EmotionType-u32 @0 layout). The byte-0 histogram across all
    /// 23,877 `Oblivion.esm` TRDT subrecords is exactly this 0–6
    /// distribution — it is the emotion, NOT a response number (the
    /// real response index is [`Self::response_number`]). 0 when TRDT is
    /// absent. See #1304 (was mislabeled `response_type`).
    pub emotion_type: u8,
    /// `TRDT` Response number — byte 12, after `EmotionType` (u32 @0),
    /// `Emotion Value` (i32 @4), and 4 unused bytes @8. The actual
    /// dialogue-response index within the branch. 0 when TRDT is shorter
    /// than 13 bytes. See #1304.
    pub response_number: u8,
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
                if let Ok(q) = SubReader::new(&sub.data).u32() {
                    out.quest_refs.push(q);
                }
            }
            // DATA byte 0 = dialogue type, cross-game safe (Oblivion: 1 byte;
            // FO3+: wider, byte 0 still the type). #1307 / OBL-D3-...-03.
            b"DATA" if !sub.data.is_empty() => out.dial_type = sub.data[0],
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
                // TES4 TRDT layout: EmotionType(u32 @0) + EmotionValue
                // (i32 @4) + unused[4] @8 + Response number(u8 @12) +
                // unused[3]. Byte 0 is the emotion (0–6), not a response
                // number; the response index lives at offset 12. #1304.
                out.emotion_type = sub.data[0];
                if sub.data.len() >= 13 {
                    out.response_number = sub.data[12];
                }
            }
            b"TCLT" if sub.data.len() >= 4 => {
                if let Ok(t) = SubReader::new(&sub.data).u32() {
                    out.topic_links.push(t);
                }
            }
            b"PNAM" if sub.data.len() >= 4 => {
                out.previous_info = SubReader::new(&sub.data).u32_or_default();
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
                out.owner_quest = SubReader::new(&sub.data).u32_or_default();
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
                out.csty_flags = SubReader::new(&sub.data).u32_or_default();
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
    fn parse_qust_decodes_two_stages_and_one_objective() {
        // Synthetic FNV-style quest: two stages (10 and 20) where stage
        // 10 is the start-up stage carrying log text, plus one objective
        // (1) with two QSTA targets. Mimics the on-disk INDX/QSDT/CNAM
        // ... QOBJ/NNAM/QSTA grammar.
        let start_log = b"Begin investigation.\0".to_vec();
        let mid_log = b"Reach the vault.\0".to_vec();
        let obj_text = b"Find the dam.\0".to_vec();

        let subs = vec![
            sub(b"EDID", b"DLC02\0"),
            sub(b"FULL", b"Honest Hearts\0"),
            sub(b"DATA", &[0x05, 30]), // quest_flags + priority
            // Stage 10 — start-up (QSDT bit 0) + log text + advance script.
            sub(b"INDX", &10u16.to_le_bytes()),
            sub(b"QSDT", &[0x01]),
            sub(b"CNAM", &start_log),
            sub(b"SCHR", &[0u8; 20]), // dummy SCHR — flags has_script
            // Stage 20 — log text only, no script, no start-up flag.
            sub(b"INDX", &20u16.to_le_bytes()),
            sub(b"QSDT", &[0x00]),
            sub(b"CNAM", &mid_log),
            // Objective 1 — text via CNAM (FO3/FNV path) + two targets.
            sub(b"QOBJ", &1u16.to_le_bytes()),
            sub(b"CNAM", &obj_text),
            sub(b"QSTA", &0x0010_F001u32.to_le_bytes()),
            sub(b"QSTA", &0x0010_F002u32.to_le_bytes()),
        ];
        let q = parse_qust(0xDADAu32, &subs);

        assert_eq!(q.editor_id, "DLC02");
        assert_eq!(q.full_name, "Honest Hearts");
        assert_eq!(q.quest_flags, 0x05);
        assert_eq!(q.priority, 30);

        // Stages.
        assert_eq!(q.stages.len(), 2, "two INDX blocks expected");
        assert_eq!(q.stages[0].index, 10);
        assert_eq!(q.stages[0].flags, 0x01);
        assert_eq!(q.stages[0].log_text, "Begin investigation.");
        assert!(q.stages[0].has_script);
        assert_eq!(q.stages[1].index, 20);
        assert_eq!(q.stages[1].flags, 0x00);
        assert_eq!(q.stages[1].log_text, "Reach the vault.");
        assert!(!q.stages[1].has_script);

        // First stage with QSDT bit 0 is the Start Up Stage.
        assert_eq!(q.start_up_stage, Some(10));

        // Objectives.
        assert_eq!(q.objectives.len(), 1);
        let obj = &q.objectives[0];
        assert_eq!(obj.index, 1);
        assert_eq!(obj.text, "Find the dam.");
        assert_eq!(obj.target_refs, vec![0x0010_F001, 0x0010_F002]);
    }

    #[test]
    fn parse_qust_objective_text_via_nnam_on_skyrim_path() {
        // Skyrim-shaped quest: objective text arrives via NNAM rather
        // than CNAM. Demonstrates the dual-keyword dispatch — the
        // parser doesn't care which game the bytes came from, as long
        // as the block's open marker (QOBJ) precedes the text.
        let subs = vec![
            sub(b"EDID", b"MQ302\0"),
            sub(b"QOBJ", &10u16.to_le_bytes()),
            sub(b"NNAM", b"Find the Elder Scroll.\0"),
            sub(b"QSTA", &0x0001_AAAAu32.to_le_bytes()),
        ];
        let q = parse_qust(0xEAEAu32, &subs);
        assert_eq!(q.objectives.len(), 1);
        assert_eq!(q.objectives[0].index, 10);
        assert_eq!(q.objectives[0].text, "Find the Elder Scroll.");
        assert_eq!(q.objectives[0].target_refs, vec![0x0001_AAAA]);
    }

    #[test]
    fn parse_qust_no_blocks_keeps_stages_empty() {
        // Identity-only quest with no INDX / QOBJ — stages and
        // objectives both empty, no panic.
        let subs = vec![sub(b"EDID", b"Tutorial\0"), sub(b"DATA", &[0, 0])];
        let q = parse_qust(0xF00Fu32, &subs);
        assert!(q.stages.is_empty());
        assert!(q.objectives.is_empty());
        assert_eq!(q.start_up_stage, None);
    }

    #[test]
    fn parse_qust_qsta_zero_target_dropped() {
        // QSTA with form_id 0 is the "no target" sentinel — the
        // objective opens but the empty target shouldn't push.
        let subs = vec![
            sub(b"QOBJ", &5u16.to_le_bytes()),
            sub(b"QSTA", &0u32.to_le_bytes()),
            sub(b"QSTA", &0x0010_F001u32.to_le_bytes()),
        ];
        let q = parse_qust(0xF11Fu32, &subs);
        assert_eq!(q.objectives.len(), 1);
        assert_eq!(q.objectives[0].target_refs, vec![0x0010_F001]);
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
        // DATA absent → dial_type defaults to 0 (Topic).
        assert_eq!(d.dial_type, 0);
    }

    /// #1307 / OBL-D3-...-03 — DIAL DATA byte 0 is the dialogue type.
    /// Captured for all games (Oblivion single-byte DATA here; FO3+ widen
    /// it but byte 0 is still the type). Pre-fix this byte was dropped for
    /// all 3817 Oblivion DIAL records.
    #[test]
    fn parse_dial_captures_dialogue_type_byte() {
        // Oblivion DATA: a single type byte. 3 = Persuasion in the TES4 enum.
        let subs = vec![
            sub(b"EDID", b"PersuasionTopic\0"),
            sub(b"DATA", &[3u8]),
        ];
        let d = parse_dial(0xDEAD, &subs);
        assert_eq!(d.dial_type, 3);

        // FO3+ widen DATA (type byte + flags); byte 0 still the type.
        let subs_fo3 = vec![sub(b"DATA", &[5u8, 0x01, 0x00, 0x00])];
        assert_eq!(parse_dial(0xBEEF, &subs_fo3).dial_type, 5);

        // Empty DATA must not panic and leaves the default.
        let subs_empty = vec![sub(b"DATA", &[])];
        assert_eq!(parse_dial(0xF00D, &subs_empty).dial_type, 0);
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
