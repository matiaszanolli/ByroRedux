//! `QUST` quest records — stages, objectives, and the Skyrim+ VMAD
//! fragment-dispatch bindings.

use super::super::common::{read_lstring_or_zstring, read_zstring, CommonNamedFields};
use super::super::condition::{push_ctda, ConditionList};
use super::super::script_instance::{
    parse_quest_fragments, QuestScriptFragment, ScriptInstanceData,
};
use crate::esm::reader::SubRecord;
use crate::esm::sub_reader::SubReader;

/// `QUST.DATA` / `QUST.DNAM` bit 0: activate the quest during game bootstrap
/// instead of waiting for Story Manager or an explicit Papyrus `Start()` call.
pub const QUEST_FLAG_START_GAME_ENABLED: u16 = 0x0001;
pub const QUEST_FLAG_COMPLETED: u16 = 0x0002;
pub const QUEST_FLAG_ALLOW_REPEATED_STAGES: u16 = 0x0008;
pub const QUEST_FLAG_FAILED: u16 = 0x0040;
pub const QUEST_FLAG_ACTIVE: u16 = 0x0800;
pub const QUEST_STAGE_FLAG_START_UP: u8 = 0x02;
pub const QUEST_STAGE_FLAG_SHUT_DOWN: u8 = 0x04;
pub const QUEST_LOG_FLAG_COMPLETE_QUEST: u8 = 0x01;
pub const QUEST_LOG_FLAG_FAIL_QUEST: u8 = 0x02;

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
    /// Stage index from INDX. All supported layouts store this as u16.
    pub index: u16,
    /// Version-specific stage flags. FO3/FNV store them in `QSDT`; Skyrim+
    /// stores them in `INDX` byte 2 (`0x02` = Start Up Stage, `0x04` = Shut
    /// Down Stage, `0x08` = Keep Instance Data From Here On).
    pub flags: u8,
    /// Individual QSDT log entries. A stage may carry multiple conditional
    /// entries with distinct terminal flags; preserving them prevents a
    /// conditional Complete/Fail pair from being flattened incorrectly.
    pub log_entries: Vec<QuestStageLogEntry>,
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestStageLogEntry {
    /// QSDT flags (`Complete Quest`, `Fail Quest`).
    pub flags: u8,
    pub text: String,
    /// FO4 `NAM2` authoring note.
    pub note: String,
    pub has_script: bool,
    pub conditions: ConditionList,
    pub next_quest: Option<u32>,
}

/// One objective of a quest, defined by a `QOBJ` block. Objectives
/// surface in the Pip-Boy / quest journal; their targets drive the
/// map marker / compass indicator.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestObjective {
    /// Objective index from QOBJ: signed 32-bit on FO3/FNV and u16 on
    /// Skyrim+/FO4. The common i32 representation retains both layouts and
    /// matches Papyrus's `int` API without narrowing legacy mod-authored IDs.
    pub index: i32,
    /// Objective text (`NNAM` on Skyrim+, `CNAM` on FO3/FNV). Empty
    /// when the objective ships no display text — rare but
    /// permissible per the on-disk schema.
    pub text: String,
    /// FNAM objective flags (`ORed With Previous`, and FO4's tracking bit).
    pub flags: u32,
    /// Canonical target records. FO3/FNV store placed-reference FormIDs;
    /// Skyrim+ stores quest alias IDs in the same leading four bytes.
    pub targets: Vec<QuestObjectiveTarget>,
    /// Compatibility projection of pre-Skyrim reference targets.
    pub target_refs: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuestObjectiveTarget {
    pub target: QuestObjectiveTargetKind,
    pub flags: u32,
    /// FO4 form version 82+ optional target keyword.
    pub keyword: Option<u32>,
    pub conditions: ConditionList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestObjectiveTargetKind {
    Reference(u32),
    Alias(i32),
}

/// One quest alias, defined by an `ALST` (Reference alias) or `ALLS`
/// (Location alias) block. Aliases are Radiant Story's targeting
/// mechanism — a quest names a *role* ("QuestGiver", "Location") rather
/// than a specific reference, and this is filled in at runtime per
/// [`AliasFillType`]. This parser remains pure data; the live fill/apply
/// consumer is `byroredux_scripting::scene::refresh_scene_actor_bindings`.
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
    /// FO4 `ALCS` reference-collection alias. Excluded from the ordinary
    /// single-entity fill loop in
    /// `byroredux_scripting::scene::refresh_scene_actor_bindings` (#2661 /
    /// SCR-D6-NEW11-04) — reference collections are a documented Phase 4+
    /// deferral with no collection-fill runtime yet; the diagnostic
    /// reports `ReferenceCollectionRuntimeUnavailable` for these until
    /// one exists.
    pub is_collection: bool,
    /// FO4 `ALMI` collection fill limit. Parsed and carried, but has NO
    /// consumer yet (#2661) — collection aliases decline at the fill
    /// stage entirely (see `is_collection`), so there is nothing to bound
    /// with a fill limit today. Intentionally not dropped: the Phase 4+
    /// collection-fill runtime will need this value when it exists.
    pub max_initial_fill_count: Option<u8>,
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
    /// `ALCC` — choose the candidate closest to this already-filled alias.
    pub closest_to_alias: Option<i32>,
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
    /// `ALCO` + `ALCA` + `ALCL` — instantiate a reference at/in another
    /// alias using the authored encounter level.
    CreatedObject {
        base: u32,
        target_alias: i16,
        create_mode: u16,
        level: u32,
    },
    /// `ALEQ` + `ALEA` — copy the value from another quest's alias
    /// (`quest`'s alias `alias_id`).
    ExternalAlias { quest: u32, alias_id: i32 },
    /// `ALFA` + optional `KNAM`/`ALRT` — resolve relative to another alias,
    /// optionally constrained by linked-reference keyword/reference type.
    LocationAliasReference {
        alias_id: i32,
        keyword: Option<u32>,
        ref_type: Option<u32>,
    },
    /// `ALNA` + `ALNT` — find a linked reference near another alias.
    NearAlias { alias_id: i32, relation: u32 },
    /// `ALFE` + `ALFD` — filled from a Story Manager event.
    FromEvent { event_type: [u8; 4], data: i32 },
}

/// `ALST`/`ALLS` `FNAM` alias flags. A plain bit-constant newtype
/// (mirrors `LIGHT_FLAG_*` in `components/light.rs`), not a `bitflags!`
/// type — matches this crate's existing convention for parsed on-disk
/// flag fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AliasFlags(pub u32);

