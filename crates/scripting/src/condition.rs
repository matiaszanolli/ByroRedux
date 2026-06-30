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
//! lineage. This catalog ships **13 functions** at their canonical CTDA
//! function indices, verified against TES5Edit `wbDefinitions*.pas`
//! (the same value `parse_ctda` reads from CTDA bytes 8–11; FO3 == FNV
//! for every shared function):
//!
//! | Index | Function               | Reads                       |
//! |-------|------------------------|-----------------------------|
//! | 1     | GetDistance            | `GlobalTransform`           |
//! | 14    | GetActorValue          | `ActorValues[param_1]`      |
//! | 58    | GetStage               | `QuestStageState[param_1]`  |
//! | 59    | GetStageDone           | `QuestStageState[param_1]`  |
//! | 68    | GetIsClass             | `Background.class_form_id`  |
//! | 69    | GetIsRace              | `Background.race_form_id`   |
//! | 72    | GetIsID                | `FormIdComponent`           |
//! | 73    | GetFactionRank         | `FactionRanks`              |
//! | 80    | GetLevel               | `CharacterLevel`            |
//! | 449   | HasPerk (448 on Skyrim)| `PerkList`                  |
//! | 533   | GetXPForNextLevel      | `CharacterRuleset` leveling |
//! | 573   | GetReputation          | `FactionReputation` (FNV)   |
//! | 575   | GetReputationThreshold | `FactionReputation` + bands |
//!
//! Unknown function indices evaluate to `0.0` (the Bethesda "unknown
//! function → safe-default" contract) and are logged at debug for
//! future-catalog tracking.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::{Condition, ConditionList, ConditionValue, RunOn};

