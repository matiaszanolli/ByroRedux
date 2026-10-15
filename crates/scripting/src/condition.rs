//! M47.1 Phases 2 + 3 — `ConditionFunction` enum + OR-precedence
//! evaluator.
//!
//! Plugin-side ([`byroredux_plugin::esm::records::condition`]) parses
//! CTDA sub-records into [`Condition`] values; scripting-side (here)
//! interprets the function indices against ECS state and combines
//! the per-condition booleans with the OR-precedence rule.
//!
//! ## What "OR-precedence" means
//!
//! Default operator between conditions is AND. Setting `or_next` on
//! a condition combines it with the next via OR. **Consecutive ORs
//! form a block that binds tighter than the surrounding AND chain.**
//!
//! `A AND B OR C AND D` evaluates as `A AND (B OR C) AND D`, NOT
//! `(A AND B) OR (C AND D)` (the standard boolean reading).
//!
//! This is the opposite of standard boolean precedence. Bethesda
//! designers compose complex expressions by exploiting the distributive
//! law (`(A AND B) OR (C AND D) ⇔ (A OR C) AND (A OR D) AND (B OR C) AND (B OR D)`).
//!
//! ## Function catalog status
//!
//! Bethesda ships ~300 condition functions across the four-game
//! lineage. This catalog ships **48 functions** at their canonical CTDA
//! function indices, verified against TES5Edit `wbDefinitions*.pas`
//! (the same value `parse_ctda` reads from CTDA bytes 8–11; FO3 == FNV
//! for every shared function):
//!
//! | Index | Function               | Reads                       |
//! |-------|------------------------|-----------------------------|
//! | 1     | GetDistance            | `GlobalTransform`           |
//! | 14    | GetActorValue          | `ActorValues[param_1]`      |
//! | 46    | GetDead                | `Dead` marker               |
//! | 56    | GetQuestRunning        | `QuestStageState[param_1]`  |
//! | 58    | GetStage               | `QuestStageState[param_1]`  |
//! | 79    | GetQuestVariable       | `ObScriptQuestLocals[param_1][param_2]` (FO3 / FNV / Oblivion) |
//! | 59    | GetStageDone           | `QuestStageState[param_1]`  |
//! | 67    | GetInCell              | `CellRoot` + `CellFormId`   |
//! | 68    | GetIsClass             | `Background.class_form_id`  |
//! | 69    | GetIsRace              | `Background.race_form_id`   |
//! | 71    | GetInFaction           | `FactionRanks` (Skyrim)     |
//! | 72    | GetIsID                | `SceneAliasCandidate` base  |
//! | 73    | GetFactionRank         | `FactionRanks`              |
//! | 74    | GetGlobalValue         | `Globals[param_1]`          |
//! | 77    | GetRandomPercent       | `StoryManagerRng`           |
//! | 80    | GetLevel               | `CharacterLevel`            |
//! | 182   | GetEquipped            | `Inventory` + `EquipmentSlots` |
//! | 449   | HasPerk (448 on Skyrim)| `Perks`                      |
//! | 533   | GetXPForNextLevel      | `CharacterRuleset` leveling |
//! | 550   | IsSceneActionComplete  | `ScenePlayer` history       |
//! | 555   | HasLoaded3D             | `GlobalTransform`           |
//! | 573   | GetReputation          | `FactionReputation` (FNV)   |
//! | 575   | GetReputationThreshold | `FactionReputation` + bands |
//! | 586   | IsHardcore             | `HardcoreMode` (FNV)        |
//! | 629   | GetVMQuestVariable     | `QuestScriptVariables`      |
//! | 630   | GetVMScriptVariable    | `ScriptVariables`           |
//! | 359   | GetInCurrentLoc        | `LocationTable` + cell      |
//! | 360   | GetInCurrentLocAlias   | `LocationAliasBindings`     |
//! | 562   | LocationHasKeyword     | `LocationTable` keywords    |
//! | 565   | GetIsEditorLocation    | `LocationTable` editor refs |
//! | 566   | GetIsAliasRef          | `SceneActorBindings`        |
//! | 567   | GetIsEditorLocAlias    | `LocationAliasBindings`     |
//! | 605   | LocAliasIsLocation     | `LocationAliasBindings`     |
//! | 563   | LocationHasRefType     | `LocationTable` ref types   |
//! | 181   | HasSameEditorLocAsRefAlias | `LocationTable` editor refs |
//! | 576   | GetEventData           | `EventDataSlots`            |
//! | 250   | GetLocationCleared     | `LocationClearState`        |
//! | 598   | GetLocationAliasCleared | `LocationClearState`       |
//! | 606   | GetKeywordDataForLocation | `LocationKeywordData`    |
//! | 608   | GetKeywordDataForAlias | `LocationKeywordData`       |
//! | 651   | GetKeywordDataForCurrentLocation | `LocationKeywordData` |
//! | 354   | IsEssential            | `ActorProtection` + aliases |
//! | 476   | IsProtected            | `ActorProtection` + aliases |
//! | 580   | IsBleedingOut          | `Bleedout`                  |
//! | 453   | GetPlayerTeammate      | `FollowerState`             |
//! | 403   | GetRelationshipRank    | `relationships`             |
//! | 560   | HasKeyword             | `keywords`                  |
//! | 237   | GetIsGhost             | `ActorControlState`         |
//!
//! Unknown function indices evaluate to `0.0` (the Bethesda "unknown
//! function → safe-default" contract) and are logged at debug for
//! future-catalog tracking.

use std::collections::HashMap;
use std::ops::RangeInclusive;

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::{
    event_data_slot, Condition, ConditionList, ConditionValue, EventDataSlot, RunOn,
};

use crate::quest_stages::QuestFormId;
use crate::scene::SceneActorBindings;

/// Identifier for a condition function. Wraps the raw u32 function
/// index from `Condition.function_index` with a typed constructor so
/// the dispatcher's `match` is exhaustive against the known catalog.
///
/// New functions land by adding a variant + a match arm in
/// [`evaluate_function`]. Unknown indices fall through to
/// [`Self::Unknown`] which evaluates to `0.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConditionFunction {
    /// `GetActorValue(avif_form_id) → f32`. Reads the Run-On's composed value
    /// for the `param_1` actor value from its `ActorValues` component
    /// (`ActorValue::current`), or 0.0 when the actor carries
    /// no such value (#1663). `param_1` is the global-space AVIF FormID.
    /// FO3 / FNV / Skyrim function index **14**.
    GetActorValue,
    /// `GetQuestRunning(quest_form_id) → f32`. 1.0 while the quest is
    /// running or started in `QuestStageState`. FO3/FNV/Skyrim index
    /// **56**. One of the #5380 SM-vocabulary trio.
    GetQuestRunning,
    /// `GetGlobalValue(global_form_id) → f32`. The GLOB's current
    /// value from the `Globals` resource. FO3/FNV/Skyrim index **74**.
    GetGlobalValue,
    /// `GetRandomPercent() → f32`. [0, 100] — rides the SM's seeded RNG
    /// when installed. FO3/FNV/Skyrim index **77**.
    GetRandomPercent,
    /// `GetDistance(target_form_id) → f32`. Squared-distance reduces
    /// to a single sqrt at evaluation time. FO3 / FNV / Skyrim index **1**.
    GetDistance,
    /// `GetDead → f32`. Returns 1.0 while the Run-On actor carries the
    /// sparse `Dead` lifecycle marker, otherwise 0.0. FO3 / FNV / Skyrim
    /// function index **46**.
    GetDead,
    /// `GetStage(quest_form_id) → f32`. Looks up the current stage
    /// for `param_1` quest in the `QuestStageState` resource.
    /// FO3 / FNV / Skyrim index 58.
    GetStage,
    /// `GetQuestVariable(quest, variable) → f32` (Oblivion / Fallout 3 / New Vegas
    /// index **79**; Fallout 4 and Starfield use 79 for `WouldBeStealing`, so the
    /// evaluator passes it there rather than misread it, and Skyrim's takes a
    /// string name this decoder does not carry, so it passes too). Reads the
    /// quest script's local `variable` (its 1-based `SLSD` slot, `param_2`) from
    /// [`crate::obscript_quests::ObScriptQuestLocals`]: what the quest script's
    /// own GameMode runs left, 0 until one assigns it.
    GetQuestVariable,
    /// `GetStageDone(quest_form_id, stage) → f32`. Returns 1.0 if
    /// `stage` (param_2) for `quest_form_id` (param_1) has been
    /// reached at any point, 0.0 otherwise. Bethesda's standard
    /// idempotency primitive for "this quest milestone has fired
    /// before." FO3 / FNV / Skyrim index 59.
    GetStageDone,
    /// `GetInCell(cell_form_id) → f32`. Follows the Run-On entity's
    /// `CellRoot` to the root's global-space `CellFormId` and returns 1.0 on
    /// an exact match. FO3 / FNV / Skyrim function index **67**.
    GetInCell,
    /// `GetFactionRank(faction_form_id) → f32`. Reads the Run-On actor's
    /// `FactionRanks` component: the integer rank when the actor is in
    /// the faction, else -1 (Bethesda's "not in faction" sentinel — also
    /// returned when the actor carries no `FactionRanks`).
    /// FO3 / FNV / Skyrim index **73**.
    GetFactionRank,
    /// `GetInFaction(faction_form_id) → f32`. Boolean membership probe
    /// off the same `FactionRanks` component as `GetFactionRank`: 1.0
    /// when the actor holds any rank in `param_1`'s faction, 0.0
    /// otherwise. Skyrim function index **71** (6 904 corpus uses, every
    /// `param_1` a FACT; FO3 / FNV author no CTDA at 71 across their
    /// condition-bearing record types — INFO/PACK/QUST/NPC_/PERK
    /// censused — so the shared catalog cannot mislabel them there).
    GetInFaction,
    /// `GetLevel → f32`. The Run-On actor's character level from its
    /// `CharacterLevel` component (0.0 when absent). No parameter.
    /// FO3 / FNV / Skyrim index **80**.
    GetLevel,
    /// `GetEquipped(item_form_id) → f32`. Returns 1.0 when an inventory
    /// stack matching `param_1` occupies at least one biped equipment slot on
    /// the Run-On actor. Skyrim function index **182**.
    GetEquipped,
    /// `GetXPForNextLevel → f32`. The experience required for the actor's
    /// next level, from the per-game `CharacterRuleset`'s `LevelingModel`
    /// (`xp_to_next(level)`); 0.0 without the ruleset resource. No parameter.
    /// The first runtime consumer of the leveling model. FNV index **533**.
    GetXPForNextLevel,
    /// `IsSceneActionComplete(scene_form_id, action_index) → f32`. Returns
    /// 1.0 after the named action completes during the scene's current run,
    /// including after the action has left the active set. Skyrim function
    /// index **550**.
    IsSceneActionComplete,
    /// `HasLoaded3D → f32`. ByroRedux instantiates a reference's spatial 3D
    /// state as a `GlobalTransform`; returns 1.0 while that component is live.
    /// Skyrim function index **555**.
    HasLoaded3D,
    /// `GetIsClass(class_form_id) → f32`. 1.0 when the Run-On actor's
    /// `Background.class_form_id` matches `param_1` (a `CLAS` FormID), else
    /// 0.0 (also 0.0 without `Background`). FO3 / FNV / Skyrim index **68**.
    GetIsClass,
    /// `GetIsRace(race_form_id) → f32`. 1.0 when the Run-On actor's
    /// `Background.race_form_id` matches `param_1` (a `RACE` FormID), else
    /// 0.0 (also 0.0 without `Background`). FO3 / FNV / Skyrim index **69**.
    GetIsRace,
    /// `GetIsID(base_form_id) → f32`. Returns 1.0 when the Run-On's base
    /// object (`SceneAliasCandidate::base_form_id`) matches `param_1`, 0.0
    /// otherwise — "is this actor an instance of that NPC_?", the common
    /// INFO speaker gate. Not a placed-reference test (#5041). FO3 / FNV /
    /// Skyrim index **72**.
    GetIsID,
    /// `HasPerk(perk_form_id) → f32`. Reads the Run-On actor's ranked `Perks`
    /// component: 1.0 when `param_1` has a positive rank, 0.0 otherwise
    /// (also 0.0 when the actor carries no `Perks`). Index **449** (FO3 / FNV),
    /// **448** (Skyrim) — both map here.
    HasPerk,
    /// New Vegas CTDA 586: the saved Hardcore selection.
    IsHardcore,
    /// `GetReputation(reputation_form_id, axis) → f32`. The Run-On actor's
    /// raw Fame/Infamy with `param_1` (a FNV `REPU` FormID) from its
    /// `FactionReputation` component; `param_2` selects the axis
    /// (`1` = Fame, else Infamy — the console convention), 0.0 if unknown.
    /// **FNV-only**, function index **573**.
    GetReputation,
    /// `GetReputationThreshold(reputation_form_id, axis) → f32`. The Range
    /// `0..=3` band the actor's Fame/Infamy (axis per `param_2`) falls into,
    /// per the faction's thresholds — the gameplay-load-bearing reputation
    /// output (the wiki notes vanilla scripts that wrongly read the raw value
    /// instead). **FNV-only**, function index **575**.
    GetReputationThreshold,
    /// `GetVMScriptVariable(reference_form_id, variable_name) → f32`.
    /// Resolves the explicit object-reference parameter and reads the
    /// `CIS2`-named variable from its canonical [`crate::ScriptVariables`]
    /// component. Skyrim function index **630**.
    GetVMScriptVariable,
    /// `GetVMQuestVariable(quest_form_id, variable_name) → f32` (M47.2). The
    /// quest's numeric script variable named by `CIS2` (the backing
    /// `::Name_var` spelling) from [`crate::QuestScriptVariables`]: the
    /// scripted write, else the quest `VMAD`'s authored value, else 0.0.
    /// Skyrim / FO4 function index **629** (xEdit; no FO3/FNV function uses
    /// it).
    GetVMQuestVariable,
    /// `GetInCurrentLoc(location) → f32` (M47.2). 1.0 when the Run-On
    /// reference's current location is `param_1` or has it as a parent; run
    /// on a location (a location-alias fill), when that location's *parent*
    /// is or descends from it. Skyrim / FO4 index **359**.
    GetInCurrentLoc,
    /// `GetInCurrentLocAlias(alias) → f32` (M47.2). As
    /// [`GetInCurrentLoc`](Self::GetInCurrentLoc), against the location bound
    /// to the owning quest's location alias `param_1`. Skyrim / FO4 index
    /// **360**.
    GetInCurrentLocAlias,
    /// `LocationHasKeyword(keyword) → f32` (M47.2). 1.0 when the Run-On
    /// reference's current location carries keyword `param_1` (not its
    /// parents); run on a location, that location itself. Skyrim / FO4 index
    /// **562**.
    LocationHasKeyword,
    /// `GetIsEditorLocation(location) → f32` (M47.2). 1.0 when the Run-On
    /// reference's editor location is `param_1` or has it as a parent.
    /// Skyrim / FO4 index **565**.
    GetIsEditorLocation,
    /// `GetIsAliasRef(alias) → f32` (M47.2). 1.0 when the Run-On reference
    /// fills the owning quest's alias `param_1`. Skyrim / FO4 index **566**.
    GetIsAliasRef,
    /// `GetIsEditorLocAlias(alias) → f32` (M47.2). As
    /// [`GetIsEditorLocation`](Self::GetIsEditorLocation), against the
    /// owning quest's location alias `param_1`. Skyrim / FO4 index **567**.
    GetIsEditorLocAlias,
    /// `LocAliasIsLocation(alias, location) → f32` (M47.2). 1.0 when the
    /// owning quest's location alias `param_1` holds exactly `param_2`.
    /// Reads no Run-On. Skyrim / FO4 index **605**.
    LocAliasIsLocation,
    /// `LocationHasRefType(location_ref_type) → f32` (M47.2). 1.0 when the
    /// Run-On reference's current location (run on a location, that location)
    /// tags an enabled reference with `param_1` (`LCTN` `ACSR`/`LCSR`). Skyrim
    /// / FO4 index **563**.
    LocationHasRefType,
    /// `HasSameEditorLocAsRefAlias(alias, keyword) → f32` (M47.2). 1.0 when the
    /// Run-On reference and the owning quest's reference alias `param_1` share
    /// an editor location — or, with keyword `param_2`, the same nearest
    /// keyworded parent; run on a location, when that location (carrying the
    /// keyword) is or contains the alias's editor location. Skyrim / FO4 index
    /// **181**.
    HasSameEditorLocAsRefAlias,
    /// `GetEventData(function:member, form) → f32` (M47.2) — a Story Manager
    /// event's data. `param_1` packs an event function (low 16 bits:
    /// 0 `GetIsID`, 1 `IsInList`, 2 `GetValue`, 3 `HasKeyword`, 4
    /// `GetItemValue`) and an event member (high 16 bits: `K1` keyword,
    /// `L1` / `L2` locations, `V1` / `V2` values, `F1` form, `O1` created
    /// object), per xEdit's `wbEventFunctionAndMember`; `param_2` is the form
    /// compared. Reads the [`ConditionContext::event_data`] — outside Story
    /// Manager dispatch it is 0. Skyrim / FO4 index **576**.
    GetEventData,
    /// `GetLocationCleared(location) → f32` (M47.2): the location is cleared
    /// ([`crate::LocationClearState`]). Skyrim / FO4 index **250**.
    GetLocationCleared,
    /// `GetLocationAliasCleared(alias) → f32` (M47.2): the owning quest's
    /// location alias holds a cleared location. Skyrim / FO4 index **598**.
    GetLocationAliasCleared,
    /// `GetKeywordDataForLocation(location, keyword) → f32` (M47.2): the data a
    /// script attached to the keyword on the location, "if any. It otherwise
    /// returns 0" ([`crate::LocationKeywordData`]). Skyrim / FO4 index **606**.
    GetKeywordDataForLocation,
    /// `GetKeywordDataForAlias(alias, keyword) → f32` (M47.2): as
    /// [`GetKeywordDataForLocation`](Self::GetKeywordDataForLocation), on the
    /// location the owning quest's location alias holds. Skyrim / FO4 index
    /// **608**.
    GetKeywordDataForAlias,
    /// `GetKeywordDataForCurrentLocation(keyword) → f32` (M47.2): as
    /// [`GetKeywordDataForLocation`](Self::GetKeywordDataForLocation), on the
    /// Run-On subject's current location. Skyrim / FO4 index **651**.
    GetKeywordDataForCurrentLocation,
    /// `IsEssential() → f32` (M47.2): the subject is essential, by its base
    /// record or an Essential alias ([`crate::is_essential`]). Index **354**
    /// in every catalogued game.
    IsEssential,
    /// `IsProtected() → f32` (M47.2). Skyrim / FO4 index **476**.
    IsProtected,
    /// `IsBleedingOut() → f32` (M47.2): the subject is in bleedout. Skyrim /
    /// FO4 index **580**.
    IsBleedingOut,
    /// `GetPlayerTeammate() → f32` (M47.2): 1.0 when the subject is currently a
    /// player teammate set by Papyrus `SetPlayerTeammate` (CK *GetPlayerTeammate*,
    /// marked obsolete by the developers but "does appear to work"; Skyrim / FO4
    /// index **453**) — [`crate::FollowerState`].
    GetPlayerTeammate,
    /// `GetRelationshipRank(actor) → f32` (M47.2): the relationship rank
    /// between the subject and the parameter actor, +4 Lover … −4
    /// Archnemesis ([`crate::relationships`]). Skyrim / FO4 index **403**.
    GetRelationshipRank,
    /// `HasKeyword(keyword) → f32` (M47.2): 1.0 when the Run-On reference
    /// has the keyword — its base object's (an actor's race's too), an
    /// alias it fills, or a script's `AddKeyword` ([`crate::keywords`]).
    HasKeyword,
    /// `GetIsGhost → f32` (M47.2): 1.0 when the Run-On actor is a ghost
    /// (`SetGhost`, [`crate::ActorControlState::ghost`]).
    GetIsGhost,
    /// Function index outside the M47.1 catalog. Evaluates to 0.0
    /// (the Bethesda "unknown function safe-default" — see file-
    /// header doc-comment).
    Unknown(u32),
}