// The full bit catalog stays parser-owned. The M47.3 runtime consumes the
// reservation/reuse/dead subset and exposes remaining authored metadata for
// later gameplay components. Every constant is exercised by an `AliasFlags::has`
// assertion in the test module below (dead-code analysis just doesn't
// credit test-only usage for the non-test build), so all are
// `dead_code`-allowed here rather than left to warn.
//
// #2983 — that "every constant" claim used to be unenforced: the test roster
// was hand-copied and a 26th constant would simply never have been exercised.
// `alias_flag_roster_covers_every_declared_constant` now counts these
// declarations from source text, so adding a constant here without a roster
// entry and value pin below fails the build.
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
pub const ALIAS_FLAG_FORCED_BY_ALIASES: u32 = 0x0000_0800;
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
#[allow(dead_code)]
pub const ALIAS_FLAG_CLEAR_NAMES_WHEN_REMOVED: u32 = 0x0002_0000;
#[allow(dead_code)]
pub const ALIAS_FLAG_ACTORS_ONLY: u32 = 0x0004_0000;
#[allow(dead_code)]
pub const ALIAS_FLAG_CREATE_TEMPORARY: u32 = 0x0008_0000;
#[allow(dead_code)]
pub const ALIAS_FLAG_EXTERNAL_LINKED: u32 = 0x0010_0000;
#[allow(dead_code)]
pub const ALIAS_FLAG_NO_PICKPOCKET: u32 = 0x0020_0000;
#[allow(dead_code)]
pub const ALIAS_FLAG_APPLY_TO_NON_ALIASED_REFS: u32 = 0x0040_0000;
#[allow(dead_code)]
pub const ALIAS_FLAG_COMPANION: u32 = 0x0080_0000;
#[allow(dead_code)]
pub const ALIAS_FLAG_OPTIONAL_ALL_SCENES: u32 = 0x0100_0000;

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
    /// FO4 `ALFV` → forced `VTYP`.
    pub forced_voice: Option<u32>,
    /// FO4 `ALDI` → death-item leveled list.
    pub death_item: Option<u32>,
    /// `SPOR` → spectator override package list.
    pub spectator_override: Option<u32>,
    /// `OCOR` → observe-dead-body override package list.
    pub observe_dead_body_override: Option<u32>,
    /// `GWOR` → guard-warn override package list.
    pub guard_warn_override: Option<u32>,
    /// `ECOR` → `FLST`, combat override package list.
    pub combat_override: Option<u32>,
    /// FO4 `ALLA` keyword/alias relationships.
    pub linked_aliases: Vec<AliasLinkedAlias>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AliasLinkedAlias {
    pub keyword: Option<u32>,
    pub alias_id: i32,
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
    /// True when the Skyrim+/FO4 `DNAM` general layout was observed.
    pub skyrim_plus: bool,
    /// Optional FO3/FNV quest script reference (pre-Papyrus bytecode).
    pub script_ref: u32,
    /// Quest flags from DATA byte 0 (`Start Game Enabled`, `Allow
    /// Repeated Stages`, `Event Based`, ...).
    pub quest_flags: u16,
    /// Priority from DATA byte 1. Higher = displayed first in pip-boy.
    pub priority: u8,
    /// DNAM byte 3. Skyrim calls this the QUST form version; FO4 leaves it
    /// unused. Preserved without guessing the game-specific interpretation.
    pub general_version: u8,
    /// DNAM bytes 4..8. FO4 interprets these as the Story Manager delay
    /// (`f32`); Skyrim labels them unknown. Raw preservation keeps the parser
    /// lossless across both layouts.
    pub general_aux: [u8; 4],
    /// DNAM quest type. Skyrim stores a u32; FO4 stores a u8 followed by three
    /// unused zero bytes, so the common little-endian u32 decode is lossless.
    pub quest_type: u32,
    /// Skyrim+/FO4 Story Manager event (`ENAM`).
    pub event: Option<[u8; 4]>,
    /// FO4 owning location (`LNAM`) and completion-XP global (`XNAM`).
    pub location: Option<u32>,
    pub completion_xp: Option<u32>,
    pub text_display_globals: Vec<u32>,
    pub object_window_filter: String,
    pub dialogue_conditions: ConditionList,
    pub secondary_conditions: ConditionList,
    /// All defined stages, in authoring order (which is also INDX
    /// order on every vanilla master sampled). Most quests ship 5-20
    /// stages; the longest vanilla FNV quest (Heartache by the
    /// Number) has 60+.
    pub stages: Vec<QuestStage>,
    /// `INDX` of the Skyrim+ stage carrying `INDX` flag `0x02`. The Story
    /// Manager advances the quest to
    /// this index on activation; `None` when no stage has the bit set.
    pub start_up_stage: Option<u16>,
    /// `INDX` of the Skyrim+ stage carrying `INDX` flag `0x04`. Papyrus
    /// `Quest.Stop()` advances through this stage so its QUST fragment and
    /// log-entry side effects run before the quest becomes inactive.
    pub shut_down_stage: Option<u16>,
    /// All defined objectives, in authoring order. Objectives are
    /// usually a strict subset of stages (one objective per major
    /// player-visible step).
    pub objectives: Vec<QuestObjective>,
    /// Legacy/Skyrim quest-level reference targets authored after aliases.
    pub targets: Vec<QuestObjectiveTarget>,
    pub next_alias_id: Option<u32>,
    pub description: String,
    pub quest_group: Option<u32>,
    pub swf_file: String,
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
    /// Consumed by the scripting alias-fill/injection runtime. See
    /// [`QuestAlias`].
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
    Alias(Box<QuestAlias>),
}

// Hoisted out of `parse_qust` by the #2412 data-group split so every
// `parse_qust_*` group can reach them; bodies unchanged.
fn remap_form_id(raw: u32, remap: &Option<crate::esm::reader::FormIdRemap>) -> u32 {
    if raw == 0 {
        0
    } else {
        remap.as_ref().map_or(raw, |mapping| mapping.remap(raw))
    }
}

// Close whichever block is currently open and push it onto the
// record. Called when a new block-opener appears or when the walk
// finishes.
fn flush_block(out: &mut QustRecord, block: QustBlock) {
    match block {
        QustBlock::Stage(stage) => {
            if stage.flags & QUEST_STAGE_FLAG_START_UP != 0 && out.start_up_stage.is_none() {
                out.start_up_stage = Some(stage.index);
            }
            if stage.flags & QUEST_STAGE_FLAG_SHUT_DOWN != 0 && out.shut_down_stage.is_none() {
                out.shut_down_stage = Some(stage.index);
            }
            out.stages.push(stage);
        }
        QustBlock::Objective(obj) => out.objectives.push(obj),
        QustBlock::Alias(alias) => out.aliases.push(*alias),
        QustBlock::None => {}
    }
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
    // #2414 / TD2-117 — EDID/FULL via the shared walker. The rest of the
    // record-level fields stay in `parse_qust_header`: QUST's `VMAD` arm
    // decodes the stage→fragment table on top of the common script data,
    // so it is not the shared walker's arm.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    let mut block = QustBlock::None;
    let mut skyrim_plus = false;
    let mut secondary_conditions = false;

    for sub in subs {
        // #2412 / TD1-012 — dispatched per QUST data group instead of one
        // 63-arm match (cognitive complexity 119/25, the highest measured in
        // the repo). Every sub-record signature appears in exactly one arm,
        // so the group functions partition the old match rather than
        // reordering it; each keeps its own `_ => {}` fallback and a
        // signature no group claims is ignored exactly as before. Mirrors
        // the `parse_npc_*` per-data-group split (#2055).
        parse_qust_header(
            &mut out,
            sub,
            remap,
            &mut skyrim_plus,
            &mut secondary_conditions,
        );
        parse_qust_stage(&mut out, &mut block, sub, remap);
        parse_qust_objective(&mut out, &mut block, sub, remap, skyrim_plus);
        parse_qust_alias(&mut out, &mut block, sub, remap);
        parse_qust_contextual(&mut out, &mut block, sub, remap, secondary_conditions);
    }

    // Flush whichever block was open at the end of the stream.
    flush_block(&mut out, block);

    out
}

