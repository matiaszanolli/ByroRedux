//! Structured record extraction beyond cells and statics.
//!
//! `cell.rs` (in the parent module) walks an ESM and pulls out interior /
//! exterior cells, placed references, and base records that have a MODL
//! sub-record. That covers everything the renderer needs to draw a cell.
//! This module adds extraction for record types game systems need beyond
//! rendering: items, containers, leveled lists, NPCs, races, classes,
//! factions, globals, and game settings.
//!
//! The high-level entry point is `parse_esm`, which walks the GRUP tree
//! once and fills an `EsmIndex` aggregating cells + the new record maps.
//! Existing callers that only want cell data continue to use
//! `parse_esm_cells` (now a thin wrapper over `parse_esm`).

pub mod actor;
pub mod actor_value_derive;
pub mod climate;
pub mod common;
pub mod condition;
pub mod container;
pub mod global;
pub mod gras;
pub mod items;
pub mod list_record;
pub mod load_screen;
pub mod misc;
pub mod movs;
pub mod mswp;
pub mod outfit;
pub mod pathgrid;
pub mod pkin;
pub mod scol;
pub mod script;
pub mod script_instance;
pub mod soun;
/// Shared `SubRecord` fixture builders (#3865). `#[cfg(test)]` so it costs
/// nothing in a release build; `pub(crate)` so every record module's test
/// submodule can reach it.
#[cfg(test)]
pub(crate) mod test_support;
pub mod tree;
pub mod weather;

pub use list_record::{parse_flst, FlstRecord};
pub use load_screen::{parse_lscr, LoadScreenLocation, LoadScreenRecord};
pub use movs::{parse_movs, MovableStaticRecord};
pub use mswp::{parse_mswp, MaterialSwapEntry, MaterialSwapRecord};
pub use outfit::{parse_otft, OtftRecord};
pub use pathgrid::{parse_pgrd, InterCellConnection, PathGridPoint, PathGridRecord};
pub use pkin::{parse_pkin, PkinRecord};
pub use scol::{parse_scol, ScolPart, ScolPlacement, ScolRecord};
pub use soun::{parse_soun, SounRecord};

