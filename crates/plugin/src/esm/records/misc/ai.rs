//! AI / dialogue / quest / combat-style records.

use super::super::common::{read_lstring_or_zstring, read_zstring};
use super::super::condition::{parse_ctda, remap_condition_form_ids, ConditionList};
use super::super::script_instance::{parse_quest_fragments, QuestScriptFragment};
use crate::esm::reader::SubRecord;
use crate::esm::sub_reader::SubReader;

/// `PACK` AI package record. 30-procedure scheduling system
/// (guard patrols, merchant behavior, dialogue triggers, ambient
/// idles). `NpcRecord.ai_packages` carries PKID form refs; pre-#446
/// those dangled.
///
/// PKDT (package flags + procedure type), PSDT (schedule), and PLDT
/// (location) are captured here. PTDT / PKTG(Skyrim+) / PKCU / PKPA
/// decoding lands with the AI runtime per the `ai_packages_procedures.md`
/// memo. Layout verified against the FO3/FNV xEdit-derived spec:
/// <https://tes5edit.github.io/fopdoc/FalloutNV/Records/PACK.html>.
#[derive(Debug, Clone, Default)]
pub struct PackRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Flags bitfield from PKDT (schedule / location repeat / weapon
    /// draw / etc.). Low 16 bits on FO3/FNV, u32 on Skyrim+.
    pub package_flags: u32,
    /// Procedure type — the FO3/FNV package-type enum (0..=16):
    /// 0 Find, 1 Follow, 2 Escort, 3 Eat, 4 Sleep, 5 Wander, 6 Travel,
    /// 7 Accompany, 8 UseItemAt, 9 Ambush, 10 FleeNotCombat,
    /// 11 CastMagic, 12 **Sandbox**, 13 Patrol, 14 Guard, 15 Dialogue,
    /// 16 UseWeapon. Read as a single **byte** at PKDT offset 4.
    pub procedure_type: u32,
    /// Schedule from PSDT (FO3/FNV). `None` when the package has no PSDT
    /// (treated as always-active). Drives which package is *active* at a
    /// given game hour — the M42.1 seat-assignment selector.
    pub schedule: Option<PackSchedule>,
    /// Authored activity center from PLDT. `None` when the package has
    /// no PLDT (rare — most FO3/FNV packages carry one). This is the
    /// Sandbox procedure's "Location" parameter (param #1 of 15 per the
    /// `ai_packages_procedures.md` memo) — the real center a Sandbox
    /// package should search around, replacing the v0 actor-position
    /// approximation in `sandbox_seat_system`.
    pub location: Option<PackLocation>,
}

/// PACK location data from PLDT — where a package's activity centers.
/// FormIDs in [`PackLocationTarget`] are plugin-local at parse time; the
/// caller must remap them the same way `parse_pack` does internally
/// (via the `FormIdRemap` threaded through `extract_records`, #1666).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackLocation {
    /// Raw Location Type enum (0..=7) — kept alongside `target` so a
    /// consumer can distinguish `Other` variants without re-deriving it.
    pub location_type: u32,
    pub target: PackLocationTarget,
    /// Search radius (game units) around `target`.
    pub radius: i32,
}

/// The `union` half of PLDT — its meaning depends on the Location Type
/// enum. Only types 0/1/4 carry a resolvable FormID per the FO3/FNV spec;
/// types 2/3/6/7 (Near Current Location / Near Editor Location / Near
/// Linked Reference / At Package Location) are self-referential to the
/// runtime actor or package and carry no FormID to look up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PackLocationTarget {
    /// Type 0 — Near Reference. FormID of a REFR/PGRE/PMIS/ACHR/ACRE/PLYR.
    NearReference(u32),
    /// Type 1 — In Cell. FormID of a CELL.
    InCell(u32),
    /// Type 4 — Object ID. FormID of a base-object record (ACTI/DOOR/
    /// STAT/FURN/CREA/SPEL/NPC_/CONT/ARMO/AMMO/MISC/WEAP/BOOK/KEYM/
    /// ALCH/LIGH/…).
    ObjectId(u32),
    /// Types 2 (Near Current Location), 3 (Near Editor Location), 5
    /// (Object Type — an enum value, not a FormID), 6 (Near Linked
    /// Reference), 7 (At Package Location). The raw union bytes are kept
    /// but not interpreted as a FormID.
    Other(u32),
}