impl ConditionFunction {
    /// Map a raw u32 function index to a typed variant. Indices
    /// outside the catalog fall through to [`Self::Unknown`].
    pub fn from_index(index: u32) -> Self {
        // Indices are the CTDA function index xEdit stores (and `parse_ctda`
        // reads from bytes 8–11) — verified against TES5Edit `wbDefinitions*.pas`
        // (FO3 == FNV for all shared functions). HasPerk is the one game-variant
        // (448 Skyrim, 449 FO3/FNV); both map to the same behaviour. Reputation
        // (573/575) is FNV-only.
        match index {
            // #5431 — index 0 is deliberately unmapped: xEdit has it as
            // `GetWantBlocking` on TES5/FO4 and has no index 0 on
            // TES4/FO3/FNV, so it is not a MessageBox query. It stays
            // `Unknown` (fail-open) like every other uncatalogued function.
            1 => Self::GetDistance,
            14 => Self::GetActorValue,
            // 56 = GetQuestRunning(quest). The cheap SM-vocabulary
            // trio #5380 cataloged: three of the four functions the
            // story-manager node corpus gates most on after the
            // unknown-fn decline landed (the census: fn 79 x59 on FNV
            // packages; 56/74/77 across Skyrim SM nodes).
            56 => Self::GetQuestRunning,
            // 74 = GetGlobalValue(global).
            74 => Self::GetGlobalValue,
            // 77 = GetRandomPercent() — [0, 100].
            77 => Self::GetRandomPercent,
            46 => Self::GetDead,
            58 => Self::GetStage,
            79 => Self::GetQuestVariable,
            59 => Self::GetStageDone,
            67 => Self::GetInCell,
            68 => Self::GetIsClass,
            69 => Self::GetIsRace,
            72 => Self::GetIsID,
            73 => Self::GetFactionRank,
            71 => Self::GetInFaction,
            80 => Self::GetLevel,
            182 => Self::GetEquipped,
            448 | 449 => Self::HasPerk,
            533 => Self::GetXPForNextLevel,
            550 => Self::IsSceneActionComplete,
            555 => Self::HasLoaded3D,
            573 => Self::GetReputation,
            575 => Self::GetReputationThreshold,
            586 => Self::IsHardcore,
            629 => Self::GetVMQuestVariable,
            630 => Self::GetVMScriptVariable,
            // M47.2 — the location family. FO3/FNV/Oblivion author none of
            // these indices (CTDA census of their masters); 610
            // (LocAliasHasKeyword) is left out because FNV authors 610 as a
            // different function.
            359 => Self::GetInCurrentLoc,
            360 => Self::GetInCurrentLocAlias,
            562 => Self::LocationHasKeyword,
            565 => Self::GetIsEditorLocation,
            566 => Self::GetIsAliasRef,
            567 => Self::GetIsEditorLocAlias,
            605 => Self::LocAliasIsLocation,
            563 => Self::LocationHasRefType,
            // 181, not its reference twin 180: Oblivion authors 180 as a
            // different function.
            181 => Self::HasSameEditorLocAsRefAlias,
            576 => Self::GetEventData,
            250 => Self::GetLocationCleared,
            598 => Self::GetLocationAliasCleared,
            606 => Self::GetKeywordDataForLocation,
            608 => Self::GetKeywordDataForAlias,
            651 => Self::GetKeywordDataForCurrentLocation,
            354 => Self::IsEssential,
            476 => Self::IsProtected,
            580 => Self::IsBleedingOut,
            453 => Self::GetPlayerTeammate,
            403 => Self::GetRelationshipRank,
            560 => Self::HasKeyword,
            237 => Self::GetIsGhost,
            other => Self::Unknown(other),
        }
    }

    /// Every known (non-[`Unknown`](Self::Unknown)) function — the catalog the
    /// debug console enumerates and resolves names against.
    pub const CATALOG: [ConditionFunction; 48] = [
        Self::GetDistance,
        Self::GetQuestVariable,
        Self::GetActorValue,
        Self::GetDead,
        Self::GetStage,
        Self::GetStageDone,
        Self::GetInCell,
        Self::GetIsClass,
        Self::GetIsRace,
        Self::GetIsID,
        Self::GetFactionRank,
        Self::GetInFaction,
        Self::GetLevel,
        Self::GetEquipped,
        Self::HasPerk,
        Self::IsHardcore,
        Self::GetXPForNextLevel,
        Self::IsSceneActionComplete,
        Self::HasLoaded3D,
        Self::GetReputation,
        Self::GetReputationThreshold,
        Self::GetVMScriptVariable,
        Self::GetVMQuestVariable,
        Self::GetQuestRunning,
        Self::GetGlobalValue,
        Self::GetRandomPercent,
        Self::GetInCurrentLoc,
        Self::GetInCurrentLocAlias,
        Self::LocationHasKeyword,
        Self::GetKeywordDataForCurrentLocation,
        Self::GetIsEditorLocation,
        Self::GetIsAliasRef,
        Self::GetIsEditorLocAlias,
        Self::LocAliasIsLocation,
        Self::LocationHasRefType,
        Self::HasSameEditorLocAsRefAlias,
        Self::GetEventData,
        Self::GetLocationCleared,
        Self::GetKeywordDataForLocation,
        Self::GetLocationAliasCleared,
        Self::GetKeywordDataForAlias,
        Self::IsEssential,
        Self::IsProtected,
        Self::IsBleedingOut,
        Self::GetPlayerTeammate,
        Self::GetRelationshipRank,
        Self::HasKeyword,
        Self::GetIsGhost,
    ];

    /// Whether `param_1` of this function is a FormID — the slice of catalog
    /// knowledge the plugin crate's load-order remap
    /// (`byroredux_plugin::esm::records::condition::param1_is_form_id`)
    /// mirrors by raw index. The match is exhaustive on purpose: adding a
    /// variant forces this decision, and
    /// `param1_form_id_table_matches_the_condition_catalog` then fails until
    /// the plugin table agrees (#5637).
    pub fn first_param_is_form_id(self) -> bool {
        match self {
            Self::GetActorValue
            | Self::GetQuestRunning
            | Self::GetGlobalValue
            | Self::GetDistance
            | Self::GetStage
            | Self::GetQuestVariable
            | Self::GetStageDone
            | Self::GetInCell
            | Self::GetIsClass
            | Self::GetIsRace
            | Self::GetIsID
            | Self::GetFactionRank
            | Self::GetInFaction
            | Self::GetEquipped
            | Self::HasPerk
            | Self::IsSceneActionComplete
            | Self::GetReputation
            | Self::GetReputationThreshold
            | Self::GetVMQuestVariable
            | Self::GetVMScriptVariable
            | Self::GetInCurrentLoc
            | Self::LocationHasKeyword
            | Self::GetKeywordDataForCurrentLocation
            | Self::GetIsEditorLocation
            | Self::LocationHasRefType
            | Self::GetLocationCleared
            | Self::GetKeywordDataForLocation
            | Self::GetRelationshipRank
            | Self::HasKeyword => true,
            // Alias ids are quest-local indices, not FormIDs; GetEventData's
            // first parameter is a function:member pair.
            Self::GetEventData
            | Self::GetLocationAliasCleared
            | Self::GetKeywordDataForAlias
            | Self::GetInCurrentLocAlias
            | Self::HasSameEditorLocAsRefAlias
            | Self::GetIsAliasRef
            | Self::GetIsEditorLocAlias
            | Self::LocAliasIsLocation
            | Self::GetRandomPercent
            | Self::GetDead
            | Self::GetLevel
            | Self::GetXPForNextLevel
            | Self::HasLoaded3D
            | Self::IsHardcore
            | Self::IsEssential
            | Self::IsProtected
            | Self::IsBleedingOut
            | Self::GetPlayerTeammate
            | Self::GetIsGhost
            | Self::Unknown(_) => false,
        }
    }

    /// The canonical xEdit function name (for console listing / parsing).
    pub fn name(self) -> &'static str {
        match self {
            Self::GetQuestRunning => "GetQuestRunning",
            Self::GetGlobalValue => "GetGlobalValue",
            Self::GetRandomPercent => "GetRandomPercent",
            Self::GetDistance => "GetDistance",
            Self::GetActorValue => "GetActorValue",
            Self::GetDead => "GetDead",
            Self::GetStage => "GetStage",
            Self::GetQuestVariable => "GetQuestVariable",
            Self::GetStageDone => "GetStageDone",
            Self::GetInCell => "GetInCell",
            Self::GetIsClass => "GetIsClass",
            Self::GetIsRace => "GetIsRace",
            Self::GetIsID => "GetIsID",
            Self::GetFactionRank => "GetFactionRank",
            Self::GetInFaction => "GetInFaction",
            Self::GetLevel => "GetLevel",
            Self::GetEquipped => "GetEquipped",
            Self::HasPerk => "HasPerk",
            Self::IsHardcore => "IsHardcore",
            Self::GetXPForNextLevel => "GetXPForNextLevel",
            Self::IsSceneActionComplete => "IsSceneActionComplete",
            Self::HasLoaded3D => "HasLoaded3D",
            Self::GetReputation => "GetReputation",
            Self::GetReputationThreshold => "GetReputationThreshold",
            Self::GetVMScriptVariable => "GetVMScriptVariable",
            Self::GetVMQuestVariable => "GetVMQuestVariable",
            Self::GetInCurrentLoc => "GetInCurrentLoc",
            Self::GetInCurrentLocAlias => "GetInCurrentLocAlias",
            Self::LocationHasKeyword => "LocationHasKeyword",
            Self::GetKeywordDataForCurrentLocation => "GetKeywordDataForCurrentLocation",
            Self::GetIsEditorLocation => "GetIsEditorLocation",
            Self::GetIsAliasRef => "GetIsAliasRef",
            Self::GetIsEditorLocAlias => "GetIsEditorLocAlias",
            Self::LocAliasIsLocation => "LocAliasIsLocation",
            Self::LocationHasRefType => "LocationHasRefType",
            Self::HasSameEditorLocAsRefAlias => "HasSameEditorLocAsRefAlias",
            Self::GetEventData => "GetEventData",
            Self::GetLocationCleared => "GetLocationCleared",
            Self::GetKeywordDataForLocation => "GetKeywordDataForLocation",
            Self::GetLocationAliasCleared => "GetLocationAliasCleared",
            Self::GetKeywordDataForAlias => "GetKeywordDataForAlias",
            Self::IsEssential => "IsEssential",
            Self::IsProtected => "IsProtected",
            Self::IsBleedingOut => "IsBleedingOut",
            Self::GetPlayerTeammate => "GetPlayerTeammate",
            Self::GetRelationshipRank => "GetRelationshipRank",
            Self::HasKeyword => "HasKeyword",
            Self::GetIsGhost => "GetIsGhost",
            Self::Unknown(_) => "Unknown",
        }
    }

    /// Resolve a function by its canonical name (case-insensitive). `None` for
    /// names outside [`CATALOG`](Self::CATALOG).
    pub fn from_name(name: &str) -> Option<Self> {
        Self::CATALOG
            .iter()
            .copied()
            .find(|f| f.name().eq_ignore_ascii_case(name))
    }

    /// Whether the value depends on the entity the Run-On resolves to.
    /// `false` for functions that read only their parameters or global
    /// state — quest stages, scene progress, the Hardcore flag,
    /// `GetVMScriptVariable` (whose object is an explicit parameter) — and
    /// for every index outside the catalog, a constant 0.0.
    ///
    /// Exhaustive on purpose: [`subject_requirement`] evaluates a `false`
    /// function once on behalf of every subject, so a new variant has to be
    /// classified here rather than inherit a default.
    pub fn reads_run_on_entity(self) -> bool {
        match self {
            Self::GetDistance
            | Self::GetActorValue
            | Self::GetDead
            | Self::GetInCell
            | Self::GetIsClass
            | Self::GetIsRace
            | Self::GetIsID
            | Self::GetFactionRank
            | Self::GetInFaction
            | Self::GetLevel
            | Self::GetEquipped
            | Self::HasPerk
            | Self::GetXPForNextLevel
            | Self::HasLoaded3D
            | Self::GetReputation
            | Self::GetReputationThreshold
            | Self::GetInCurrentLoc
            | Self::GetInCurrentLocAlias
            | Self::LocationHasKeyword
            | Self::GetKeywordDataForCurrentLocation
            | Self::GetIsEditorLocation
            | Self::GetIsAliasRef
            | Self::GetIsEditorLocAlias
            | Self::LocationHasRefType
            | Self::HasSameEditorLocAsRefAlias
            | Self::IsEssential
            | Self::IsProtected
            | Self::IsBleedingOut
            | Self::GetPlayerTeammate
            | Self::GetIsGhost
            | Self::GetRelationshipRank
            | Self::HasKeyword => true,
            Self::LocAliasIsLocation
            | Self::GetEventData
            | Self::GetLocationCleared
            | Self::GetKeywordDataForLocation
            | Self::GetLocationAliasCleared
            | Self::GetKeywordDataForAlias
            | Self::GetQuestRunning
            | Self::GetGlobalValue
            | Self::GetRandomPercent
            | Self::GetStage
            | Self::GetQuestVariable
            | Self::GetStageDone
            | Self::IsSceneActionComplete
            | Self::IsHardcore
            | Self::GetVMQuestVariable
            | Self::GetVMScriptVariable
            | Self::Unknown(_) => false,
        }
    }

    /// Whether the same inputs always give the same value. `false` only for
    /// `GetRandomPercent`, which draws from the Story Manager's RNG: it
    /// differs per evaluation and each evaluation advances the generator.
    ///
    /// [`subject_requirement`] must not fold such a function into "one value
    /// on behalf of every subject" — a single draw is not what each subject's
    /// own [`evaluate`] would see, so a block it decides could be wrongly
    /// emptied (or kept) for subjects that would have drawn the other way,
    /// and the pre-filter would stop being an exact superset of `evaluate`
    /// (#5645). Exhaustive on purpose, like
    /// [`reads_run_on_entity`](Self::reads_run_on_entity).
    pub fn is_deterministic(self) -> bool {
        match self {
            Self::GetRandomPercent => false,
            Self::GetDistance
            | Self::GetActorValue
            | Self::GetDead
            | Self::GetInCell
            | Self::GetIsClass
            | Self::GetIsRace
            | Self::GetIsID
            | Self::GetFactionRank
            | Self::GetInFaction
            | Self::GetLevel
            | Self::GetEquipped
            | Self::HasPerk
            | Self::GetXPForNextLevel
            | Self::HasLoaded3D
            | Self::GetReputation
            | Self::GetReputationThreshold
            | Self::GetQuestRunning
            | Self::GetGlobalValue
            | Self::GetStage
            | Self::GetQuestVariable
            | Self::GetStageDone
            | Self::IsSceneActionComplete
            | Self::IsHardcore
            | Self::GetVMQuestVariable
            | Self::GetVMScriptVariable
            | Self::GetInCurrentLoc
            | Self::GetInCurrentLocAlias
            | Self::LocationHasKeyword
            | Self::GetKeywordDataForCurrentLocation
            | Self::GetIsEditorLocation
            | Self::GetIsAliasRef
            | Self::GetIsEditorLocAlias
            | Self::LocAliasIsLocation
            | Self::LocationHasRefType
            | Self::HasSameEditorLocAsRefAlias
            | Self::GetEventData
            | Self::GetLocationCleared
            | Self::GetKeywordDataForLocation
            | Self::GetLocationAliasCleared
            | Self::GetKeywordDataForAlias
            | Self::IsEssential
            | Self::IsProtected
            | Self::IsBleedingOut
            | Self::GetPlayerTeammate
            | Self::GetIsGhost
            | Self::GetRelationshipRank
            | Self::HasKeyword
            | Self::Unknown(_) => true,
        }
    }

    /// Whether the value depends on the [`ConditionContext`] beyond the
    /// resolved Run-On entity — the owning quest's aliases, or the location
    /// a location-alias fill tests. [`evaluate_condition`] routes these
    /// through [`evaluate_contextual`]; a context-free [`evaluate_function`]
    /// call evaluates them with an empty context.
    pub fn reads_context(self) -> bool {
        matches!(
            self,
            Self::GetInCurrentLoc
                | Self::GetInCurrentLocAlias
                | Self::LocationHasKeyword
                | Self::GetKeywordDataForCurrentLocation
                | Self::GetIsEditorLocation
                | Self::GetIsAliasRef
                | Self::GetIsEditorLocAlias
                | Self::LocAliasIsLocation
                | Self::LocationHasRefType
                | Self::HasSameEditorLocAsRefAlias
                | Self::GetEventData
                | Self::GetLocationAliasCleared
                | Self::GetKeywordDataForAlias
        )
    }

    /// Whether this is an identity test: 1.0 exactly when
    /// [`Self::run_on_identity`] equals `param_1`, else 0.0.
    pub fn is_identity_test(self) -> bool {
        matches!(self, Self::GetIsClass | Self::GetIsRace | Self::GetIsID)
    }

    /// The id an identity test compares with `param_1`. `None` when the
    /// entity carries no such id (the test is then 0.0) and for every
    /// function that is not an identity test.
    pub fn run_on_identity(self, entity: EntityId, world: &World) -> Option<u32> {
        use byroredux_core::character::Background;
        match self {
            // The actor's `Background.class` / `.race`, a remapped `CLAS` /
            // `RACE` FormID. Compared in the actor's stored space —
            // identity-equal to a remapped `param_1` in single-plugin loads,
            // same contract as `GetFactionRank`.
            Self::GetIsClass => world.get::<Background>(entity).map(|b| b.class_form_id),
            Self::GetIsRace => world.get::<Background>(entity).map(|b| b.race_form_id),
            // #5041 — `GetIsID ObjectID` names the Run-On's *base object*
            // (19,344 of 19,345 `Oblivion.esm` params resolve to an `NPC_`),
            // not its placed REFR/ACHR, which is what `FormIdComponent`
            // carries. The cell loader stamps both ids on
            // `SceneAliasCandidate` (the player: base `0x7`); the base id is
            // already in the global load-order space the CTDA remap (#1666)
            // put `param_1` in, so this is a direct compare.
            Self::GetIsID => world
                .get::<crate::scene::SceneAliasCandidate>(entity)
                .map(|candidate| candidate.base_form_id)
                .filter(|&base| base != 0),
            _ => None,
        }
    }
}