pub use actor::{
    effective_actor_level, parse_clas, parse_fact, parse_npc, parse_race, ClassRecord,
    CreatureStats, FactionMembership, FactionRecord, FactionRelation, NpcInventoryEntry, NpcRecord,
    RaceRecord, ACBS_PC_LEVEL_MULT, CREATURE_DATA_LEN,
};
pub use actor_value_derive::{derive_npc_actor_values, derive_resolved_actor_values};
pub use climate::{parse_clmt, ClimateRecord, ClimateWeather};
pub use common::StringsTableGuard;
pub use container::{
    parse_cont, parse_leveled_list, ContainerRecord, InventoryEntry, LeveledEntry, LeveledList,
};
pub use global::{parse_glob, parse_gmst, GameSetting, GlobalRecord, SettingValue};
pub use gras::{
    parse_gras, GrasRecord, GRAS_DATA_LEN, GRAS_FLAG_FIT_TO_SLOPE, GRAS_FLAG_UNIFORM_SCALING,
    GRAS_FLAG_VERTEX_LIGHTING,
};
pub use items::{
    parse_alch, parse_alch_for_game, parse_ammo, parse_appa, parse_armo, parse_book,
    parse_carryable_light, parse_ingr, parse_keym, parse_misc, parse_note, parse_omod_loose_item,
    parse_scrl, parse_weap, ItemKind, ItemRecord,
};
pub use misc::{
    active_package, parse_acti, parse_arma, parse_avif, parse_bptd, parse_cobj, parse_csty,
    parse_dial, parse_eczn, parse_efsh, parse_ench, parse_expl, parse_eyes, parse_hair, parse_hdpt,
    parse_idle, parse_imad, parse_imgs, parse_imod, parse_info, parse_ipct, parse_ipds, parse_lgtm,
    parse_mesg, parse_mgef, parse_mgef_for_game, parse_minimal_esm_record, parse_navi, parse_navm,
    parse_pack, parse_perk, parse_proj, parse_qust, parse_regn, parse_repu, parse_scen, parse_slgm,
    parse_spel, parse_term, parse_watr, select_active_region_sound, ActiRecord, AliasFillType,
    AliasFlags, AliasInjectedData, AliasLinkedAlias, ArmaRecord, AvifRecord, BptdRecord,
    CobjRecord, CstyRecord, DialRecord, EcznRecord, EfshRecord, EnchRecord, ExplRecord, EyesRecord,
    HairRecord, HdptRecord, IdleRecord, ImadColorKey, ImadRecord, ImadScalarKey, ImgsRecord,
    ImodRecord, InfoRecord, IpctRecord, IpdsRecord, LgtmRecord, MagicEffectItem, MesgRecord,
    MgefRecord, MinimalEsmRecord, NaviRecord, NavmExternalConnection, NavmRecord, NavmTriangle,
    PackDataInput, PackDataTarget, PackDataTargetKind, PackDataValue, PackLocation,
    PackLocationTarget, PackProcedure, PackRecord, PackSchedule, PackTarget, PackTargetKind,
    PackTopicData, PerkRecord, ProjRecord, QuestAlias, QuestObjective, QuestObjectiveTarget,
    QuestObjectiveTargetKind, QuestStage, QuestStageLogEntry, QustRecord, RegionArea,
    RegionDataEntry, RegionDataKind, RegionDataPayload, RegionSound, RegionWeather, RegnRecord,
    RepuRecord, ScenRecord, SceneAction, SceneActionType, SceneActor, ScenePhase, SlgmRecord,
    SpelRecord, TermRecord, WatrRecord, ALIAS_FLAG_ACTORS_ONLY, ALIAS_FLAG_ALLOW_CLEARED,
    ALIAS_FLAG_ALLOW_DEAD, ALIAS_FLAG_ALLOW_DESTROYED, ALIAS_FLAG_ALLOW_DISABLED,
    ALIAS_FLAG_ALLOW_RESERVED, ALIAS_FLAG_ALLOW_REUSE, ALIAS_FLAG_APPLY_TO_NON_ALIASED_REFS,
    ALIAS_FLAG_CLEAR_NAMES_WHEN_REMOVED, ALIAS_FLAG_CLOSEST, ALIAS_FLAG_COMPANION,
    ALIAS_FLAG_CREATE_TEMPORARY, ALIAS_FLAG_ESSENTIAL, ALIAS_FLAG_EXTERNAL_LINKED,
    ALIAS_FLAG_FORCED_BY_ALIASES, ALIAS_FLAG_INITIALLY_DISABLED, ALIAS_FLAG_IN_LOADED_AREA,
    ALIAS_FLAG_NO_PICKPOCKET, ALIAS_FLAG_OPTIONAL, ALIAS_FLAG_OPTIONAL_ALL_SCENES,
    ALIAS_FLAG_PROTECTED, ALIAS_FLAG_QUEST_OBJECT, ALIAS_FLAG_RESERVES, ALIAS_FLAG_STORES_TEXT,
    ALIAS_FLAG_USES_STORED_TEXT, QUEST_FLAG_ACTIVE, QUEST_FLAG_ALLOW_REPEATED_STAGES,
    QUEST_FLAG_COMPLETED, QUEST_FLAG_FAILED, QUEST_FLAG_START_GAME_ENABLED,
    QUEST_LOG_FLAG_COMPLETE_QUEST, QUEST_LOG_FLAG_FAIL_QUEST, QUEST_STAGE_FLAG_SHUT_DOWN,
    QUEST_STAGE_FLAG_START_UP, SCENE_BEGIN_ON_QUEST_START, SCENE_INTERRUPTIBLE,
    SCENE_REPEAT_CONDITIONS, SCENE_SHOW_ALL_TEXT, SCENE_STOP_QUEST_ON_END,
};
pub use script::{parse_scpt, ScriptLocalVar, ScriptRecord, ScriptType};
pub use tree::{parse_tree, TreeRecord};
pub use weather::{parse_wthr, OblivionHdrLighting, SkyColor, WeatherRecord};

use super::cell::StaticObject;
use super::reader::{EsmReader, GameKind};
use anyhow::Result;
use std::collections::HashMap;

// ── #1118 / TD9-003 split — see `index.rs` and `grup_walker.rs` ─────
mod grup_walker;
mod index;

pub use index::EsmIndex;

use grup_walker::{
    extract_dial_with_info, extract_quest_dialogue_scene_tree, extract_records,
    extract_records_with_modl,
};

// ── #2060 split — `parse_esm_with_load_order`'s per-domain dispatch
// tables. The cell-only group (CELL/WRLD/LTEX/TXST/SCOL/PKIN/MOVS/MSWP/
// PDCL) stays inline: its dozen-odd interdependent locals (cells,
// exterior_cells, per-game warn flags, …) don't decompose into a small
// parameter list the way the typed-record domains below do.
mod dispatch_actor;
mod dispatch_container;
mod dispatch_global;
mod dispatch_items;
mod dispatch_misc_gameplay_a;
mod dispatch_misc_gameplay_b;
mod dispatch_misc_stub;
mod dispatch_world_placement;

// ── #4219 split — the parsing entry point lives in `parse.rs` so this
// file can be the pure re-export barrel it mostly already was. Both
// public entry points are re-exported here, so no caller's path moved.
mod parse;
mod spatial_units;

pub use parse::{parse_esm, parse_esm_with_load_order, DISPATCH_HANDLED_FOURCCS};

#[cfg(test)]
mod tests;