/// FO3/FNV package procedure-type index for `Sandbox` — idle activities
/// in an area (sit, wander, use furniture). 56 % of vanilla FNV NPCs
/// carry one; it's the dominant ambient idle behavior. See M42.
pub const PROCEDURE_SANDBOX: u32 = 12;

/// PACK schedule window from PSDT (FO3/FNV). `start_hour = None` when the raw
/// `time` byte is -1 (0xFF) = "any time". `duration_hours` is the PSDT
/// duration (in hours for FO3/FNV — verified against FalloutNV.esm: a
/// bartender's `8x12` = 08:00 for 12 h, an evening idle `20x2` = 20:00 for 2 h,
/// a `22x10` sleep = 22:00 for 10 h wrapping to 08:00).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PackSchedule {
    pub start_hour: Option<u8>,
    pub duration_hours: u32,
}

impl PackSchedule {
    /// True when `hour` (0..24) falls in `[start, start + duration)` mod 24.
    /// Any-time (`start_hour == None`) is always active.
    pub fn active_at(&self, hour: f32) -> bool {
        let Some(start) = self.start_hour else {
            return true;
        };
        let start = start as f32;
        let end = start + self.duration_hours as f32;
        let h = hour.rem_euclid(24.0);
        if end <= 24.0 {
            h >= start && h < end
        } else {
            h >= start || h < end - 24.0 // wraps past midnight
        }
    }
}

impl PackRecord {
    /// True when this package's procedure is `Sandbox` (the idle-in-area
    /// behavior that drives furniture use).
    pub fn is_sandbox(&self) -> bool {
        self.procedure_type == PROCEDURE_SANDBOX
    }

    /// Whether this package's schedule includes `hour`. No PSDT → always
    /// active (the package is condition/location-gated, not time-gated).
    pub fn scheduled_active_at(&self, hour: f32) -> bool {
        self.schedule.map_or(true, |s| s.active_at(hour))
    }
}

/// Selection rule (M42.1): an NPC's *active* package at `hour` — the first
/// package, in priority order (`NpcRecord.ai_packages` order), whose
/// schedule includes `hour`. This keeps day-shift workers from being
/// treated as idle sandboxers — e.g. a bartender whose 08:00–20:00 `AtBar`
/// package outranks an evening `Sandbox` package is *not* seated at 10:00.
///
/// Schedule + priority only; package conditions (CTDA) are not yet
/// evaluated. Unresolved packages are skipped by the caller (pass only
/// resolved records, in order).
fn active_package<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
) -> Option<&'a PackRecord> {
    packages.into_iter().find(|pk| pk.scheduled_active_at(hour))
}

/// Whether an NPC's active package at `hour` (see [`active_package`]) is a
/// Sandbox package.
pub fn active_package_is_sandbox<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
) -> bool {
    active_package(packages, hour).is_some_and(PackRecord::is_sandbox)
}

/// The PLDT location of an NPC's active package at `hour` (see
/// [`active_package`]), when that package is Sandbox-type. `None` when the
/// active package isn't Sandbox, carries no PLDT, or nothing is scheduled
/// active. M42.1's seat system uses this to size its search radius around
/// the authored center instead of a fixed guess.
pub fn active_sandbox_location<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
) -> Option<PackLocation> {
    active_package(packages, hour)
        .filter(|pk| pk.is_sandbox())
        .and_then(|pk| pk.location)
}

/// Remap a raw plugin-local FormID to global space, leaving 0 (no
/// FormID / null ref) untouched. Mirrors the null-guard in
/// `remap_condition_form_ids` for the single-field PLDT case.
fn remap_fid(raw: u32, remap: &Option<crate::esm::reader::FormIdRemap>) -> u32 {
    if raw == 0 {
        return 0;
    }
    remap.as_ref().map_or(raw, |r| r.remap(raw))
}