/// Per-evaluation context — the abstract Run-On targets a CTDA may
/// name (`Subject` / `Target` / `CombatTarget` / …) resolved to
/// concrete entity ids by the caller. Each consumer
/// (perk dispatcher, AI package head, quest_advance system) fills
/// the slots it knows about.
///
/// `None` slots cause the evaluator to short-circuit `false` when a
/// condition's Run-On references that slot — matches Bethesda's
/// "missing reference → condition fails" contract. The exception is
/// [`RunOn::Subject`], which defaults to `subject` (always Some, since
/// every condition list runs in the context of a subject).
#[derive(Debug, Clone, Copy)]
pub struct ConditionContext<'a> {
    /// The "self" entity — quest target, dialogue speaker, magic
    /// effect caster. Always populated.
    pub subject: EntityId,
    /// The "other" entity — dialogue listener, package target, effect
    /// target. `None` when the context doesn't carry one (e.g., a
    /// quest stage condition with no specific target).
    pub target: Option<EntityId>,
    /// Subject's current combat target. `None` when not in combat or
    /// out of M47.1's scope.
    pub combat_target: Option<EntityId>,
    /// Subject's linked-reference chain head. `None` until M47.0.x
    /// linked-ref wiring lands.
    pub linked_reference: Option<EntityId>,
    /// Owning quest for `RunOn::QuestAlias` resolution. Scene evaluation
    /// supplies this; consumers outside a quest leave it `None`.
    pub quest: Option<QuestFormId>,
    /// Alias bindings resolved so far *in the refresh currently running*,
    /// consulted by `RunOn::QuestAlias` ahead of the committed
    /// [`SceneActorBindings`] table (#2671).
    ///
    /// The alias fill loop accumulates into a local map and commits it only
    /// at the end of the pass, so without this a match-CTDA referencing a
    /// *sibling* alias saw last refresh's binding — or nothing at all on the
    /// first refresh — even when the sibling had already been filled earlier
    /// in the same pass. That made cross-alias conditional fills lag one
    /// refresh and, worse, order-dependent in a way the design does not
    /// intend.
    ///
    /// `None` for every consumer outside that loop, which then reads the
    /// committed table exactly as before.
    pub pending_alias_bindings: Option<&'a HashMap<(QuestFormId, i32), EntityId>>,
    /// The Story Manager event's positional data slots, consulted by
    /// `RunOn::EventData` (#5366 Phase 2). `None` outside SM dispatch —
    /// a quest-stage or dialogue CTDA authored with run-on EventData
    /// fails exactly as before.
    pub event_data: Option<&'a EventDataSlots>,
    /// The location a location-alias fill is testing (M47.2). Location
    /// functions run on [`RunOn::Subject`] read it instead of a reference's
    /// current location — the CK's "if used as a condition to fill a Location
    /// Alias, it will test the locations themselves".
    pub location_subject: Option<u32>,
    /// Location aliases bound so far in the fill pass running, ahead of the
    /// committed [`crate::location::LocationAliasBindings`] — the location
    /// twin of [`Self::pending_alias_bindings`].
    pub pending_location_bindings: Option<&'a HashMap<(QuestFormId, i32), u32>>,
}

/// The Story Manager event's positional data slots (#5366 Phase 2) —
/// the runtime half of [`EventDataSlot`]'s wire tags. R1/R2 hold the
/// event's two references (KILL: victim, killer; CLOC: actor), L1/L2
/// its locations as authored LCTN FormIDs (CLOC: old, new).
///
/// Locations are FormIDs, not entities — the runtime has no
/// location-as-entity yet — so an L-slot tagged by a condition stays
/// unresolved (the condition fails) and an L-slot tagged by a
/// `FromEvent` alias fill stays unbound. Both are documented Phase-3+
/// deferrals, not silent drops: the resolve paths trace when they hit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EventDataSlots {
    pub reference_1: Option<EntityId>,
    pub reference_2: Option<EntityId>,
    pub location_1: Option<u32>,
    pub location_2: Option<u32>,
    /// `K1` — a script event's keyword (M47.2, `Keyword.SendStoryEvent`).
    pub keyword: Option<u32>,
    /// `V1` / `V2` — a script event's two integers.
    pub values: [i32; 2],
}

impl EventDataSlots {
    /// The entity an [`EventDataSlot`] tag selects, if that slot is a
    /// populated reference.
    pub fn reference(&self, slot: EventDataSlot) -> Option<EntityId> {
        match slot {
            EventDataSlot::Reference1 => self.reference_1,
            EventDataSlot::Reference2 => self.reference_2,
            EventDataSlot::Location1 | EventDataSlot::Location2 => None,
        }
    }
}

impl<'a> ConditionContext<'a> {
    /// Build a minimal context with only the subject populated. Most
    /// consumers (quest stage gates, dialogue branches) start here
    /// and add `target` per call.
    pub fn for_subject(subject: EntityId) -> Self {
        Self {
            subject,
            target: None,
            combat_target: None,
            linked_reference: None,
            quest: None,
            pending_alias_bindings: None,
            event_data: None,
            location_subject: None,
            pending_location_bindings: None,
        }
    }

    /// Run [`RunOn::Subject`] location functions on `location` itself (a
    /// location-alias fill). See [`Self::location_subject`].
    pub fn with_location_subject(mut self, location: u32) -> Self {
        self.location_subject = Some(location);
        self
    }

    /// Attach the in-progress location-alias bindings. See
    /// [`Self::pending_location_bindings`].
    pub fn with_pending_location_bindings(
        mut self,
        bindings: &'a HashMap<(QuestFormId, i32), u32>,
    ) -> Self {
        self.pending_location_bindings = Some(bindings);
        self
    }

    /// The entity filling the owning quest's reference alias `alias_id`: the
    /// in-progress pass first (#2671), then the committed table.
    fn alias_entity(&self, world: &World, alias_id: i32) -> Option<EntityId> {
        let quest = self.quest?;
        // #2671 — the in-progress pass wins. A sibling alias filled
        // earlier in THIS refresh is not in `SceneActorBindings`
        // yet (the fill loop commits once, at the end), so reading
        // only the committed table made cross-alias conditions see
        // stale data and self-correct a refresh later.
        if let Some(&entity) = self
            .pending_alias_bindings
            .and_then(|pending| pending.get(&(quest, alias_id)))
        {
            return Some(entity);
        }
        let bindings = world.try_resource::<SceneActorBindings>()?;
        bindings.resolve(quest, alias_id)
    }

    /// The location bound to the owning quest's location alias `alias_id`:
    /// the in-progress pass first, then the committed table.
    fn alias_location(&self, world: &World, alias_id: i32) -> Option<u32> {
        let quest = self.quest?;
        if let Some(&location) = self
            .pending_location_bindings
            .and_then(|pending| pending.get(&(quest, alias_id)))
        {
            return Some(location);
        }
        world
            .try_resource::<crate::location::LocationAliasBindings>()?
            .resolve(quest, alias_id)
    }

    /// Attach the in-progress alias-binding map so `RunOn::QuestAlias` can
    /// see siblings filled earlier in the same refresh (#2671). See
    /// [`Self::pending_alias_bindings`].
    pub fn with_pending_alias_bindings(
        mut self,
        bindings: &'a HashMap<(QuestFormId, i32), EntityId>,
    ) -> Self {
        self.pending_alias_bindings = Some(bindings);
        self
    }

    /// Attach an owning quest so `RunOn::QuestAlias` CTDAs can resolve the
    /// alias id carried in `Condition.extra_data_id`.
    pub fn with_quest(mut self, quest: QuestFormId) -> Self {
        self.quest = Some(quest);
        self
    }

    /// Attach the Story Manager event's data slots so `RunOn::EventData`
    /// CTDAs can resolve the R1/R2 tag in `Condition.extra_data_id`
    /// (#5366 Phase 2).
    pub fn with_event_data(mut self, event_data: &'a EventDataSlots) -> Self {
        self.event_data = Some(event_data);
        self
    }

    /// Resolve a [`RunOn`] choice to a concrete EntityId. Returns
    /// `None` when the slot isn't populated or the choice references
    /// data the evaluator doesn't yet plumb (package data, location
    /// event slots).
    fn resolve(&self, run_on: RunOn, condition: &Condition, world: &World) -> Option<EntityId> {
        match run_on {
            RunOn::Subject => Some(self.subject),
            RunOn::Target => self.target,
            RunOn::CombatTarget => self.combat_target,
            RunOn::LinkedReference => self.linked_reference,
            // #2123 — `condition.reference_form_id` goes through the same
            // parse-time global-form-id remap as `GetDistance`'s `param_1`
            // below; `resolve_entity_by_global_form_id` is the shared
            // resolver for that space, not a FormID-pool lookup unique to
            // this arm.
            RunOn::Reference => {
                resolve_entity_by_global_form_id(world, condition.reference_form_id)
            }
            RunOn::QuestAlias => self.alias_entity(world, condition.extra_data_id as i32),
            RunOn::PackageData => {
                log::trace!(
                    "M47.1: RunOn::PackageData (extra_data_id={:08X}) — package \
                     resolver deferred",
                    condition.extra_data_id,
                );
                None
            }
            // #5366 Phase 2 — the tag in `extra_data_id`'s low half names
            // one of the event's positional slots. R-tags resolve to the
            // event's references; L-tags name locations, which are FormIDs
            // rather than entities here, so they stay unresolved (and
            // fail the condition) until a location runtime exists.
            RunOn::EventData => {
                let slots = self.event_data?;
                match event_data_slot(condition.extra_data_id) {
                    Some(slot @ (EventDataSlot::Reference1 | EventDataSlot::Reference2)) => {
                        slots.reference(slot)
                    }
                    Some(slot @ (EventDataSlot::Location1 | EventDataSlot::Location2)) => {
                        log::trace!(
                            "#5366: RunOn::EventData location tag {:?} — no \
                             location-as-entity runtime yet, condition fails",
                            slot,
                        );
                        None
                    }
                    None => {
                        log::debug!(
                            "#5366: RunOn::EventData with unrecognized slot tag \
                             ({:08X}) — condition fails",
                            condition.extra_data_id,
                        );
                        None
                    }
                }
            }
        }
    }
}

/// Evaluate a single [`Condition`] against world state + the
/// resolution context.
///
/// Returns the boolean result. Used internally by [`evaluate`]; exposed
/// for tests + diagnostic dumps.
pub fn evaluate_condition(condition: &Condition, world: &World, ctx: &ConditionContext) -> bool {
    // Resolve Run-On to a concrete entity. `None` → condition fails
    // (Bethesda: "missing reference fails the predicate").
    let Some(entity) = ctx.resolve(condition.run_on, condition, world) else {
        return false;
    };

    // Run the function — returns the f32 the comparator works against.
    let function = ConditionFunction::from_index(condition.function_index);
    if function == ConditionFunction::GetQuestVariable && !uses_obscript_quest_variables(world) {
        // Index 79 is another function (or a string-named variable) outside the
        // pre-Papyrus games: pass it, as every uncatalogued function does.
        return true;
    }
    let function_result = if function.reads_context() {
        let subject = match (condition.run_on, ctx.location_subject) {
            (RunOn::Subject, Some(location)) => LocationSubject::Location(location),
            _ => LocationSubject::Reference(entity),
        };
        evaluate_contextual(function, condition, subject, world, ctx)
    } else {
        evaluate_function(function, condition, entity, world)
    };

    condition
        .comparator
        .apply(function_result, comparand_value(condition, world))
}

/// Whether the load order's game is one whose index-79 `GetQuestVariable` reads an
/// ObScript quest script's local (Oblivion, Fallout 3 and New Vegas).
pub fn uses_obscript_quest_variables(world: &World) -> bool {
    use byroredux_plugin::esm::reader::GameKind;
    world
        .try_resource::<crate::ScriptGame>()
        .is_some_and(|game| matches!(game.0, GameKind::Oblivion | GameKind::Fallout3NV))
}

/// Whether the evaluator can really evaluate `condition` in this world: its
/// function is catalogued, and an index that is a different function in another
/// game (79) is catalogued for the loaded one. The fail-closed gates (dialogue
/// quest gates, Story Manager nodes, perk entries, intrusive packages) decline a
/// condition for which this is `false`.
pub fn is_modeled(world: &World, condition: &Condition) -> bool {
    match ConditionFunction::from_index(condition.function_index) {
        ConditionFunction::Unknown(_) => false,
        ConditionFunction::GetQuestVariable => uses_obscript_quest_variables(world),
        _ => true,
    }
}

/// The value a condition's function result is compared against.
///
/// Globals route through the `Globals` resource (mirrored from
/// `EsmIndex.globals`, #1668). `form_id` is already in global load-order
/// space (remapped at CTDA parse time), matching the resource's key space. A
/// missing resource or unknown GLOB resolves to 0.0 — Bethesda's "missing
/// GLOB defaults to 0".
fn comparand_value(condition: &Condition, world: &World) -> f32 {
    match condition.comparand {
        ConditionValue::Literal(v) => v,
        ConditionValue::Global(form_id) => world
            .try_resource::<crate::globals::Globals>()
            .and_then(|g| g.get(form_id))
            .unwrap_or(0.0),
    }
}

/// Resolve a global-load-order FormID to the entity that carries it.
///
/// `World::find_by_form_id` keys by an interned [`FormId`] handle; a remapped
/// CTDA `param_1` is a raw global `u32`, so this resolves through the
/// [`FormIdPool`] instead — an entity matches when its `FormIdComponent`
/// resolves to a pair whose `local` equals `form_id` (the cell loader stores
/// the full global FormID as the `LocalFormId`). O(n) over entities carrying
/// a `FormIdComponent`, which is fine for the rare condition-eval path.
/// Find the live entity whose `FormIdComponent` resolves (through the
/// `FormIdPool`) to `form_id` in global-plugin space. `O(n)` over every
/// `FormIdComponent`-tagged entity — fine for the occasional resolution
/// this and `travel_system` (M42.4) perform, not a per-frame hot path.
pub fn resolve_entity_by_global_form_id(world: &World, form_id: u32) -> Option<EntityId> {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::FormIdPool;
    // FormIdComponent before FormIdPool — matches the order established
    // elsewhere (save registry/driver, physics sync) for this pair (#313).
    // #4694 (GAME-D5-2026-09-21-01) — PlayerRef (`0x14`) is Bethesda's
    // "the player" sentinel, not a placed reference: no cell authors an
    // `0x14` ACHR, and the player body's `FormIdComponent` resolves to
    // `PLAYER_FORM_ID_PAIR` (local 1), so the scan below could never
    // return the player. Every caller of this function needs PlayerRef to
    // mean the player — the ambient movers' Follow/Escort/Travel/Guard
    // targeting, `GetDistance`, `RunOn::Reference`, fragment object
    // properties, quest stages — so the special case lives here (the same
    // identity `package.rs` used to special-case locally at two sites).
    // Bethesda's conventional PlayerRef sentinel (never a placed record).
    const PLAYER_REF_SENTINEL: u32 = 0x14;
    if form_id == PLAYER_REF_SENTINEL {
        return world
            .try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
            .map(|player| player.0);
    }
    let q = world.query::<FormIdComponent>()?;
    let pool = world.try_resource::<FormIdPool>()?;
    let found = q
        .iter()
        .find(|(_, fid)| pool.resolve(fid.0).is_some_and(|p| p.local.0 == form_id))
        .map(|(eid, _)| eid);
    found
}

/// The global form ID of a live entity — the inverse of
/// [`resolve_entity_by_global_form_id`], and keyed the same way
/// (`FormIdPool::resolve(..).local.0`) so the two agree.
///
/// #3278 — needed by `Effect::Disable`, whose sink (`ReferenceEnableState`)
/// is deliberately FormID-keyed rather than entity-keyed so a disable
/// survives its reference's cell being unloaded; the location runtime keys
/// a reference's editor location the same way.
pub fn entity_global_form_id(world: &World, entity: EntityId) -> Option<u32> {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::FormIdPool;
    let component = world.get::<FormIdComponent>(entity)?;
    let pool = world.try_resource::<FormIdPool>()?;
    pool.resolve(component.0).map(|pair| pair.local.0)
}

/// What a context-reading condition runs on: a reference, or — Run On
/// Subject during a location-alias fill — the candidate location itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocationSubject {
    Reference(EntityId),
    Location(u32),
}

fn truth(value: bool) -> f32 {
    if value {
        1.0
    } else {
        0.0
    }
}

