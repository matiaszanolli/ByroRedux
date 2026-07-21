//! `QUST` quest records — stages, objectives, and the Skyrim+ VMAD
//! fragment-dispatch bindings.

use super::super::common::{read_lstring_or_zstring, read_zstring};
use super::super::condition::{parse_ctda, remap_condition_form_ids, ConditionList};
use super::super::script_instance::{parse_quest_fragments, QuestScriptFragment, ScriptInstanceData};
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

/// One quest alias, defined by an `ALST` (Reference alias) or `ALLS`
/// (Location alias) block. Aliases are Radiant Story's targeting
/// mechanism — a quest names a *role* ("QuestGiver", "Location") rather
/// than a specific reference, and this is filled in at runtime per
/// [`AliasFillType`]. Parser-side only: this is pure data, decoded and
/// cross-validated against the field table in
/// [`docs/engine/m47-3-quest-alias-design.md`]; the fill-and-apply
/// runtime is that milestone's later phases.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestAlias {
    /// `ALST`/`ALLS` payload — the numerical id Papyrus/VMAD reference
    /// this alias by (the parameter to `LocAliasHasKeyword` and
    /// friends). Kept at the on-disk `int32` width rather than narrowed
    /// to VMAD's `i16` alias-index field — the consumer widens/narrows
    /// for comparison; this layer doesn't guess a range is safe.
    pub alias_id: i32,
    /// `true` for an `ALLS` (Location) alias, `false` for `ALST`
    /// (Reference). Most fill-type fields are exclusive to one kind
    /// (documented per-variant on [`AliasFillType`]).
    pub is_location: bool,
    /// `ALID` — the alias name (e.g. `"Location"`, `"QuestGiver"`),
    /// substituted into dynamically-generated journal/dialogue text.
    pub name: String,
    /// How this alias's value is determined at runtime — the fill-type
    /// field that was present on disk (mutually exclusive per the
    /// source). `None` for the "Find Matching Reference/Location" case,
    /// which has no dedicated fill field — only `match_conditions`.
    pub fill_type: Option<AliasFillType>,
    /// `FNAM` flags — see the `ALIAS_FLAG_*` constants below.
    pub flags: AliasFlags,
    /// `ALFI` — "Force Into Alias": once this alias fills (via its own
    /// `fill_type`/`match_conditions`), its resolved value is *also*
    /// propagated onto the alias index named here (last writer wins if
    /// multiple aliases force into the same target). The source's field
    /// table only says "Unknown, int32"; the propagation behavior comes
    /// from separately-sourced CK documentation, not this sub-record —
    /// carried raw, the M47.3 runtime resolves the propagation. Real-data
    /// finding (2026-07-21, verified against raw bytes via
    /// `qust_alias_rawdump` on `Skyrim.esm` quest `0002C258`): the
    /// *target* of a Force Into Alias typically carries no
    /// `fill_type`/`match_conditions` of its own — its value comes
    /// entirely from the propagation. Concretely: alias 1 (`Nurelion`,
    /// `ALFR`-filled) carries `ALFI = 8`; alias 8 (`NurelionEssential`)
    /// has no fill field and no `CTDA` at all — it exists purely to
    /// receive alias 1's value under the Essential flag. Detecting this
    /// from a single `QuestAlias` in isolation isn't possible; the
    /// runtime must cross-reference every alias's `force_into_alias`
    /// against sibling aliases' `alias_id` within the same `QustRecord`.
    pub force_into_alias: Option<i32>,
    /// `CTDA` conditions attached to this alias. The "Match Conditions"
    /// fill type's predicate list (reusing M47.1's `ConditionList`
    /// verbatim) — also legal alongside another fill type per the
    /// source ("multiple CTDA fields can be used together").
    pub match_conditions: ConditionList,
    /// Data applied to the alias's target for the duration of the quest,
    /// once filled (factions/packages/spells/keywords/inventory/display
    /// name/voice type/combat override).
    pub injected: AliasInjectedData,
}