pub fn parse_pack(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<crate::esm::reader::FormIdRemap>,
) -> PackRecord {
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
                // FO3/FNV PKDT: the procedure type is a single BYTE at
                // offset 4, followed by type-specific / flags2 bytes.
                // Reading it as a u32 (the pre-M42 bug) polluted the
                // type with the next 3 bytes — e.g. a Sandbox (12)
                // package parsed as 3452816652 / 268 / 65292. Masking to
                // the byte restores the clean 0..=16 enum (verified
                // against a full FalloutNV.esm sweep).
                out.procedure_type = r.u8_or_default() as u32;
            }
            b"PSDT" if sub.data.len() >= 8 => {
                // FO3/FNV PSDT: month i8, dayOfWeek i8, date u8, time i8
                // (hour; -1/0xFF = any), duration i32 (hours). Verified vs
                // FalloutNV.esm (AtBar 8x12, Evening 20x2, Sleep 22x10).
                let time = sub.data[3] as i8;
                let duration = i32::from_le_bytes([
                    sub.data[4], sub.data[5], sub.data[6], sub.data[7],
                ]);
                out.schedule = Some(PackSchedule {
                    start_hour: if time < 0 { None } else { Some(time as u8) },
                    duration_hours: duration.max(0) as u32,
                });
            }
            // FO3/FNV PLDT: Location Type u32, Location union u32
            // (FormID or raw value depending on type), Radius i32. Per
            // https://tes5edit.github.io/fopdoc/FalloutNV/Records/PACK.html.
            // Only types 0 (Near Reference) / 1 (In Cell) / 4 (Object ID)
            // carry a FormID that needs remapping to global space; the
            // others are self-referential and pass through raw.
            b"PLDT" if sub.data.len() >= 12 => {
                let mut r = SubReader::new(&sub.data);
                let location_type = r.u32_or_default();
                let raw = r.u32_or_default();
                let radius = r.i32_or_default();
                let target = match location_type {
                    0 => PackLocationTarget::NearReference(remap_fid(raw, remap)),
                    1 => PackLocationTarget::InCell(remap_fid(raw, remap)),
                    4 => PackLocationTarget::ObjectId(remap_fid(raw, remap)),
                    _ => PackLocationTarget::Other(raw),
                };
                out.location = Some(PackLocation {
                    location_type,
                    target,
                    radius,
                });
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

/// Resolved conversation tree structure — groups INFOs into PNAM chains
/// (reading-order sequences), and surfaces TCLT as inter-topic edges.
/// Built as a pure function over already-parsed DialRecord data.
#[derive(Debug, Clone)]
pub struct ConversationTree {
    /// PNAM chains ordered from head (previous_info==0) to tail.
    /// Each chain is a Vec of INFO form_ids in reading order.
    pub chains: Vec<Vec<u32>>,
    /// Inter-topic edges: source_info_form_id → [destination_topic_form_ids].
    /// Maps each INFO (by form_id) to the topics it routes to via TCLT.
    pub topic_links: std::collections::HashMap<u32, Vec<u32>>,
}

/// Error building a conversation tree (e.g., cycles in PNAM chain).
#[derive(Debug, Clone)]
pub enum ConversationTreeError {
    PnamCycle { info_form_id: u32 },
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
    /// `ANAM` actor form ID — restricts this response to a specific NPC.
    /// 0 means the response works for any actor.
    pub actor_form_id: u32,
    /// Conditions attached to this response (CTDA sub-records).
    pub conditions: ConditionList,
}

pub fn parse_dial(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<crate::esm::reader::FormIdRemap>,
) -> DialRecord {
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
                    let remapped = remap.as_ref().map_or(q, |r| r.remap(q));
                    out.quest_refs.push(remapped);
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

pub fn parse_info(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<crate::esm::reader::FormIdRemap>,
) -> InfoRecord {
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
                    let remapped = remap.as_ref().map_or(t, |r| r.remap(t));
                    out.topic_links.push(remapped);
                }
            }
            b"PNAM" if sub.data.len() >= 4 => {
                let raw = SubReader::new(&sub.data).u32_or_default();
                let remapped = remap.as_ref().map_or(raw, |r| r.remap(raw));
                out.previous_info = remapped;
            }
            b"ANAM" if sub.data.len() >= 4 => {
                let raw = u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                let remapped = remap.as_ref().map_or(raw, |r| r.remap(raw));
                out.actor_form_id = remapped;
            }
            b"CTDA" => {
                if let Some(mut cond) = parse_ctda(sub) {
                    remap_condition_form_ids(&mut cond, remap);
                    out.conditions.push(cond);
                }
            }
            _ => {}
        }
    }
    out
}