/// Record-level QUST fields: identity, flags, the priority /
/// event / owner FormIDs, and the VMAD script block. None of these
/// open or close a block.
fn parse_qust_header(
    out: &mut QustRecord,
    sub: &SubRecord,
    remap: &Option<crate::esm::reader::FormIdRemap>,
    skyrim_plus: &mut bool,
    secondary_conditions: &mut bool,
) {
    match &sub.sub_type {
        b"SCRI" if sub.data.len() >= 4 => {
            out.script_ref = remap_form_id(SubReader::new(&sub.data).u32_or_default(), remap);
        }
        b"DATA" if sub.data.len() >= 2 => {
            out.quest_flags = u16::from(sub.data[0]);
            out.priority = sub.data[1];
        }
        // Skyrim+ widened flags to u16 and moved the quest header to
        // DNAM: flags, priority, form version, four unknown bytes, type.
        b"DNAM" if sub.data.len() >= 3 => {
            *skyrim_plus = true;
            out.skyrim_plus = true;
            out.quest_flags = u16::from_le_bytes([sub.data[0], sub.data[1]]);
            out.priority = sub.data[2];
            out.general_version = sub.data.get(3).copied().unwrap_or_default();
            if sub.data.len() >= 8 {
                out.general_aux.copy_from_slice(&sub.data[4..8]);
            }
            if sub.data.len() >= 9 {
                let mut raw = [0; 4];
                let available = (sub.data.len() - 8).min(4);
                raw[..available].copy_from_slice(&sub.data[8..8 + available]);
                out.quest_type = u32::from_le_bytes(raw);
            }
        }
        b"ENAM" if sub.data.len() >= 4 => {
            let mut event = [0; 4];
            event.copy_from_slice(&sub.data[..4]);
            out.event = Some(event);
        }
        b"LNAM" if sub.data.len() >= 4 => {
            let form_id = remap_form_id(SubReader::new(&sub.data).u32_or_default(), remap);
            out.location = (form_id != 0).then_some(form_id);
        }
        b"XNAM" if sub.data.len() >= 4 => {
            let form_id = remap_form_id(SubReader::new(&sub.data).u32_or_default(), remap);
            out.completion_xp = (form_id != 0).then_some(form_id);
        }
        b"QTGL" if sub.data.len() >= 4 => {
            let form_id = remap_form_id(SubReader::new(&sub.data).u32_or_default(), remap);
            if form_id != 0 {
                out.text_display_globals.push(form_id);
            }
        }
        b"FLTR" => out.object_window_filter = read_zstring(&sub.data),
        b"NEXT" => *secondary_conditions = true,
        // Skyrim+ Papyrus attachment. Two independent decodes of the
        // same bytes: the trailing fragment section (stage→
        // `Fragment_N`, the M47.2 fragment-dispatch input) and the
        // leading scripts section (the QF_ script's own property
        // table — how a fragment's cross-quest `Quest Property`
        // effect resolves to a FormID).
        b"VMAD" => {
            out.fragments = parse_quest_fragments(&sub.data);
            out.script_instance = Some(ScriptInstanceData::parse_with_remap(&sub.data, remap));
        }
        b"ANAM" if sub.data.len() >= 4 => {
            out.next_alias_id = Some(SubReader::new(&sub.data).u32_or_default());
        }
        b"GNAM" if sub.data.len() >= 4 => {
            let form_id = remap_form_id(SubReader::new(&sub.data).u32_or_default(), remap);
            out.quest_group = (form_id != 0).then_some(form_id);
        }
        b"SNAM" => out.swf_file = read_zstring(&sub.data),
        _ => {}
    }
}

/// Stage data group: `INDX` opens a stage block, the rest fill it.
fn parse_qust_stage(
    out: &mut QustRecord,
    block: &mut QustBlock,
    sub: &SubRecord,
    remap: &Option<crate::esm::reader::FormIdRemap>,
) {
    match &sub.sub_type {
        // INDX opens a stage block. Anything still open (a prior
        // stage or objective) is flushed first. Skyrim+ stores the
        // u16 index followed by the stage flags and an unused byte.
        b"INDX" if sub.data.len() >= 2 => {
            let prev = std::mem::replace(block, QustBlock::None);
            flush_block(out, prev);
            let mut r = SubReader::new(&sub.data);
            let index = r.u16_or_default();
            *block = QustBlock::Stage(QuestStage {
                index,
                flags: sub.data.get(2).copied().unwrap_or_default(),
                ..Default::default()
            });
        }
        b"QSDT" if !sub.data.is_empty() => {
            if let QustBlock::Stage(stage) = &mut *block {
                stage.log_entries.push(QuestStageLogEntry {
                    flags: sub.data[0],
                    ..Default::default()
                });
            }
        }
        // SCHR / SCDA inside a stage block mean the stage has an
        // advance-time bytecode block (Oblivion / FO3 / FNV).
        // The bytecode itself isn't decoded here — flagged for
        // M47.2's consumer.
        b"SCHR" | b"SCDA" => {
            if let QustBlock::Stage(stage) = &mut *block {
                stage.has_script = true;
                if let Some(log) = stage.log_entries.last_mut() {
                    log.has_script = true;
                }
            }
        }
        b"NAM2" => {
            if let QustBlock::Stage(stage) = &mut *block {
                if let Some(log) = stage.log_entries.last_mut() {
                    log.note = read_zstring(&sub.data);
                }
            }
        }
        b"NAM0" if sub.data.len() >= 4 => {
            if let QustBlock::Stage(stage) = &mut *block {
                if let Some(log) = stage.log_entries.last_mut() {
                    let quest = remap_form_id(SubReader::new(&sub.data).u32_or_default(), remap);
                    log.next_quest = (quest != 0).then_some(quest);
                }
            }
        }
        _ => {}
    }
}

/// Objective data group: `QOBJ` opens an objective block, `QSTA`
/// appends its targets (a placed REFR on FO3/FNV, an alias id on
/// Skyrim+).
fn parse_qust_objective(
    out: &mut QustRecord,
    block: &mut QustBlock,
    sub: &SubRecord,
    remap: &Option<crate::esm::reader::FormIdRemap>,
    skyrim_plus: bool,
) {
    match &sub.sub_type {
        // QOBJ opens an objective block. Same flush rule as INDX.
        b"QOBJ" if sub.data.len() >= 2 => {
            let prev = std::mem::replace(block, QustBlock::None);
            flush_block(out, prev);
            let mut r = SubReader::new(&sub.data);
            let index = if skyrim_plus || sub.data.len() == 2 {
                i32::from(r.u16_or_default())
            } else {
                r.i32_or_default()
            };
            *block = QustBlock::Objective(QuestObjective {
                index,
                ..Default::default()
            });
        }
        // QSTA is a placed-reference FormID on FO3/FNV and an alias ID on
        // Skyrim+. Both carry flags in bytes 4..8; FO4 may append a KYWD.
        b"QSTA" if sub.data.len() >= 4 => {
            let mut r = SubReader::new(&sub.data);
            let raw_target = r.u32_or_default();
            let flags = if r.remaining() >= 4 {
                r.u32_or_default()
            } else {
                0
            };
            let keyword = (r.remaining() >= 4)
                .then(|| remap_form_id(r.u32_or_default(), remap))
                .filter(|keyword| *keyword != 0);
            match &mut *block {
                QustBlock::Objective(obj) if skyrim_plus => {
                    obj.targets.push(QuestObjectiveTarget {
                        target: QuestObjectiveTargetKind::Alias(raw_target as i32),
                        flags,
                        keyword,
                        conditions: Vec::new(),
                    });
                }
                QustBlock::Objective(obj) if raw_target != 0 => {
                    let target = remap_form_id(raw_target, remap);
                    obj.target_refs.push(target);
                    obj.targets.push(QuestObjectiveTarget {
                        target: QuestObjectiveTargetKind::Reference(target),
                        flags,
                        keyword,
                        conditions: Vec::new(),
                    });
                }
                QustBlock::None if raw_target != 0 => {
                    out.targets.push(QuestObjectiveTarget {
                        target: QuestObjectiveTargetKind::Reference(remap_form_id(
                            raw_target, remap,
                        )),
                        flags,
                        keyword,
                        conditions: Vec::new(),
                    });
                }
                QustBlock::Stage(_)
                | QustBlock::Alias(_)
                | QustBlock::Objective(_)
                | QustBlock::None => {}
            }
        }
        _ => {}
    }
}

