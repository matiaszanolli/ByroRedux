//! #5804 — the `ConditionFunction` catalog, split out of `condition.rs`
//! (see that hub's module docs for the evaluator overview).
//!
//! The enum below carries the per-function documentation; everything the
//! six former exhaustive `match` blocks knew — index, canonical name, and
//! the four boolean classifications — is stated ONCE in the [`ROWS`]
//! table, and the accessors derive from it. The exhaustive-match
//! guarantee those blocks got from the compiler is recovered by
//! `rows_cover_every_variant_exactly_once_in_catalog_order` below.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::reader::GameKind;

/// Identifier for a condition function. Wraps the raw u32 function
/// index from `Condition.function_index` with a typed constructor so
/// the dispatcher's `match` is exhaustive against the known catalog.
///
/// New functions land by adding a variant, a [`ROWS`] row and a match arm in
/// [`evaluate_function`]. Unknown indices fall through to
/// [`Self::Unknown`] which evaluates to `0.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConditionFunction {
    /// `GetActorValue(avif_form_id) → f32`. Reads the Run-On's composed value
    /// for the `param_1` actor value from its `ActorValues` component
    /// (`ActorValue::current`), or 0.0 when the actor carries
    /// no such value (#1663). `param_1` is the global-space AVIF FormID.
    /// FO3 / FNV / Skyrim function index **14**. Only the engine-built
    /// conditions and FO4+ authoring carry an AVIF FormID: an *authored*
    /// condition on Oblivion / FO3 / FNV / Skyrim stores an actor-value enum
    /// code; [`condition_actor_value_form`](crate::actor_value_names::condition_actor_value_form)
    /// translates it to the loaded AVIF at evaluation (#5989), and
    /// [`is_modeled`](super::is_modeled) reports it unmodeled where it cannot
    /// (Oblivion has no AVIF records).
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
    /// otherwise. Function index **71** on Skyrim (6 904 corpus uses, every
    /// `param_1` a FACT), and also on FO3 (2 450 uses) and FNV (3 535) where
    /// 71 is `GetInFaction` too (#5996).
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
    /// `GetIsPlayableRace → f32` (#5778): 1.0 when the Run-On actor's race
    /// carries the RACE *Playable* flag ([`crate::obscript_more::PlayableRaces`]),
    /// else 0.0 (also without `Background`). Index **254** in every game
    /// (xEdit `wbDefinitions*.pas`). Oblivion authors it on 308 quest-level
    /// dialogue lists.
    GetIsPlayableRace,
    /// `GetPlayerInSEWorld → f32` (#5778, Oblivion): 1.0 while the engine's
    /// Shivering Isles flag is set (CS *SetPlayerInSEWorld*; CS wiki: "used
    /// extensively by Oblivion.esm to activate/deactivate quests (and thus
    /// standard dialog)"). Reads no Run-On. Index **365** on Oblivion only —
    /// every later game authors 365 as `IsChild` ([`FOREIGN_INDICES`]).
    GetPlayerInSEWorld,
    /// `GetIsSex(sex) → f32` (#5778): 1.0 when the Run-On actor's base is
    /// female and `param_1` is 1, or male and `param_1` is 0
    /// ([`crate::actor_base_traits::is_female`]); 0.0 without an actor base.
    /// Index **70** in every game.
    GetIsSex,
    /// `GetIsVoiceType(voice_type_or_list) → f32` (#5750): 1.0 when the Run-On
    /// actor's voice type — its leveled base's `VTCK`
    /// ([`crate::actor_base_traits::voice_type`]) — is `param_1`, a `VTYP`, or
    /// any member of `param_1` when that is a `FLST` of voice types (CK wiki:
    /// "will also accept a FormList ... and will return true if the calling
    /// actor's voicelist appears in the supplied formlist"); 0.0 for an actor
    /// with no voice type. Index **426** on Skyrim / FO4 but **427** on FO3 / FNV
    /// — measured from the masters (the only function whose first parameter is a
    /// `VTYP`: Skyrim 4,728 / FO4 204 uses at 426, FO3 774 / FNV 16,982 at 427),
    /// so each index is foreign in the other family ([`FOREIGN_INDICES`]).
    GetIsVoiceType,
    /// `IsCommandedActor → f32` (#5750; Skyrim / FO4 index **700**, xEdit
    /// `wbDefinitionsTES5` / `FO4`): CK wiki, "Returns 1 if the subject has been
    /// raised as a zombie or was summoned by a summoning spell". The engine has
    /// no summon or reanimate mechanic, so no actor can be commanded and this
    /// answers 0.0 for every subject — a fact of the engine's state, not a
    /// default: the day a summon exists, this is where it is read. Catalogued
    /// because `DialogueGeneric`'s quest-level gate (the quest every generic
    /// greeting hangs off) is `GetIsVoiceType` and `IsCommandedActor == 0`, and a
    /// fail-closed gate with an unmodeled term declines the whole quest.
    IsCommandedActor,
    /// `GetInWorldspace(worldspace) → f32` (#5778): 1.0 when the Run-On
    /// reference is in one of `param_1`'s exterior cells, 0.0 in an interior
    /// ([`crate::cell_kind::in_worldspace`]). Index **310** in every game.
    GetInWorldspace,
    /// `GetDisabled → f32` (#6251): 1.0 while the Run-On reference is disabled,
    /// by a script or its authored initially-disabled flag through its enable
    /// parent ([`crate::fragment::reference_enabled`]). Index **35** in every game.
    GetDisabled,
    /// `GetItemCount(item) → f32` (#6251): how many of `param_1` the Run-On's
    /// inventory holds, every member of a form list counted
    /// ([`item_count`](super::eval::item_count)). Index **47** in every game.
    GetItemCount,
    /// `GetIsCreature → f32` (#6251): 1.0 when the Run-On actor is a creature
    /// (`CreatureActor`). Index **64** in every game.
    GetIsCreature,
    /// `GetDeadCount(actor_base) → f32` (#6251): how many actors of base
    /// `param_1` have died this game ([`crate::obscript_state::KillRecords`]).
    /// Reads no Run-On. Index **84** in every game.
    GetDeadCount,
    /// `GetPCIsSex(sex) → f32` (#6251): as [`GetIsSex`](Self::GetIsSex), on the
    /// player whatever the Run-On. Index **131** on Oblivion / FO3 / FNV /
    /// Skyrim; FO4 onward author 131 as `GetSceneActionPercent`
    /// ([`FOREIGN_INDICES`]).
    GetPCIsSex,
    /// `IsInCombat → f32` (#6251): 1.0 while the Run-On actor is in combat
    /// ([`crate::is_in_combat`]). Index **289** in every game.
    IsInCombat,
    /// `GetObjectiveCompleted(quest, objective) → f32` (#6251): the quest's
    /// objective `param_2` is completed ([`crate::quest_stages::QuestObjectiveState`]).
    /// Reads no Run-On. **FNV** index **420** (xEdit lists it in no other game).
    GetObjectiveCompleted,
    /// `GetObjectiveDisplayed(quest, objective) → f32` (#6251): as
    /// [`GetObjectiveCompleted`](Self::GetObjectiveCompleted), for displayed.
    /// **FNV** index **421**.
    GetObjectiveDisplayed,
    /// `GetHealthPercentage → f32` (#6251): the Run-On actor's current Health
    /// over its undamaged ceiling, 0 to 1
    /// ([`health_percentage`](super::eval::health_percentage)). FO3 / FNV index
    /// **431** (Skyrim / FO4 moved it to 430, not catalogued here); Starfield
    /// authors 431 as `GetActiveBoostDuration` ([`FOREIGN_INDICES`]).
    GetHealthPercentage,
    /// `GetQuestCompleted(quest) → f32` (#6251): the quest has completed —
    /// GECK: "does not require the quest to be running". Reads no Run-On.
    /// FO3 / FNV index **546** (Skyrim / FO4 moved it to 543, not catalogued
    /// here).
    GetQuestCompleted,
    /// Oblivion's `GetCrimeGold` (CTDA **116**; FO3 / FNV author 116 as
    /// `GetMinorCrimeCount`, Skyrim as `IsIntimidatedbyPlayer`, so it answers
    /// only through [`OBLIVION_DIALECT`]). The Run-On actor's bounty: the
    /// player's saved ledger value, 0 for anyone else.
    GetCrimeGold,
    /// Oblivion's `CanPayCrimeGold` (CTDA **127**; `HasBeenEaten` elsewhere):
    /// 1 when the Run-On actor holds at least their bounty in gold.
    CanPayCrimeGold,
    /// Oblivion's `GetPCFame` (CTDA **249**; `IsInDialogueWithPlayer` in
    /// Skyrim). The player's fame, whoever the Run-On is: "only the player
    /// character has fame or infamy values".
    GetPCFame,
    /// Oblivion's `GetPCInfamy` (CTDA **251**).
    GetPCInfamy,
    /// `IsGuard` (CTDA **125**, the same name in FO3 / FNV / Skyrim; only
    /// Oblivion's is answered, from the `CLAS` Guard flag — see
    /// [`crate::guards`]). 1 when the Run-On actor's class is a guard class.
    IsGuard,
    /// Oblivion's `IsTurnArrest` (CTDA **329**; `IsFleeing` in Skyrim): "1 if
    /// the player is speaking to a guard while he has bounty on his head"
    /// (CS). The Run-On is the speaker; the player is the listener.
    IsTurnArrest,
    /// Oblivion's `GetPCExpelled(faction)` (CTDA **193**; the same function in
    /// FO3 / FNV / Skyrim, answered only for Oblivion, whose
    /// `SetPCExpelled` fills the flag): 1 when the player is expelled from
    /// the faction (the saved ledger's Expelled flag).
    GetPCExpelled,
    /// Oblivion's `IsPlayerInJail` (CTDA **171**; not a function of the later
    /// games). Always 0: the engine has no jail (`GoToJail` is not built), so
    /// the player is never in one.
    IsPlayerInJail,
    /// Function index outside the M47.1 catalog. Evaluates to 0.0
    /// (the Bethesda "unknown function safe-default" — see file-
    /// header doc-comment).
    Unknown(u32),
}