/// Build a conversation tree from flat INFO list.
/// Orders INFOs by PNAM chains (head = previous_info == 0).
/// Detects cycles to ensure chain termination.
pub fn build_conversation_tree(
    infos: &[InfoRecord],
) -> Result<ConversationTree, ConversationTreeError> {
    use std::collections::HashMap;

    // Index by form_id for fast lookup and cycle detection.
    let mut info_map: HashMap<u32, &InfoRecord> = HashMap::new();
    for info in infos {
        info_map.insert(info.form_id, info);
    }

    let mut visited = std::collections::HashSet::new();
    let mut chains = Vec::new();

    // Find all chain heads (previous_info == 0) and follow each to its tail.
    for info in infos {
        if info.previous_info == 0 && !visited.contains(&info.form_id) {
            let mut chain = Vec::new();
            let mut current = info.form_id;

            loop {
                chain.push(current);
                visited.insert(current);

                // Follow the chain: look up the next INFO by its own form_id
                // in the infos list (the NEXT INFO points back to this one
                // via previous_info).
                let next_info = infos.iter().find(|i| i.previous_info == current);
                match next_info {
                    Some(nxt) => {
                        // Cycle detection: if the next form_id is already in this chain, bail.
                        if chain.contains(&nxt.form_id) {
                            return Err(ConversationTreeError::PnamCycle {
                                info_form_id: nxt.form_id,
                            });
                        }
                        current = nxt.form_id;
                    }
                    None => break, // End of chain.
                }
            }

            chains.push(chain);
        }
    }

    // Orphans: infos not in any chain. Check for cycles in orphaned sub-chains.
    for info in infos {
        if !visited.contains(&info.form_id) {
            // This INFO is not a head and not yet visited.
            // Start from it and walk backward via previous_info to find the chain head.
            let mut walk_back = Vec::new();
            let mut current = info.form_id;

            loop {
                if walk_back.contains(&current) {
                    // Cycle detected (no head exists for this chain).
                    return Err(ConversationTreeError::PnamCycle {
                        info_form_id: current,
                    });
                }
                walk_back.push(current);

                // If current has previous_info == 0, it's the head.
                if let Some(curr_info) = info_map.get(&current) {
                    if curr_info.previous_info == 0 {
                        break; // Found the head; this chain should already be visited.
                    }
                    current = curr_info.previous_info;
                } else {
                    // current form_id not in infos — dangling reference.
                    // The last valid INFO we saw is the actual head.
                    if !walk_back.is_empty() {
                        walk_back.pop(); // Remove the invalid form_id
                    }
                    break;
                }
            }

            // walk_back is now [starting_info, ..., head]. Reverse to get proper order.
            walk_back.reverse();
            if let Some(&head_fid) = walk_back.first() {
                let mut chain = vec![head_fid];
                visited.insert(head_fid);
                let mut current = head_fid;

                loop {
                    let next_info = infos.iter().find(|i| i.previous_info == current);
                    match next_info {
                        Some(nxt) => {
                            if chain.contains(&nxt.form_id) {
                                return Err(ConversationTreeError::PnamCycle {
                                    info_form_id: nxt.form_id,
                                });
                            }
                            chain.push(nxt.form_id);
                            visited.insert(nxt.form_id);
                            current = nxt.form_id;
                        }
                        None => break,
                    }
                }

                chains.push(chain);
            }
        }
    }

    // Build topic_links map: info_form_id → destination topics.
    let mut topic_links = HashMap::new();
    for info in infos {
        if !info.topic_links.is_empty() {
            topic_links.insert(info.form_id, info.topic_links.clone());
        }
    }

    Ok(ConversationTree {
        chains,
        topic_links,
    })
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
        pkdt.extend_from_slice(&6u32.to_le_bytes()); // procedure 6 = Travel
        let subs = vec![sub(b"EDID", b"TravelToWork\0"), sub(b"PKDT", &pkdt)];
        let p = parse_pack(0xA1A1, &subs, &None);
        assert_eq!(p.editor_id, "TravelToWork");
        assert_eq!(p.package_flags, 0x0000_0421);
        assert_eq!(p.procedure_type, 6);
        assert!(!p.is_sandbox());
    }

    /// The procedure type is a single BYTE at PKDT offset 4. Real FNV
    /// PKDTs carry type-specific data in the 3 bytes after it; the
    /// pre-M42 u32 read polluted the type with them (a Sandbox package
    /// parsed as e.g. 0xCC…0C instead of 12). Masking to the byte must
    /// recover 12.
    #[test]
    fn parse_pack_reads_procedure_as_byte_not_polluted_u32() {
        let mut pkdt = Vec::new();
        pkdt.extend_from_slice(&0x0000_1234u32.to_le_bytes()); // flags
        pkdt.push(12); // procedure byte = Sandbox
        pkdt.extend_from_slice(&[0xAB, 0xCD, 0xEF]); // type-specific junk
        let subs = vec![sub(b"EDID", b"DefaultSandbox\0"), sub(b"PKDT", &pkdt)];
        let p = parse_pack(0xB2B2, &subs, &None);
        assert_eq!(
            p.procedure_type, 12,
            "procedure must be the byte value, not the polluted u32"
        );
        assert!(p.is_sandbox());
    }

    fn pack(procedure: u32, schedule: Option<PackSchedule>) -> PackRecord {
        PackRecord {
            procedure_type: procedure,
            schedule,
            ..Default::default()
        }
    }

    fn sched(start_hour: Option<u8>, duration_hours: u32) -> Option<PackSchedule> {
        Some(PackSchedule {
            start_hour,
            duration_hours,
        })
    }

    #[test]
    fn parse_pack_reads_psdt_schedule() {
        // AtBar `8x12`: time byte = 8, duration i32 = 12 → 08:00 for 12 h.
        let psdt = [0xff, 0xff, 0x00, 0x08, 0x0c, 0, 0, 0];
        assert_eq!(
            parse_pack(0x1, &[sub(b"PSDT", &psdt)], &None).schedule,
            sched(Some(8), 12)
        );
        // Any-time sandbox: time byte = -1 (0xFF) → start_hour None.
        let any = [0xff, 0xff, 0x00, 0xff, 0, 0, 0, 0];
        assert_eq!(
            parse_pack(0x2, &[sub(b"PSDT", &any)], &None).schedule,
            sched(None, 0)
        );
    }

    fn pldt(location_type: u32, raw: u32, radius: i32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&location_type.to_le_bytes());
        data.extend_from_slice(&raw.to_le_bytes());
        data.extend_from_slice(&radius.to_le_bytes());
        data
    }

    #[test]
    fn parse_pack_no_pldt_leaves_location_none() {
        let p = parse_pack(0x1, &[sub(b"EDID", b"NoLocation\0")], &None);
        assert!(p.location.is_none());
    }

    #[test]
    fn parse_pack_reads_pldt_near_reference() {
        // Type 0 = Near Reference, radius 512 (the FNV DefaultSandbox
        // package radius).
        let data = pldt(0, 0x0001_2345, 512);
        let p = parse_pack(0x1, &[sub(b"PLDT", &data)], &None);
        let loc = p.location.expect("PLDT should populate location");
        assert_eq!(loc.location_type, 0);
        assert_eq!(loc.target, PackLocationTarget::NearReference(0x0001_2345));
        assert_eq!(loc.radius, 512);
    }

    #[test]
    fn parse_pack_reads_pldt_in_cell_and_object_id() {
        let cell = pldt(1, 0x0002_ABCD, 0);
        let p = parse_pack(0x1, &[sub(b"PLDT", &cell)], &None);
        assert_eq!(
            p.location.unwrap().target,
            PackLocationTarget::InCell(0x0002_ABCD)
        );

        let obj = pldt(4, 0x0003_1111, 256);
        let p = parse_pack(0x1, &[sub(b"PLDT", &obj)], &None);
        assert_eq!(
            p.location.unwrap().target,
            PackLocationTarget::ObjectId(0x0003_1111)
        );
    }

    /// Types other than 0/1/4 (Near Current Location, Near Editor
    /// Location, Object Type, Near Linked Reference, At Package
    /// Location) carry no FormID — the raw union value passes through
    /// unremapped as `Other`.
    #[test]
    fn parse_pack_reads_pldt_other_types_pass_through_raw() {
        for location_type in [2u32, 3, 5, 6, 7] {
            let data = pldt(location_type, 0x0009_9999, 128);
            let p = parse_pack(0x1, &[sub(b"PLDT", &data)], &None);
            let loc = p.location.unwrap();
            assert_eq!(loc.location_type, location_type);
            assert_eq!(loc.target, PackLocationTarget::Other(0x0009_9999));
        }
    }

    /// PLDT's Near Reference FormID is plugin-local at parse time; a
    /// self-reference (top byte == master count) must remap to the
    /// plugin's own global slot, mirroring `remap_condition_form_ids`'s
    /// contract for the same #1666 pattern.
    #[test]
    fn parse_pack_pldt_near_reference_remaps_form_id() {
        let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);
        // mod_index 1 == master_slots.len() → self-reference.
        let raw = (1u32 << 24) | 0x0000_5678;
        let data = pldt(0, raw, 512);
        let p = parse_pack(0x1, &[sub(b"PLDT", &data)], &Some(remap));
        assert_eq!(
            p.location.unwrap().target,
            PackLocationTarget::NearReference((2u32 << 24) | 0x0000_5678)
        );
    }

    #[test]
    fn pack_schedule_active_at_windows() {
        let bar = PackSchedule {
            start_hour: Some(8),
            duration_hours: 12,
        }; // 08:00–20:00
        assert!(bar.active_at(10.0));
        assert!(!bar.active_at(21.0));
        assert!(!bar.active_at(7.9));
        let sleep = PackSchedule {
            start_hour: Some(22),
            duration_hours: 10,
        }; // 22:00–08:00 (wraps midnight)
        assert!(sleep.active_at(23.0));
        assert!(sleep.active_at(2.0));
        assert!(!sleep.active_at(10.0));
        let any = PackSchedule {
            start_hour: None,
            duration_hours: 0,
        };
        assert!(any.active_at(0.0) && any.active_at(15.0));
    }

    #[test]
    fn active_package_selector_respects_priority_and_schedule() {
        // Bartender's daytime package outranks an evening Sandbox fallback.
        let bartender = pack(6, sched(Some(8), 12)); // Travel/AtBar 08–20
        let evening = pack(PROCEDURE_SANDBOX, sched(Some(20), 2)); // sandbox 20–22
        // 10:00 → bartender active → NOT treated as sandbox (the Trudy bug).
        assert!(!active_package_is_sandbox([&bartender, &evening], 10.0));
        // 21:00 → bartender off-shift, evening sandbox active.
        assert!(active_package_is_sandbox([&bartender, &evening], 21.0));
        // Any-time saloon sandbox behind an inactive sleep package → sandbox.
        let sleep = pack(4, sched(Some(22), 10));
        let anytime_sandbox = pack(PROCEDURE_SANDBOX, None);
        assert!(active_package_is_sandbox([&sleep, &anytime_sandbox], 10.0));
        // No resolvable packages → not sandbox.
        assert!(!active_package_is_sandbox(std::iter::empty::<&PackRecord>(), 10.0));
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
    fn parse_dial_accumulates_multiple_quest_refs() {
        let subs = vec![
            sub(b"EDID", b"GREETING\0"),
            sub(b"FULL", b"Greeting\0"),
            sub(b"QSTI", &0x0100_0001u32.to_le_bytes()),
            sub(b"QSTI", &0x0100_0002u32.to_le_bytes()),
            sub(b"QSTI", &0x0100_0003u32.to_le_bytes()),
        ];
        let d = parse_dial(0xC3C3, &subs, &None);
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
        let subs = vec![sub(b"EDID", b"PersuasionTopic\0"), sub(b"DATA", &[3u8])];
        let d = parse_dial(0xDEAD, &subs, &None);
        assert_eq!(d.dial_type, 3);

        // FO3+ widen DATA (type byte + flags); byte 0 still the type.
        let subs_fo3 = vec![sub(b"DATA", &[5u8, 0x01, 0x00, 0x00])];
        assert_eq!(parse_dial(0xBEEF, &subs_fo3, &None).dial_type, 5);

        // Empty DATA must not panic and leaves the default.
        let subs_empty = vec![sub(b"DATA", &[])];
        assert_eq!(parse_dial(0xF00D, &subs_empty, &None).dial_type, 0);
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

    #[test]
    fn parse_info_picks_anam_actor() {
        let anam = 0xDEAD_BEEFu32.to_le_bytes();
        let subs = vec![sub(b"NAM1", b"hello\0"), sub(b"ANAM", &anam)];
        let info = parse_info(0x1234, &subs, &None);
        assert_eq!(info.actor_form_id, 0xDEAD_BEEF);
    }

    #[test]
    fn parse_info_ctda_conditions_stored() {
        let mut ctda = Vec::new();
        ctda.push(0x00u8); // type_byte (offset 0)
        ctda.extend_from_slice(&[0u8; 3]); // pad (offsets 1-3)
        ctda.extend_from_slice(&1.0f32.to_le_bytes()); // comparand (offsets 4-7)
        ctda.extend_from_slice(&36u32.to_le_bytes()); // function_index (offsets 8-11, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // param_1 (offsets 12-15, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // param_2 (offsets 16-19, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // run_on (offsets 20-23, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // ref_fid (offsets 24-27, u32)

        let subs = vec![sub(b"NAM1", b"hi\0"), sub(b"CTDA", &ctda)];
        let info = parse_info(0x5678, &subs, &None);
        assert_eq!(info.conditions.len(), 1);
        assert_eq!(info.conditions[0].function_index, 36);
    }

    #[test]
    fn parse_info_remaps_formids_with_remap() {
        use crate::esm::reader::FormIdRemap;
        // PNAM (previous_info) and TCLT (topic_links) and ANAM (actor)
        // should be remapped when a remap is provided.
        // This plugin at index 1, master at index 0 (all regular, no ESL).
        let remap = FormIdRemap::regular(1, vec![0]);
        let subs = vec![
            sub(b"PNAM", &0x00_050000u32.to_le_bytes()), // plugin 0 (master), form 0x050000
            sub(b"TCLT", &0x01_030000u32.to_le_bytes()), // plugin 1 (this), form 0x030000
            sub(b"ANAM", &0x00_020000u32.to_le_bytes()), // plugin 0 (master), form 0x020000
        ];
        // With remap: plugin 0 stays 0 (master), plugin 1 stays 1 (this)
        let info = parse_info(0x5678, &subs, &Some(remap));
        assert_eq!(info.previous_info, 0x00_050000);
        assert_eq!(info.topic_links[0], 0x01_030000);
        assert_eq!(info.actor_form_id, 0x00_020000);
        // Verify that without remap, values are identical (no remap = identity)
        let info_no_remap = parse_info(0x5678, &subs, &None);
        assert_eq!(info_no_remap.previous_info, info.previous_info);
    }

    #[test]
    fn build_conversation_tree_orders_pnam_chain() {
        // Three INFOs: A (head), B, C.
        // PNAM chain: A (previous_info=0) <- B <- C (C.previous_info=B.form_id)
        // Insert them in scrambled order to test ordering.
        let infos = vec![
            InfoRecord {
                form_id: 0xBBBB,
                response_text: "B response".to_string(),
                previous_info: 0xAAAA, // Points back to A
                ..Default::default()
            },
            InfoRecord {
                form_id: 0xAAAA,
                response_text: "A response".to_string(),
                previous_info: 0, // Head
                ..Default::default()
            },
            InfoRecord {
                form_id: 0xCCCC,
                response_text: "C response".to_string(),
                previous_info: 0xBBBB, // Points back to B
                ..Default::default()
            },
        ];

        let tree = build_conversation_tree(&infos).expect("should build tree");
        assert_eq!(tree.chains.len(), 1, "should have 1 chain");
        assert_eq!(
            tree.chains[0],
            vec![0xAAAA, 0xBBBB, 0xCCCC],
            "chain should be ordered A→B→C"
        );
    }

    #[test]
    fn build_conversation_tree_detects_pnam_cycle() {
        // Cycle: A <- B <- C <- A (C.previous_info=A)
        let infos = vec![
            InfoRecord {
                form_id: 0xAAAA,
                response_text: "A response".to_string(),
                previous_info: 0xCCCC, // Points back to C (cycle!)
                ..Default::default()
            },
            InfoRecord {
                form_id: 0xBBBB,
                response_text: "B response".to_string(),
                previous_info: 0xAAAA,
                ..Default::default()
            },
            InfoRecord {
                form_id: 0xCCCC,
                response_text: "C response".to_string(),
                previous_info: 0xBBBB,
                ..Default::default()
            },
        ];

        let result = build_conversation_tree(&infos);
        assert!(result.is_err(), "should detect cycle");
        match result.unwrap_err() {
            ConversationTreeError::PnamCycle { info_form_id } => {
                assert_eq!(
                    info_form_id, 0xAAAA,
                    "cycle detection should report the repeating form_id"
                );
            }
        }
    }

    #[test]
    fn build_conversation_tree_surfaces_tclt_edges() {
        // Two separate PNAM chains; first INFO of first chain has TCLT edges.
        let infos = vec![
            InfoRecord {
                form_id: 0xAAAA,
                response_text: "Chain1 head".to_string(),
                previous_info: 0,
                topic_links: vec![0x1111, 0x2222], // Routes to two topics
                ..Default::default()
            },
            InfoRecord {
                form_id: 0xBBBB,
                response_text: "Chain2 head".to_string(),
                previous_info: 0,
                topic_links: vec![],
                ..Default::default()
            },
        ];

        let tree = build_conversation_tree(&infos).expect("should build tree");
        assert_eq!(
            tree.topic_links.len(),
            1,
            "should have 1 INFO with topic_links"
        );
        assert_eq!(
            tree.topic_links.get(&0xAAAA),
            Some(&vec![0x1111, 0x2222]),
            "should surface TCLT edges for chain1 head"
        );
        assert!(
            !tree.topic_links.contains_key(&0xBBBB),
            "chain2 head has no TCLT"
        );
    }

    #[test]
    fn build_conversation_tree_handles_orphaned_infos() {
        // An INFO with previous_info pointing to a non-existent INFO becomes a 1-element chain.
        let infos = vec![InfoRecord {
            form_id: 0xAAAA,
            response_text: "Orphan".to_string(),
            previous_info: 0x9999, // Points to non-existent INFO
            ..Default::default()
        }];

        let tree = build_conversation_tree(&infos).expect("should build tree");
        assert_eq!(
            tree.chains.len(),
            1,
            "orphan should become a 1-element chain"
        );
        assert_eq!(tree.chains[0], vec![0xAAAA]);
    }
}