/// The [`ConditionFunction::reads_context`] functions (M47.2): the quest's
/// aliases and the location hierarchy. Semantics are the CK wiki's condition
/// function pages; a missing location (an untagged cell, an unbound alias, no
/// [`crate::location::LocationTable`]) fails the test.
fn evaluate_contextual(
    function: ConditionFunction,
    condition: &Condition,
    subject: LocationSubject,
    world: &World,
    ctx: &ConditionContext,
) -> f32 {
    use crate::location::{current_location, editor_location, LocationTable};
    let alias_id = condition.param_1 as i32;
    // Each location is resolved before the table guard is taken —
    // `current_location` / `editor_location` take it themselves.
    let within = |location: Option<u32>, ancestor: Option<u32>| {
        let (Some(location), Some(ancestor)) = (location, ancestor) else {
            return false;
        };
        world
            .try_resource::<LocationTable>()
            .is_some_and(|table| table.is_within(location, ancestor))
    };
    let parent = |location: u32| {
        world
            .try_resource::<LocationTable>()
            .and_then(|table| table.parent(location))
    };
    match function {
        ConditionFunction::GetIsAliasRef => match subject {
            LocationSubject::Reference(entity) => {
                truth(ctx.alias_entity(world, alias_id) == Some(entity))
            }
            LocationSubject::Location(_) => 0.0,
        },
        ConditionFunction::LocAliasIsLocation => {
            truth(ctx.alias_location(world, alias_id) == Some(condition.param_2))
        }
        ConditionFunction::GetInCurrentLoc | ConditionFunction::GetInCurrentLocAlias => {
            let ancestor = if function == ConditionFunction::GetInCurrentLoc {
                Some(condition.param_1)
            } else {
                ctx.alias_location(world, alias_id)
            };
            // "If run on a location, returns 1 if the parent of the location
            // is (or has a parent location of) the supplied location."
            let location = match subject {
                LocationSubject::Reference(entity) => current_location(world, entity),
                LocationSubject::Location(location) => parent(location),
            };
            truth(within(location, ancestor))
        }
        ConditionFunction::GetIsEditorLocation | ConditionFunction::GetIsEditorLocAlias => {
            let ancestor = if function == ConditionFunction::GetIsEditorLocation {
                Some(condition.param_1)
            } else {
                ctx.alias_location(world, alias_id)
            };
            // A location has no editor location; the CK defines these on
            // references only.
            let LocationSubject::Reference(entity) = subject else {
                return 0.0;
            };
            truth(within(editor_location(world, entity), ancestor))
        }
        ConditionFunction::LocationHasKeyword => {
            let location = match subject {
                LocationSubject::Reference(entity) => current_location(world, entity),
                LocationSubject::Location(location) => Some(location),
            };
            truth(location.is_some_and(|location| {
                world
                    .try_resource::<LocationTable>()
                    .is_some_and(|table| table.has_keyword(location, condition.param_1))
            }))
        }
        ConditionFunction::LocationHasRefType => {
            let location = match subject {
                LocationSubject::Reference(entity) => current_location(world, entity),
                LocationSubject::Location(location) => Some(location),
            };
            let Some(location) = location else {
                return 0.0;
            };
            let tagged: Vec<u32> = world
                .try_resource::<LocationTable>()
                .map(|table| table.refs_of_type(location, condition.param_1).collect())
                .unwrap_or_default();
            // "Returns false if the ref type is dead or disabled": a scripted
            // disable is FormID-keyed and visible here. Death is not — an
            // unloaded actor's death state lives with the cell streamer — so,
            // like the missing cleared state, every tagged ref counts as alive.
            let enabled = world.try_resource::<crate::fragment::ReferenceEnableState>();
            truth(tagged.iter().any(|&reference| {
                enabled
                    .as_ref()
                    .is_none_or(|state| state.override_for(reference) != Some(false))
            }))
        }
        ConditionFunction::HasSameEditorLocAsRefAlias => {
            let Some(other) = ctx
                .alias_entity(world, alias_id)
                .and_then(|entity| editor_location(world, entity))
            else {
                return 0.0;
            };
            let keyword = (condition.param_2 != 0).then_some(condition.param_2);
            let own = match subject {
                LocationSubject::Reference(entity) => match editor_location(world, entity) {
                    Some(own) => Some(own),
                    None => return 0.0,
                },
                LocationSubject::Location(_) => None,
            };
            let Some(table) = world.try_resource::<LocationTable>() else {
                return 0.0;
            };
            truth(match (subject, own) {
                (LocationSubject::Reference(_), Some(own)) => match keyword {
                    None => own == other,
                    Some(keyword) => table
                        .with_keyword(own, keyword)
                        .is_some_and(|level| table.with_keyword(other, keyword) == Some(level)),
                },
                // "Filled with a location having the keyword type and that is
                // (or is the parent location of) the parameter reference's
                // editor location."
                (LocationSubject::Location(location), _) => {
                    keyword.is_none_or(|keyword| table.has_keyword(location, keyword))
                        && table.is_within(other, location)
                }
                (LocationSubject::Reference(_), None) => false,
            })
        }
        ConditionFunction::GetLocationAliasCleared => truth(
            ctx.alias_location(world, alias_id)
                .is_some_and(|location| crate::location::location_cleared(world, location)),
        ),
        ConditionFunction::GetKeywordDataForAlias => ctx
            .alias_location(world, alias_id)
            .map_or(0.0, |location| crate::location_keyword_data(world, location, condition.param_2)),
        ConditionFunction::GetKeywordDataForCurrentLocation => {
            let location = match subject {
                LocationSubject::Reference(entity) => current_location(world, entity),
                LocationSubject::Location(location) => Some(location),
            };
            location.map_or(0.0, |location| crate::location_keyword_data(world, location, condition.param_1))
        }
        ConditionFunction::GetEventData => {
            let Some(event) = ctx.event_data else {
                return 0.0;
            };
            let member = (condition.param_1 >> 16) as u16;
            let member = member.to_le_bytes();
            let form = match &member {
                b"K1" => event.keyword,
                b"L1" => event.location_1,
                b"L2" => event.location_2,
                _ => None,
            };
            match condition.param_1 & 0xFFFF {
                // GetIsID — the member is this form.
                0 => truth(form.is_some_and(|form| form == condition.param_2)),
                // GetValue — V1 / V2.
                2 => match &member {
                    b"V1" => event.values[0] as f32,
                    b"V2" => event.values[1] as f32,
                    _ => 0.0,
                },
                // HasKeyword — a location member carries the keyword.
                3 => truth(matches!(&member, b"L1" | b"L2") && form.is_some_and(|location| {
                    world
                        .try_resource::<LocationTable>()
                        .is_some_and(|table| table.has_keyword(location, condition.param_2))
                })),
                // IsInList / GetItemValue (form lists, item values) are not
                // modelled.
                _ => 0.0,
            }
        }
        // Only `reads_context` functions are routed here.
        _ => 0.0,
    }
}

/// Which layers of an actor value a read keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorValueView {
    /// `ActorValue::current` (every layer): `GetActorValue` / `GetValue`.
    Current,
    /// The base alone, no modifiers or damage: `GetBaseValue` /
    /// `GetBaseActorValue`.
    Base,
    /// The ceiling `GetValuePercentage` divides by: everything but damage.
    Undamaged,
}

/// `entity`'s `avif` actor value as `view` keeps its layers. The current
/// view is the `GetActorValue` condition; the others compose the same way
/// from a copy with the dropped layers cleared, so a derived stat takes its
/// formula value either way.
pub fn actor_value_view(world: &World, entity: EntityId, avif: u32, view: ActorValueView) -> f32 {
    // GetActorValue(avif_form_id) → the Run-On actor's composed value
    // for that actor value (`ActorValue::current`, every layer), or
    // 0.0 when the actor carries no `ActorValues` (or hasn't that value
    // set — Bethesda's absent-AV default). `param_1` is the AVIF FormID,
    // already promoted to global load-order space at parse time
    // (function 14 is in the CTDA form-id-param list, #1666), the same
    // space `ActorValues` is keyed in — a direct lookup, no FormIdPool
    // hop (the key IS the AV's id, not the actor's identity). #1663.
    use byroredux_core::character::{CharacterLevel, CharacterRuleset};
    use byroredux_core::ecs::components::ActorValues;
    // #3441 — the `ActorValues` storage guard must NOT stay live
    // across the `CharacterRuleset` resource acquire below.
    // `pool_regen_tick_system` (`character/regen.rs`) and
    // `melee_damage_charal_bonus` (`combat.rs`) both take the pair the
    // other way round (`CharacterRuleset` → `ActorValues`, the write
    // side in the former), so holding the guard here closes a 2-cycle
    // in the `BYRO_LOCK_ORDER_CHECK` graph — `lock_tracker` keys
    // storages and resources into one `TypeId` map. Snapshot what the
    // ruleset branch needs and drop the guard first; the clone only
    // happens on the fall-through (the authored-value fast path
    // returns while the guard is still the cheap borrow).
    let avs = {
        let Some(avs) = world.get::<ActorValues>(entity) else {
            return 0.0; // no `ActorValues` → absent-AV default
        };
        // An authored carried value wins — populated SPECIAL/skills,
        // baked NPC Health/AP, the player's stamped pools (#4674,
        // kept current by #5039's refresh). #5042 — a modifier-only
        // entry (a constant spell, `modav` or the SDK on a stat the
        // actor does not carry) is NOT authored: its `0.0` base is a
        // placeholder, so it falls through to the ruleset composer.
        if let Some(value) = avs.get(avif).filter(|value| value.base_authored) {
            return match view {
                ActorValueView::Current => value.current(),
                ActorValueView::Base => value.base + value.set_override,
                ActorValueView::Undamaged => value.undamaged(),
            };
        }
        let mut avs = avs.clone();
        // The layers the view drops come off the copy, so the ruleset
        // composes the same value it would for a read of that view.
        if let Some(value) = avs.get(avif).copied() {
            if view == ActorValueView::Base {
                avs.set_permanent(avif, 0.0);
                avs.mod_temporary(avif, -value.temporary_mod);
            }
            if view != ActorValueView::Current {
                avs.restore(avif, value.damage);
            }
        }
        avs
    };
    // `CharacterLevel` too: read it before the ruleset so the surviving
    // direction stays `CharacterRuleset` → (nothing).
    let level = world.get::<CharacterLevel>(entity).map_or(0, |l| l.level);
    // The one composer `melee_damage_charal_bonus` shares (#5042):
    // formula + modifier layers for an actor-general Absolute row;
    // the carried composition for everything else — including
    // `PlayerOnly` rows on an NPC (the derived player answer must
    // not leak onto an NPC that lacks the key) and `Multiplier` rows,
    // whose `eval` is a ratio, not an actor-value reading (#2933).
    match world.try_resource::<CharacterRuleset>() {
        Some(rs) => rs.actor_value(avif, &avs, level),
        None => avs.current(avif),
    }
}

/// Dispatch one of the M47.1-catalog functions against the supplied
/// entity. Returns `0.0` for unknown indices (Bethesda safe-default)
/// or when the function's backing ECS storage isn't registered.
pub fn evaluate_function(
    function: ConditionFunction,
    condition: &Condition,
    entity: EntityId,
    world: &World,
) -> f32 {
    if function.reads_context() {
        return evaluate_contextual(
            function,
            condition,
            LocationSubject::Reference(entity),
            world,
            &ConditionContext::for_subject(entity),
        );
    }
    match function {
        ConditionFunction::GetActorValue => {
            actor_value_view(world, entity, condition.param_1, ActorValueView::Current)
        }
        ConditionFunction::GetQuestRunning => {
            // GetQuestRunning(quest) → 1.0 while the quest is running.
            use crate::quest_stages::{QuestFormId, QuestStageState};
            let Some(state) = world.try_resource::<QuestStageState>() else {
                return 0.0;
            };
            // `is_running` alone — the predicate ObScript's `GetQuestRunning`,
            // dialogue ownership, SM pool eligibility and alias refresh all
            // use. `is_started` is plain map membership, so it stays true
            // after `Quest.Stop()` and after Complete/Fail; folding it in
            // made every `GetQuestRunning(X) == 0` gate permanently false
            // once X had run and stopped (#5636).
            let running = state.is_running(QuestFormId(condition.param_1));
            if running { 1.0 } else { 0.0 }
        }
        ConditionFunction::GetGlobalValue => {
            // GetGlobalValue(global) → the GLOB's current value.
            world
                .try_resource::<crate::globals::Globals>()
                .and_then(|g| g.get(condition.param_1))
                .unwrap_or(0.0)
        }
        ConditionFunction::GetRandomPercent => {
            // GetRandomPercent() → [0, 100]. Deterministic per world in
            // tests: the engine has no seeded general RNG on the World,
            // so this rides the StoryManagerRng when present (SM nodes
            // are its native consumer) and the quest-random state
            // otherwise; both are seeded resources.
            use crate::story_manager::StoryManagerRng;
            if let Some(mut rng) = world.try_resource_mut::<StoryManagerRng>() {
                let value = rng.pick(101);
                value as f32
            } else {
                // No RNG installed (unit worlds): a constant mid-range
                // value keeps `GetRandomPercent < N` gates decidable
                // and is documented here rather than hidden.
                50.0
            }
        }
        ConditionFunction::GetDistance => {
            // GetDistance(target_form_id) → ‖subject − target‖ in world units.
            // `param_1` is the target FormID, already remapped to global
            // load-order space at parse time (#1666) — the same space entity
            // `FormIdComponent`s resolve to — so the resolve is exact.
            use byroredux_core::ecs::components::GlobalTransform;
            let Some(target) = resolve_entity_by_global_form_id(world, condition.param_1) else {
                // Target not currently spawned (e.g. in an unloaded cell).
                // Model an absent target as infinitely far so a proximity
                // gate (`GetDistance < N`) correctly fails rather than
                // reading a missing ref as "right here" (a 0.0 would).
                return f32::MAX;
            };
            let Some(subject_pos) = world.get::<GlobalTransform>(entity).map(|t| t.translation)
            else {
                return f32::MAX;
            };
            // First `get` borrow has been dropped (translation is Copy) before
            // the second — avoids holding two read guards on one storage lock.
            let Some(target_pos) = world.get::<GlobalTransform>(target).map(|t| t.translation)
            else {
                return f32::MAX;
            };
            (subject_pos - target_pos).length()
        }
        ConditionFunction::GetRelationshipRank => {
            match resolve_entity_by_global_form_id(world, condition.param_1) {
                Some(other) => f32::from(crate::relationships::relationship_rank(world, entity, other)),
                None => 0.0,
            }
        }
        ConditionFunction::GetIsGhost => truth(crate::is_ghost(world, entity)),
        ConditionFunction::HasKeyword => {
            truth(crate::keywords::has_keyword(world, entity, condition.param_1))
        }
        ConditionFunction::IsEssential => truth(crate::bleedout::is_essential(world, entity)),
        ConditionFunction::IsProtected => truth(crate::bleedout::is_protected(world, entity)),
        ConditionFunction::IsBleedingOut => {
            truth(crate::bleedout::is_bleeding_out(world, entity))
        }
        ConditionFunction::GetPlayerTeammate => {
            let form = if world
                .try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
                .is_some_and(|player| player.0 == entity)
            {
                Some(crate::PLAYER_REFERENCE)
            } else {
                entity_global_form_id(world, entity)
            };
            truth(form.is_some_and(|form| {
                world
                    .try_resource::<crate::FollowerState>()
                    .is_some_and(|state| state.read(form, crate::FollowerFlag::Teammate))
            }))
        }
        ConditionFunction::GetDead => {
            // Death is a sparse lifecycle marker: absence is the overwhelmingly
            // common alive state, while combat/scripts/resurrection can share
            // one explicit component with authored CTDA checks.
            use byroredux_core::ecs::components::Dead;
            if world.has::<Dead>(entity) {
                1.0
            } else {
                0.0
            }
        }
        ConditionFunction::GetQuestVariable => {
            // GetQuestVariable(quest, variable): the quest script's local in its
            // 1-based `SLSD` slot (`param_2`), 0 until a GameMode run assigns it.
            world
                .try_resource::<crate::obscript_quests::ObScriptQuestLocals>()
                .map_or(0.0, |locals| locals.get(condition.param_1, condition.param_2 as u16))
        }
        ConditionFunction::GetStage => {
            // GetStage(quest_form_id). The current quest stage lives
            // in the [`crate::quest_stages::QuestStageState`] resource.
            // `param_1` is the quest FormID.
            use crate::quest_stages::{QuestFormId, QuestStageState};
            let Some(state) = world.try_resource::<QuestStageState>() else {
                return 0.0;
            };
            state.get_stage(QuestFormId(condition.param_1)) as f32
        }
        ConditionFunction::GetStageDone => {
            // GetStageDone(quest_form_id, stage). param_1 = quest
            // FormID, param_2 = stage. Returns 1.0 when the stage
            // has been reached, 0.0 otherwise. The Bethesda
            // idempotency primitive — "this milestone fired before."
            use crate::quest_stages::{QuestFormId, QuestStageState};
            let Some(state) = world.try_resource::<QuestStageState>() else {
                return 0.0;
            };
            let quest = QuestFormId(condition.param_1);
            let stage = condition.param_2 as u16;
            if state.get_stage_done(quest, stage) {
                1.0
            } else {
                0.0
            }
        }
        ConditionFunction::GetInCell => {
            // Cell ownership is stamped on every loaded entity. The global
            // authored CELL FormID lives once on its root, avoiding duplicate
            // identity data across every mesh, actor, and child entity.
            use byroredux_core::ecs::components::{CellFormId, CellRoot};
            let cell_root = world.get::<CellRoot>(entity).map_or(entity, |root| root.0);
            world.get::<CellFormId>(cell_root).map_or(0.0, |cell| {
                if cell.0 == condition.param_1 {
                    1.0
                } else {
                    0.0
                }
            })
        }
        ConditionFunction::GetFactionRank => {
            // GetFactionRank(faction_form_id) → the Run-On actor's rank in
            // `param_1`'s faction, or -1.0 (Bethesda's "not in faction"
            // sentinel) when the actor has no `FactionRanks` or isn't a
            // member. Faction ids are compared in the NPC's source space —
            // identity-equal to a remapped `param_1` in single-plugin loads
            // (see `FactionRanks` docs).
            use byroredux_core::ecs::components::FactionRanks;
            world
                .get::<FactionRanks>(entity)
                .and_then(|f| f.rank(condition.param_1))
                .map_or(-1.0, |rank| rank as f32)
        }
        ConditionFunction::GetInFaction => {
            // GetInFaction(faction_form_id) → 1.0 when the Run-On actor
            // holds any rank in `param_1`'s faction, 0.0 otherwise. Same
            // `FactionRanks` read and the same source-space faction-id
            // compare as `GetFactionRank` above, collapsed to membership.
            use byroredux_core::ecs::components::FactionRanks;
            if world
                .get::<FactionRanks>(entity)
                .is_some_and(|f| f.rank(condition.param_1).is_some())
            {
                1.0
            } else {
                0.0
            }
        }
        ConditionFunction::GetLevel => {
            // GetLevel → the Run-On actor's character level (0.0 when the
            // actor carries no `CharacterLevel`).
            use byroredux_core::character::CharacterLevel;
            world
                .get::<CharacterLevel>(entity)
                .map_or(0.0, |l| f32::from(l.level))
        }
        ConditionFunction::GetEquipped => {
            // `EquipmentSlots` stores inventory indices rather than item
            // FormIDs. Resolve each occupied index through the actor's
            // Inventory and compare its base record with param_1.
            use byroredux_core::ecs::components::{EquipmentSlots, Inventory};
            // #3112 — `equipped_indices()` spans the biped occupants AND the
            // wielded weapon, which lives outside the occupancy array. A bare
            // `occupants` scan reports every equipped weapon as unequipped.
            //
            // #3580 — collect the indices and DROP the `EquipmentSlots` guard
            // before taking `Inventory`. Holding the two overlapped recorded
            // `EquipmentSlots -> Inventory`, while the fragment runtime
            // reaches the same pair through
            // `query_2_mut_mut::<Inventory, EquipmentSlots>`, which acquires
            // in TypeId order. One of the two orders is then always wrong,
            // and the cycle detector aborted whichever ran second. Not
            // overlapping the guards makes this site order-independent.
            let Some(equipped_indices) = world
                .get::<EquipmentSlots>(entity)
                .map(|slots| slots.equipped_indices().collect::<Vec<_>>())
            else {
                return 0.0;
            };
            let Some(inventory) = world.get::<Inventory>(entity) else {
                return 0.0;
            };
            let equipped = equipped_indices.into_iter().any(|index| {
                inventory
                    .get(index)
                    .is_some_and(|stack| stack.count > 0 && stack.base_form_id == condition.param_1)
            });
            if equipped {
                1.0
            } else {
                0.0
            }
        }
        ConditionFunction::GetXPForNextLevel => {
            // GetXPForNextLevel → XP required for the actor's next level, from
            // the per-game `LevelingModel` in the `CharacterRuleset` resource.
            // 0.0 without the ruleset (the leveling curve is game-supplied).
            use byroredux_core::character::{CharacterLevel, CharacterRuleset};
            let Some(rs) = world.try_resource::<CharacterRuleset>() else {
                return 0.0;
            };
            let level = world.get::<CharacterLevel>(entity).map_or(0, |l| l.level);
            rs.leveling.xp_to_next(level)
        }
        ConditionFunction::IsSceneActionComplete => {
            // param_1 is a global-space SCEN FormID and param_2 is the
            // authored action index. The runtime retains completion history
            // until the next start because completed actions are removed from
            // the active set when their end phase closes.
            use crate::scene::{ScenePlayer, SceneRegistry};
            let Some(scene_entity) = world
                .try_resource::<SceneRegistry>()
                .and_then(|registry| registry.scene_entity(condition.param_1))
            else {
                return 0.0;
            };
            world
                .get::<ScenePlayer>(scene_entity)
                .map_or(0.0, |player| {
                    if player.completed_actions.contains(&condition.param_2)
                        || player.active_actions.iter().any(|action| {
                            action.action_index == condition.param_2 && action.completed
                        })
                    {
                        1.0
                    } else {
                        0.0
                    }
                })
        }
        ConditionFunction::GetIsClass
        | ConditionFunction::GetIsRace
        | ConditionFunction::GetIsID => {
            if function.run_on_identity(entity, world) == Some(condition.param_1) {
                1.0
            } else {
                0.0
            }
        }
        ConditionFunction::HasLoaded3D => {
            // A loaded reference has joined the spatial ECS and therefore
            // carries a GlobalTransform. Cell unload removes that component,
            // making stale alias bindings correctly report false.
            use byroredux_core::ecs::components::GlobalTransform;
            if world.get::<GlobalTransform>(entity).is_some() {
                1.0
            } else {
                0.0
            }
        }
        ConditionFunction::IsHardcore => {
            if world.try_resource::<byroredux_core::ecs::resources::HardcoreMode>()
                .is_some_and(|mode| mode.enabled) { 1.0 } else { 0.0 }
        }
        ConditionFunction::HasPerk => {
            // 1.0 iff the Run-On actor's canonical ranked `Perks` component
            // owns `param_1`. Spawned NPCs carry this component; the legacy
            // `PerkList` projection is not populated by production code.
            use byroredux_core::character::Perks;
            let Some(perks) = world.get::<Perks>(entity) else {
                return 0.0;
            };
            if perks.rank(condition.param_1) > 0 {
                1.0
            } else {
                0.0
            }
        }
        ConditionFunction::GetReputation => {
            // GetReputation(repu_form_id, axis) → the Run-On actor's raw
            // Fame/Infamy with `param_1`. `param_2` is the axis selector
            // (Bethesda console convention: 1 = Fame, 0 = Infamy). 0.0 when the
            // actor carries no `FactionReputation` or the faction is unknown.
            // `param_1` is the global-space `REPU` FormID (#1666 remap).
            use byroredux_core::character::FactionReputation;
            let Some(rep) = world.get::<FactionReputation>(entity) else {
                return 0.0;
            };
            let points = if condition.param_2 == 1 {
                rep.fame(condition.param_1)
            } else {
                rep.infamy(condition.param_1)
            };
            f32::from(points)
        }
        ConditionFunction::GetReputationThreshold => {
            // GetReputationThreshold(repu_form_id, axis) → the Range 0..=3 band
            // the chosen axis falls into for `param_1`'s thresholds. Uses the
            // vanilla FNV threshold table as the fallback source until the live
            // faction/REPU record path supplies per-faction thresholds; an
            // unknown faction yields Range 0 (the safe "Neutral" default).
            use byroredux_core::character::reputation::fnv_faction_thresholds;
            use byroredux_core::character::FactionReputation;
            let Some(rep) = world.get::<FactionReputation>(entity) else {
                return 0.0;
            };
            let points = if condition.param_2 == 1 {
                rep.fame(condition.param_1)
            } else {
                rep.infamy(condition.param_1)
            };
            fnv_faction_thresholds::thresholds_for(condition.param_1)
                .map_or(0.0, |t| f32::from(t.range(u32::from(points))))
        }
        ConditionFunction::GetVMScriptVariable => {
            // Unlike most CTDAs, the object being inspected is an explicit
            // function parameter rather than the Run-On entity. CIS2 carries
            // the variable name; the numeric CTDA param_2 bytes are an
            // editor/runtime artifact and must not be interpreted as an id.
            let Some(target) = resolve_entity_by_global_form_id(world, condition.param_1) else {
                return 0.0;
            };
            let Some(variable) = condition.param_2_text else {
                return 0.0;
            };
            world
                .get::<crate::vm_state::ScriptVariables>(target)
                .and_then(|variables| variables.get(variable))
                .unwrap_or(0.0)
        }
        ConditionFunction::GetVMQuestVariable => {
            // The quest is an explicit parameter (remapped to global space at
            // parse, #1666); CIS2 names the variable.
            let Some(variable) = condition.param_2_text else {
                return 0.0;
            };
            crate::quest_variables::quest_variable(world, QuestFormId(condition.param_1), variable)
                .unwrap_or(0.0)
        }
        // Answered by `evaluate_contextual` above.
        ConditionFunction::GetInCurrentLoc
        | ConditionFunction::GetInCurrentLocAlias
        | ConditionFunction::LocationHasKeyword
        | ConditionFunction::GetKeywordDataForCurrentLocation
        | ConditionFunction::GetIsEditorLocation
        | ConditionFunction::GetIsAliasRef
        | ConditionFunction::GetIsEditorLocAlias
        | ConditionFunction::LocAliasIsLocation
        | ConditionFunction::LocationHasRefType
        | ConditionFunction::HasSameEditorLocAsRefAlias
        | ConditionFunction::GetEventData
        | ConditionFunction::GetLocationAliasCleared
        | ConditionFunction::GetKeywordDataForAlias => 0.0,
        ConditionFunction::GetLocationCleared => {
            truth(crate::location::location_cleared(world, condition.param_1))
        }
        ConditionFunction::GetKeywordDataForLocation => {
            crate::location_keyword_data(world, condition.param_1, condition.param_2)
        }
        ConditionFunction::Unknown(index) => {
            log::trace!(
                "M47.1: condition function index {index} not in M47.1 catalog — \
                 evaluates to 0.0 (Bethesda safe-default)",
            );
            0.0
        }
    }
}