/// How an [`QuestAlias`]'s runtime value is determined — the fill-type
/// field present in its `ALST`/`ALLS` block. Raw FormIds; the M47.3
/// alias-fill runtime resolves and applies these, this layer only
/// decodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AliasFillType {
    /// `ALFR` (Reference alias only) — a fixed `ACHR`/`REFR`.
    ForcedReference(u32),
    /// `ALFL` (Location alias only) — a fixed `LCTN`.
    ForcedLocation(u32),
    /// `ALUA` (Reference alias only) — an `NPC_`'s existing unique
    /// `ACHR` instance (not a spawn).
    UniqueActor(u32),
    /// `ALCO` (Reference alias only) — instantiate a new reference to
    /// this base record. `alca`/`alcl` are the companion `ALCA`/`ALCL`
    /// fields, whose meaning the source itself doesn't know ("Unknown.
    /// Associated with ALCO") — carried raw rather than interpreted.
    CreatedObject { base: u32, alca: i32, alcl: i32 },
    /// `ALEQ` + `ALEA` — copy the value from another quest's alias
    /// (`quest`'s alias `alias_id`).
    ExternalAlias { quest: u32, alias_id: i32 },
    /// `ALRT` (Reference alias only) — an `LCRT` lookup against the
    /// quest's location. `alfa` (`ALFA`) is unconfirmed by the source
    /// ("may be a formid, but with first byte(s) co-opted as a flag?")
    /// — carried raw rather than interpreted.
    LocationAliasReference { lcrt: u32, alfa: i32 },
    /// `ALFE` + `ALFD` — filled from a Story Manager event.
    FromEvent { event_type: [u8; 4], data: i32 },
}

/// `ALST`/`ALLS` `FNAM` alias flags. A plain bit-constant newtype
/// (mirrors `LIGHT_FLAG_*` in `components/light.rs`), not a `bitflags!`
/// type — matches this crate's existing convention for parsed on-disk
/// flag fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AliasFlags(pub u32);

// The full bit catalog ships now even though it has no *production*
// consumer yet — the M47.3 alias-fill runtime (reservation tracking,
// essential/protected, allow-dead/disabled/destroyed relaxations, …) is
// a later phase. Every constant is exercised by an `AliasFlags::has`
// assertion in the test module below (dead-code analysis just doesn't
// credit test-only usage for the non-test build), so all are
// `dead_code`-allowed here rather than left to warn.
/// Reserves Location (`ALLS`) / Reserves Reference (`ALST`).
#[allow(dead_code)]
pub const ALIAS_FLAG_RESERVES: u32 = 0x0000_0001;
#[allow(dead_code)]
pub const ALIAS_FLAG_OPTIONAL: u32 = 0x0000_0002;
#[allow(dead_code)]
pub const ALIAS_FLAG_QUEST_OBJECT: u32 = 0x0000_0004;
#[allow(dead_code)]
pub const ALIAS_FLAG_ALLOW_REUSE: u32 = 0x0000_0008;
#[allow(dead_code)]
pub const ALIAS_FLAG_ALLOW_DEAD: u32 = 0x0000_0010;
/// "Find Matching Reference" sub-option.
#[allow(dead_code)]
pub const ALIAS_FLAG_IN_LOADED_AREA: u32 = 0x0000_0020;
#[allow(dead_code)]
pub const ALIAS_FLAG_ESSENTIAL: u32 = 0x0000_0040;
#[allow(dead_code)]
pub const ALIAS_FLAG_ALLOW_DISABLED: u32 = 0x0000_0080;
#[allow(dead_code)]
pub const ALIAS_FLAG_STORES_TEXT: u32 = 0x0000_0100;
#[allow(dead_code)]
pub const ALIAS_FLAG_ALLOW_RESERVED: u32 = 0x0000_0200;
#[allow(dead_code)]
pub const ALIAS_FLAG_PROTECTED: u32 = 0x0000_0400;
#[allow(dead_code)]
pub const ALIAS_FLAG_ALLOW_DESTROYED: u32 = 0x0000_1000;
/// "Find Matching Reference" sub-option; requires [`ALIAS_FLAG_IN_LOADED_AREA`].
#[allow(dead_code)]
pub const ALIAS_FLAG_CLOSEST: u32 = 0x0000_2000;
#[allow(dead_code)]
pub const ALIAS_FLAG_USES_STORED_TEXT: u32 = 0x0000_4000;
#[allow(dead_code)]
pub const ALIAS_FLAG_INITIALLY_DISABLED: u32 = 0x0000_8000;
/// `ALLS` only.
#[allow(dead_code)]
pub const ALIAS_FLAG_ALLOW_CLEARED: u32 = 0x0001_0000;

