//! `QUST` quest records — stages, objectives, and the Skyrim+ VMAD
//! fragment-dispatch bindings.

use super::super::common::{read_lstring_or_zstring, read_zstring};
use super::super::condition::{parse_ctda, remap_condition_form_ids, ConditionList};
use super::super::script_instance::{parse_quest_fragments, QuestScriptFragment};
use crate::esm::reader::SubRecord;
use crate::esm::sub_reader::SubReader;

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
    /// Conditions attached to this stage (CTDA sub-records). Evaluated
    /// when the stage is displayed or executed.
    pub conditions: ConditionList,
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
    /// Stage→`Fragment_N` bindings from the QUST `VMAD` fragment section
    /// (Skyrim+). Each entry names the compiled quest script + the
    /// fragment function the runtime runs when the quest reaches that
    /// stage — the M47.2 fragment-dispatch input. Empty on FO3/FNV
    /// (pre-Papyrus; they use `script_ref`) and on Skyrim+ quests whose
    /// VMAD attaches only non-fragment utility scripts.
    pub fragments: Vec<QuestScriptFragment>,
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

pub fn parse_qust(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<crate::esm::reader::FormIdRemap>,
) -> QustRecord {
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
            // Skyrim+ Papyrus attachment. The scripts section is decoded
            // by the generic path elsewhere; here we only want the QUST-
            // specific trailing fragment section (stage→`Fragment_N`),
            // the M47.2 fragment-dispatch input.
            b"VMAD" => {
                out.fragments = parse_quest_fragments(&sub.data);
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
            b"CTDA" => {
                if let QustBlock::Stage(stage) = &mut block {
                    if let Some(mut cond) = parse_ctda(sub) {
                        remap_condition_form_ids(&mut cond, remap);
                        stage.conditions.push(cond);
                    }
                }
            }
            _ => {}
        }
    }

    // Flush whichever block was open at the end of the stream.
    flush_block(&mut out, block);

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
    fn parse_qust_picks_scri_and_data_flags() {
        let subs = vec![
            sub(b"EDID", b"MQ01\0"),
            sub(b"FULL", b"Main Quest\0"),
            sub(b"SCRI", &0x0010_BEEFu32.to_le_bytes()),
            sub(b"DATA", &[0x05, 20]), // flags + priority
        ];
        let q = parse_qust(0xB2B2, &subs, &None);
        assert_eq!(q.editor_id, "MQ01");
        assert_eq!(q.full_name, "Main Quest");
        assert_eq!(q.script_ref, 0x0010_BEEF);
        assert_eq!(q.quest_flags, 0x05);
        assert_eq!(q.priority, 20);
    }

    #[test]
    fn parse_qust_decodes_vmad_stage_fragment_bindings() {
        // A QUST carrying a VMAD with an empty scripts section + one
        // stage fragment (stage 30 → Fragment_4) surfaces the binding on
        // `QustRecord.fragments` — the M47.2 fragment-dispatch input.
        let mut vmad = Vec::new();
        // scripts section: version 5, objFmt 2, zero scripts.
        vmad.extend_from_slice(&5i16.to_le_bytes());
        vmad.extend_from_slice(&2i16.to_le_bytes());
        vmad.extend_from_slice(&0u16.to_le_bytes());
        // fragment section: version 2, one fragment.
        vmad.push(2u8);
        vmad.extend_from_slice(&1u16.to_le_bytes()); // fragmentCount
        let file = b"QF_TestQuest_00001234";
        vmad.extend_from_slice(&(file.len() as u16).to_le_bytes());
        vmad.extend_from_slice(file);
        vmad.extend_from_slice(&30u16.to_le_bytes()); // stage
        vmad.extend_from_slice(&0i16.to_le_bytes());
        vmad.extend_from_slice(&0i32.to_le_bytes());
        vmad.push(1u8);
        vmad.extend_from_slice(&(file.len() as u16).to_le_bytes());
        vmad.extend_from_slice(file);
        let frag = b"Fragment_4";
        vmad.extend_from_slice(&(frag.len() as u16).to_le_bytes());
        vmad.extend_from_slice(frag);
        vmad.extend_from_slice(&0u16.to_le_bytes()); // aliasCount

        let subs = vec![sub(b"EDID", b"TestQuest\0"), sub(b"VMAD", &vmad)];
        let q = parse_qust(0x0000_1234, &subs, &None);
        assert_eq!(q.fragments.len(), 1);
        assert_eq!(q.fragments[0].stage, 30);
        assert_eq!(q.fragments[0].script_name, "QF_TestQuest_00001234");
        assert_eq!(q.fragments[0].fragment_name, "Fragment_4");
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
        let q = parse_qust(0xDADAu32, &subs, &None);

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
        let q = parse_qust(0xEAEAu32, &subs, &None);
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
        let q = parse_qust(0xF00Fu32, &subs, &None);
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
        let q = parse_qust(0xF11Fu32, &subs, &None);
        assert_eq!(q.objectives.len(), 1);
        assert_eq!(q.objectives[0].target_refs, vec![0x0010_F001]);
    }

    #[test]
    fn parse_qust_stage_ctda_attaches_to_its_stage() {
        // Minimal CTDA: type_byte=0x00, pad[3], comparand f32=1.0 LE,
        // function_index=9 LE (u32), param_1=0 (u32), param_2=0 (u32),
        // run_on=0 (u32), ref_fid=0 (u32). FO3/FNV layout (28 bytes).
        let mut ctda = Vec::new();
        ctda.push(0x00u8); // type_byte (offset 0)
        ctda.extend_from_slice(&[0u8; 3]); // pad (offsets 1-3)
        ctda.extend_from_slice(&1.0f32.to_le_bytes()); // comparand (offsets 4-7)
        ctda.extend_from_slice(&9u32.to_le_bytes()); // function_index (offsets 8-11, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // param_1 (offsets 12-15, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // param_2 (offsets 16-19, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // run_on (offsets 20-23, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // ref_fid (offsets 24-27, u32)

        let subs = vec![
            sub(b"EDID", b"TestQuest\0"),
            sub(b"INDX", &0u16.to_le_bytes()),
            sub(b"QSDT", &[0x01]),
            sub(b"CTDA", &ctda),
        ];
        let q = parse_qust(0xABCD, &subs, &None);
        assert_eq!(q.stages.len(), 1);
        assert_eq!(q.stages[0].conditions.len(), 1);
        assert_eq!(q.stages[0].conditions[0].function_index, 9);
    }
}