/// Identifier for a condition function. Wraps the raw u32 function
/// index from `Condition.function_index` with a typed constructor so
/// the dispatcher's `match` is exhaustive against the known catalog.
///
/// New functions land by adding a variant + a match arm in
/// [`evaluate_function`]. Unknown indices fall through to
/// [`Self::Unknown`] which evaluates to `0.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionFunction {
    /// `GetActorValue(avif_form_id) → f32`. Reads the Run-On's composed value
    /// for the `param_1` actor value from its `ActorValues` component
    /// (base + permanent + temporary − damage), or 0.0 when the actor carries
    /// no such value (#1663). `param_1` is the global-space AVIF FormID.
    /// FO3 / FNV / Skyrim function index **14**.
    GetActorValue,
    /// `GetDistance(target_form_id) → f32`. Squared-distance reduces
    /// to a single sqrt at evaluation time. FO3 / FNV / Skyrim index **1**.
    GetDistance,
    /// `GetStage(quest_form_id) → f32`. Looks up the current stage
    /// for `param_1` quest in the `QuestStageState` resource.
    /// FO3 / FNV / Skyrim index 58.
    GetStage,
    /// `GetStageDone(quest_form_id, stage) → f32`. Returns 1.0 if
    /// `stage` (param_2) for `quest_form_id` (param_1) has been
    /// reached at any point, 0.0 otherwise. Bethesda's standard
    /// idempotency primitive for "this quest milestone has fired
    /// before." FO3 / FNV / Skyrim index 59.
    GetStageDone,
    /// `GetFactionRank(faction_form_id) → f32`. Reads the Run-On actor's
    /// `FactionRanks` component: the integer rank when the actor is in
    /// the faction, else -1 (Bethesda's "not in faction" sentinel — also
    /// returned when the actor carries no `FactionRanks`).
    /// FO3 / FNV / Skyrim index **73**.
    GetFactionRank,
    /// `GetLevel → f32`. The Run-On actor's character level from its
    /// `CharacterLevel` component (0.0 when absent). No parameter.
    /// FO3 / FNV / Skyrim index **80**.
    GetLevel,
    /// `GetXPForNextLevel → f32`. The experience required for the actor's
    /// next level, from the per-game `CharacterRuleset`'s `LevelingModel`
    /// (`xp_to_next(level)`); 0.0 without the ruleset resource. No parameter.
    /// The first runtime consumer of the leveling model. FNV index **533**.
    GetXPForNextLevel,
    /// `GetIsClass(class_form_id) → f32`. 1.0 when the Run-On actor's
    /// `Background.class_form_id` matches `param_1` (a `CLAS` FormID), else
    /// 0.0 (also 0.0 without `Background`). FO3 / FNV / Skyrim index **68**.
    GetIsClass,
    /// `GetIsRace(race_form_id) → f32`. 1.0 when the Run-On actor's
    /// `Background.race_form_id` matches `param_1` (a `RACE` FormID), else
    /// 0.0 (also 0.0 without `Background`). FO3 / FNV / Skyrim index **69**.
    GetIsRace,
    /// `GetIsID(base_form_id) → f32`. Returns 1.0 when the Run-On's
    /// `FormIdComponent` matches `param_1`, 0.0 otherwise. Common
    /// gate for "is this specific REFR?" checks. FO3 / FNV / Skyrim
    /// index **72**.
    GetIsID,
    /// `HasPerk(perk_form_id) → f32`. Reads the Run-On actor's `PerkList`:
    /// 1.0 when it contains `param_1`, 0.0 otherwise (also 0.0 when the
    /// actor carries no `PerkList`). Index **449** (FO3 / FNV), **448**
    /// (Skyrim) — both map here.
    HasPerk,
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
            1 => Self::GetDistance,
            14 => Self::GetActorValue,
            58 => Self::GetStage,
            59 => Self::GetStageDone,
            68 => Self::GetIsClass,
            69 => Self::GetIsRace,
            72 => Self::GetIsID,
            73 => Self::GetFactionRank,
            80 => Self::GetLevel,
            448 | 449 => Self::HasPerk,
            533 => Self::GetXPForNextLevel,
            573 => Self::GetReputation,
            575 => Self::GetReputationThreshold,
            other => Self::Unknown(other),
        }
    }

    /// Every known (non-[`Unknown`](Self::Unknown)) function — the catalog the
    /// debug console enumerates and resolves names against.
    pub const CATALOG: [ConditionFunction; 13] = [
        Self::GetDistance,
        Self::GetActorValue,
        Self::GetStage,
        Self::GetStageDone,
        Self::GetIsClass,
        Self::GetIsRace,
        Self::GetIsID,
        Self::GetFactionRank,
        Self::GetLevel,
        Self::HasPerk,
        Self::GetXPForNextLevel,
        Self::GetReputation,
        Self::GetReputationThreshold,
    ];

    /// The canonical xEdit function name (for console listing / parsing).
    pub fn name(self) -> &'static str {
        match self {
            Self::GetDistance => "GetDistance",
            Self::GetActorValue => "GetActorValue",
            Self::GetStage => "GetStage",
            Self::GetStageDone => "GetStageDone",
            Self::GetIsClass => "GetIsClass",
            Self::GetIsRace => "GetIsRace",
            Self::GetIsID => "GetIsID",
            Self::GetFactionRank => "GetFactionRank",
            Self::GetLevel => "GetLevel",
            Self::HasPerk => "HasPerk",
            Self::GetXPForNextLevel => "GetXPForNextLevel",
            Self::GetReputation => "GetReputation",
            Self::GetReputationThreshold => "GetReputationThreshold",
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
pub struct ConditionContext {
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
}

impl ConditionContext {
    /// Build a minimal context with only the subject populated. Most
    /// consumers (quest stage gates, dialogue branches) start here
    /// and add `target` per call.
    pub fn for_subject(subject: EntityId) -> Self {
        Self {
            subject,
            target: None,
            combat_target: None,
            linked_reference: None,
        }
    }

    /// Resolve a [`RunOn`] choice to a concrete EntityId. Returns
    /// `None` when the slot isn't populated or the choice references
    /// data M47.1 doesn't yet plumb (alias / package / event data).
    fn resolve(&self, run_on: RunOn, condition: &Condition, _world: &World) -> Option<EntityId> {
        match run_on {
            RunOn::Subject => Some(self.subject),
            RunOn::Target => self.target,
            RunOn::CombatTarget => self.combat_target,
            RunOn::LinkedReference => self.linked_reference,
            RunOn::Reference => {
                // FormID→EntityId resolver not yet wired: condition.reference_form_id
                // is a raw u32 ESM form ID; find_by_form_id requires an interned FormId.
                // Returns None until a u32→FormId pool lookup is plumbed here.
                log::trace!(
                    "M47.1: RunOn::Reference for form_id {:08X} — \
                     FormID→EntityId resolver not yet wired",
                    condition.reference_form_id,
                );
                None
            }
            RunOn::QuestAlias | RunOn::PackageData | RunOn::EventData => {
                log::trace!(
                    "M47.1: RunOn::{:?} (extra_data_id={:08X}) — \
                     alias / package / event resolvers deferred",
                    run_on,
                    condition.extra_data_id,
                );
                None
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
    let function_result = evaluate_function(function, condition, entity, world);

    // Resolve comparand — Globals route through the `Globals` resource
    // (mirrored from `EsmIndex.globals`, #1668). `form_id` is already in
    // global load-order space (remapped at CTDA parse time), matching the
    // resource's key space. A missing resource or unknown GLOB resolves to
    // 0.0 — Bethesda's "missing GLOB defaults to 0".
    let comparand = match condition.comparand {
        ConditionValue::Literal(v) => v,
        ConditionValue::Global(form_id) => world
            .try_resource::<crate::globals::Globals>()
            .and_then(|g| g.get(form_id))
            .unwrap_or(0.0),
    };

    condition.comparator.apply(function_result, comparand)
}

/// Resolve a global-load-order FormID to the entity that carries it.
///
/// `World::find_by_form_id` keys by an interned [`FormId`] handle; a remapped
/// CTDA `param_1` is a raw global `u32`, so this resolves through the
/// [`FormIdPool`] instead — an entity matches when its `FormIdComponent`
/// resolves to a pair whose `local` equals `form_id` (the cell loader stores
/// the full global FormID as the `LocalFormId`). O(n) over entities carrying
/// a `FormIdComponent`, which is fine for the rare condition-eval path.
fn resolve_entity_by_global_form_id(world: &World, form_id: u32) -> Option<EntityId> {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::FormIdPool;
    let pool = world.try_resource::<FormIdPool>()?;
    let q = world.query::<FormIdComponent>()?;
    let found = q
        .iter()
        .find(|(_, fid)| pool.resolve(fid.0).is_some_and(|p| p.local.0 == form_id))
        .map(|(eid, _)| eid);
    found
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
    match function {
        ConditionFunction::GetActorValue => {
            // GetActorValue(avif_form_id) → the Run-On actor's composed value
            // for that actor value (base + permanent + temporary − damage), or
            // 0.0 when the actor carries no `ActorValues` (or hasn't that value
            // set — Bethesda's absent-AV default). `param_1` is the AVIF FormID,
            // already promoted to global load-order space at parse time
            // (function 14 is in the CTDA form-id-param list, #1666), the same
            // space `ActorValues` is keyed in — a direct lookup, no FormIdPool
            // hop (the key IS the AV's id, not the actor's identity). #1663.
            use byroredux_core::character::{CharacterLevel, CharacterRuleset, DerivedScope};
            use byroredux_core::ecs::components::ActorValues;
            let Some(avs) = world.get::<ActorValues>(entity) else {
                return 0.0; // no `ActorValues` → absent-AV default
            };
            // A carried value wins — populated SPECIAL/skills, baked FO4
            // Health/AP, perk/effect modifiers.
            if avs.get(condition.param_1).is_some() {
                return avs.current(condition.param_1);
            }
            // Absent → if this game *derives* the stat actor-generally (Carry
            // Weight / Melee Damage / Crit Chance / Unarmed Damage from
            // SPECIAL/skills), compute it from the per-game `CharacterRuleset`.
            // Player-only stats (Health/AP) stay at the absent default for an
            // arbitrary actor — NPCs bake them, the player isn't modelled yet.
            if let Some(rs) = world.try_resource::<CharacterRuleset>() {
                if let Some(formula) = rs.derived_formula(condition.param_1) {
                    if formula.scope == DerivedScope::ActorGeneral {
                        let level = world.get::<CharacterLevel>(entity).map_or(0, |l| l.level);
                        return formula.eval(&avs, level);
                    }
                }
            }
            0.0
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
        ConditionFunction::GetLevel => {
            // GetLevel → the Run-On actor's character level (0.0 when the
            // actor carries no `CharacterLevel`).
            use byroredux_core::character::CharacterLevel;
            world
                .get::<CharacterLevel>(entity)
                .map_or(0.0, |l| f32::from(l.level))
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
        ConditionFunction::GetIsClass => {
            // GetIsClass(class_form_id) → 1.0 iff the actor's `Background.class`
            // matches `param_1` (a remapped `CLAS` FormID). Compared in the
            // actor's stored space — identity-equal to a remapped `param_1` in
            // single-plugin loads, same contract as `GetFactionRank`.
            use byroredux_core::character::Background;
            world.get::<Background>(entity).map_or(0.0, |b| {
                if b.class_form_id == condition.param_1 {
                    1.0
                } else {
                    0.0
                }
            })
        }
        ConditionFunction::GetIsRace => {
            // GetIsRace(race_form_id) → 1.0 iff the actor's `Background.race`
            // matches `param_1` (a remapped `RACE` FormID). Same space contract
            // as `GetIsClass`.
            use byroredux_core::character::Background;
            world.get::<Background>(entity).map_or(0.0, |b| {
                if b.race_form_id == condition.param_1 {
                    1.0
                } else {
                    0.0
                }
            })
        }
        ConditionFunction::GetIsID => {
            // Test the Run-On entity's identity against `param_1`. The CTDA
            // form-id remap (#1666, applied at parse time in the plugin crate)
            // has already promoted `param_1` into global load-order space — the
            // same space the entity's `FormIdComponent` resolves to via
            // `FormIdPool` — so this is a direct, false-positive-free compare
            // across multi-plugin loads (no lower-24-bits shortcut).
            use byroredux_core::ecs::components::FormIdComponent;
            use byroredux_core::form_id::FormIdPool;
            let Some(fid_comp) = world.get::<FormIdComponent>(entity) else {
                return 0.0;
            };
            let Some(pool) = world.try_resource::<FormIdPool>() else {
                return 0.0;
            };
            // `local` carries the full global FormID — the cell loader stores
            // the remapped placement/base id as the LocalFormId
            // (references.rs), so `pair.local.0` is directly comparable.
            match pool.resolve(fid_comp.0) {
                Some(pair) if pair.local.0 == condition.param_1 => 1.0,
                _ => 0.0,
            }
        }
        ConditionFunction::HasPerk => {
            // 1.0 iff the Run-On actor's `PerkList` holds `param_1`. Same
            // remap contract as GetIsID: `param_1` is global, and each perk
            // FormId resolves through `FormIdPool` to its global FormIdPair.
            use byroredux_core::ecs::components::PerkList;
            use byroredux_core::form_id::FormIdPool;
            let Some(perks) = world.get::<PerkList>(entity) else {
                return 0.0;
            };
            let Some(pool) = world.try_resource::<FormIdPool>() else {
                return 0.0;
            };
            let held = perks.0.iter().any(|&perk| {
                pool.resolve(perk)
                    .is_some_and(|pair| pair.local.0 == condition.param_1)
            });
            if held {
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
    if conditions.is_empty() {
        return true; // "no conditions = always fires" contract
    }

    let mut i = 0usize;
    while i < conditions.len() {
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
        // terminate the block at its last real member — `while i < len`
        // guarantees `len >= 1`, so the subtraction can't underflow.
        let block_end_inclusive = i.min(conditions.len() - 1);
        i += 1; // step past the block for next iteration

        // Evaluate the block. Single-condition blocks (no preceding
        // OR flag) reduce to one evaluation; multi-condition blocks
        // are OR-combined with short-circuit.
        let block_result = (block_start..=block_end_inclusive)
            .any(|j| evaluate_condition(&conditions[j], world, ctx));
        if !block_result {
            // AND-combine with the surrounding chain — false block
            // fails the whole list.
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::world::World;
    use byroredux_plugin::esm::records::condition::{ComparisonOp, RunOn};

    fn ctx(subject: EntityId) -> ConditionContext {
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

    #[test]
    fn get_is_id_matches_entity_global_form_id() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

        let mut world = World::new();
        let mut pool = FormIdPool::new();
        // The cell loader stores the full global FormID as the LocalFormId,
        // so `param_1` (also global, post-remap) compares directly.
        let pair = FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x0001_4D8A),
        };
        let fid = pool.intern(pair);
        world.insert_resource(pool);
        let actor = world.spawn();
        world.insert(actor, FormIdComponent(fid));

        // GetIsID(0x00014D8A) == 1 — matches the entity's id.
        let list = vec![cond(72, ComparisonOp::Eq, 1.0, false).with_param_1(0x0001_4D8A)];
        assert!(evaluate(&list, &world, &ctx(actor)));

        // A different id → 0.
        let list = vec![cond(72, ComparisonOp::Eq, 0.0, false).with_param_1(0x0001_9999)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    #[test]
    fn get_is_id_zero_without_form_id_component() {
        // No FormIdComponent (and no FormIdPool) → GetIsID returns 0.0.
        let world = World::new();
        let actor: EntityId = 7;
        let list = vec![cond(72, ComparisonOp::Eq, 0.0, false).with_param_1(0x0001_4D8A)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    // ── HasPerk (#1667) ─────────────────────────────────────────────────

    #[test]
    fn has_perk_checks_actor_perk_list() {
        use byroredux_core::ecs::components::PerkList;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

        let mut world = World::new();
        let mut pool = FormIdPool::new();
        let held = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(0x0005_8F80),
        });
        world.insert_resource(pool);
        let actor = world.spawn();
        world.insert(actor, PerkList::from_perks([held]));

        // HasPerk(0x00058F80) == 1 — the actor holds it.
        let list = vec![cond(449, ComparisonOp::Eq, 1.0, false).with_param_1(0x0005_8F80)];
        assert!(evaluate(&list, &world, &ctx(actor)));

        // HasPerk(0x00058F81) == 0 — not held.
        let list = vec![cond(449, ComparisonOp::Eq, 0.0, false).with_param_1(0x0005_8F81)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    #[test]
    fn has_perk_zero_without_perk_list() {
        // No PerkList component → HasPerk returns 0.0.
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
        assert!(evaluate(&list, &world, &ctx(actor)), "Carry Weight derived from SPECIAL");

        // Health is player-only → NOT computed for an NPC; absent default 0.
        let list = vec![cond(14, ComparisonOp::Eq, 0.0, false).with_param_1(0x2C9)];
        assert!(evaluate(&list, &world, &ctx(actor)), "Health stays 0 (player-only)");
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
}