/// One catalog row: everything the six former per-variant `match` blocks
/// knew about a function, stated once (#5804). `from_index` and every
/// classification accessor walks [`ROWS`].
struct Row {
    /// The function this row describes.
    func: ConditionFunction,
    /// The CTDA function index(ices) that map to `func` — a slice because
    /// HasPerk answers to 448 (Skyrim) as well as 449 (FO3/FNV).
    indices: &'static [u32],
    /// The canonical xEdit function name (for console listing / parsing).
    name: &'static str,
    /// Whether `param_1` is a FormID — `false` for alias ids (quest-local
    /// indices, not FormIDs; GetEventData's first parameter is a
    /// function:member pair) and the no-parameter functions.
    first_param_form_id: bool,
    /// Whether the value depends on the entity the Run-On resolves to.
    reads_run_on_entity: bool,
    /// Whether the same inputs always give the same value.
    deterministic: bool,
    /// Whether the value depends on the `ConditionContext` beyond the
    /// resolved Run-On entity.
    reads_context: bool,
    /// Whether this is an identity test.
    identity_test: bool,
}

/// The whole catalog, one row per known function, in
/// [`CATALOG`](ConditionFunction::CATALOG) order; every index across rows
/// is unique.
const ROWS: &[Row] = &[
    Row { func: ConditionFunction::GetDistance, indices: &[1], name: "GetDistance", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetQuestVariable, indices: &[79], name: "GetQuestVariable", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetActorValue, indices: &[14], name: "GetActorValue", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetDead, indices: &[46], name: "GetDead", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetStage, indices: &[58], name: "GetStage", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetStageDone, indices: &[59], name: "GetStageDone", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetInCell, indices: &[67], name: "GetInCell", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetIsClass, indices: &[68], name: "GetIsClass", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: true },
    Row { func: ConditionFunction::GetIsRace, indices: &[69], name: "GetIsRace", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: true },
    Row { func: ConditionFunction::GetIsID, indices: &[72], name: "GetIsID", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: true },
    Row { func: ConditionFunction::GetFactionRank, indices: &[73], name: "GetFactionRank", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetInFaction, indices: &[71], name: "GetInFaction", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetLevel, indices: &[80], name: "GetLevel", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetEquipped, indices: &[182], name: "GetEquipped", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    // The one game-variant pair: 448 Skyrim, 449 FO3/FNV — both map to the
// same behaviour.
    Row { func: ConditionFunction::HasPerk, indices: &[448, 449], name: "HasPerk", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::IsHardcore, indices: &[586], name: "IsHardcore", first_param_form_id: false, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetXPForNextLevel, indices: &[533], name: "GetXPForNextLevel", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::IsSceneActionComplete, indices: &[550], name: "IsSceneActionComplete", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::HasLoaded3D, indices: &[555], name: "HasLoaded3D", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetReputation, indices: &[573], name: "GetReputation", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetReputationThreshold, indices: &[575], name: "GetReputationThreshold", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetVMScriptVariable, indices: &[630], name: "GetVMScriptVariable", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetVMQuestVariable, indices: &[629], name: "GetVMQuestVariable", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    // 56 = GetQuestRunning(quest). The cheap SM-vocabulary trio
// #5380 cataloged: three of the four functions the story-manager
// node corpus gates most on after the unknown-fn decline landed
// (the census: fn 79 x59 on FNV packages; 56/74/77 across Skyrim
// SM nodes).
    Row { func: ConditionFunction::GetQuestRunning, indices: &[56], name: "GetQuestRunning", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    // 74 = GetGlobalValue(global).
    Row { func: ConditionFunction::GetGlobalValue, indices: &[74], name: "GetGlobalValue", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    // 77 = GetRandomPercent() — [0, 100].
    Row { func: ConditionFunction::GetRandomPercent, indices: &[77], name: "GetRandomPercent", first_param_form_id: false, reads_run_on_entity: false, deterministic: false, reads_context: false, identity_test: false },
    // M47.2 — the location family. FO3/FNV/Oblivion author none of
// these indices (CTDA census of their masters); 610
// (LocAliasHasKeyword) is left out because FNV authors 610 as a
// different function.
    Row { func: ConditionFunction::GetInCurrentLoc, indices: &[359], name: "GetInCurrentLoc", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: true, identity_test: false },
    Row { func: ConditionFunction::GetInCurrentLocAlias, indices: &[360], name: "GetInCurrentLocAlias", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: true, identity_test: false },
    Row { func: ConditionFunction::LocationHasKeyword, indices: &[562], name: "LocationHasKeyword", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: true, identity_test: false },
    Row { func: ConditionFunction::GetKeywordDataForCurrentLocation, indices: &[651], name: "GetKeywordDataForCurrentLocation", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: true, identity_test: false },
    Row { func: ConditionFunction::GetIsEditorLocation, indices: &[565], name: "GetIsEditorLocation", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: true, identity_test: false },
    Row { func: ConditionFunction::GetIsAliasRef, indices: &[566], name: "GetIsAliasRef", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: true, identity_test: false },
    Row { func: ConditionFunction::GetIsEditorLocAlias, indices: &[567], name: "GetIsEditorLocAlias", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: true, identity_test: false },
    Row { func: ConditionFunction::LocAliasIsLocation, indices: &[605], name: "LocAliasIsLocation", first_param_form_id: false, reads_run_on_entity: false, deterministic: true, reads_context: true, identity_test: false },
    Row { func: ConditionFunction::LocationHasRefType, indices: &[563], name: "LocationHasRefType", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: true, identity_test: false },
    // 181, not its reference twin 180: Oblivion authors 180 as a
// different function.
    Row { func: ConditionFunction::HasSameEditorLocAsRefAlias, indices: &[181], name: "HasSameEditorLocAsRefAlias", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: true, identity_test: false },
    Row { func: ConditionFunction::GetEventData, indices: &[576], name: "GetEventData", first_param_form_id: false, reads_run_on_entity: false, deterministic: true, reads_context: true, identity_test: false },
    Row { func: ConditionFunction::GetLocationCleared, indices: &[250], name: "GetLocationCleared", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetKeywordDataForLocation, indices: &[606], name: "GetKeywordDataForLocation", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetLocationAliasCleared, indices: &[598], name: "GetLocationAliasCleared", first_param_form_id: false, reads_run_on_entity: false, deterministic: true, reads_context: true, identity_test: false },
    Row { func: ConditionFunction::GetKeywordDataForAlias, indices: &[608], name: "GetKeywordDataForAlias", first_param_form_id: false, reads_run_on_entity: false, deterministic: true, reads_context: true, identity_test: false },
    Row { func: ConditionFunction::IsEssential, indices: &[354], name: "IsEssential", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::IsProtected, indices: &[476], name: "IsProtected", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::IsBleedingOut, indices: &[580], name: "IsBleedingOut", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetPlayerTeammate, indices: &[453], name: "GetPlayerTeammate", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetRelationshipRank, indices: &[403], name: "GetRelationshipRank", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::HasKeyword, indices: &[560], name: "HasKeyword", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetIsGhost, indices: &[237], name: "GetIsGhost", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetIsPlayableRace, indices: &[254], name: "GetIsPlayableRace", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetPlayerInSEWorld, indices: &[365], name: "GetPlayerInSEWorld", first_param_form_id: false, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    // #5750 — 426 Skyrim / FO4, 427 FO3 / FNV: the game-variant pair shape
    // `HasPerk` already has (both map to the same behaviour; each is foreign
    // in the other family, see `FOREIGN_INDICES`).
    Row { func: ConditionFunction::GetIsVoiceType, indices: &[426, 427], name: "GetIsVoiceType", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::IsCommandedActor, indices: &[700], name: "IsCommandedActor", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetIsSex, indices: &[70], name: "GetIsSex", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetInWorldspace, indices: &[310], name: "GetInWorldspace", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    // #6251 — the getters the ObScript VM already answers (its command id is
    // `0x1000 +` the index), catalogued so the fail-closed dialogue and
    // objective gates can read them too. Indices and parameter types from
    // xEdit `wbDefinitions{TES4,FO3,FNV,TES5,FO4,FO76,SF1}.pas`.
    Row { func: ConditionFunction::GetDisabled, indices: &[35], name: "GetDisabled", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetItemCount, indices: &[47], name: "GetItemCount", first_param_form_id: true, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetIsCreature, indices: &[64], name: "GetIsCreature", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetDeadCount, indices: &[84], name: "GetDeadCount", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetPCIsSex, indices: &[131], name: "GetPCIsSex", first_param_form_id: false, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::IsInCombat, indices: &[289], name: "IsInCombat", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetObjectiveCompleted, indices: &[420], name: "GetObjectiveCompleted", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetObjectiveDisplayed, indices: &[421], name: "GetObjectiveDisplayed", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetHealthPercentage, indices: &[431], name: "GetHealthPercentage", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetQuestCompleted, indices: &[546], name: "GetQuestCompleted", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    // The Oblivion dialect: these answer to no index of their own (the same
    // numbers are other functions in FO3 / FNV / Skyrim); `OBLIVION_DIALECT`
    // maps the Oblivion index to them. Indices and names from xEdit
    // `wbDefinitionsTES4.pas`.
    Row { func: ConditionFunction::GetCrimeGold, indices: &[], name: "GetCrimeGold", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::CanPayCrimeGold, indices: &[], name: "CanPayCrimeGold", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetPCFame, indices: &[], name: "GetPCFame", first_param_form_id: false, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetPCInfamy, indices: &[], name: "GetPCInfamy", first_param_form_id: false, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::IsGuard, indices: &[], name: "IsGuard", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::IsTurnArrest, indices: &[], name: "IsTurnArrest", first_param_form_id: false, reads_run_on_entity: true, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::GetPCExpelled, indices: &[], name: "GetPCExpelled", first_param_form_id: true, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
    Row { func: ConditionFunction::IsPlayerInJail, indices: &[], name: "IsPlayerInJail", first_param_form_id: false, reads_run_on_entity: false, deterministic: true, reads_context: false, identity_test: false },
];

/// The CTDA indices whose meaning is Oblivion's own, with the function each
/// names there. They are the same numbers as other functions in the later
/// games (116 `GetMinorCrimeCount`, 127 `HasBeenEaten`, 249
/// `IsInDialogueWithPlayer`, 329 `IsFleeing`), so [`ROWS`] cannot hold them:
/// [`ConditionFunction::from_index_in`] consults this table first when the
/// loaded game is Oblivion.
const OBLIVION_DIALECT: &[(u32, ConditionFunction)] = &[
    (116, ConditionFunction::GetCrimeGold),
    (125, ConditionFunction::IsGuard),
    (127, ConditionFunction::CanPayCrimeGold),
    (249, ConditionFunction::GetPCFame),
    (251, ConditionFunction::GetPCInfamy),
    (329, ConditionFunction::IsTurnArrest),
    (193, ConditionFunction::GetPCExpelled),
    (171, ConditionFunction::IsPlayerInJail),
];

/// CTDA function indices that are a *different* function in some games than
/// the catalog row that owns the index, with the games where that holds
/// (#5990). xEdit's per-game definitions (`wbDefinitionsFO3.pas` /
/// `FNV.pas` / `TES5.pas` / `FO4.pas`) give:
///
/// | idx | catalogued as | is another function in |
/// |---|---|---|
/// | 403 | `GetRelationshipRank` (Skyrim/FO4) | FO3/FNV: `HasFriendDisposition` |
/// | 449 | `HasPerk` (FO3/FNV) | Skyrim, FO4: `GetFactionRelation` |
/// | 533 | `GetXPForNextLevel` (FNV) | Skyrim, FO4: `GetInfamy` |
/// | 550 | `IsSceneActionComplete` (Skyrim/FO4) | FO3/FNV: `IsGoreDisabled` |
/// | 555 | `HasLoaded3D` (Skyrim/FO4) | FO3/FNV: `GetSpellUsageNum` |
/// | 630 | `GetVMScriptVariable` (Skyrim) | FO4: `GetCombatAudioDetection` |
/// | 365 | `GetPlayerInSEWorld` (Oblivion) | FO3/FNV, Skyrim, FO4, FO76, Starfield: `IsChild` |
/// | 131 | `GetPCIsSex` (Oblivion, FO3/FNV, Skyrim) | FO4, FO76, Starfield: `GetSceneActionPercent` |
/// | 426 | `GetIsVoiceType` (Skyrim/FO4) | FO3/FNV: a different function (theirs is 427) |
/// | 427 | `GetIsVoiceType` (FO3/FNV) | Skyrim, FO4: a different function (theirs is 426) |
/// | 431 | `GetHealthPercentage` (FO3/FNV) | Starfield: `GetActiveBoostDuration` |
///
/// Index 79 (`GetQuestVariable`) is the opposite shape — valid only on the
/// pre-Papyrus games — and keeps its own check in the evaluator. A game not
/// named in a row is left as it was: for every row but 365, 131 and 431 no
/// FO76 or Starfield definition has been read, so nothing claims the index is
/// foreign there.
const FOREIGN_INDICES: &[(u32, &[GameKind])] = &[
    (403, &[GameKind::Fallout3NV]),
    (426, &[GameKind::Fallout3NV]),
    (427, &[GameKind::Skyrim, GameKind::Fallout4]),
    (449, &[GameKind::Skyrim, GameKind::Fallout4]),
    (533, &[GameKind::Skyrim, GameKind::Fallout4]),
    (550, &[GameKind::Fallout3NV]),
    (555, &[GameKind::Fallout3NV]),
    (630, &[GameKind::Fallout4]),
    (131, &[GameKind::Fallout4, GameKind::Fallout76, GameKind::Starfield]),
    (431, &[GameKind::Starfield]),
    (
        365,
        &[
            GameKind::Fallout3NV,
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Starfield,
        ],
    ),
];

/// Whether CTDA function `index` is a different function in `game` than the
/// catalog's row for it — the evaluator must not run the row's function there.
pub fn index_is_foreign_in(index: u32, game: GameKind) -> bool {
    FOREIGN_INDICES
        .iter()
        .any(|(foreign, games)| *foreign == index && games.contains(&game))
}

/// Whether an authored `GetActorValue` (14) stores its `param_1` as an
/// actor-value *enum code* in `game` (#5989). Oblivion, FO3/FNV and Skyrim do
/// (xEdit `ptActorValue` = `wbActorValue` enum); only FO4 onward stores an
/// AVIF FormID, which is what the evaluator and `ActorValues` key on. Such a
/// condition is read through `condition_actor_value_form`, which translates it.
pub fn authored_actor_value_is_enum_code(game: GameKind) -> bool {
    matches!(game, GameKind::Oblivion | GameKind::Fallout3NV | GameKind::Skyrim)
}

impl ConditionFunction {
    /// Map a raw u32 function index to a typed variant. Indices
    /// outside the catalog fall through to [`Self::Unknown`].
    ///
    /// #5804 — the 49-arm `match` this used to be is a scan of [`ROWS`];
    /// the per-index commentary lives on the rows themselves.
    pub fn from_index(index: u32) -> Self {
        // Indices are the CTDA function index xEdit stores (and `parse_ctda`
        // reads from bytes 8–11) — verified against TES5Edit `wbDefinitions*.pas`
        // (FO3 == FNV for all shared functions). HasPerk is the one game-variant
        // (448 Skyrim, 449 FO3/FNV); both map to the same behaviour. Reputation
        // (573/575) is FNV-only.
        // #5431 — index 0 is deliberately unmapped: xEdit has it as
        // `GetWantBlocking` on TES5/FO4 and has no index 0 on
        // TES4/FO3/FNV, so it is not a MessageBox query. It stays
        // `Unknown` (fail-open) like every other uncatalogued function.
        match ROWS.iter().find(|row| row.indices.contains(&index)) {
            Some(row) => row.func,
            None => Self::Unknown(index),
        }
    }

    /// [`Self::from_index`] for the loaded game: Oblivion's own meanings of
    /// shared numbers ([`OBLIVION_DIALECT`]) win there; every other game, and
    /// every other index, resolves as `from_index` does.
    pub fn from_index_in(game: Option<GameKind>, index: u32) -> Self {
        if game == Some(GameKind::Oblivion) {
            if let Some(&(_, function)) = OBLIVION_DIALECT.iter().find(|(dialect, _)| *dialect == index) {
                return function;
            }
        }
        Self::from_index(index)
    }

    /// Oblivion's own `(CTDA index, function)` pairs (see `OBLIVION_DIALECT`).
    pub fn oblivion_dialect() -> &'static [(u32, ConditionFunction)] {
        OBLIVION_DIALECT
    }

    /// Every known (non-[`Unknown`](Self::Unknown)) function — the catalog the
    /// debug console enumerates and resolves names against.
    pub const CATALOG: [ConditionFunction; 72] = [
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
        Self::GetIsPlayableRace,
        Self::GetPlayerInSEWorld,
        Self::GetIsVoiceType,
        Self::IsCommandedActor,
        Self::GetIsSex,
        Self::GetInWorldspace,
        Self::GetDisabled,
        Self::GetItemCount,
        Self::GetIsCreature,
        Self::GetDeadCount,
        Self::GetPCIsSex,
        Self::IsInCombat,
        Self::GetObjectiveCompleted,
        Self::GetObjectiveDisplayed,
        Self::GetHealthPercentage,
        Self::GetQuestCompleted,
        Self::GetCrimeGold,
        Self::CanPayCrimeGold,
        Self::GetPCFame,
        Self::GetPCInfamy,
        Self::IsGuard,
        Self::IsTurnArrest,
        Self::GetPCExpelled,
        Self::IsPlayerInJail,
    ];

    /// This function's [`ROWS`] row. `Unknown` has none — the panicking
    /// expect is the loud edge a forgotten row hits; the exhaustive walk
    /// in `tests` below is what keeps it unreachable for real variants
    /// (#5804).
    fn row(self) -> &'static Row {
        ROWS.iter()
            .find(|&row| row.func == self)
            .expect("every catalogued ConditionFunction has a ROWS entry (#5804)")
    }

    /// Whether `param_1` of this function is a FormID — the slice of catalog
    /// knowledge the plugin crate's load-order remap
    /// (`byroredux_plugin::esm::records::condition::param1_is_form_id`)
    /// mirrors by raw index. [`ROWS`] is exhaustive on purpose: adding a
    /// variant forces this decision (the ROWS walk test fails until the
    /// row exists), and `param1_form_id_table_matches_the_condition_catalog`
    /// then fails until the plugin table agrees (#5637).
    pub fn first_param_is_form_id(self) -> bool {
        !matches!(self, Self::Unknown(_)) && self.row().first_param_form_id
    }

    /// The canonical xEdit function name (for console listing / parsing).
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown(_) => "Unknown",
            _ => self.row().name,
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
    /// function once on behalf of every subject, so a new variant has to
    /// carry a [`ROWS`] row rather than inherit a default.
    pub fn reads_run_on_entity(self) -> bool {
        !matches!(self, Self::Unknown(_)) && self.row().reads_run_on_entity
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
        matches!(self, Self::Unknown(_)) || self.row().deterministic
    }

    /// Whether the value depends on the [`ConditionContext`] beyond the
    /// resolved Run-On entity — the owning quest's aliases, or the location
    /// a location-alias fill tests. [`evaluate_condition`] routes these
    /// through [`evaluate_contextual`]; a context-free [`evaluate_function`]
    /// call evaluates them with an empty context.
    pub fn reads_context(self) -> bool {
        !matches!(self, Self::Unknown(_)) && self.row().reads_context
    }

    /// Whether this is an identity test: 1.0 exactly when
    /// [`Self::run_on_identity`] equals `param_1`, else 0.0.
    pub fn is_identity_test(self) -> bool {
        !matches!(self, Self::Unknown(_)) && self.row().identity_test
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

/// #5804 — the six per-variant `match` blocks this file used to carry
/// (`from_index` / `name` / `first_param_is_form_id` /
/// `reads_run_on_entity` / `is_deterministic` / `reads_context` /
/// `is_identity_test`) failed to compile the moment a variant was added;
/// the [`ROWS`]-derived accessors cannot. This walk recovers that
/// guarantee.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_cover_every_variant_exactly_once_in_catalog_order() {
        // ROWS follows CATALOG's ordering — the console listing and the
        // doc-table pins depend on it.
        assert_eq!(
            ROWS.len(),
            ConditionFunction::CATALOG.len(),
            "ROWS and CATALOG disagree on the catalog's size"
        );
        for (row, catalogued) in ROWS.iter().zip(ConditionFunction::CATALOG) {
            assert_eq!(
                row.func, catalogued,
                "ROWS must list its rows in ConditionFunction::CATALOG order"
            );
        }

        // Every variant, exactly once. The `match` is what makes the walk
        // exhaustive: a new variant fails compilation here until it is
        // listed, and the count below then forces its ROWS row.
        let rows_of =
            |function: ConditionFunction| ROWS.iter().filter(|&row| row.func == function).count();
        for function in ConditionFunction::CATALOG {
            match function {
                ConditionFunction::GetActorValue |
                ConditionFunction::GetQuestRunning |
                ConditionFunction::GetGlobalValue |
                ConditionFunction::GetRandomPercent |
                ConditionFunction::GetDistance |
                ConditionFunction::GetDead |
                ConditionFunction::GetStage |
                ConditionFunction::GetQuestVariable |
                ConditionFunction::GetStageDone |
                ConditionFunction::GetInCell |
                ConditionFunction::GetFactionRank |
                ConditionFunction::GetInFaction |
                ConditionFunction::GetLevel |
                ConditionFunction::GetEquipped |
                ConditionFunction::GetXPForNextLevel |
                ConditionFunction::IsSceneActionComplete |
                ConditionFunction::HasLoaded3D |
                ConditionFunction::GetIsClass |
                ConditionFunction::GetIsRace |
                ConditionFunction::GetIsID |
                ConditionFunction::HasPerk |
                ConditionFunction::IsHardcore |
                ConditionFunction::GetReputation |
                ConditionFunction::GetReputationThreshold |
                ConditionFunction::GetVMScriptVariable |
                ConditionFunction::GetVMQuestVariable |
                ConditionFunction::GetInCurrentLoc |
                ConditionFunction::GetInCurrentLocAlias |
                ConditionFunction::LocationHasKeyword |
                ConditionFunction::GetIsEditorLocation |
                ConditionFunction::GetIsAliasRef |
                ConditionFunction::GetIsEditorLocAlias |
                ConditionFunction::LocAliasIsLocation |
                ConditionFunction::LocationHasRefType |
                ConditionFunction::HasSameEditorLocAsRefAlias |
                ConditionFunction::GetEventData |
                ConditionFunction::GetLocationCleared |
                ConditionFunction::GetLocationAliasCleared |
                ConditionFunction::GetKeywordDataForLocation |
                ConditionFunction::GetKeywordDataForAlias |
                ConditionFunction::GetKeywordDataForCurrentLocation |
                ConditionFunction::IsEssential |
                ConditionFunction::IsProtected |
                ConditionFunction::IsBleedingOut |
                ConditionFunction::GetPlayerTeammate |
                ConditionFunction::GetRelationshipRank |
                ConditionFunction::HasKeyword |
                ConditionFunction::GetIsGhost |
                ConditionFunction::GetIsPlayableRace |
                ConditionFunction::GetPlayerInSEWorld |
                ConditionFunction::GetIsVoiceType |
                ConditionFunction::IsCommandedActor |
                ConditionFunction::GetIsSex |
                ConditionFunction::GetInWorldspace |
                ConditionFunction::GetDisabled |
                ConditionFunction::GetItemCount |
                ConditionFunction::GetIsCreature |
                ConditionFunction::GetDeadCount |
                ConditionFunction::GetPCIsSex |
                ConditionFunction::IsInCombat |
                ConditionFunction::GetObjectiveCompleted |
                ConditionFunction::GetObjectiveDisplayed |
                ConditionFunction::GetHealthPercentage |
                ConditionFunction::GetQuestCompleted |
                ConditionFunction::GetCrimeGold |
                ConditionFunction::CanPayCrimeGold |
                ConditionFunction::GetPCFame |
                ConditionFunction::GetPCInfamy |
                ConditionFunction::IsGuard |
                ConditionFunction::IsTurnArrest |
                ConditionFunction::GetPCExpelled |
                ConditionFunction::IsPlayerInJail => {
                    assert_eq!(
                        rows_of(function),
                        1,
                        "{} must hold exactly one ROWS row",
                        function.name()
                    );
                }
                ConditionFunction::Unknown(_) => unreachable!("CATALOG never lists Unknown"),
            }
        }
        assert_eq!(rows_of(ConditionFunction::Unknown(0)), 0);

        // Indices are unique across rows and map back to their row's
        // function; the derived accessors agree with the row's fields.
        let mut seen = std::collections::HashSet::new();
        for row in ROWS {
            assert_eq!(row.func.name(), row.name, "name() must derive from ROWS");
            assert_eq!(
                row.func.first_param_is_form_id(),
                row.first_param_form_id,
                "{}: first_param_is_form_id drifted from ROWS",
                row.name
            );
            assert_eq!(
                row.func.reads_run_on_entity(),
                row.reads_run_on_entity,
                "{}: reads_run_on_entity drifted from ROWS",
                row.name
            );
            assert_eq!(
                row.func.is_deterministic(),
                row.deterministic,
                "{}: is_deterministic drifted from ROWS",
                row.name
            );
            assert_eq!(
                row.func.reads_context(),
                row.reads_context,
                "{}: reads_context drifted from ROWS",
                row.name
            );
            assert_eq!(
                row.func.is_identity_test(),
                row.identity_test,
                "{}: is_identity_test drifted from ROWS",
                row.name
            );
            for &index in row.indices {
                assert!(seen.insert(index), "index {index} appears in two ROWS rows");
                assert_eq!(
                    ConditionFunction::from_index(index),
                    row.func,
                    "from_index({index}) must resolve to {}'s row",
                    row.name
                );
            }
        }
        // CATALOG plus the two game-variant twins: HasPerk's 448 (Skyrim) and
        // GetIsVoiceType's 427 (FO3/FNV, #5750).
        // The dialect functions have no index of their own.
        assert_eq!(seen.len(), ConditionFunction::CATALOG.len() + 2 - OBLIVION_DIALECT.len());

        // The dialect resolves for Oblivion only, never shadows a catalogued
        // index, and every dialect function has a row.
        for &(index, function) in OBLIVION_DIALECT {
            assert_eq!(ConditionFunction::from_index_in(Some(GameKind::Oblivion), index), function);
            for other in [None, Some(GameKind::Fallout3NV), Some(GameKind::Skyrim)] {
                assert_eq!(ConditionFunction::from_index_in(other, index), ConditionFunction::Unknown(index));
            }
            assert!(!seen.contains(&index), "{index} is catalogued for another function");
        }
        assert_eq!(
            ConditionFunction::from_index_in(Some(GameKind::Oblivion), 58),
            ConditionFunction::GetStage,
            "an index outside the dialect resolves as before"
        );

        // Unknown keeps its hardcoded classifications — no ROWS row.
        let unknown = ConditionFunction::Unknown(u32::MAX);
        assert_eq!(unknown.name(), "Unknown");
        assert!(!unknown.first_param_is_form_id());
        assert!(!unknown.reads_run_on_entity());
        assert!(unknown.is_deterministic());
        assert!(!unknown.reads_context());
        assert!(!unknown.is_identity_test());
        assert_eq!(ConditionFunction::from_index(u32::MAX), unknown);
    }
}