impl AliasFlags {
    pub fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

/// Data applied to an alias's target for the duration of the quest, once
/// filled. Raw FormIds — the M47.3 runtime resolves and applies; this
/// parser stays a pure decode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AliasInjectedData {
    /// `ALDN` → `MESG`, dynamically renames the alias target.
    pub display_name: Option<u32>,
    /// `VTCK` → `NPC_`/`FLST`, additional valid voice types for export.
    pub voice_type: Option<u32>,
    /// `ECOR` → `FLST`, combat override package list.
    pub combat_override: Option<u32>,
    /// `ALFC` → `FACT`, added on fill, removed on clear.
    pub factions: Vec<u32>,
    /// `ALPC` → `PACK`, stacked on top of the target's base packages.
    pub packages: Vec<u32>,
    /// `ALSP` → `SPEL`, added on fill, removed on clear.
    pub spells: Vec<u32>,
    /// `KWDA` → `KYWD`, added while in the alias.
    pub keywords: Vec<u32>,
    /// `CNTO` → `(item FormId, count)`. Added on fill; per the source,
    /// **not** removed on clear (a permanent grant, unlike factions/
    /// spells) — the eventual runtime must not "fix" this into symmetry
    /// it doesn't have.
    pub inventory: Vec<(u32, u32)>,
}

/// `QUST` quest record. Lifecycle container for the Story Manager and
/// Radiant Story systems. Stages, objectives, and aliases are decoded;
/// CTDA conditions are already handled by M47.1's `ConditionList` at the
/// script-consumer side rather than per-stage here.
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
    /// The QUST `VMAD` scripts section — the compiled `QF_` script's own
    /// attached-script + property bindings (e.g. a `Quest Property
    /// OtherQuest Auto` a fragment targets). `None` on FO3/FNV (no VMAD)
    /// or when the VMAD carries no scripts section. This is the property
    /// table a fragment's cross-quest `Property`-targeted effect (a
    /// `SomeOtherQuest.SetStage(..)` call bound via a `Quest Property`)
    /// resolves through at dispatch time.
    pub script_instance: Option<ScriptInstanceData>,
    /// All defined aliases (`ALST`/`ALLS` blocks), in authoring order.
    /// M47.3 Phase 0 substrate — pure data, no fill-and-apply runtime
    /// yet. See [`QuestAlias`].
    pub aliases: Vec<QuestAlias>,
}