/// Evaluate a full [`ConditionList`] with OR-precedence semantics.
///
/// Empty lists return `true` (Bethesda contract: "no conditions =
/// always fires"). Otherwise, walks the list grouping consecutive
/// ORs into a single "OR block" — the block evaluates `true` if ANY
/// of its members evaluate `true`. The list then AND-combines the
/// per-block results.
///
/// Walked formally: `[c0, c1, c2, c3, …]` with `or_next` flags
/// `[true, true, false, …]` groups as `[c0 OR c1 OR c2] AND [c3 OR …]`.
/// A block continues as long as the PREVIOUS condition's `or_next`
/// is set; the LAST condition's `or_next` is meaningless (no `next`).
///
/// Short-circuits: any AND-block returning `false` terminates the
/// scan early. Any OR-block returning `true` finishes the block and
/// moves on without evaluating the remaining OR members.
pub fn evaluate(conditions: &ConditionList, world: &World, ctx: &ConditionContext) -> bool {
    // An empty list has no blocks: "no conditions = always fires". A false
    // block fails the whole AND chain; within a block, single-condition
    // blocks (no preceding OR flag) reduce to one evaluation and
    // multi-condition blocks are OR-combined with short-circuit.
    or_blocks(conditions).all(|block| {
        conditions[block]
            .iter()
            .any(|condition| evaluate_condition(condition, world, ctx))
    })
}

/// The OR blocks of `conditions`, in order, as index ranges — the units
/// [`evaluate`] AND-combines.
fn or_blocks(conditions: &[Condition]) -> impl Iterator<Item = RangeInclusive<usize>> + '_ {
    let mut i = 0usize;
    std::iter::from_fn(move || {
        if i >= conditions.len() {
            return None;
        }
        // Discover the end of the current OR block. A block extends
        // while the CURRENT condition's `or_next` flag is set.
        let block_start = i;
        while i < conditions.len() && conditions[i].or_next {
            i += 1;
        }
        // `i` now points at the LAST condition of the block (its
        // `or_next` is false). Block = [block_start ..= i].
        //
        // Clamp: if the FINAL condition has `or_next == true` (a
        // malformed/truncated CTDA tail leaves the OR bit set), the inner
        // loop walks `i` to `len`, which the inclusive range would index
        // out of bounds. A trailing OR flag is meaningless (no `next`), so
        // terminate the block at its last real member — `i < len` held on
        // entry, so `len >= 1` and the subtraction can't underflow.
        let block_end_inclusive = i.min(conditions.len() - 1);
        i += 1; // step past the block for next iteration
        Some(block_start..=block_end_inclusive)
    })
}

/// A test a subject passes when `function`'s
/// [`run_on_identity`](ConditionFunction::run_on_identity) equals `id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityTest {
    pub function: ConditionFunction,
    pub id: u32,
}

/// What a condition list demands of its subject, for a caller about to
/// evaluate the same list against many subjects in one context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubjectRequirement {
    /// A block is false whatever the subject, so [`evaluate`] is false for
    /// every subject.
    Unsatisfiable,
    /// A subject can pass only by passing one identity test from each listed
    /// block. Necessary, not sufficient — the list still has to be evaluated
    /// for each subject that does. Empty when no block constrains the
    /// subject that way.
    IdentityBlocks(Vec<Vec<IdentityTest>>),
}

