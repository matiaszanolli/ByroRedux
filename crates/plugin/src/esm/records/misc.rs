//! Stub parsers for ~40 record types that were previously falling
//! through the `parse_esm` catch-all and getting skipped wholesale
//! (#458 / audit FO3-3-07). Each parser extracts enough data for
//! *references* into the record to resolve — typically EDID + a
//! handful of form refs + a couple of scalar fields — without doing
//! deep sub-record decoding. Full parsing of each can be tightened
//! up per-type when the consuming system lands.
//!
//! Split across themed submodules (each owns a handful of related
//! records + their regression tests):
//!
//! - [`water`] — `WATR`
//! - [`character`] — `HDPT` / `EYES` / `HAIR` / `CSTY` / `IDLE`
//! - [`world`] — `NAVI` / `NAVM` / `REGN` / `ECZN` / `LGTM` / `IMGS`
//!   / `ACTI` / `TERM`
//! - [`pack`] — `PACK`
//! - [`quest`] — `QUST`
//! - [`scene`] — Skyrim+ `SCEN` phase/action orchestration
//! - [`dialogue`] — `DIAL` / `INFO` / `MESG`
//! - [`magic`] — `PERK` / `SPEL` / `MGEF` / `ENCH`
//! - [`effects`] — `AVIF` / `PROJ` / `EFSH` / `IMOD` / `EXPL` / `IPCT`
//!   / `IPDS` / `REPU`
//! - [`imagespace`] — animated `IMAD` lens and color-grade curves
//! - [`equipment`] — `ARMA` / `BPTD` / `COBJ` / `SLGM` /
//!   `MinimalEsmRecord`
//!
//! Per-game bit layouts vary on the LGTM + DATA / HDPT / EYES / HAIR
//! records past Skyrim; the stubs parse the FO3/FNV byte layout and
//! gracefully return defaults on short buffers — Skyrim+ re-parsing
//! lands alongside the consuming system.

mod character;
pub mod dialogue;
mod effects;
mod equipment;
mod imagespace;
mod magic;
pub mod pack;
mod quest;
mod scene;
mod water;
mod world;

pub use character::{
    parse_csty, parse_eyes, parse_hair, parse_hdpt, parse_idle, CstyRecord, EyesRecord, HairRecord,
    HdptRecord, IdleRecord,
};
pub use dialogue::{
    build_conversation_tree, parse_dial, parse_info, parse_mesg, ConversationTree,
    ConversationTreeError, DialRecord, InfoRecord, MesgRecord,
};
pub use effects::{
    parse_avif, parse_efsh, parse_expl, parse_imod, parse_ipct, parse_ipds, parse_proj, parse_repu,
    AvifRecord, EfshRecord, ExplRecord, ImodRecord, IpctRecord, IpdsRecord, ProjRecord, RepuRecord,
};
pub use equipment::{
    parse_arma, parse_bptd, parse_cobj, parse_minimal_esm_record, parse_slgm, ArmaRecord,
    BptdRecord, CobjRecord, MinimalEsmRecord, SlgmRecord,
};
pub use imagespace::{parse_imad, ImadColorKey, ImadRecord, ImadScalarKey};
pub use magic::{
    parse_ench, parse_mgef, parse_perk, parse_spel, EnchRecord, MgefRecord, PerkRecord, SpelRecord,
};
pub use pack::{
    active_escort_location, active_escort_target, active_follow_target, active_guard_location,
    active_package, active_package_is_escort, active_package_is_follow, active_package_is_guard,
    active_package_is_patrol, active_package_is_sandbox, active_package_is_travel,
    active_package_is_wander, active_patrol_location, active_sandbox_location,
    active_travel_location, active_wander_location, parse_pack, PackDataInput, PackDataTarget,
    PackDataTargetKind, PackDataValue, PackLocation, PackLocationTarget, PackProcedure, PackRecord,
    PackSchedule, PackTarget, PackTargetKind, PackTopicData,
};
pub use quest::{
    parse_qust, AliasFillType, AliasFlags, AliasInjectedData, AliasLinkedAlias, QuestAlias,
    QuestObjective, QuestObjectiveTarget, QuestObjectiveTargetKind, QuestStage, QuestStageLogEntry,
    QustRecord, ALIAS_FLAG_ALLOW_DEAD, ALIAS_FLAG_ALLOW_RESERVED, ALIAS_FLAG_ALLOW_REUSE,
    ALIAS_FLAG_CLOSEST, ALIAS_FLAG_RESERVES, QUEST_FLAG_ACTIVE, QUEST_FLAG_ALLOW_REPEATED_STAGES,
    QUEST_FLAG_COMPLETED, QUEST_FLAG_FAILED, QUEST_FLAG_START_GAME_ENABLED,
    QUEST_LOG_FLAG_COMPLETE_QUEST, QUEST_LOG_FLAG_FAIL_QUEST, QUEST_STAGE_FLAG_SHUT_DOWN,
    QUEST_STAGE_FLAG_START_UP,
};
pub use scene::{
    parse_scen, ScenRecord, SceneAction, SceneActionType, SceneActor, ScenePhase,
    SCENE_BEGIN_ON_QUEST_START, SCENE_INTERRUPTIBLE, SCENE_REPEAT_CONDITIONS, SCENE_SHOW_ALL_TEXT,
    SCENE_STOP_QUEST_ON_END,
};
pub use water::{parse_watr, watr_to_params, WaterParams, WatrRecord};
pub use world::{
    parse_acti, parse_eczn, parse_imgs, parse_lgtm, parse_navi, parse_navm, parse_regn, parse_term,
    ActiRecord, EcznRecord, ImgsRecord, LgtmRecord, NaviRecord, NavmRecord, RegionArea,
    RegionDataEntry, RegionDataKind, RegionDataPayload, RegionSound, RegionWeather, RegnRecord,
    TermRecord,
};