/// Which block-structured sub-record we're currently inside while
/// walking the QUST sub-record stream. Stage, objective, and alias
/// blocks are mutually exclusive at any point — `INDX` opens a stage
/// block, `QOBJ` opens an objective block, `ALST`/`ALLS` opens an alias
/// block, and any of the three closes whatever was open before.
enum QustBlock {
    None,
    Stage(QuestStage),
    Objective(QuestObjective),
    Alias(QuestAlias),
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
            QustBlock::Alias(alias) => out.aliases.push(alias),
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
            // Skyrim+ Papyrus attachment. Two independent decodes of the
            // same bytes: the trailing fragment section (stage→
            // `Fragment_N`, the M47.2 fragment-dispatch input) and the
            // leading scripts section (the QF_ script's own property
            // table — how a fragment's cross-quest `Quest Property`
            // effect resolves to a FormID).
            b"VMAD" => {
                out.fragments = parse_quest_fragments(&sub.data);
                out.script_instance = Some(ScriptInstanceData::parse(&sub.data));
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
                QustBlock::Alias(_) | QustBlock::None => {}
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
            // CTDA also appears inside an alias block ("Match
            // Conditions" — the Find Matching Reference/Location fill
            // type's predicate list, or an additional gate alongside
            // another fill type). Stage and Alias are the two block
            // kinds that currently collect conditions here.
            b"CTDA" => match &mut block {
                QustBlock::Stage(stage) => {
                    if let Some(mut cond) = parse_ctda(sub) {
                        remap_condition_form_ids(&mut cond, remap);
                        stage.conditions.push(cond);
                    }
                }
                QustBlock::Alias(alias) => {
                    if let Some(mut cond) = parse_ctda(sub) {
                        remap_condition_form_ids(&mut cond, remap);
                        alias.match_conditions.push(cond);
                    }
                }
                QustBlock::Objective(_) | QustBlock::None => {}
            },
            // ALST/ALLS opens an alias block — a Reference alias or a
            // Location alias respectively. Same flush rule as INDX/QOBJ.
            b"ALST" if sub.data.len() >= 4 => {
                let prev = std::mem::replace(&mut block, QustBlock::None);
                flush_block(&mut out, prev);
                let alias_id = SubReader::new(&sub.data).i32_or_default();
                block = QustBlock::Alias(QuestAlias {
                    alias_id,
                    is_location: false,
                    ..Default::default()
                });
            }
            b"ALLS" if sub.data.len() >= 4 => {
                let prev = std::mem::replace(&mut block, QustBlock::None);
                flush_block(&mut out, prev);
                let alias_id = SubReader::new(&sub.data).i32_or_default();
                block = QustBlock::Alias(QuestAlias {
                    alias_id,
                    is_location: true,
                    ..Default::default()
                });
            }
            // ALED is the explicit "end of this alias" terminator (the
            // source: "always the final field in a set of ALID
            // entries") — flush now rather than waiting for the next
            // block-opener or end of stream.
            b"ALED" => {
                let prev = std::mem::replace(&mut block, QustBlock::None);
                flush_block(&mut out, prev);
            }
            b"ALID" => {
                if let QustBlock::Alias(alias) = &mut block {
                    alias.name = read_zstring(&sub.data);
                }
            }
            // ── Fill-type fields (mutually exclusive on disk) ──
            b"ALFR" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    let fid = SubReader::new(&sub.data).u32_or_default();
                    alias.fill_type = Some(AliasFillType::ForcedReference(fid));
                }
            }
            b"ALFL" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    let fid = SubReader::new(&sub.data).u32_or_default();
                    alias.fill_type = Some(AliasFillType::ForcedLocation(fid));
                }
            }
            b"ALUA" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    let fid = SubReader::new(&sub.data).u32_or_default();
                    alias.fill_type = Some(AliasFillType::UniqueActor(fid));
                }
            }
            b"ALCO" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    let fid = SubReader::new(&sub.data).u32_or_default();
                    alias.fill_type = Some(AliasFillType::CreatedObject {
                        base: fid,
                        alca: 0,
                        alcl: 0,
                    });
                }
            }
            b"ALEQ" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    let fid = SubReader::new(&sub.data).u32_or_default();
                    alias.fill_type = Some(AliasFillType::ExternalAlias {
                        quest: fid,
                        alias_id: 0,
                    });
                }
            }
            b"ALRT" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    let fid = SubReader::new(&sub.data).u32_or_default();
                    alias.fill_type = Some(AliasFillType::LocationAliasReference {
                        lcrt: fid,
                        alfa: 0,
                    });
                }
            }
            b"ALFE" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    let mut event_type = [0u8; 4];
                    event_type.copy_from_slice(&sub.data[..4]);
                    alias.fill_type = Some(AliasFillType::FromEvent {
                        event_type,
                        data: 0,
                    });
                }
            }
            // ── Fill-type companion fields — arrive after their
            // primary field per the source's documented order; a no-op
            // if the primary field somehow didn't land first (declines
            // rather than fabricating a fill type from a companion
            // alone). ──
            b"ALCA" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    if let Some(AliasFillType::CreatedObject { alca, .. }) = &mut alias.fill_type {
                        *alca = SubReader::new(&sub.data).i32_or_default();
                    }
                }
            }
            b"ALCL" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    if let Some(AliasFillType::CreatedObject { alcl, .. }) = &mut alias.fill_type {
                        *alcl = SubReader::new(&sub.data).i32_or_default();
                    }
                }
            }
            b"ALEA" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    if let Some(AliasFillType::ExternalAlias { alias_id, .. }) =
                        &mut alias.fill_type
                    {
                        *alias_id = SubReader::new(&sub.data).i32_or_default();
                    }
                }
            }
            b"ALFA" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    if let Some(AliasFillType::LocationAliasReference { alfa, .. }) =
                        &mut alias.fill_type
                    {
                        *alfa = SubReader::new(&sub.data).i32_or_default();
                    }
                }
            }
            b"ALFD" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    if let Some(AliasFillType::FromEvent { data, .. }) = &mut alias.fill_type {
                        *data = SubReader::new(&sub.data).i32_or_default();
                    }
                }
            }
            // ALFI — "Force Into Alias" (see `QuestAlias::force_into_alias`
            // doc). Independent of `fill_type`: an alias can carry an
            // ALFI propagation target alongside its own fill type (the
            // common real-data shape — a real fill type propagating its
            // value onto a fill-type-less "shadow" alias elsewhere in
            // the same quest, verified via `qust_alias_rawdump`).
            b"ALFI" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    alias.force_into_alias = Some(SubReader::new(&sub.data).i32_or_default());
                }
            }
            // ── FNAM flags + injected data. FNAM also appears at QOBJ
            // level (a different meaning, "ORed With Previous") — not
            // yet decoded there, so only the Alias arm fires today. ──
            b"FNAM" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    alias.flags = AliasFlags(SubReader::new(&sub.data).u32_or_default());
                }
            }
            b"ALDN" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    alias.injected.display_name = Some(SubReader::new(&sub.data).u32_or_default());
                }
            }
            b"VTCK" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    alias.injected.voice_type = Some(SubReader::new(&sub.data).u32_or_default());
                }
            }
            b"ECOR" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    alias.injected.combat_override =
                        Some(SubReader::new(&sub.data).u32_or_default());
                }
            }
            b"ALFC" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    alias
                        .injected
                        .factions
                        .push(SubReader::new(&sub.data).u32_or_default());
                }
            }
            b"ALPC" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    alias
                        .injected
                        .packages
                        .push(SubReader::new(&sub.data).u32_or_default());
                }
            }
            b"ALSP" if sub.data.len() >= 4 => {
                if let QustBlock::Alias(alias) = &mut block {
                    alias
                        .injected
                        .spells
                        .push(SubReader::new(&sub.data).u32_or_default());
                }
            }
            // KWDA holds `KSIZ` concatenated keyword FormIds in one
            // sub-record (not one KWDA per keyword) — read every u32 in
            // the payload. KSIZ itself is redundant with the payload
            // length (same "read what's there" approach QSTA/CTDA use
            // elsewhere in this parser) so it isn't separately tracked.
            b"KWDA" => {
                if let QustBlock::Alias(alias) = &mut block {
                    let mut r = SubReader::new(&sub.data);
                    while r.remaining() >= 4 {
                        alias.injected.keywords.push(r.u32_or_default());
                    }
                }
            }
            // CNTO: {formid item, uint32 count}. COCT (the count of
            // CNTO records) is likewise redundant with just reading each
            // CNTO sub-record as it appears.
            b"CNTO" if sub.data.len() >= 8 => {
                if let QustBlock::Alias(alias) = &mut block {
                    let mut r = SubReader::new(&sub.data);
                    let item = r.u32_or_default();
                    let count = r.u32_or_default();
                    alias.injected.inventory.push((item, count));
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

    /// Minimal 28-byte FO3/FNV-layout CTDA (see
    /// `parse_qust_stage_ctda_attaches_to_its_stage` for the field-by-
    /// field byte breakdown) carrying only `function_index` — enough to
    /// tell one synthetic condition apart from another in the alias
    /// tests below.
    fn minimal_ctda(function_index: u32) -> Vec<u8> {
        let mut ctda = Vec::new();
        ctda.push(0x00u8);
        ctda.extend_from_slice(&[0u8; 3]);
        ctda.extend_from_slice(&1.0f32.to_le_bytes());
        ctda.extend_from_slice(&function_index.to_le_bytes());
        ctda.extend_from_slice(&0u32.to_le_bytes());
        ctda.extend_from_slice(&0u32.to_le_bytes());
        ctda.extend_from_slice(&0u32.to_le_bytes());
        ctda.extend_from_slice(&0u32.to_le_bytes());
        ctda
    }

    #[test]
    fn parse_qust_alias_forced_reference() {
        // The cheapest fill type (M47.3 Phase 1's first target): ALST +
        // ALID + ALFR + FNAM + ALED, no companions.
        let subs = vec![
            sub(b"EDID", b"TestQuest\0"),
            sub(b"ALST", &7i32.to_le_bytes()),
            sub(b"ALID", b"QuestGiver\0"),
            sub(b"ALFR", &0x0001_2345u32.to_le_bytes()),
            sub(b"FNAM", &ALIAS_FLAG_ESSENTIAL.to_le_bytes()),
            sub(b"ALED", &[]),
        ];
        let q = parse_qust(0xFEED, &subs, &None);
        assert_eq!(q.aliases.len(), 1);
        let alias = &q.aliases[0];
        assert_eq!(alias.alias_id, 7);
        assert!(!alias.is_location);
        assert_eq!(alias.name, "QuestGiver");
        assert_eq!(
            alias.fill_type,
            Some(AliasFillType::ForcedReference(0x0001_2345))
        );
        assert!(alias.flags.has(ALIAS_FLAG_ESSENTIAL));
        assert!(!alias.flags.has(ALIAS_FLAG_OPTIONAL));
    }

    #[test]
    fn parse_qust_alias_unique_actor() {
        let subs = vec![
            sub(b"ALST", &0i32.to_le_bytes()),
            sub(b"ALID", b"Bandit\0"),
            sub(b"ALUA", &0x000A_0001u32.to_le_bytes()),
        ];
        let q = parse_qust(0x1, &subs, &None);
        assert_eq!(
            q.aliases[0].fill_type,
            Some(AliasFillType::UniqueActor(0x000A_0001))
        );
    }

    #[test]
    fn parse_qust_alias_created_object_with_companions() {
        // ALCO opens the fill type; ALCA/ALCL are companion fields that
        // arrive after it and must attach to the SAME fill_type variant.
        let subs = vec![
            sub(b"ALST", &1i32.to_le_bytes()),
            sub(b"ALCO", &0x000B_0002u32.to_le_bytes()),
            sub(b"ALCA", &11i32.to_le_bytes()),
            sub(b"ALCL", &22i32.to_le_bytes()),
        ];
        let q = parse_qust(0x2, &subs, &None);
        assert_eq!(
            q.aliases[0].fill_type,
            Some(AliasFillType::CreatedObject {
                base: 0x000B_0002,
                alca: 11,
                alcl: 22,
            })
        );
    }

    #[test]
    fn parse_qust_alias_external_reference_with_companion() {
        let subs = vec![
            sub(b"ALST", &2i32.to_le_bytes()),
            sub(b"ALEQ", &0x000C_0003u32.to_le_bytes()),
            sub(b"ALEA", &4i32.to_le_bytes()),
        ];
        let q = parse_qust(0x3, &subs, &None);
        assert_eq!(
            q.aliases[0].fill_type,
            Some(AliasFillType::ExternalAlias {
                quest: 0x000C_0003,
                alias_id: 4,
            })
        );
    }

    #[test]
    fn parse_qust_alias_from_event_with_companion() {
        let subs = vec![
            sub(b"ALST", &3i32.to_le_bytes()),
            sub(b"ALFE", b"Scri"),
            sub(b"ALFD", &99i32.to_le_bytes()),
        ];
        let q = parse_qust(0x4, &subs, &None);
        assert_eq!(
            q.aliases[0].fill_type,
            Some(AliasFillType::FromEvent {
                event_type: *b"Scri",
                data: 99,
            })
        );
    }

    #[test]
    fn parse_qust_alias_forced_location_is_alls_only() {
        let subs = vec![
            sub(b"ALLS", &5i32.to_le_bytes()),
            sub(b"ALID", b"Location\0"),
            sub(b"ALFL", &0x000D_0004u32.to_le_bytes()),
        ];
        let q = parse_qust(0x5, &subs, &None);
        assert!(q.aliases[0].is_location);
        assert_eq!(
            q.aliases[0].fill_type,
            Some(AliasFillType::ForcedLocation(0x000D_0004))
        );
    }

    #[test]
    fn parse_qust_alias_location_alias_reference_with_companion() {
        let subs = vec![
            sub(b"ALST", &6i32.to_le_bytes()),
            sub(b"ALRT", &0x000E_0005u32.to_le_bytes()),
            sub(b"ALFA", &(-1i32).to_le_bytes()),
        ];
        let q = parse_qust(0x6, &subs, &None);
        assert_eq!(
            q.aliases[0].fill_type,
            Some(AliasFillType::LocationAliasReference {
                lcrt: 0x000E_0005,
                alfa: -1,
            })
        );
    }

    #[test]
    fn parse_qust_alias_find_matching_reference_has_no_fill_type() {
        // No fill-type field at all — only Match Conditions. The alias
        // still decodes; `fill_type` stays `None`, exactly the "Find
        // Matching Reference/Location" shape the source describes.
        let subs = vec![
            sub(b"ALST", &8i32.to_le_bytes()),
            sub(b"ALID", b"AnyBandit\0"),
            sub(b"CTDA", &minimal_ctda(60)),
            sub(b"CTDA", &minimal_ctda(61)),
            sub(b"FNAM", &(ALIAS_FLAG_IN_LOADED_AREA | ALIAS_FLAG_CLOSEST).to_le_bytes()),
        ];
        let q = parse_qust(0x7, &subs, &None);
        let alias = &q.aliases[0];
        assert_eq!(alias.fill_type, None);
        assert_eq!(alias.match_conditions.len(), 2);
        assert_eq!(alias.match_conditions[0].function_index, 60);
        assert_eq!(alias.match_conditions[1].function_index, 61);
        assert!(alias.flags.has(ALIAS_FLAG_IN_LOADED_AREA));
        assert!(alias.flags.has(ALIAS_FLAG_CLOSEST));
    }

    #[test]
    fn parse_qust_alias_match_conditions_alongside_a_fill_type() {
        // The source notes CTDA can accompany another fill type too
        // (not just Find Matching) — both must land.
        let subs = vec![
            sub(b"ALST", &9i32.to_le_bytes()),
            sub(b"ALFR", &0x0001_0000u32.to_le_bytes()),
            sub(b"CTDA", &minimal_ctda(71)),
        ];
        let q = parse_qust(0x8, &subs, &None);
        let alias = &q.aliases[0];
        assert_eq!(alias.fill_type, Some(AliasFillType::ForcedReference(0x0001_0000)));
        assert_eq!(alias.match_conditions.len(), 1);
        assert_eq!(alias.match_conditions[0].function_index, 71);
    }

    #[test]
    fn parse_qust_alias_force_into_alias_target_has_no_fill_type() {
        // The real, raw-byte-verified shape from Skyrim.esm quest
        // `0002C258` (`qust_alias_rawdump`): alias 1 ("Nurelion") is
        // ALFR-filled and carries `ALFI = 8`; alias 8
        // ("NurelionEssential") is the *target* — it has no fill-type
        // field and no CTDA at all, existing purely to receive alias 1's
        // value. Both sides decode correctly and independently; nothing
        // about alias 8 alone reveals *why* it has no fill type — that
        // requires cross-referencing alias 1's `force_into_alias`
        // (deferred to the M47.3 runtime, not this parser).
        let subs = vec![
            sub(b"ALST", &1i32.to_le_bytes()),
            sub(b"ALID", b"Nurelion\0"),
            sub(b"FNAM", &0u32.to_le_bytes()),
            sub(b"ALFI", &8i32.to_le_bytes()),
            sub(b"ALFR", &0x0001_B115u32.to_le_bytes()),
            sub(b"ALED", &[]),
            sub(b"ALST", &8i32.to_le_bytes()),
            sub(b"ALID", b"NurelionEssential\0"),
            sub(b"FNAM", &(ALIAS_FLAG_ESSENTIAL | ALIAS_FLAG_OPTIONAL).to_le_bytes()),
            sub(b"ALED", &[]),
        ];
        let q = parse_qust(0x2C258, &subs, &None);
        assert_eq!(q.aliases.len(), 2);

        let nurelion = &q.aliases[0];
        assert_eq!(nurelion.name, "Nurelion");
        assert_eq!(nurelion.fill_type, Some(AliasFillType::ForcedReference(0x0001_B115)));
        assert_eq!(nurelion.force_into_alias, Some(8));

        let essential = &q.aliases[1];
        assert_eq!(essential.name, "NurelionEssential");
        assert_eq!(essential.fill_type, None);
        assert_eq!(essential.force_into_alias, None);
        assert!(essential.match_conditions.is_empty());
        assert!(essential.flags.has(ALIAS_FLAG_ESSENTIAL));
    }

    #[test]
    fn parse_qust_alias_force_into_alias_alongside_a_fill_type() {
        // ALFI can also accompany a real fill type — both must land.
        let subs = vec![
            sub(b"ALST", &9i32.to_le_bytes()),
            sub(b"ALFR", &0x0001_0000u32.to_le_bytes()),
            sub(b"ALFI", &2i32.to_le_bytes()),
        ];
        let q = parse_qust(0x1, &subs, &None);
        let alias = &q.aliases[0];
        assert_eq!(alias.fill_type, Some(AliasFillType::ForcedReference(0x0001_0000)));
        assert_eq!(alias.force_into_alias, Some(2));
    }

    #[test]
    fn parse_qust_alias_injected_data() {
        let subs = vec![
            sub(b"ALST", &10i32.to_le_bytes()),
            sub(b"ALFR", &0x0002_0000u32.to_le_bytes()),
            sub(b"ALDN", &0x0000_AAAAu32.to_le_bytes()),
            sub(b"VTCK", &0x0000_BBBBu32.to_le_bytes()),
            sub(b"ECOR", &0x0000_CCCCu32.to_le_bytes()),
            sub(b"ALFC", &0x0000_1111u32.to_le_bytes()),
            sub(b"ALFC", &0x0000_2222u32.to_le_bytes()),
            sub(b"ALPC", &0x0000_3333u32.to_le_bytes()),
            sub(b"ALSP", &0x0000_4444u32.to_le_bytes()),
            sub(
                b"KWDA",
                &[
                    0x0000_5555u32.to_le_bytes(),
                    0x0000_6666u32.to_le_bytes(),
                ]
                .concat(),
            ),
            sub(b"CNTO", &[0x0000_7777u32.to_le_bytes(), 3u32.to_le_bytes()].concat()),
        ];
        let q = parse_qust(0x9, &subs, &None);
        let injected = &q.aliases[0].injected;
        assert_eq!(injected.display_name, Some(0x0000_AAAA));
        assert_eq!(injected.voice_type, Some(0x0000_BBBB));
        assert_eq!(injected.combat_override, Some(0x0000_CCCC));
        assert_eq!(injected.factions, vec![0x0000_1111, 0x0000_2222]);
        assert_eq!(injected.packages, vec![0x0000_3333]);
        assert_eq!(injected.spells, vec![0x0000_4444]);
        assert_eq!(injected.keywords, vec![0x0000_5555, 0x0000_6666]);
        assert_eq!(injected.inventory, vec![(0x0000_7777, 3)]);
    }

    #[test]
    fn parse_qust_multiple_aliases_flush_independently() {
        // Three ALST blocks in a row, each with its own fill type — the
        // flush-on-next-opener rule must not bleed one alias's fields
        // into the next.
        let subs = vec![
            sub(b"ALST", &0i32.to_le_bytes()),
            sub(b"ALID", b"First\0"),
            sub(b"ALFR", &0x0000_1000u32.to_le_bytes()),
            sub(b"ALST", &1i32.to_le_bytes()),
            sub(b"ALID", b"Second\0"),
            sub(b"ALUA", &0x0000_2000u32.to_le_bytes()),
            sub(b"ALED", &[]),
            sub(b"ALLS", &2i32.to_le_bytes()),
            sub(b"ALID", b"Third\0"),
            sub(b"ALFL", &0x0000_3000u32.to_le_bytes()),
        ];
        let q = parse_qust(0xA, &subs, &None);
        assert_eq!(q.aliases.len(), 3);
        assert_eq!(q.aliases[0].name, "First");
        assert_eq!(
            q.aliases[0].fill_type,
            Some(AliasFillType::ForcedReference(0x0000_1000))
        );
        assert_eq!(q.aliases[1].name, "Second");
        assert_eq!(
            q.aliases[1].fill_type,
            Some(AliasFillType::UniqueActor(0x0000_2000))
        );
        assert!(!q.aliases[1].is_location);
        assert_eq!(q.aliases[2].name, "Third");
        assert!(q.aliases[2].is_location);
        assert_eq!(
            q.aliases[2].fill_type,
            Some(AliasFillType::ForcedLocation(0x0000_3000))
        );
    }

    #[test]
    fn parse_qust_alias_companion_without_primary_is_a_noop() {
        // A companion field with no matching primary fill-type field
        // beforehand must not fabricate a fill type — the alias just
        // stays `fill_type: None`.
        let subs = vec![sub(b"ALST", &0i32.to_le_bytes()), sub(b"ALCA", &5i32.to_le_bytes())];
        let q = parse_qust(0xB, &subs, &None);
        assert_eq!(q.aliases[0].fill_type, None);
    }

    #[test]
    fn alias_flags_has_recognizes_every_named_bit() {
        // Every `ALIAS_FLAG_*` constant, individually — guards against a
        // copy-paste bit-value typo in the catalog (each must be its own
        // distinct, correctly-shifted bit).
        const ALL_FLAGS: &[u32] = &[
            ALIAS_FLAG_RESERVES,
            ALIAS_FLAG_OPTIONAL,
            ALIAS_FLAG_QUEST_OBJECT,
            ALIAS_FLAG_ALLOW_REUSE,
            ALIAS_FLAG_ALLOW_DEAD,
            ALIAS_FLAG_IN_LOADED_AREA,
            ALIAS_FLAG_ESSENTIAL,
            ALIAS_FLAG_ALLOW_DISABLED,
            ALIAS_FLAG_STORES_TEXT,
            ALIAS_FLAG_ALLOW_RESERVED,
            ALIAS_FLAG_PROTECTED,
            ALIAS_FLAG_ALLOW_DESTROYED,
            ALIAS_FLAG_CLOSEST,
            ALIAS_FLAG_USES_STORED_TEXT,
            ALIAS_FLAG_INITIALLY_DISABLED,
            ALIAS_FLAG_ALLOW_CLEARED,
        ];
        let combined = AliasFlags(ALL_FLAGS.iter().fold(0u32, |acc, &f| acc | f));
        for &flag in ALL_FLAGS {
            assert!(combined.has(flag), "bit {flag:#x} not set in the combined mask");
        }
        // Every constant is a distinct bit — the fold above didn't
        // silently collapse two identical values.
        let mut sorted = ALL_FLAGS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ALL_FLAGS.len(), "duplicate flag value in the catalog");
        // A bit outside the catalog is correctly absent.
        assert!(!combined.has(0x0000_0800));
    }
}