/// Work out, once, what `conditions` demand of any subject evaluated in
/// `ctx`. `ctx.subject` is never read, so any placeholder will do.
///
/// A condition that cannot depend on the subject — its Run-On names another
/// slot, or its function ignores the entity
/// ([`ConditionFunction::reads_run_on_entity`]) — has the same value for
/// every subject, so it is evaluated here once: a block with one true member
/// is always true, and a block whose members are all false in this way is
/// always false. A subject-dependent member narrows its block only when it
/// is an identity test that fails on a 0.0 result; then the block can only
/// pass where one of those identities matches. A non-deterministic member
/// ([`ConditionFunction::is_deterministic`]) is never evaluated and never
/// narrows: it leaves its block open.
pub fn subject_requirement(
    conditions: &ConditionList,
    world: &World,
    ctx: &ConditionContext,
) -> SubjectRequirement {
    let mut identity_blocks = Vec::new();
    'blocks: for block in or_blocks(conditions) {
        let mut tests = Vec::new();
        let mut unconstrained = false;
        for condition in &conditions[block] {
            let function = ConditionFunction::from_index(condition.function_index);
            if !function.is_deterministic() {
                // #5645 — never evaluated here: one draw cannot stand for
                // every subject's own, and evaluating would advance the
                // RNG. It narrows nothing, so the block stays open.
                unconstrained = true;
            } else if !matches!(condition.run_on, RunOn::Subject)
                || !function.reads_run_on_entity()
            {
                if evaluate_condition(condition, world, ctx) {
                    continue 'blocks;
                }
            } else if function.is_identity_test()
                && !condition
                    .comparator
                    .apply(0.0, comparand_value(condition, world))
            {
                tests.push(IdentityTest {
                    function,
                    id: condition.param_1,
                });
            } else {
                unconstrained = true;
            }
        }
        if unconstrained {
            continue;
        }
        if tests.is_empty() {
            return SubjectRequirement::Unsatisfiable;
        }
        identity_blocks.push(tests);
    }
    SubjectRequirement::IdentityBlocks(identity_blocks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::world::World;
    use byroredux_plugin::esm::records::condition::{ComparisonOp, RunOn};

    #[test]
    fn hardcore_condition_reads_the_live_mode_flag() {
        use byroredux_core::ecs::resources::HardcoreMode;
        let mut world = World::new();
        let actor = world.spawn();
        let condition = cond(586, ComparisonOp::Eq, 1.0, false);
        assert!(!evaluate_condition(&condition, &world, &ctx(actor)));
        world.insert_resource(HardcoreMode { enabled: true });
        assert!(evaluate_condition(&condition, &world, &ctx(actor)));
        world.resource_mut::<HardcoreMode>().enabled = false;
        assert!(!evaluate_condition(&condition, &world, &ctx(actor)));
    }

    /// CK *GetPlayerTeammate*: "Returns 1 if an actor is currently set as a player
    /// teammate via the Papyrus SetPlayerTeammate() function, 0 if not."
    #[test]
    fn get_player_teammate_reads_the_follower_flags() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
        let mut world = World::new();
        crate::register(&mut world);
        world.register::<FormIdComponent>();
        let mut pool = FormIdPool::default();
        let id = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(0x0001_0F90),
        });
        world.insert_resource(pool);
        let actor = world.spawn();
        world.insert(actor, FormIdComponent(id));
        let condition = cond(453, ComparisonOp::Eq, 1.0, false);
        assert!(!evaluate_condition(&condition, &world, &ctx(actor)));
        world.resource_mut::<crate::FollowerState>().set_teammate(0x0001_0F90, true, false);
        assert!(evaluate_condition(&condition, &world, &ctx(actor)));
        world.resource_mut::<crate::FollowerState>().set_teammate(0x0001_0F90, false, false);
        assert!(!evaluate_condition(&condition, &world, &ctx(actor)));
    }

    /// CK *GetKeywordDataForLocation*: "returns this value, if any. It otherwise
    /// returns 0."
    #[test]
    fn keyword_data_conditions_read_the_stored_float() {
        const LOCATION: u32 = 0x0001_0F93;
        const KEYWORD: u32 = 0x0001_0F94;
        let mut world = World::new();
        crate::register(&mut world);
        let subject = world.spawn();
        let at_location = Condition {
            function_index: 606,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(10.0),
            param_1: LOCATION,
            param_2: KEYWORD,
            ..Default::default()
        };
        let mut other_keyword = at_location;
        other_keyword.param_2 = KEYWORD + 1;
        assert!(!evaluate_condition(&at_location, &world, &ctx(subject)));
        world.resource_mut::<crate::LocationKeywordData>().set(LOCATION, KEYWORD, 10.0);
        assert!(evaluate_condition(&at_location, &world, &ctx(subject)));
        assert!(!evaluate_condition(&other_keyword, &world, &ctx(subject)), "another keyword reads 0");
    }

    fn ctx(subject: EntityId) -> ConditionContext<'static> {
        ConditionContext::for_subject(subject)
    }

    fn cond(
        function_index: u32,
        comparator: ComparisonOp,
        comparand: f32,
        or_next: bool,
    ) -> Condition {
        Condition {
            function_index,
            comparator,
            comparand: ConditionValue::Literal(comparand),
            run_on: RunOn::Subject,
            or_next,
            ..Default::default()
        }
    }

    #[test]
    fn empty_condition_list_returns_true() {
        let world = World::new();
        let result = evaluate(&Vec::new(), &world, &ctx(0));
        assert!(result, "no conditions = always fires");
    }

    #[test]
    fn single_true_condition_returns_true() {
        use crate::quest_stages::{QuestFormId, QuestStageState};
        let mut world = World::new();
        crate::register(&mut world);
        let mut state = QuestStageState::default();
        state.set_stage(QuestFormId(0xAA), 42);
        world.insert_resource(state);

        // GetStage(0xAA) == 42
        let list = vec![cond(58, ComparisonOp::Eq, 42.0, false).with_param_1(0xAA)];
        assert!(evaluate(&list, &world, &ctx(0)));
    }

    #[test]
    fn single_false_condition_returns_false() {
        use crate::quest_stages::{QuestFormId, QuestStageState};
        let mut world = World::new();
        crate::register(&mut world);
        let mut state = QuestStageState::default();
        state.set_stage(QuestFormId(0xAA), 42);
        world.insert_resource(state);

        // GetStage(0xAA) == 99 (actual is 42)
        let list = vec![cond(58, ComparisonOp::Eq, 99.0, false).with_param_1(0xAA)];
        assert!(!evaluate(&list, &world, &ctx(0)));
    }

    #[test]
    fn unknown_function_index_returns_zero_and_satisfies_eq_zero() {
        // Bethesda's "unknown function safe-default" — function returns
        // 0.0, so a `== 0` comparator matches.
        let world = World::new();
        let list = vec![cond(99999, ComparisonOp::Eq, 0.0, false)];
        assert!(evaluate(&list, &world, &ctx(0)));
    }

    #[test]
    fn and_chain_short_circuits_on_first_false() {
        // [false, true, true] = AND chain — first failure ends scan.
        let world = World::new();
        let list = vec![
            cond(99999, ComparisonOp::Ne, 0.0, false), // false (Unknown→0, !=0 fails)
            cond(99999, ComparisonOp::Eq, 0.0, false), // would be true
        ];
        assert!(!evaluate(&list, &world, &ctx(0)));
    }

    #[test]
    fn or_block_returns_true_when_any_member_true() {
        // [c0 OR c1 OR c2] — c0 false, c1 true short-circuits the block.
        let world = World::new();
        let list = vec![
            cond(99999, ComparisonOp::Ne, 0.0, true),  // false, or_next
            cond(99999, ComparisonOp::Eq, 0.0, true),  // true (block hit)
            cond(99999, ComparisonOp::Ne, 0.0, false), // would be false; block already true
        ];
        assert!(evaluate(&list, &world, &ctx(0)));
    }

    #[test]
    fn or_precedence_quirk_a_and_b_or_c_and_d_groups_b_or_c() {
        // The load-bearing test: `A AND B OR C AND D` evaluates as
        // `A AND (B OR C) AND D`, NOT `(A AND B) OR (C AND D)`.
        //
        // We construct:
        //   A: false (failing condition)
        //   B: true, or_next=true   ┐
        //   C: true, or_next=false  ┘  → (B OR C) = true
        //   D: true
        //
        // Standard interpretation `(A AND B) OR (C AND D)` would be
        // `(false AND true) OR (true AND true)` = false OR true = true.
        // OR-precedence interpretation `A AND (B OR C) AND D` is
        // `false AND true AND true` = false.
        //
        // Expecting the OR-precedence interpretation → false.
        let world = World::new();
        let list = vec![
            cond(99999, ComparisonOp::Ne, 0.0, false), // A: false
            cond(99999, ComparisonOp::Eq, 0.0, true),  // B: true, or_next
            cond(99999, ComparisonOp::Eq, 0.0, false), // C: true (no or_next: end of OR block)
            cond(99999, ComparisonOp::Eq, 0.0, false), // D: true
        ];
        assert!(
            !evaluate(&list, &world, &ctx(0)),
            "OR-precedence: A AND (B OR C) AND D = false AND (true OR true) AND true = false"
        );
    }

    #[test]
    fn or_precedence_quirk_swap_test_a_true() {
        // Flip A to true so the AND chain succeeds — proves the
        // OR-precedence grouping isn't just always-false.
        //
        //   A: true
        //   B: false, or_next=true   ┐
        //   C: true,  or_next=false  ┘  → (B OR C) = true
        //   D: true
        //
        // → true AND (false OR true) AND true = true
        let world = World::new();
        let list = vec![
            cond(99999, ComparisonOp::Eq, 0.0, false), // A: true (Unknown=0, ==0)
            cond(99999, ComparisonOp::Ne, 0.0, true),  // B: false, or_next
            cond(99999, ComparisonOp::Eq, 0.0, false), // C: true
            cond(99999, ComparisonOp::Eq, 0.0, false), // D: true
        ];
        assert!(evaluate(&list, &world, &ctx(0)));
    }

    /// SCR-D6-NEW-01: a single condition whose `or_next` is set (a
    /// malformed/truncated CTDA tail leaving the OR bit on the FINAL
    /// member). The block-discovery loop walks the index to `len`; without
    /// the clamp the inclusive range indexes one past the end and panics
    /// when no earlier member short-circuits true. With the only member
    /// false, evaluation must reach the clamp and return `false`, not panic.
    #[test]
    fn trailing_or_next_on_final_condition_does_not_panic() {
        let world = World::new();
        let list = vec![cond(99999, ComparisonOp::Ne, 0.0, true)]; // false, or_next
        assert!(
            !evaluate(&list, &world, &ctx(0)),
            "a trailing or_next on the final condition must clamp, not panic",
        );
    }

    /// SCR-D6-NEW-01, multi-member block: every OR member is false and the
    /// LAST still carries `or_next`. The whole block is false, so the clamp
    /// must be reached for all members (no short-circuit to mask the OOB).
    #[test]
    fn trailing_or_next_block_all_false_returns_false() {
        let world = World::new();
        let list = vec![
            cond(99999, ComparisonOp::Ne, 0.0, true), // false, or_next
            cond(99999, ComparisonOp::Ne, 0.0, true), // false, or_next (final → meaningless)
        ];
        assert!(
            !evaluate(&list, &world, &ctx(0)),
            "an all-false OR block ending in or_next must return false, not panic",
        );
    }

    #[test]
    fn get_stage_returns_quest_state_stage() {
        use crate::quest_stages::{QuestFormId, QuestStageState};
        let mut world = World::new();
        crate::register(&mut world);
        let mut state = QuestStageState::default();
        state.set_stage(QuestFormId(0xAA), 42);
        world.insert_resource(state);

        // GetStage(0xAA) == 42
        let list = vec![cond(58, ComparisonOp::Eq, 42.0, false).with_param_1(0xAA)];
        assert!(evaluate(&list, &world, &ctx(0)));
        // GetStage(0xAA) < 100
        let list = vec![cond(58, ComparisonOp::Lt, 100.0, false).with_param_1(0xAA)];
        assert!(evaluate(&list, &world, &ctx(0)));
        // GetStage(0xAA) > 50 (false — actually 42)
        let list = vec![cond(58, ComparisonOp::Gt, 50.0, false).with_param_1(0xAA)];
        assert!(!evaluate(&list, &world, &ctx(0)));
    }

    // ── GetIsID (#1666) ─────────────────────────────────────────────────

    /// #5041 — the param is the Run-On's base `NPC_`, not the placed ACHR:
    /// an actor whose base is `param_1` passes even though its own
    /// (reference) FormID differs, and naming the reference id fails.
    #[test]
    fn get_is_id_matches_run_on_base_object_not_placed_reference() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

        const ACHR: u32 = 0x0001_4D8A;
        const NPC: u32 = 0x0000_A1B2;
        let mut world = World::new();
        let mut pool = FormIdPool::new();
        let fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(ACHR),
        });
        world.insert_resource(pool);
        let actor = world.spawn();
        world.insert(actor, FormIdComponent(fid));
        world.insert(
            actor,
            crate::scene::SceneAliasCandidate {
                reference_form_id: ACHR,
                base_form_id: NPC,
                ..Default::default()
            },
        );

        let list = vec![cond(72, ComparisonOp::Eq, 1.0, false).with_param_1(NPC)];
        assert!(evaluate(&list, &world, &ctx(actor)));
        let list = vec![cond(72, ComparisonOp::Eq, 0.0, false).with_param_1(ACHR)];
        assert!(
            evaluate(&list, &world, &ctx(actor)),
            "GetIsID must not match the placed reference's own FormID"
        );
        let list = vec![cond(72, ComparisonOp::Eq, 0.0, false).with_param_1(0x0001_9999)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    #[test]
    fn get_is_id_zero_without_identity() {
        // No SceneAliasCandidate → GetIsID returns 0.0.
        let world = World::new();
        let actor: EntityId = 7;
        let list = vec![cond(72, ComparisonOp::Eq, 0.0, false).with_param_1(0x0001_4D8A)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    #[test]
    fn get_vm_script_variable_reads_cis2_named_state_from_reference() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
        use byroredux_plugin::esm::records::condition::ConditionStringId;

        const GATE: u32 = 0x0009_0A05;
        let mut world = World::new();
        crate::register(&mut world);
        let mut pool = FormIdPool::new();
        let gate_form_id = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(GATE),
        });
        world.insert_resource(pool);
        let subject = world.spawn();
        let gate = world.spawn();
        world.insert(gate, FormIdComponent(gate_form_id));
        let mut variables = crate::ScriptVariables::default();
        variables.set_by_name("::isOpen_var", 1.0);
        world.insert(gate, variables);
        let mut condition = cond(630, ComparisonOp::Eq, 1.0, false).with_param_1(GATE);
        condition.param_2_text = Some(ConditionStringId::from_text("::isOpen_var"));

        assert!(evaluate(&vec![condition], &world, &ctx(subject)));
    }

    /// M47.2 — `GetVMQuestVariable` (629) reads the named quest variable:
    /// the scripted write, else the VMAD-authored value, else 0. A condition
    /// without its CIS2 name reads 0.
    #[test]
    fn get_vm_quest_variable_reads_writes_then_authored_values() {
        use crate::quest_variables::{install_quest_script_variables, QuestScriptVariables};
        use crate::QuestFormId;
        use byroredux_plugin::esm::records::condition::ConditionStringId;
        use byroredux_plugin::esm::records::script_instance::{
            PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
        };

        const MS11: u32 = 0x0004_4B4B;
        let mut world = World::new();
        let subject = world.spawn();
        install_quest_script_variables(
            &mut world,
            [(
                MS11,
                &ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![ScriptInstance {
                    name: "MS11QuestScript".into(),
                    status: 0,
                    properties: vec![ScriptProperty {
                        name: "FriendsKilled".into(),
                        status: 1,
                        value: PropertyValue::Int32(2),
                    }],
                }],
                },
            )],
        );
        let gate = |variable: &str, comparand: f32| {
            let mut condition = cond(629, ComparisonOp::Eq, comparand, false).with_param_1(MS11);
            condition.param_2_text = Some(ConditionStringId::from_text(variable));
            vec![condition]
        };

        assert!(evaluate(&gate("::FriendsKilled_var", 2.0), &world, &ctx(subject)));
        assert!(evaluate(&gate("::Unwritten_var", 0.0), &world, &ctx(subject)));
        world
            .resource_mut::<QuestScriptVariables>()
            .set(QuestFormId(MS11), QuestScriptVariables::property_key("FriendsKilled"), 5.0);
        assert!(evaluate(&gate("::FriendsKilled_var", 5.0), &world, &ctx(subject)));
        assert!(!evaluate(&gate("::FriendsKilled_var", 2.0), &world, &ctx(subject)));

        let mut nameless = cond(629, ComparisonOp::Eq, 0.0, false).with_param_1(MS11);
        nameless.param_2_text = None;
        assert!(evaluate(&vec![nameless], &world, &ctx(subject)));
    }

    // ── GetDead / GetInCell (MQ101 scene gates) ─────────────────────────

    #[test]
    fn get_dead_tracks_sparse_actor_lifecycle_marker() {
        use byroredux_core::ecs::components::Dead;

        let mut world = World::new();
        let alive = world.spawn();
        let dead = world.spawn();
        world.insert(dead, Dead);

        let condition = vec![cond(46, ComparisonOp::Eq, 1.0, false)];
        assert!(!evaluate(&condition, &world, &ctx(alive)));
        assert!(evaluate(&condition, &world, &ctx(dead)));
    }

    #[test]
    fn get_in_cell_resolves_authored_identity_through_cell_root() {
        use byroredux_core::ecs::components::{CellFormId, CellRoot};

        const HELGEN_KEEP: u32 = 0x000A_6A7D;
        let mut world = World::new();
        let root = world.spawn();
        let actor = world.spawn();
        let unowned = world.spawn();
        world.insert(root, CellRoot(root));
        world.insert(root, CellFormId(HELGEN_KEEP));
        world.insert(actor, CellRoot(root));

        let in_helgen = vec![cond(67, ComparisonOp::Eq, 1.0, false).with_param_1(HELGEN_KEEP)];
        assert!(evaluate(&in_helgen, &world, &ctx(actor)));
        assert!(evaluate(&in_helgen, &world, &ctx(root)));
        assert!(!evaluate(&in_helgen, &world, &ctx(unowned)));

        let in_other_cell = vec![cond(67, ComparisonOp::Eq, 1.0, false).with_param_1(0x0000_1234)];
        assert!(!evaluate(&in_other_cell, &world, &ctx(actor)));
    }

    // ── HasPerk (#1667) ─────────────────────────────────────────────────

    #[test]
    fn has_perk_checks_actor_perks() {
        use byroredux_core::character::Perks;

        let mut world = World::new();
        let actor = world.spawn();
        let mut perks = Perks::default();
        perks.set_rank(0x0005_8F80, 1);
        world.insert(actor, perks);

        // HasPerk(0x00058F80) == 1 — the actor holds it.
        let list = vec![cond(449, ComparisonOp::Eq, 1.0, false).with_param_1(0x0005_8F80)];
        assert!(evaluate(&list, &world, &ctx(actor)));

        // HasPerk(0x00058F81) == 0 — not held.
        let list = vec![cond(449, ComparisonOp::Eq, 0.0, false).with_param_1(0x0005_8F81)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    #[test]
    fn has_perk_zero_without_perks() {
        // No Perks component → HasPerk returns 0.0.
        let world = World::new();
        let actor: EntityId = 3;
        let list = vec![cond(449, ComparisonOp::Eq, 0.0, false).with_param_1(0x0005_8F80)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    #[test]
    fn catalog_names_round_trip_and_match_indices() {
        // Every catalog entry resolves by name (case-insensitively) back to
        // itself, and no entry is `Unknown`.
        for f in ConditionFunction::CATALOG {
            assert_ne!(f.name(), "Unknown");
            assert_eq!(ConditionFunction::from_name(f.name()), Some(f));
            assert_eq!(
                ConditionFunction::from_name(&f.name().to_lowercase()),
                Some(f)
            );
        }
        assert_eq!(ConditionFunction::from_name("nope"), None);
        // The console-facing name maps to the same variant `from_index` yields.
        assert_eq!(
            ConditionFunction::from_name("GetReputationThreshold"),
            Some(ConditionFunction::from_index(575))
        );
    }

    /// #5431 — CTDA fn 0 is not a catalogued function. It was briefly mapped
    /// to a constant-`-1` `GetButtonPressed`, but xEdit has index 0 as
    /// `GetWantBlocking` on TES5/FO4 and no index 0 on TES4/FO3/FNV, so that
    /// mapping relabelled Skyrim's IDLE combat predicates. It must stay
    /// `Unknown` (fail-open) until it is mapped from xEdit's table.
    #[test]
    fn function_index_zero_is_not_catalogued() {
        assert_eq!(ConditionFunction::from_index(0), ConditionFunction::Unknown(0));
        assert_eq!(ConditionFunction::from_name("GetButtonPressed"), None);
    }

    /// #5431 — rustdoc attaches a doc comment to the next item, so a variant
    /// inserted between a doc comment and its variant silently re-homes the
    /// doc (the same defect class as #5269). Pin `GetDistance`'s doc to it.
    #[test]
    fn get_distance_variant_keeps_its_doc_comment() {
        let src = include_str!("condition.rs");
        let at = src
            .find("    GetDistance,\n")
            .expect("the GetDistance variant declaration");
        let doc_line = src[..at]
            .lines()
            .last()
            .expect("a line precedes the variant");
        assert!(
            doc_line.contains("FO3 / FNV / Skyrim index **1**"),
            "GetDistance's doc comment must sit directly above the variant, \
             found {doc_line:?}"
        );
    }

    // ── GetLevel / GetIsClass / GetIsRace / GetXPForNextLevel (CHARAL) ──

    #[test]
    fn get_level_reads_character_level() {
        use byroredux_core::character::CharacterLevel;
        let mut world = World::new();
        let actor = world.spawn();
        world.insert(actor, CharacterLevel { level: 12, xp: 40 });
        // GetLevel == 12.
        let list = vec![cond(80, ComparisonOp::Eq, 12.0, false)];
        assert!(evaluate(&list, &world, &ctx(actor)));
        // Absent component → 0.
        let bare: EntityId = 9;
        let list = vec![cond(80, ComparisonOp::Eq, 0.0, false)];
        assert!(evaluate(&list, &world, &ctx(bare)));
    }

    #[test]
    fn get_equipped_resolves_occupied_inventory_item() {
        use byroredux_core::ecs::components::{EquipmentSlots, Inventory, ItemStack};

        const WORN: u32 = 0x0001_2E49;
        const CARRIED: u32 = 0x0001_2EB7;
        let mut world = World::new();
        let actor = world.spawn();
        let mut inventory = Inventory::new();
        let worn = inventory.push(ItemStack::new(WORN, 1));
        inventory.push(ItemStack::new(CARRIED, 1));
        let mut equipment = EquipmentSlots::new();
        equipment.equip(1 << 2, worn);
        world.insert(actor, inventory);
        world.insert(actor, equipment);

        assert!(evaluate(
            &vec![cond(182, ComparisonOp::Eq, 1.0, false).with_param_1(WORN)],
            &world,
            &ctx(actor),
        ));
        assert!(evaluate(
            &vec![cond(182, ComparisonOp::Eq, 0.0, false).with_param_1(CARRIED)],
            &world,
            &ctx(actor),
        ));
    }

    #[test]
    fn has_loaded_3d_tracks_live_spatial_component() {
        use byroredux_core::ecs::components::GlobalTransform;

        let mut world = World::new();
        let loaded = world.spawn();
        let not_loaded = world.spawn();
        world.insert(loaded, GlobalTransform::default());

        let condition = vec![cond(555, ComparisonOp::Eq, 1.0, false)];
        assert!(evaluate(&condition, &world, &ctx(loaded)));
        assert!(!evaluate(&condition, &world, &ctx(not_loaded)));
    }

    #[test]
    fn get_is_class_and_race_match_background() {
        use byroredux_core::character::Background;
        const RACE: u32 = 0x0004_4C07;
        const CLASS: u32 = 0x0001_38B9;
        let mut world = World::new();
        let actor = world.spawn();
        world.insert(
            actor,
            Background {
                race_form_id: RACE,
                class_form_id: CLASS,
            },
        );
        // GetIsRace(RACE) == 1, wrong race == 0.
        assert!(evaluate(
            &vec![cond(69, ComparisonOp::Eq, 1.0, false).with_param_1(RACE)],
            &world,
            &ctx(actor)
        ));
        assert!(evaluate(
            &vec![cond(69, ComparisonOp::Eq, 0.0, false).with_param_1(0x9999)],
            &world,
            &ctx(actor)
        ));
        // GetIsClass(CLASS) == 1.
        assert!(evaluate(
            &vec![cond(68, ComparisonOp::Eq, 1.0, false).with_param_1(CLASS)],
            &world,
            &ctx(actor)
        ));
        // No Background → 0.
        assert!(evaluate(
            &vec![cond(68, ComparisonOp::Eq, 0.0, false).with_param_1(CLASS)],
            &world,
            &ctx(123)
        ));
    }

    #[test]
    fn get_xp_for_next_level_uses_leveling_model() {
        use byroredux_core::character::{CharacterLevel, CharacterRuleset, LevelingModel};
        let mut world = World::new();
        // FNV curve: xp_to_next(L) = 150·L + 50 → L10 = 1550.
        world.insert_resource(CharacterRuleset::new(LevelingModel::FNV));
        let actor = world.spawn();
        world.insert(actor, CharacterLevel { level: 10, xp: 0 });
        let list = vec![cond(533, ComparisonOp::Eq, 1550.0, false)];
        assert!(evaluate(&list, &world, &ctx(actor)));
        // No ruleset resource → 0.0.
        let bare = World::new();
        let list = vec![cond(533, ComparisonOp::Eq, 0.0, false)];
        assert!(evaluate(&list, &bare, &ctx(1)));
    }

    // ── GetReputation / GetReputationThreshold (FNV, indices 573 / 575) ──

    #[test]
    fn get_reputation_reads_fame_and_infamy_by_axis() {
        use byroredux_core::character::FactionReputation;
        // Boomers — the vanilla FNV REPU FormID (top byte 00 = master slot).
        const BOOMERS: u32 = 0x000F_FAE8;

        let mut world = World::new();
        let actor = world.spawn();
        let mut rep = FactionReputation::default();
        rep.add_fame(BOOMERS, 30);
        rep.add_infamy(BOOMERS, 8);
        world.insert(actor, rep);

        // GetReputation(Boomers, axis=1=Fame) == 30.
        let list = vec![cond(573, ComparisonOp::Eq, 30.0, false)
            .with_param_1(BOOMERS)
            .with_param_2(1)];
        assert!(evaluate(&list, &world, &ctx(actor)));
        // axis=0=Infamy == 8.
        let list = vec![cond(573, ComparisonOp::Eq, 8.0, false)
            .with_param_1(BOOMERS)
            .with_param_2(0)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    #[test]
    fn get_reputation_threshold_classifies_range() {
        use byroredux_core::character::FactionReputation;
        const BOOMERS: u32 = 0x000F_FAE8; // thresholds 0 / 8 / 25 / 50

        let mut world = World::new();
        let actor = world.spawn();
        let mut rep = FactionReputation::default();
        rep.add_fame(BOOMERS, 30); // 30 ≥ 25 → Range 2
        world.insert(actor, rep);

        // GetReputationThreshold(Boomers, Fame) == 2.
        let list = vec![cond(575, ComparisonOp::Eq, 2.0, false)
            .with_param_1(BOOMERS)
            .with_param_2(1)];
        assert!(evaluate(&list, &world, &ctx(actor)));
        // Infamy 0 → Range 0.
        let list = vec![cond(575, ComparisonOp::Eq, 0.0, false)
            .with_param_1(BOOMERS)
            .with_param_2(0)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    #[test]
    fn get_reputation_zero_without_component_or_unknown_faction() {
        use byroredux_core::character::FactionReputation;
        // No FactionReputation component → 0.0.
        let world = World::new();
        let actor: EntityId = 7;
        let list = vec![cond(573, ComparisonOp::Eq, 0.0, false)
            .with_param_1(0x000F_FAE8)
            .with_param_2(1)];
        assert!(evaluate(&list, &world, &ctx(actor)));

        // Component present but faction has no captured thresholds → Range 0.
        let mut world = World::new();
        let actor = world.spawn();
        world.insert(actor, FactionReputation::default());
        let list = vec![cond(575, ComparisonOp::Eq, 0.0, false)
            .with_param_1(0xDEAD_BEEF)
            .with_param_2(1)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    // ── GetActorValue (#1663) ───────────────────────────────────────────

    #[test]
    fn get_actor_value_reads_composed_value() {
        use byroredux_core::ecs::components::ActorValues;
        // Stand-in global-space AVIF FormIDs (real ids come from
        // `EsmIndex::actor_values`; the evaluator keys on whatever `param_1`
        // resolves to).
        const AV_HEALTH: u32 = 0x0000_02C9;
        const AV_SNEAK: u32 = 0x0000_02E1;

        let mut world = World::new();
        let actor = world.spawn();
        let mut av = ActorValues::new();
        av.set_base(AV_HEALTH, 100.0);
        av.mod_permanent(AV_HEALTH, 20.0); // +20 perk
        av.apply_damage(AV_HEALTH, 30.0); // −30 damage → current 90
        world.insert(actor, av);

        // GetActorValue(Health) == 90 (the composed value, not the base).
        let list = vec![cond(14, ComparisonOp::Eq, 90.0, false).with_param_1(AV_HEALTH)];
        assert!(evaluate(&list, &world, &ctx(actor)));

        // GetActorValue(Health) > 50 — a typical gate passes on the composite.
        let list = vec![cond(14, ComparisonOp::Gt, 50.0, false).with_param_1(AV_HEALTH)];
        assert!(evaluate(&list, &world, &ctx(actor)));

        // An actor value the actor doesn't carry composes to 0.0.
        let list = vec![cond(14, ComparisonOp::Eq, 0.0, false).with_param_1(AV_SNEAK)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    #[test]
    fn get_actor_value_derives_actor_general_stat_from_ruleset() {
        use byroredux_core::character::fallout4_ruleset;
        use byroredux_core::ecs::components::ActorValues;

        // Stand-in AVIF FormIDs the FO4 ruleset resolves against.
        let resolve = |id: &str| match id {
            "Strength" => Some(0x05u32),
            "Endurance" => Some(0x07),
            "Agility" => Some(0x0A),
            "CarryWeight" => Some(0x2D1),
            "Health" => Some(0x2C9),
            "ActionPoints" => Some(0x2D0),
            "MeleeDamage" => Some(0x2D2),
            _ => None,
        };
        let mut world = World::new();
        world.insert_resource(fallout4_ruleset(resolve));
        let actor = world.spawn();
        // Carries Strength 7, but NOT Carry Weight or Health.
        world.insert(actor, ActorValues::from_pairs([(0x05, 7.0)]));

        // Carry Weight is actor-general → derived on demand: 200 + 10·7 = 270.
        let list = vec![cond(14, ComparisonOp::Eq, 270.0, false).with_param_1(0x2D1)];
        assert!(
            evaluate(&list, &world, &ctx(actor)),
            "Carry Weight derived from SPECIAL"
        );

        // Health is player-only → NOT computed for an NPC; absent default 0.
        let list = vec![cond(14, ComparisonOp::Eq, 0.0, false).with_param_1(0x2C9)];
        assert!(
            evaluate(&list, &world, &ctx(actor)),
            "Health stays 0 (player-only)"
        );
    }

    /// #5042 — FNV `PerkFinesse` shape: a constant CritChance +5 on an NPC
    /// whose derivation carries no CritChance. The modifier-only entry used
    /// to take the carried fast path and read the bare `5`; the reading is
    /// the formula (`1·Luck`) plus the modifier.
    #[test]
    fn get_actor_value_composes_a_modifier_only_entry_onto_the_formula() {
        use byroredux_core::character::falloutnv_ruleset;
        use byroredux_core::ecs::components::ActorValues;

        const LUCK: u32 = 0x0B;
        const CRIT: u32 = 0x2D5;
        let resolve = |id: &str| match id {
            "Luck" => Some(LUCK),
            "CritChance" => Some(CRIT),
            _ => None,
        };
        let mut world = World::new();
        world.insert_resource(falloutnv_ruleset(resolve));
        let actor = world.spawn();
        let mut values = ActorValues::from_pairs([(LUCK, 4.0)]);
        values.mod_permanent(CRIT, 5.0); // `stamp_spell_list` / `modav`
        world.insert(actor, values);

        let list = vec![cond(14, ComparisonOp::Eq, 9.0, false).with_param_1(CRIT)];
        assert!(
            evaluate(&list, &world, &ctx(actor)),
            "Luck 4 + Finesse 5, not the bare modifier"
        );

        // An authored CritChance still wins over the formula.
        world
            .get_mut::<ActorValues>(actor)
            .unwrap()
            .set_base(CRIT, 1.0);
        let list = vec![cond(14, ComparisonOp::Eq, 6.0, false).with_param_1(CRIT)];
        assert!(evaluate(&list, &world, &ctx(actor)), "authored 1 + 5");
    }

    /// `GetBaseValue` / `GetValuePercentage` read the same value with the
    /// modifiers (and damage) dropped, for a derived stat as well as an
    /// authored one.
    #[test]
    fn actor_value_views_drop_modifiers_and_damage() {
        use byroredux_core::character::falloutnv_ruleset;
        use byroredux_core::ecs::components::ActorValues;

        const LUCK: u32 = 0x0B;
        const CRIT: u32 = 0x2D5;
        let resolve = |id: &str| match id {
            "Luck" => Some(LUCK),
            "CritChance" => Some(CRIT),
            _ => None,
        };
        let mut world = World::new();
        world.insert_resource(falloutnv_ruleset(resolve));
        let actor = world.spawn();
        let mut values = ActorValues::from_pairs([(LUCK, 4.0)]);
        values.mod_permanent(CRIT, 5.0);
        values.mod_temporary(CRIT, 2.0);
        values.apply_damage(CRIT, 3.0);
        world.insert(actor, values);
        // Derived CritChance: formula 4 (Luck) plus the layers.
        let read = |world: &World, avif, view| actor_value_view(world, actor, avif, view);
        assert_eq!(read(&world, CRIT, ActorValueView::Current), 4.0 + 5.0 + 2.0 - 3.0);
        assert_eq!(read(&world, CRIT, ActorValueView::Undamaged), 4.0 + 5.0 + 2.0);
        assert_eq!(read(&world, CRIT, ActorValueView::Base), 4.0);
        // Authored Luck with a modifier and damage.
        {
            let avs = world.get_mut::<ActorValues>(actor).unwrap();
            avs.mod_permanent(LUCK, 2.0);
            avs.apply_damage(LUCK, 1.0);
        }
        assert_eq!(read(&world, LUCK, ActorValueView::Current), 5.0);
        assert_eq!(read(&world, LUCK, ActorValueView::Undamaged), 6.0);
        assert_eq!(read(&world, LUCK, ActorValueView::Base), 4.0);
    }

    /// #3441 — `pool_regen_tick_system` and `melee_damage_charal_bonus` both
    /// acquire `CharacterRuleset` → `ActorValues` (the former for write), so
    /// the `GetActorValue` read must not hold its `ActorValues` guard across
    /// the ruleset acquire. `lock_tracker` keys storages and resources into
    /// one `TypeId` graph, so the reverse hold is a real 2-cycle.
    #[test]
    fn get_actor_value_does_not_hold_actor_values_across_ruleset() {
        let src = include_str!("condition.rs");
        let arm_start = src
            .find("pub fn actor_value_view(")
            .expect("the GetActorValue read must exist");
        let arm = &src[arm_start..];
        let arm = &arm[..arm
            .find("/// Dispatch one of the M47.1-catalog functions")
            .expect("evaluate_function must follow the actor value read")];

        let snapshot = arm
            .find("avs.clone()")
            .expect("the arm must snapshot ActorValues instead of holding the guard");
        let level = arm
            .find("world.get::<CharacterLevel>")
            .expect("the arm must read CharacterLevel");
        let ruleset = arm
            .find("try_resource::<CharacterRuleset>()")
            .expect("the arm must consult the ruleset");
        assert!(
            snapshot < ruleset,
            "ActorValues must be cloned and its guard dropped before CharacterRuleset is acquired"
        );
        assert!(
            level < ruleset,
            "CharacterLevel must be read before CharacterRuleset is acquired"
        );

        // Under the opt-in detector, drive the real evaluator after the
        // production hold order has been observed: a surviving reverse hold
        // panics inside `record_and_check`.
        if std::env::var_os("BYRO_LOCK_ORDER_CHECK").as_deref() != Some(std::ffi::OsStr::new("1")) {
            return;
        }
        use byroredux_core::character::fallout4_ruleset;
        use byroredux_core::ecs::components::ActorValues;

        let resolve = |id: &str| match id {
            "Strength" => Some(0x05u32),
            "Endurance" => Some(0x07),
            "Agility" => Some(0x0A),
            "CarryWeight" => Some(0x2D1),
            _ => None,
        };
        let mut world = World::new();
        world.insert_resource(fallout4_ruleset(resolve));
        let actor = world.spawn();
        world.insert(actor, ActorValues::from_pairs([(0x05, 7.0)]));

        // CharacterRuleset → ActorValues (W) — `pool_regen_tick_system`'s order.
        {
            let _rs = world
                .try_resource::<byroredux_core::character::CharacterRuleset>()
                .unwrap();
            let _avs = world.query_mut::<ActorValues>().unwrap();
        }
        // Carry Weight is absent → the arm falls through to the ruleset branch.
        let list = vec![cond(14, ComparisonOp::Eq, 270.0, false).with_param_1(0x2D1)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    #[test]
    fn get_actor_value_zero_without_component() {
        // No `ActorValues` component on the Run-On → GetActorValue returns 0.0
        // (the honest absent-data default — now a real component miss, not a
        // hardcoded stub).
        let world = World::new();
        let actor: EntityId = 7;
        let list = vec![cond(14, ComparisonOp::Eq, 0.0, false).with_param_1(0x0000_02C9)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    // ── GetDistance (#1664) ─────────────────────────────────────────────

    #[test]
    fn get_distance_measures_subject_to_target() {
        use byroredux_core::ecs::components::{FormIdComponent, GlobalTransform};
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        let mut pool = FormIdPool::new();
        // Target carries global FormID 0x000159E2 (cell loader stores the
        // full global id as the LocalFormId).
        let target_fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x0001_59E2),
        });
        world.insert_resource(pool);

        let subject = world.spawn();
        world.insert(
            subject,
            GlobalTransform::new(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY, 1.0),
        );
        let target = world.spawn();
        world.insert(target, FormIdComponent(target_fid));
        world.insert(
            target,
            GlobalTransform::new(Vec3::new(3.0, 4.0, 0.0), Quat::IDENTITY, 1.0),
        );

        // GetDistance(0x000159E2) < 10 → 5.0 < 10 → true.
        let mut lt = cond(1, ComparisonOp::Lt, 10.0, false).with_param_1(0x0001_59E2);
        lt.comparand = ConditionValue::Literal(10.0);
        assert!(evaluate(&vec![lt], &world, &ctx(subject)));

        // GetDistance(0x000159E2) < 4 → 5.0 < 4 → false.
        let lt = cond(1, ComparisonOp::Lt, 4.0, false).with_param_1(0x0001_59E2);
        assert!(!evaluate(&vec![lt], &world, &ctx(subject)));
    }

    #[test]
    fn get_distance_unresolved_target_is_far() {
        use byroredux_core::ecs::components::GlobalTransform;
        use byroredux_core::form_id::FormIdPool;
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let subject = world.spawn();
        world.insert(
            subject,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );

        // No entity carries 0x000159E2 → distance is f32::MAX, so a
        // proximity gate `GetDistance < 100` fails.
        let lt = cond(1, ComparisonOp::Lt, 100.0, false).with_param_1(0x0001_59E2);
        assert!(!evaluate(&vec![lt], &world, &ctx(subject)));
    }

    // ── RunOn::Reference (#2123) ─────────────────────────────────────────

    #[test]
    fn run_on_reference_resolves_the_entity_carrying_the_form_id() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

        let mut world = World::new();
        let mut pool = FormIdPool::new();
        let referenced_fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x0002_ABCD),
        });
        world.insert_resource(pool);

        let subject = world.spawn();
        let referenced = world.spawn();
        world.insert(referenced, FormIdComponent(referenced_fid));

        let mut condition = cond(0, ComparisonOp::Eq, 0.0, false);
        condition.run_on = RunOn::Reference;
        condition.reference_form_id = 0x0002_ABCD;

        let resolved = ctx(subject).resolve(RunOn::Reference, &condition, &world);
        assert_eq!(
            resolved,
            Some(referenced),
            "RunOn::Reference must resolve to the entity whose FormIdComponent \
             maps to reference_form_id via the FormIdPool (#2123)"
        );
    }

    #[test]
    fn run_on_reference_unresolved_form_id_is_none() {
        use byroredux_core::form_id::FormIdPool;

        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let subject = world.spawn();

        let mut condition = cond(0, ComparisonOp::Eq, 0.0, false);
        condition.run_on = RunOn::Reference;
        condition.reference_form_id = 0x0002_ABCD;

        // No entity carries this FormID — must decline (None), not panic
        // or fall back to subject.
        assert_eq!(
            ctx(subject).resolve(RunOn::Reference, &condition, &world),
            None
        );
    }

    // ── GetFactionRank (#1665) ──────────────────────────────────────────

    #[test]
    fn get_faction_rank_reads_membership() {
        use byroredux_core::ecs::components::FactionRanks;

        let mut world = World::new();
        let actor = world.spawn();
        world.insert(
            actor,
            FactionRanks::from_pairs([(0x0001_38B8, 2), (0x0001_5FFB, 0)]),
        );

        // GetFactionRank(0x000138B8) == 2.
        let eq = cond(73, ComparisonOp::Eq, 2.0, false).with_param_1(0x0001_38B8);
        assert!(evaluate(&vec![eq], &world, &ctx(actor)));

        // GetFactionRank(non-member) == -1.
        let eq = cond(73, ComparisonOp::Eq, -1.0, false).with_param_1(0x0009_9999);
        assert!(evaluate(&vec![eq], &world, &ctx(actor)));
    }

    #[test]
    fn get_faction_rank_minus_one_without_component() {
        // No FactionRanks component → -1.0 (not-in-faction sentinel).
        let world = World::new();
        let actor: EntityId = 4;
        let eq = cond(73, ComparisonOp::Eq, -1.0, false).with_param_1(0x0001_38B8);
        assert!(evaluate(&vec![eq], &world, &ctx(actor)));
    }

    // ── Global comparand (#1668) ────────────────────────────────────────

    #[test]
    fn global_comparand_resolves_from_globals_resource() {
        use crate::globals::Globals;
        use crate::quest_stages::{QuestFormId, QuestStageState};

        let mut world = World::new();
        crate::register(&mut world);
        let mut state = QuestStageState::default();
        state.set_stage(QuestFormId(0xAA), 5);
        world.insert_resource(state);

        let mut globals = Globals::new();
        globals.set(0x0100_0042, 5.0); // a GLOB whose value is 5
        world.insert_resource(globals);

        // GetStage(0xAA) == Global(0x01000042) → 5 == 5 → true.
        let mut eq = cond(58, ComparisonOp::Eq, 0.0, false).with_param_1(0xAA);
        eq.comparand = ConditionValue::Global(0x0100_0042);
        assert!(evaluate(&vec![eq], &world, &ctx(0)));

        // GetStage(0xAA) > Global(0x01000042) → 5 > 5 → false.
        let mut gt = cond(58, ComparisonOp::Gt, 0.0, false).with_param_1(0xAA);
        gt.comparand = ConditionValue::Global(0x0100_0042);
        assert!(!evaluate(&vec![gt], &world, &ctx(0)));
    }

    #[test]
    fn global_comparand_defaults_to_zero_when_unresolved() {
        use crate::quest_stages::{QuestFormId, QuestStageState};

        let mut world = World::new();
        crate::register(&mut world);
        let mut state = QuestStageState::default();
        state.set_stage(QuestFormId(0xAA), 0);
        world.insert_resource(state);

        // No Globals resource at all → comparand defaults to 0.0.
        // GetStage(0xAA) == Global(missing) → 0 == 0 → true.
        let mut eq = cond(58, ComparisonOp::Eq, 0.0, false).with_param_1(0xAA);
        eq.comparand = ConditionValue::Global(0x0100_0099);
        assert!(evaluate(&vec![eq], &world, &ctx(0)));
    }

    // ── CTDA remap table drift guard (#5637) ───────────────────────────

    /// The plugin crate's `param1_is_form_id` mirrors this catalog by raw
    /// index (it cannot depend on this crate). Walk every index the catalog
    /// claims and require the two to agree, so a function catalogued
    /// without its remap row — GetQuestRunning / GetInFaction /
    /// GetGlobalValue were — fails here instead of silently reading the
    /// "absent" default on every plugin whose masters are not a load-order
    /// prefix.
    #[test]
    fn param1_form_id_table_matches_the_condition_catalog() {
        use byroredux_plugin::esm::records::condition::param1_is_form_id;
        let mut covered = 0usize;
        for index in 0..2048u32 {
            let function = ConditionFunction::from_index(index);
            if matches!(function, ConditionFunction::Unknown(_)) {
                continue;
            }
            covered += 1;
            assert_eq!(
                param1_is_form_id(index),
                function.first_param_is_form_id(),
                "fn {index} ({}): plugin remap table and catalog disagree on \
                 whether param_1 is a FormID",
                function.name()
            );
        }
        // 24 catalog entries, HasPerk answering to two indices.
        assert_eq!(covered, ConditionFunction::CATALOG.len() + 1);
    }

    // ── GetQuestRunning (#5636) ────────────────────────────────────────

    /// A quest that has been started and then stopped (or completed /
    /// failed) is not running: `GetQuestRunning` must read 0.0 for it, not
    /// the "has ever entered the lifecycle" membership bit.
    #[test]
    fn get_quest_running_is_false_for_stopped_completed_and_failed_quests() {
        use crate::quest_stages::{QuestFormId, QuestStageState};
        // Fresh world per case: `setup` drives the quest through its
        // lifecycle, then `GetQuestRunning(0xA1) == 1` is evaluated.
        let running = |setup: &dyn Fn(&mut QuestStageState)| {
            let mut world = World::new();
            crate::register(&mut world);
            let mut state = QuestStageState::default();
            setup(&mut state);
            world.insert_resource(state);
            let eq1 = cond(56, ComparisonOp::Eq, 1.0, false).with_param_1(0xA1);
            evaluate(&vec![eq1], &world, &ctx(0))
        };
        let q = QuestFormId(0xA1);

        assert!(!running(&|_| {}), "never started");
        assert!(
            running(&|s| {
                s.start_quest(q, None);
            }),
            "started → running"
        );
        assert!(
            !running(&|s| {
                s.start_quest(q, None);
                s.stop(q);
                // Map membership stays true after a stop — the bit the
                // old `|| is_started` arm leaked into the answer.
                assert!(s.is_started(q));
            }),
            "stopped → not running"
        );
        assert!(
            !running(&|s| {
                s.start_quest(q, None);
                s.complete(q);
            }),
            "completed → not running"
        );
        assert!(
            !running(&|s| {
                s.start_quest(q, None);
                s.fail(q);
            }),
            "failed → not running"
        );
    }

    // ── subject_requirement ────────────────────────────────────────────

    /// Subjects that differ in everything the shapes below read, and the
    /// condition shapes: identity tests that do and don't fail on 0.0, a
    /// subject-dependent non-identity test, and unknown functions (a
    /// constant 0.0) that are false and true for every subject.
    fn requirement_fixture() -> (World, Vec<EntityId>, Vec<Condition>) {
        use byroredux_core::character::Background;
        use byroredux_core::ecs::components::{Dead, FormIdComponent};
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

        let mut world = World::new();
        let mut pool = FormIdPool::new();
        let mut form_id = |local| {
            FormIdComponent(pool.intern(FormIdPair {
                plugin: PluginId::from_filename("Fallout4.esm"),
                local: LocalFormId(local),
            }))
        };
        let subjects: Vec<EntityId> = (0..4).map(|_| world.spawn()).collect();
        let (a0, a1) = (form_id(0xA0), form_id(0xA1));
        world.insert(subjects[0], a0);
        world.insert(subjects[1], a1);
        world.insert(subjects[2], a0);
        world.insert_resource(pool);
        world.insert(
            subjects[0],
            Background {
                race_form_id: 0x10,
                class_form_id: 0x20,
            },
        );
        world.insert(
            subjects[1],
            Background {
                race_form_id: 0x11,
                class_form_id: 0x20,
            },
        );
        world.insert(
            subjects[3],
            Background {
                race_form_id: 0x10,
                class_form_id: 0x21,
            },
        );
        world.insert(subjects[1], Dead);

        let shapes = vec![
            cond(69, ComparisonOp::Eq, 1.0, false).with_param_1(0x10),
            cond(69, ComparisonOp::Ne, 0.0, false).with_param_1(0x11),
            cond(69, ComparisonOp::Eq, 0.0, false).with_param_1(0x10),
            cond(68, ComparisonOp::Ge, 1.0, false).with_param_1(0x20),
            cond(72, ComparisonOp::Eq, 1.0, false).with_param_1(0xA0),
            cond(46, ComparisonOp::Eq, 1.0, false),
            // An out-of-catalog index (GetInFaction claimed 71 in
            // #5366 Phase 2; pick one no master authors anywhere).
            cond(990, ComparisonOp::Eq, 1.0, false),
            cond(74, ComparisonOp::Eq, 0.0, false),
        ];
        (world, subjects, shapes)
    }

    /// Every list of one to three shapes, under every `or_next` pattern
    /// (trailing flag included): a requirement must never rule out a subject
    /// `evaluate` passes, so an alias scan narrowed by it binds exactly what
    /// the full scan would.
    #[test]
    fn subject_requirement_never_excludes_a_passing_subject() {
        let (world, subjects, shapes) = requirement_fixture();
        let mut lists = Vec::new();
        for len in 1..=3u32 {
            for picks in 0..shapes.len().pow(len) {
                for ors in 0..1u32 << len {
                    let list: Vec<Condition> = (0..len)
                        .map(|slot| {
                            let pick = picks / shapes.len().pow(slot) % shapes.len();
                            let mut condition = shapes[pick];
                            condition.or_next = ors >> slot & 1 == 1;
                            condition
                        })
                        .collect();
                    lists.push(list);
                }
            }
        }

        let (mut unsatisfiable, mut narrowed) = (0, 0);
        for list in &lists {
            let requirement = subject_requirement(list, &world, &ctx(EntityId::MAX));
            for &subject in &subjects {
                if !evaluate(list, &world, &ctx(subject)) {
                    continue;
                }
                match &requirement {
                    SubjectRequirement::Unsatisfiable => {
                        panic!("{list:?} is unsatisfiable yet passes subject {subject}")
                    }
                    SubjectRequirement::IdentityBlocks(blocks) => {
                        for block in blocks {
                            assert!(
                                block.iter().any(|test| {
                                    test.function.run_on_identity(subject, &world) == Some(test.id)
                                }),
                                "{list:?} passes subject {subject} outside block {block:?}"
                            );
                        }
                    }
                }
            }
            match requirement {
                SubjectRequirement::Unsatisfiable => unsatisfiable += 1,
                SubjectRequirement::IdentityBlocks(blocks) if !blocks.is_empty() => narrowed += 1,
                SubjectRequirement::IdentityBlocks(_) => {}
            }
        }
        // Not vacuous: plenty of lists exercise both outcomes.
        assert!(
            unsatisfiable > 100 && narrowed > 100,
            "{unsatisfiable} / {narrowed}"
        );
    }

    /// #5645 — `GetRandomPercent` is stochastic: it is never folded into a
    /// once-for-everyone value, never narrows a block, and the pre-filter
    /// leaves the generator untouched. Whatever a subject's own `evaluate`
    /// draws, the requirement must not have excluded it.
    #[test]
    fn a_random_percent_gate_neither_narrows_nor_consumes_the_rng() {
        use crate::story_manager::StoryManagerRng;
        assert!(!ConditionFunction::GetRandomPercent.is_deterministic());
        assert!(ConditionFunction::CATALOG
            .iter()
            .filter(|f| **f != ConditionFunction::GetRandomPercent)
            .all(|f| f.is_deterministic()));

        let (mut world, subjects, shapes) = requirement_fixture();
        world.insert_resource(StoryManagerRng { state: 0x5EED });
        let gate = cond(77, ComparisonOp::Lt, 50.0, false);
        let mut gated_or = gate;
        gated_or.or_next = true;
        let lists = [
            vec![gate],
            vec![gate, shapes[0]],
            vec![gated_or, shapes[0]],
            vec![shapes[0], gate],
        ];
        let race = IdentityTest {
            function: ConditionFunction::GetIsRace,
            id: 0x10,
        };
        let expected = [
            SubjectRequirement::IdentityBlocks(vec![]),
            SubjectRequirement::IdentityBlocks(vec![vec![race]]),
            SubjectRequirement::IdentityBlocks(vec![]),
            SubjectRequirement::IdentityBlocks(vec![vec![race]]),
        ];
        for (list, expected) in lists.iter().zip(expected) {
            for round in 0..64u64 {
                world.resource_mut::<StoryManagerRng>().state = round * 0x9E37 + 1;
                let before = world.resource::<StoryManagerRng>().state;
                let requirement = subject_requirement(list, &world, &ctx(EntityId::MAX));
                assert_eq!(
                    world.resource::<StoryManagerRng>().state,
                    before,
                    "the pre-filter must not consume the RNG"
                );
                assert_eq!(requirement, expected, "{list:?}");
                // Whatever this subject's own draw is, it was not excluded.
                for &subject in &subjects {
                    if evaluate(list, &world, &ctx(subject)) {
                        assert_ne!(requirement, SubjectRequirement::Unsatisfiable);
                    }
                }
            }
        }
    }

    #[test]
    fn subject_requirement_reports_what_each_block_demands() {
        let (world, _, shapes) = requirement_fixture();
        let race = |id| IdentityTest {
            function: ConditionFunction::GetIsRace,
            id,
        };
        let judge = |list: Vec<Condition>| subject_requirement(&list, &world, &ctx(EntityId::MAX));
        let or = |mut condition: Condition| {
            condition.or_next = true;
            condition
        };

        // An unknown function is 0.0 for everyone: `== 1` fails them all.
        assert_eq!(
            judge(vec![shapes[6]]),
            SubjectRequirement::Unsatisfiable
        );
        // `== 0` passes them all, leaving the identity block to narrow by.
        assert_eq!(
            judge(vec![shapes[7], shapes[0]]),
            SubjectRequirement::IdentityBlocks(vec![vec![race(0x10)]])
        );
        // An OR block of identity tests admits either identity.
        assert_eq!(
            judge(vec![or(shapes[0]), shapes[1]]),
            SubjectRequirement::IdentityBlocks(vec![vec![race(0x10), race(0x11)]])
        );
        // A test that passes on 0.0, or a non-identity member, leaves its
        // block unconstrained.
        assert_eq!(
            judge(vec![shapes[2]]),
            SubjectRequirement::IdentityBlocks(vec![])
        );
        assert_eq!(
            judge(vec![or(shapes[0]), shapes[5]]),
            SubjectRequirement::IdentityBlocks(vec![])
        );
    }

    // ── Helper: chainable param_1 setter for compact test construction ──

    trait CondBuilder {
        fn with_param_1(self, p: u32) -> Self;
        fn with_param_2(self, p: u32) -> Self;
    }
    impl CondBuilder for Condition {
        fn with_param_1(mut self, p: u32) -> Self {
            self.param_1 = p;
            self
        }
        fn with_param_2(mut self, p: u32) -> Self {
            self.param_2 = p;
            self
        }
    }

    /// Regression for #2933 — `GetActorValue` must honour BOTH contract fields
    /// on `DerivedStatFormula`, not just `scope`.
    ///
    /// A `DerivedOutput::Multiplier` row's `eval` returns a *ratio* meant for a
    /// combat/XP consumer to multiply by. FO4's Melee Damage is
    /// `x(1 + 0.1*STR)` and actor-general, so it passed the scope check and
    /// leaked a bare 1.0..=2.0 where Bethesda's own `GetActorValue` yields an
    /// actor-value reading — small enough to satisfy `> 0` gates and fail
    /// realistic thresholds, with nothing crashing and no test failing.
    #[test]
    fn get_actor_value_skips_multiplier_kind_rows() {
        use byroredux_core::character::derived::{DerivedInput, DerivedStatFormula};
        use byroredux_core::character::leveling::LevelingModel;
        use byroredux_core::character::CharacterRuleset;
        use byroredux_core::ecs::components::ActorValues;

        const STRENGTH: u32 = 0x20;
        const MELEE_DAMAGE: u32 = 0x21;
        const CARRY_WEIGHT: u32 = 0x22;

        let mut rs = CharacterRuleset::new(LevelingModel::FO4);
        // FO4 Melee Damage: x(1 + 0.1*STR), actor-general + MULTIPLIER.
        rs.push_derived(
            MELEE_DAMAGE,
            DerivedStatFormula::affine(DerivedInput::actor_value(STRENGTH), 0.1, 1.0)
                .as_multiplier(),
        );
        // A control row of the same scope but ABSOLUTE kind, so the test
        // proves the gate keys on `kind` and not on "derived rows are skipped".
        rs.push_derived(
            CARRY_WEIGHT,
            DerivedStatFormula::affine(DerivedInput::actor_value(STRENGTH), 10.0, 25.0),
        );

        let mut world = World::new();
        world.insert_resource(rs);
        let e = world.spawn();
        world.insert(e, ActorValues::from_pairs([(STRENGTH, 5.0)]));

        let multiplier = evaluate_function(
            ConditionFunction::GetActorValue,
            &Condition {
                param_1: MELEE_DAMAGE,
                ..cond(14, ComparisonOp::Eq, 0.0, false)
            },
            e,
            &world,
        );
        assert_eq!(
            multiplier, 0.0,
            "a Multiplier-kind row must fall through to the absent-AV default, \
             exactly as a player-only row already does — returning 1.5 here is \
             the #2933 leak"
        );

        let absolute = evaluate_function(
            ConditionFunction::GetActorValue,
            &Condition {
                param_1: CARRY_WEIGHT,
                ..cond(14, ComparisonOp::Eq, 0.0, false)
            },
            e,
            &world,
        );
        assert_eq!(
            absolute, 75.0,
            "absolute actor-general rows must still resolve (10*5 + 25) — the \
             gate keys on `kind`, not on derived-ness"
        );
    }

    /// #5366 Phase 2 — `RunOn::EventData` resolves the R1/R2 tag in the
    /// CTDA tail against the context's event slots; L-tags and unknown
    /// tags fail the condition; a context without event data fails it
    /// exactly as before Phase 2.
    #[test]
    fn event_data_run_on_resolves_reference_tags_only() {
        use byroredux_core::ecs::components::FactionRanks;
        use byroredux_plugin::esm::records::condition::{event_data_slot, EventDataSlot};

        let tag = |bytes: &[u8; 2]| u32::from(bytes[0]) | (u32::from(bytes[1]) << 8);
        assert_eq!(event_data_slot(tag(b"R1")), Some(EventDataSlot::Reference1));
        assert_eq!(event_data_slot(tag(b"R2")), Some(EventDataSlot::Reference2));
        assert_eq!(event_data_slot(tag(b"L1")), Some(EventDataSlot::Location1));
        assert_eq!(event_data_slot(tag(b"L2")), Some(EventDataSlot::Location2));
        assert_eq!(event_data_slot(0x00FF_00FF), None);

        let mut world = World::new();
        let subject = world.spawn();
        let r1 = world.spawn();
        let r2 = world.spawn();
        // GetInFaction(0xF00D) == 1 on the tagged slot — true for r2 only.
        world.insert(r2, FactionRanks::from_pairs([(0xF00D, 0)]));
        let condition = |extra: u32| Condition {
            function_index: 71,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(1.0),
            param_1: 0xF00D,
            run_on: RunOn::EventData,
            extra_data_id: extra,
            ..Default::default()
        };
        let slots = EventDataSlots {
            reference_1: Some(r1),
            reference_2: Some(r2),
            location_1: Some(0x18A49),
            location_2: Some(0x18A56),
            ..Default::default()
        };
        let ctx = ConditionContext::for_subject(subject).with_event_data(&slots);
        assert!(
            evaluate_condition(&condition(tag(b"R2")), &world, &ctx),
            "R2 is in the faction"
        );
        assert!(
            !evaluate_condition(&condition(tag(b"R1")), &world, &ctx),
            "R1 is not"
        );
        assert!(
            !evaluate_condition(&condition(tag(b"L2")), &world, &ctx),
            "location tags have no entity to run on"
        );
        assert!(
            !evaluate_condition(&condition(0x00FF_00FF), &world, &ctx),
            "unknown tags fail"
        );
        assert!(
            !evaluate_condition(
                &condition(tag(b"R2")),
                &world,
                &ConditionContext::for_subject(subject)
            ),
            "no event data in the context — fails as in Phase 1"
        );
    }

    /// GetInFaction (Skyrim 71) — membership collapses the same
    /// `FactionRanks` read GetFactionRank does.
    #[test]
    fn get_in_faction_reads_membership() {
        use byroredux_core::ecs::components::FactionRanks;
        let mut world = World::new();
        let member = world.spawn();
        world.insert(member, FactionRanks::from_pairs([(0xF00D, 2), (0xBAAD, 0)]));
        let outsider = world.spawn();
        world.insert(outsider, FactionRanks::from_pairs([(0xBAAD, 3)]));
        let condition = Condition {
            function_index: 71,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(1.0),
            param_1: 0xF00D,
            ..Default::default()
        };
        assert!(evaluate_condition(&condition, &world, &ctx(member)));
        assert!(!evaluate_condition(&condition, &world, &ctx(outsider)));
        let bare = world.spawn();
        assert!(
            !evaluate_condition(&condition, &world, &ctx(bare)),
            "no FactionRanks — not in the faction"
        );
    }
}

/// #3867 — the CTDA catalog size is published in two engine docs that
/// `_audit-common.md` names as authority ("prefer these docs over
/// re-deriving facts from source"), and both had fallen behind: the
/// feature matrix said 13 (itself the #1818 correction of 7) and the
/// AI-package fail-open paragraph said ~15, against a live catalog of 19.
///
/// An auditor sizing the M47.1 gap reads the doc, not this array, so the
/// drift is load-bearing — and it has now recurred twice. `CATALOG` is a
/// `const` array, so the documented figure can simply be asserted against
/// its length rather than re-checked by hand on the next six additions.
#[cfg(test)]
mod catalog_count_doc_pin_tests {
    use super::ConditionFunction;

    const FEATURE_MATRIX: &str = include_str!("../../../docs/feature-matrix.md");
    const NPC_SPAWN: &str = include_str!("../../../docs/engine/npc-spawn-ai-packages.md");
    /// This file's own source — the module doc's function table lives
    /// here, so the pin scans the file itself. The scan counts the
    /// table's rows (each starting `//! | <index> |`), a property of the
    /// doc text, not a call-site needle, so — unlike the #5650 stub
    /// pins — there is no production literal the test's own body could
    /// satisfy.
    const CONDITION_SRC: &str = include_str!("condition.rs");

    /// The first run of ASCII digits at or after `from` in `haystack`.
    fn digits_after(haystack: &str, anchor: &str, file: &str) -> usize {
        let rest = haystack
            .split_once(anchor)
            .unwrap_or_else(|| panic!("{file} must still carry the anchor {anchor:?} — the doc \
                                       was reworded, so re-point this pin rather than deleting it"))
            .1;
        let start = rest
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or_else(|| panic!("{file}: no number follows {anchor:?}"));
        let rest = &rest[start..];
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        rest[..end].parse().expect("a run of ASCII digits parses")
    }

    /// #5648 — the module doc's own table drifted to 19 rows while the
    /// catalog grew to 24, the third recurrence of the size drift this
    /// module already pins in the two engine docs. The table is the
    /// auditor-facing catalog (the enum's variant docs carry the detail),
    /// so its row count is pinned to `CATALOG` directly: adding a variant
    /// without a table row fails here rather than in a future audit.
    #[test]
    fn the_module_doc_table_lists_the_whole_catalog() {
        let module_doc = CONDITION_SRC
            .split_once("## Function catalog status")
            .and_then(|(_, rest)| rest.split_once("Unknown function indices"))
            .expect("the module doc's catalog section must keep its \
                     `## Function catalog status` / `Unknown function indices` framing")
            .0;
        let rows = module_doc
            .lines()
            .filter(|line| {
                // Strip the `//!` doc-comment prefix before matching.
                let t = line.trim().trim_start_matches("//!").trim();
                t.starts_with("| ") && !t.starts_with("| Index") && !t.starts_with("|---")
            })
            .count();
        assert_eq!(
            rows,
            ConditionFunction::CATALOG.len(),
            "the module doc's function table has {rows} rows but `CATALOG` lists {} — every \
             new catalog entry needs its table row (and its documented index re-verified)",
            ConditionFunction::CATALOG.len(),
        );
        assert_eq!(
            digits_after(CONDITION_SRC, "This catalog ships **", "condition.rs module doc"),
            ConditionFunction::CATALOG.len(),
            "the module doc's prose count no longer matches `ConditionFunction::CATALOG`",
        );
    }

    #[test]
    fn both_engine_docs_publish_the_live_catalog_size() {
        let live = ConditionFunction::CATALOG.len();
        assert!(
            live >= 19,
            "the catalog only ever grows; {live} entries means the extraction broke, \
             not the docs",
        );

        assert_eq!(
            digits_after(
                FEATURE_MATRIX,
                "CTDA condition evaluation with OR-precedence (M47.1) |",
                "docs/feature-matrix.md",
            ),
            live,
            "docs/feature-matrix.md's M47.1 row publishes a catalog size that no longer \
             matches `ConditionFunction::CATALOG` (#3867, and #1818 before it)",
        );

        assert_eq!(
            digits_after(
                NPC_SPAWN,
                "the M47.1 catalog covers ",
                "docs/engine/npc-spawn-ai-packages.md",
            ),
            live,
            "docs/engine/npc-spawn-ai-packages.md's fail-open paragraph publishes a catalog \
             size that no longer matches `ConditionFunction::CATALOG` (#3867)",
        );
    }
}