/// Alias data group: every `AL*` opener/field plus the alias-scoped
/// spell / keyword / inventory / package sub-records.
fn parse_qust_alias(
    out: &mut QustRecord,
    block: &mut QustBlock,
    sub: &SubRecord,
    remap: &Option<crate::esm::reader::FormIdRemap>,
) {
    match &sub.sub_type {
        // ALST/ALLS opens an alias block — a Reference alias or a
        // Location alias respectively. Same flush rule as INDX/QOBJ.
        b"ALST" if sub.data.len() >= 4 => {
            let prev = std::mem::replace(block, QustBlock::None);
            flush_block(out, prev);
            let alias_id = SubReader::new(&sub.data).i32_or_default();
            *block = QustBlock::Alias(Box::new(QuestAlias {
                alias_id,
                is_location: false,
                ..Default::default()
            }));
        }
        b"ALLS" if sub.data.len() >= 4 => {
            let prev = std::mem::replace(block, QustBlock::None);
            flush_block(out, prev);
            let alias_id = SubReader::new(&sub.data).i32_or_default();
            *block = QustBlock::Alias(Box::new(QuestAlias {
                alias_id,
                is_location: true,
                ..Default::default()
            }));
        }
        b"ALCS" if sub.data.len() >= 4 => {
            let prev = std::mem::replace(block, QustBlock::None);
            flush_block(out, prev);
            let alias_id = SubReader::new(&sub.data).i32_or_default();
            *block = QustBlock::Alias(Box::new(QuestAlias {
                alias_id,
                is_collection: true,
                ..Default::default()
            }));
        }
        b"ALMI" if !sub.data.is_empty() => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.max_initial_fill_count = Some(sub.data[0]);
            }
            // FO4 collection aliases are exactly ALCS + ALMI and do not
            // carry the ALED terminator used by reference/location
            // aliases. Close now so the following quest NNAM description
            // is not misclassified as alias-local data.
            let prev = std::mem::replace(block, QustBlock::None);
            flush_block(out, prev);
        }
        // ALED is the explicit "end of this alias" terminator (the
        // source: "always the final field in a set of ALID
        // entries") — flush now rather than waiting for the next
        // block-opener or end of stream.
        b"ALED" => {
            let prev = std::mem::replace(block, QustBlock::None);
            flush_block(out, prev);
        }
        b"ALID" => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.name = read_zstring(&sub.data);
            }
        }
        // ── Fill-type fields (mutually exclusive on disk) ──
        b"ALFR" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                let fid = remap_form_id(SubReader::new(&sub.data).u32_or_default(), remap);
                alias.fill_type = Some(AliasFillType::ForcedReference(fid));
            }
        }
        b"ALFL" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                let fid = remap_form_id(SubReader::new(&sub.data).u32_or_default(), remap);
                alias.fill_type = Some(AliasFillType::ForcedLocation(fid));
            }
        }
        b"ALUA" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                let fid = remap_form_id(SubReader::new(&sub.data).u32_or_default(), remap);
                alias.fill_type = Some(AliasFillType::UniqueActor(fid));
            }
        }
        b"ALCO" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                let fid = remap_form_id(SubReader::new(&sub.data).u32_or_default(), remap);
                alias.fill_type = Some(AliasFillType::CreatedObject {
                    base: fid,
                    target_alias: 0,
                    create_mode: 0,
                    level: 0,
                });
            }
        }
        b"ALEQ" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                let fid = remap_form_id(SubReader::new(&sub.data).u32_or_default(), remap);
                alias.fill_type = Some(AliasFillType::ExternalAlias {
                    quest: fid,
                    alias_id: 0,
                });
            }
        }
        b"ALRT" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                let fid = remap_form_id(SubReader::new(&sub.data).u32_or_default(), remap);
                match &mut alias.fill_type {
                    Some(AliasFillType::LocationAliasReference { ref_type, .. }) => {
                        *ref_type = (fid != 0).then_some(fid);
                    }
                    _ => {
                        alias.fill_type = Some(AliasFillType::LocationAliasReference {
                            alias_id: 0,
                            keyword: None,
                            ref_type: (fid != 0).then_some(fid),
                        });
                    }
                }
            }
        }
        b"ALFE" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
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
            if let QustBlock::Alias(alias) = &mut *block {
                if let Some(AliasFillType::CreatedObject {
                    target_alias,
                    create_mode,
                    ..
                }) = &mut alias.fill_type
                {
                    let mut r = SubReader::new(&sub.data);
                    *target_alias = r.i16_or_default();
                    *create_mode = r.u16_or_default();
                }
            }
        }
        b"ALCL" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                if let Some(AliasFillType::CreatedObject { level, .. }) = &mut alias.fill_type {
                    *level = SubReader::new(&sub.data).u32_or_default();
                }
            }
        }
        b"ALEA" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                if let Some(AliasFillType::ExternalAlias { alias_id, .. }) = &mut alias.fill_type {
                    *alias_id = SubReader::new(&sub.data).i32_or_default();
                }
            }
        }
        b"ALFA" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                let alias_id = SubReader::new(&sub.data).i32_or_default();
                match &mut alias.fill_type {
                    Some(AliasFillType::LocationAliasReference {
                        alias_id: current, ..
                    }) => *current = alias_id,
                    _ => {
                        alias.fill_type = Some(AliasFillType::LocationAliasReference {
                            alias_id,
                            keyword: None,
                            ref_type: None,
                        });
                    }
                }
            }
        }
        b"KNAM" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                let form_id = remap_form_id(SubReader::new(&sub.data).u32_or_default(), remap);
                if let Some(AliasFillType::LocationAliasReference { keyword, .. }) =
                    &mut alias.fill_type
                {
                    *keyword = (form_id != 0).then_some(form_id);
                }
            }
        }
        b"ALNA" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.fill_type = Some(AliasFillType::NearAlias {
                    alias_id: SubReader::new(&sub.data).i32_or_default(),
                    relation: 0,
                });
            }
        }
        b"ALNT" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                if let Some(AliasFillType::NearAlias { relation, .. }) = &mut alias.fill_type {
                    *relation = SubReader::new(&sub.data).u32_or_default();
                }
            }
        }
        b"ALFD" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
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
            if let QustBlock::Alias(alias) = &mut *block {
                alias.force_into_alias = Some(SubReader::new(&sub.data).i32_or_default());
            }
        }
        b"ALCC" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.closest_to_alias = Some(SubReader::new(&sub.data).i32_or_default());
            }
        }
        b"ALDN" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.injected.display_name = Some(remap_form_id(
                    SubReader::new(&sub.data).u32_or_default(),
                    remap,
                ));
            }
        }
        b"VTCK" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.injected.voice_type = Some(remap_form_id(
                    SubReader::new(&sub.data).u32_or_default(),
                    remap,
                ));
            }
        }
        b"ALFV" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.injected.forced_voice = Some(remap_form_id(
                    SubReader::new(&sub.data).u32_or_default(),
                    remap,
                ));
            }
        }
        b"ALDI" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.injected.death_item = Some(remap_form_id(
                    SubReader::new(&sub.data).u32_or_default(),
                    remap,
                ));
            }
        }
        b"SPOR" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.injected.spectator_override = Some(remap_form_id(
                    SubReader::new(&sub.data).u32_or_default(),
                    remap,
                ));
            }
        }
        b"OCOR" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.injected.observe_dead_body_override = Some(remap_form_id(
                    SubReader::new(&sub.data).u32_or_default(),
                    remap,
                ));
            }
        }
        b"GWOR" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.injected.guard_warn_override = Some(remap_form_id(
                    SubReader::new(&sub.data).u32_or_default(),
                    remap,
                ));
            }
        }
        b"ECOR" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.injected.combat_override = Some(remap_form_id(
                    SubReader::new(&sub.data).u32_or_default(),
                    remap,
                ));
            }
        }
        b"ALFC" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.injected.factions.push(remap_form_id(
                    SubReader::new(&sub.data).u32_or_default(),
                    remap,
                ));
            }
        }
        b"ALPC" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.injected.packages.push(remap_form_id(
                    SubReader::new(&sub.data).u32_or_default(),
                    remap,
                ));
            }
        }
        b"ALSP" if sub.data.len() >= 4 => {
            if let QustBlock::Alias(alias) = &mut *block {
                alias.injected.spells.push(remap_form_id(
                    SubReader::new(&sub.data).u32_or_default(),
                    remap,
                ));
            }
        }
        // KWDA holds `KSIZ` concatenated keyword FormIds in one
        // sub-record (not one KWDA per keyword) — read every u32 in
        // the payload. KSIZ itself is redundant with the payload
        // length (same "read what's there" approach QSTA/CTDA use
        // elsewhere in this parser) so it isn't separately tracked.
        b"KWDA" => {
            if let QustBlock::Alias(alias) = &mut *block {
                let mut r = SubReader::new(&sub.data);
                while r.remaining() >= 4 {
                    alias
                        .injected
                        .keywords
                        .push(remap_form_id(r.u32_or_default(), remap));
                }
            }
        }
        b"ALLA" => {
            if let QustBlock::Alias(alias) = &mut *block {
                let mut r = SubReader::new(&sub.data);
                while r.remaining() >= 8 {
                    let keyword = remap_form_id(r.u32_or_default(), remap);
                    let alias_id = r.i32_or_default();
                    alias.injected.linked_aliases.push(AliasLinkedAlias {
                        keyword: (keyword != 0).then_some(keyword),
                        alias_id,
                    });
                }
            }
        }
        // CNTO: {formid item, uint32 count}. COCT (the count of
        // CNTO records) is likewise redundant with just reading each
        // CNTO sub-record as it appears.
        b"CNTO" if sub.data.len() >= 8 => {
            if let QustBlock::Alias(alias) = &mut *block {
                let mut r = SubReader::new(&sub.data);
                let item = remap_form_id(r.u32_or_default(), remap);
                let count = r.u32_or_default();
                alias.injected.inventory.push((item, count));
            }
        }
        _ => {}
    }
}

/// Sub-records whose meaning depends on which block is open, so they
/// cannot live in any single data group: `CNAM`/`NNAM` (stage log text
/// vs objective text vs quest description), `FNAM`, and the
/// `CTDA`/`CIS1`/`CIS2` condition stream.
fn parse_qust_contextual(
    out: &mut QustRecord,
    block: &mut QustBlock,
    sub: &SubRecord,
    remap: &Option<crate::esm::reader::FormIdRemap>,
    secondary_conditions: bool,
) {
    match &sub.sub_type {
        // CNAM is dual-purpose: stage log text inside a Stage
        // block, objective text inside an Objective block (FO3/
        // FNV authoring path; Skyrim+ moves objective text onto
        // NNAM). Dispatch on the open block.
        b"CNAM" => match &mut *block {
            QustBlock::Stage(stage) => {
                stage.log_text = read_lstring_or_zstring(&sub.data);
                if let Some(log) = stage.log_entries.last_mut() {
                    log.text = stage.log_text.clone();
                }
            }
            QustBlock::Objective(obj) => {
                obj.text = read_lstring_or_zstring(&sub.data);
            }
            QustBlock::Alias(_) | QustBlock::None => {}
        },
        // NNAM is Skyrim+ objective text. FO3/FNV objectives use
        // CNAM (handled above); both arms are defensive — an
        // older parser sniffing NNAM on FO3 just no-ops.
        b"NNAM" => match &mut *block {
            QustBlock::Objective(obj) => {
                obj.text = read_lstring_or_zstring(&sub.data);
            }
            QustBlock::None => out.description = read_lstring_or_zstring(&sub.data),
            QustBlock::Stage(_) | QustBlock::Alias(_) => {}
        },
        // CTDA also appears inside an alias block ("Match
        // Conditions" — the Find Matching Reference/Location fill
        // type's predicate list, or an additional gate alongside
        // another fill type). Stage and Alias are the two block
        // kinds that currently collect conditions here.
        b"CTDA" | b"CIS1" | b"CIS2" => match &mut *block {
            QustBlock::Stage(stage) => {
                push_ctda(sub, remap, &mut stage.conditions);
                if let Some(log) = stage.log_entries.last_mut() {
                    push_ctda(sub, remap, &mut log.conditions);
                }
            }
            QustBlock::Alias(alias) => push_ctda(sub, remap, &mut alias.match_conditions),
            QustBlock::Objective(objective) => {
                if let Some(target) = objective.targets.last_mut() {
                    push_ctda(sub, remap, &mut target.conditions);
                }
            }
            QustBlock::None => {
                if let Some(target) = out.targets.last_mut() {
                    push_ctda(sub, remap, &mut target.conditions);
                } else if secondary_conditions {
                    push_ctda(sub, remap, &mut out.secondary_conditions);
                } else {
                    push_ctda(sub, remap, &mut out.dialogue_conditions);
                }
            }
        },
        // ── FNAM flags + injected data. FNAM also appears at QOBJ
        // level with the distinct objective-flag catalog. ──
        b"FNAM" if sub.data.len() >= 4 => match &mut *block {
            QustBlock::Alias(alias) => {
                alias.flags = AliasFlags(SubReader::new(&sub.data).u32_or_default());
            }
            QustBlock::Objective(objective) => {
                objective.flags = SubReader::new(&sub.data).u32_or_default();
            }
            QustBlock::Stage(_) | QustBlock::None => {}
        },
        _ => {}
    }
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
    fn parse_skyrim_qust_uses_dnam_header_and_indx_stage_flags() {
        let subs = vec![
            sub(b"EDID", b"MQ101\0"),
            // flags=Run Once|Start Game Enabled, priority=80.
            sub(b"DNAM", &[0x01, 0x01, 80, 0, 0, 0, 0, 0, 1, 0, 0, 0]),
            // Skyrim INDX: u16 stage, u8 flags, u8 unknown. Stage zero is
            // the startup stage even though its following QSDT is clear.
            sub(b"INDX", &[0x00, 0x00, 0x02, 0x00]),
            sub(b"QSDT", &[0x00]),
            // QSDT bit 0 means Complete Quest on Skyrim, not Start Up Stage;
            // it must not make the terminal stage the startup stage.
            sub(b"INDX", &[0x84, 0x03, 0x00, 0x00]),
            sub(b"QSDT", &[0x01]),
            // Stage 1000 is the authored shutdown stage.
            sub(b"INDX", &[0xE8, 0x03, 0x04, 0x00]),
            sub(b"QSDT", &[0x00]),
        ];

        let quest = parse_qust(0x0003_372B, &subs, &None);
        assert_eq!(quest.quest_flags, 0x0101);
        assert_eq!(quest.priority, 80);
        assert_eq!(quest.quest_type, 1);
        assert_eq!(quest.start_up_stage, Some(0));
        assert_eq!(quest.shut_down_stage, Some(1000));
        assert_eq!(quest.stages[0].flags, 0x02);
        assert_eq!(quest.stages[1].flags, 0x00);
        assert_eq!(quest.stages[1].log_entries[0].flags, 0x01);
        assert_eq!(quest.stages[2].flags, 0x04);
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
        // Synthetic FNV-style quest: two stages (10 and 20), plus one
        // objective (1) with two QSTA targets. Mimics INDX/QSDT/CNAM
        // ... QOBJ/NNAM/QSTA grammar.
        let start_log = b"Begin investigation.\0".to_vec();
        let mid_log = b"Reach the vault.\0".to_vec();
        let obj_text = b"Find the dam.\0".to_vec();

        let subs = vec![
            sub(b"EDID", b"DLC02\0"),
            sub(b"FULL", b"Honest Hearts\0"),
            sub(b"DATA", &[0x05, 30]), // quest_flags + priority
            // Stage 10 — Complete Quest log entry + log text + script.
            sub(b"INDX", &10u16.to_le_bytes()),
            sub(b"QSDT", &[0x01]),
            sub(b"CNAM", &start_log),
            sub(b"SCHR", &[0u8; 20]), // dummy SCHR — flags has_script
            // Stage 20 — log text only, no script, no start-up flag.
            sub(b"INDX", &20u16.to_le_bytes()),
            sub(b"QSDT", &[0x00]),
            sub(b"CNAM", &mid_log),
            // Objective 1 — text via CNAM (FO3/FNV path) + two targets.
            sub(b"QOBJ", &70_000i32.to_le_bytes()),
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
        assert_eq!(q.stages[0].flags, 0x00);
        assert_eq!(q.stages[0].log_entries[0].flags, 0x01);
        assert_eq!(q.stages[0].log_text, "Begin investigation.");
        assert!(q.stages[0].has_script);
        assert_eq!(q.stages[1].index, 20);
        assert_eq!(q.stages[1].flags, 0x00);
        assert_eq!(q.stages[1].log_text, "Reach the vault.");
        assert!(!q.stages[1].has_script);

        // FO3/FNV QSDT bit 0 is Complete Quest, not Start Up Stage.
        assert_eq!(q.start_up_stage, None);

        // Objectives.
        assert_eq!(q.objectives.len(), 1);
        let obj = &q.objectives[0];
        assert_eq!(obj.index, 70_000);
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
            sub(b"DNAM", &[0, 0, 50]),
            sub(b"QOBJ", &10u16.to_le_bytes()),
            sub(b"NNAM", b"Find the Elder Scroll.\0"),
            sub(b"QSTA", &[7i32.to_le_bytes(), 1u32.to_le_bytes()].concat()),
        ];
        let q = parse_qust(0xEAEAu32, &subs, &None);
        assert_eq!(q.objectives.len(), 1);
        assert_eq!(q.objectives[0].index, 10);
        assert_eq!(q.objectives[0].text, "Find the Elder Scroll.");
        assert!(q.objectives[0].target_refs.is_empty());
        assert_eq!(q.objectives[0].targets.len(), 1);
        assert_eq!(
            q.objectives[0].targets[0].target,
            QuestObjectiveTargetKind::Alias(7)
        );
        assert_eq!(q.objectives[0].targets[0].flags, 1);
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
            sub(b"QOBJ", &5i32.to_le_bytes()),
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
        assert_eq!(q.stages[0].log_entries[0].conditions.len(), 1);
    }

    #[test]
    fn parse_qust_preserves_each_stage_log_entry() {
        let subs = vec![
            sub(b"INDX", &[40, 0, QUEST_STAGE_FLAG_SHUT_DOWN, 0]),
            sub(b"QSDT", &[QUEST_LOG_FLAG_COMPLETE_QUEST]),
            sub(b"CNAM", b"Succeeded.\0"),
            sub(b"CTDA", &minimal_ctda(10)),
            sub(b"NAM0", &0x0000_1234u32.to_le_bytes()),
            sub(b"QSDT", &[QUEST_LOG_FLAG_FAIL_QUEST]),
            sub(b"CNAM", b"Failed.\0"),
            sub(b"CTDA", &minimal_ctda(11)),
        ];

        let q = parse_qust(0xABCE, &subs, &None);
        let stage = &q.stages[0];
        assert_eq!(stage.flags, QUEST_STAGE_FLAG_SHUT_DOWN);
        assert_eq!(stage.log_entries.len(), 2);
        assert_eq!(stage.log_entries[0].flags, QUEST_LOG_FLAG_COMPLETE_QUEST);
        assert_eq!(stage.log_entries[0].text, "Succeeded.");
        assert_eq!(stage.log_entries[0].conditions[0].function_index, 10);
        assert_eq!(stage.log_entries[0].next_quest, Some(0x0000_1234));
        assert_eq!(stage.log_entries[1].flags, QUEST_LOG_FLAG_FAIL_QUEST);
        assert_eq!(stage.log_entries[1].text, "Failed.");
        assert_eq!(stage.log_entries[1].conditions[0].function_index, 11);
        assert_eq!(stage.log_entries[1].next_quest, None);
    }

    #[test]
    fn parse_qust_remaps_all_form_id_payloads() {
        let remap = Some(crate::esm::reader::FormIdRemap::regular(5, vec![2]));
        let subs = vec![
            sub(b"SCRI", &0x0000_0001u32.to_le_bytes()),
            sub(b"QOBJ", &1i32.to_le_bytes()),
            sub(
                b"QSTA",
                &[0x0000_0002u32.to_le_bytes(), 0u32.to_le_bytes()].concat(),
            ),
            sub(b"ALST", &0i32.to_le_bytes()),
            sub(b"ALFR", &0x0000_0003u32.to_le_bytes()),
            sub(b"ALFC", &0x0000_0004u32.to_le_bytes()),
            sub(
                b"CNTO",
                &[0x0000_0005u32.to_le_bytes(), 2u32.to_le_bytes()].concat(),
            ),
        ];

        let q = parse_qust(0x0500_0010, &subs, &remap);
        assert_eq!(q.script_ref, 0x0200_0001);
        assert_eq!(q.objectives[0].target_refs, vec![0x0200_0002]);
        assert_eq!(
            q.aliases[0].fill_type,
            Some(AliasFillType::ForcedReference(0x0200_0003))
        );
        assert_eq!(q.aliases[0].injected.factions, vec![0x0200_0004]);
        assert_eq!(q.aliases[0].injected.inventory, vec![(0x0200_0005, 2)]);
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
            sub(
                b"ALCA",
                &[11i16.to_le_bytes(), 0x8000u16.to_le_bytes()].concat(),
            ),
            sub(b"ALCL", &22i32.to_le_bytes()),
        ];
        let q = parse_qust(0x2, &subs, &None);
        assert_eq!(
            q.aliases[0].fill_type,
            Some(AliasFillType::CreatedObject {
                base: 0x000B_0002,
                target_alias: 11,
                create_mode: 0x8000,
                level: 22,
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
            sub(b"KNAM", &0x000E_0006u32.to_le_bytes()),
        ];
        let q = parse_qust(0x6, &subs, &None);
        assert_eq!(
            q.aliases[0].fill_type,
            Some(AliasFillType::LocationAliasReference {
                alias_id: -1,
                keyword: Some(0x000E_0006),
                ref_type: Some(0x000E_0005),
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
            sub(
                b"FNAM",
                &(ALIAS_FLAG_IN_LOADED_AREA | ALIAS_FLAG_CLOSEST).to_le_bytes(),
            ),
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
        assert_eq!(
            alias.fill_type,
            Some(AliasFillType::ForcedReference(0x0001_0000))
        );
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
            sub(
                b"FNAM",
                &(ALIAS_FLAG_ESSENTIAL | ALIAS_FLAG_OPTIONAL).to_le_bytes(),
            ),
            sub(b"ALED", &[]),
        ];
        let q = parse_qust(0x2C258, &subs, &None);
        assert_eq!(q.aliases.len(), 2);

        let nurelion = &q.aliases[0];
        assert_eq!(nurelion.name, "Nurelion");
        assert_eq!(
            nurelion.fill_type,
            Some(AliasFillType::ForcedReference(0x0001_B115))
        );
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
        assert_eq!(
            alias.fill_type,
            Some(AliasFillType::ForcedReference(0x0001_0000))
        );
        assert_eq!(alias.force_into_alias, Some(2));
    }

    #[test]
    fn parse_qust_alias_injected_data() {
        let subs = vec![
            sub(b"ALST", &10i32.to_le_bytes()),
            sub(b"ALFR", &0x0002_0000u32.to_le_bytes()),
            sub(b"ALDN", &0x0000_AAAAu32.to_le_bytes()),
            sub(b"VTCK", &0x0000_BBBBu32.to_le_bytes()),
            sub(b"ALFV", &0x0000_BBBCu32.to_le_bytes()),
            sub(b"ALDI", &0x0000_BBBDu32.to_le_bytes()),
            sub(b"SPOR", &0x0000_CCC9u32.to_le_bytes()),
            sub(b"OCOR", &0x0000_CCCAu32.to_le_bytes()),
            sub(b"GWOR", &0x0000_CCCBu32.to_le_bytes()),
            sub(b"ECOR", &0x0000_CCCCu32.to_le_bytes()),
            sub(
                b"ALLA",
                &[0x0000_DDDDu32.to_le_bytes(), 12i32.to_le_bytes()].concat(),
            ),
            sub(b"ALFC", &0x0000_1111u32.to_le_bytes()),
            sub(b"ALFC", &0x0000_2222u32.to_le_bytes()),
            sub(b"ALPC", &0x0000_3333u32.to_le_bytes()),
            sub(b"ALSP", &0x0000_4444u32.to_le_bytes()),
            sub(
                b"KWDA",
                &[0x0000_5555u32.to_le_bytes(), 0x0000_6666u32.to_le_bytes()].concat(),
            ),
            sub(
                b"CNTO",
                &[0x0000_7777u32.to_le_bytes(), 3u32.to_le_bytes()].concat(),
            ),
        ];
        let q = parse_qust(0x9, &subs, &None);
        let injected = &q.aliases[0].injected;
        assert_eq!(injected.display_name, Some(0x0000_AAAA));
        assert_eq!(injected.voice_type, Some(0x0000_BBBB));
        assert_eq!(injected.forced_voice, Some(0x0000_BBBC));
        assert_eq!(injected.death_item, Some(0x0000_BBBD));
        assert_eq!(injected.spectator_override, Some(0x0000_CCC9));
        assert_eq!(injected.observe_dead_body_override, Some(0x0000_CCCA));
        assert_eq!(injected.guard_warn_override, Some(0x0000_CCCB));
        assert_eq!(injected.combat_override, Some(0x0000_CCCC));
        assert_eq!(
            injected.linked_aliases,
            vec![AliasLinkedAlias {
                keyword: Some(0x0000_DDDD),
                alias_id: 12,
            }]
        );
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
        let subs = vec![
            sub(b"ALST", &0i32.to_le_bytes()),
            sub(b"ALCA", &5i32.to_le_bytes()),
        ];
        let q = parse_qust(0xB, &subs, &None);
        assert_eq!(q.aliases[0].fill_type, None);
    }

    #[test]
    fn parse_qust_decodes_near_and_collection_alias_metadata() {
        let subs = vec![
            sub(b"ALST", &1i32.to_le_bytes()),
            sub(b"ALNA", &7i32.to_le_bytes()),
            sub(b"ALNT", &2u32.to_le_bytes()),
            sub(b"ALCC", &8i32.to_le_bytes()),
            sub(b"ALED", &[]),
            sub(b"ALCS", &2i32.to_le_bytes()),
            sub(b"ALMI", &[4]),
            sub(b"NNAM", b"Collection quest description\0"),
        ];
        let q = parse_qust(0xC, &subs, &None);
        assert_eq!(q.aliases.len(), 2);
        assert_eq!(
            q.aliases[0].fill_type,
            Some(AliasFillType::NearAlias {
                alias_id: 7,
                relation: 2,
            })
        );
        assert_eq!(q.aliases[0].closest_to_alias, Some(8));
        assert!(q.aliases[1].is_collection);
        assert_eq!(q.aliases[1].max_initial_fill_count, Some(4));
        assert_eq!(q.description, "Collection quest description");
    }

    #[test]
    fn parse_qust_decodes_general_metadata_and_condition_sections() {
        let subs = vec![
            sub(
                b"DNAM",
                &[
                    QUEST_FLAG_START_GAME_ENABLED as u8,
                    0,
                    50,
                    3,
                    0,
                    0,
                    32,
                    65,
                    6,
                    0,
                    0,
                    0,
                ],
            ),
            sub(b"ENAM", b"Kill"),
            sub(b"LNAM", &0x0000_1000u32.to_le_bytes()),
            sub(b"XNAM", &0x0000_2000u32.to_le_bytes()),
            sub(b"QTGL", &0x0000_3000u32.to_le_bytes()),
            sub(b"FLTR", b"Main Quest\0"),
            sub(b"CTDA", &minimal_ctda(10)),
            sub(b"NEXT", &[]),
            sub(b"CTDA", &minimal_ctda(11)),
            sub(b"ANAM", &9u32.to_le_bytes()),
            sub(b"NNAM", b"Quest description\0"),
            sub(b"GNAM", &0x0000_4000u32.to_le_bytes()),
            sub(b"SNAM", b"Interface/Quest.swf\0"),
            sub(
                b"QSTA",
                &[0x0000_5000u32.to_le_bytes(), 1u32.to_le_bytes()].concat(),
            ),
            sub(b"CTDA", &minimal_ctda(12)),
        ];
        let q = parse_qust(0xD, &subs, &None);
        assert_eq!(q.event, Some(*b"Kill"));
        assert_eq!(q.general_version, 3);
        assert_eq!(f32::from_le_bytes(q.general_aux), 10.0);
        assert_eq!(q.quest_type, 6);
        assert_eq!(q.location, Some(0x0000_1000));
        assert_eq!(q.completion_xp, Some(0x0000_2000));
        assert_eq!(q.text_display_globals, [0x0000_3000]);
        assert_eq!(q.object_window_filter, "Main Quest");
        assert_eq!(q.dialogue_conditions[0].function_index, 10);
        assert_eq!(q.secondary_conditions[0].function_index, 11);
        assert_eq!(q.next_alias_id, Some(9));
        assert_eq!(q.description, "Quest description");
        assert_eq!(q.quest_group, Some(0x0000_4000));
        assert_eq!(q.swf_file, "Interface/Quest.swf");
        assert_eq!(
            q.targets[0].target,
            QuestObjectiveTargetKind::Reference(0x0000_5000)
        );
        assert_eq!(q.targets[0].conditions[0].function_index, 12);
    }

    /// The catalog, paired with an independently-written value pin.
    ///
    /// #2983 — the pins are transcribed from the declarations at the top of
    /// this file, NOT derived from an external spec: no wire-level test
    /// anywhere decodes a real `FNAM` payload and asserts a named alias
    /// flag, so these 25 values have no outside authority behind them. What
    /// the pin buys is drift detection — editing a declaration without
    /// editing this table fails — not confirmation that the catalog matches
    /// the on-disk format. Treat "what is the authority for these values?"
    /// as a separate, still-open question.
    const ALL_FLAGS: &[(&str, u32, u32)] = &[
        ("RESERVES", ALIAS_FLAG_RESERVES, 0x0000_0001),
        ("OPTIONAL", ALIAS_FLAG_OPTIONAL, 0x0000_0002),
        ("QUEST_OBJECT", ALIAS_FLAG_QUEST_OBJECT, 0x0000_0004),
        ("ALLOW_REUSE", ALIAS_FLAG_ALLOW_REUSE, 0x0000_0008),
        ("ALLOW_DEAD", ALIAS_FLAG_ALLOW_DEAD, 0x0000_0010),
        ("IN_LOADED_AREA", ALIAS_FLAG_IN_LOADED_AREA, 0x0000_0020),
        ("ESSENTIAL", ALIAS_FLAG_ESSENTIAL, 0x0000_0040),
        ("ALLOW_DISABLED", ALIAS_FLAG_ALLOW_DISABLED, 0x0000_0080),
        ("STORES_TEXT", ALIAS_FLAG_STORES_TEXT, 0x0000_0100),
        ("ALLOW_RESERVED", ALIAS_FLAG_ALLOW_RESERVED, 0x0000_0200),
        ("PROTECTED", ALIAS_FLAG_PROTECTED, 0x0000_0400),
        ("FORCED_BY_ALIASES", ALIAS_FLAG_FORCED_BY_ALIASES, 0x0000_0800),
        ("ALLOW_DESTROYED", ALIAS_FLAG_ALLOW_DESTROYED, 0x0000_1000),
        ("CLOSEST", ALIAS_FLAG_CLOSEST, 0x0000_2000),
        ("USES_STORED_TEXT", ALIAS_FLAG_USES_STORED_TEXT, 0x0000_4000),
        (
            "INITIALLY_DISABLED",
            ALIAS_FLAG_INITIALLY_DISABLED,
            0x0000_8000,
        ),
        ("ALLOW_CLEARED", ALIAS_FLAG_ALLOW_CLEARED, 0x0001_0000),
        (
            "CLEAR_NAMES_WHEN_REMOVED",
            ALIAS_FLAG_CLEAR_NAMES_WHEN_REMOVED,
            0x0002_0000,
        ),
        ("ACTORS_ONLY", ALIAS_FLAG_ACTORS_ONLY, 0x0004_0000),
        ("CREATE_TEMPORARY", ALIAS_FLAG_CREATE_TEMPORARY, 0x0008_0000),
        ("EXTERNAL_LINKED", ALIAS_FLAG_EXTERNAL_LINKED, 0x0010_0000),
        ("NO_PICKPOCKET", ALIAS_FLAG_NO_PICKPOCKET, 0x0020_0000),
        (
            "APPLY_TO_NON_ALIASED_REFS",
            ALIAS_FLAG_APPLY_TO_NON_ALIASED_REFS,
            0x0040_0000,
        ),
        ("COMPANION", ALIAS_FLAG_COMPANION, 0x0080_0000),
        (
            "OPTIONAL_ALL_SCENES",
            ALIAS_FLAG_OPTIONAL_ALL_SCENES,
            0x0100_0000,
        ),
    ];

    /// #2983 — this test used to assert `(a | b | c).has(a)`, which is true
    /// for every non-zero `a` by construction: an identity over the OR-fold
    /// that produced the mask. It read as a value guard and was a presence
    /// guard, so the defect its own comment named — "a copy-paste bit-value
    /// typo … each must be its own distinct, correctly-shifted bit" — was
    /// exactly what it could not catch. A wrong-but-distinct bit
    /// (`0x0000_2000` typo'd to `0x0002_0000`) passed, and so did a
    /// multi-bit value.
    ///
    /// Five of these constants drive live alias-fill policy in
    /// `crates/scripting/src/scene/quest_alias.rs`, so a wrong bit silently
    /// changes which references fill a quest alias.
    #[test]
    fn alias_flags_has_recognizes_every_named_bit() {
        for &(name, flag, pinned) in ALL_FLAGS {
            assert_eq!(
                flag, pinned,
                "ALIAS_FLAG_{name} = {flag:#010x} but is pinned at {pinned:#010x} \
                 — the declaration moved without updating the pin"
            );
            // "Correctly-shifted" means a single bit. A multi-bit value
            // would make `has()` match on any of its bits.
            assert_eq!(
                flag.count_ones(),
                1,
                "ALIAS_FLAG_{name} = {flag:#010x} is not a single bit"
            );
            assert!(
                AliasFlags(flag).has(flag),
                "ALIAS_FLAG_{name} does not match itself"
            );
        }

        // Every constant is a distinct bit — no two collapse onto one value.
        let mut sorted: Vec<u32> = ALL_FLAGS.iter().map(|&(_, f, _)| f).collect();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            ALL_FLAGS.len(),
            "duplicate flag value in the catalog"
        );

        // The catalog occupies bits 0..=24 with no holes. Pinned from the
        // declarations as they stand, not from an external spec — its job is
        // to fail on a wrong-but-distinct bit, which the per-flag pin above
        // would also catch but which this states as one contiguous shape.
        let combined = AliasFlags(ALL_FLAGS.iter().fold(0u32, |acc, &(_, f, _)| acc | f));
        assert_eq!(
            combined.0, 0x01FF_FFFF,
            "the 25 alias flags no longer occupy exactly bits 0..=24"
        );
        // A bit outside the catalog is correctly absent.
        assert!(!combined.has(0x8000_0000));
    }

    /// #2983 — `ALL_FLAGS` is hand-maintained, so on its own it cannot notice
    /// a 26th constant being added: the roster would simply never exercise
    /// it, while the `#[allow(dead_code)]` block comment above the
    /// declarations kept claiming "every constant is exercised by an
    /// `AliasFlags::has` assertion in the test module below".
    ///
    /// Counts `pub const ALIAS_FLAG_` declarations in this file's own source
    /// text rather than re-listing them, so the roster cannot silently drift
    /// behind the catalog. Same shape as
    /// `dbg_bits_catalog_covers_every_dbg_constant` in
    /// `crates/renderer/src/shader_constants.rs`, which solved this exact
    /// problem for the `DBG_*` bits.
    #[test]
    fn alias_flag_roster_covers_every_declared_constant() {
        let declared = include_str!("quest.rs")
            .lines()
            .filter(|l| l.trim_start().starts_with("pub const ALIAS_FLAG_"))
            .filter(|l| l.contains(": u32 ="))
            .count();
        assert_eq!(
            ALL_FLAGS.len(),
            declared,
            "ALL_FLAGS lists {} flags but quest.rs declares {} `pub const \
             ALIAS_FLAG_*` constants — a new constant was added without a \
             matching roster entry and value pin",
            ALL_FLAGS.len(),
            declared,
        );
    }
}
