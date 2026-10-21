//! #5804 — the condition evaluator, split out of `condition.rs` (see that
//! hub's module docs for the OR-precedence semantics): `evaluate_function`
//! and its per-family helpers, `evaluate_contextual`, and the
//! [`evaluate`] list walk.

use std::ops::RangeInclusive;

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::{
    Condition, ConditionList, ConditionValue, RunOn,
};

mod actor;
mod global;
mod reference;

use super::catalog::{
    index_is_foreign_in, ConditionFunction,
};
use super::context::ConditionContext;

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
    if index_is_foreign_in_loaded_game(world, condition.function_index) {
        // The index names another function in this game (#5990): not the
        // catalog row's — pass it, as every uncatalogued function does.
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

fn loaded_game(world: &World) -> Option<byroredux_plugin::esm::reader::GameKind> {
    world.try_resource::<crate::ScriptGame>().map(|game| game.0)
}

/// Whether CTDA function `index` is another function in the loaded game than
/// the catalog's row for it (#5990). `false` when no game is installed.
fn index_is_foreign_in_loaded_game(world: &World, index: u32) -> bool {
    loaded_game(world).is_some_and(|game| index_is_foreign_in(index, game))
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
/// function is catalogued, an index that is a different function in another
/// game (79, and the [`index_is_foreign_in`] set) is catalogued for the loaded
/// one, and an authored `GetActorValue` index translates to a loaded AVIF (#5989). The fail-closed gates (dialogue
/// quest gates, Story Manager nodes, perk entries, intrusive packages) decline a
/// condition for which this is `false`.
pub fn is_modeled(world: &World, condition: &Condition) -> bool {
    if index_is_foreign_in_loaded_game(world, condition.function_index) {
        return false;
    }
    match ConditionFunction::from_index(condition.function_index) {
        ConditionFunction::Unknown(_) => false,
        ConditionFunction::GetQuestVariable => uses_obscript_quest_variables(world),
        // An authored enum index is readable once it translates to a loaded
        // AVIF (#5989); one that cannot (Oblivion has no AVIFs, an index the
        // record set lacks) is declined by the fail-closed gates.
        ConditionFunction::GetActorValue => {
            crate::actor_value_names::condition_actor_value_form(world, condition.param_1).is_some()
        }
        _ => true,
    }
}

/// The raw result of CTDA function `function_index`, for a legacy script
/// *getter command* (M47.3 phase 2): an ObScript `<ref>.GetDead` /
/// `GetDistance <ref>` is the same function a CTDA names, and its command id
/// is `0x1000 + function index` (checked by source alignment over vanilla
/// Oblivion, FO3 and FNV: `GetDead` 46 = `0x102e`, `GetInCell` 67 = `0x1043`,
/// `GetFactionRank` 73 = `0x1049`, `GetReputationThreshold` 575 = `0x123f`).
///
/// `None` when the evaluator cannot honestly answer in this world — the
/// function is uncatalogued, is another function in the loaded game
/// ([`is_modeled`]), or needs a subject reference and none was resolved — so
/// the caller declines the command instead of reading a made-up `0.0`.
/// `subject` is the object the command was invoked on; a function that does
/// not read its subject (`GetStage`, `GetRandomPercent`, …) ignores it.
pub fn evaluate_script_getter(
    world: &World,
    function_index: u32,
    param_1: u32,
    param_2: u32,
    subject: Option<EntityId>,
) -> Option<f32> {
    let condition = Condition {
        function_index,
        param_1,
        param_2,
        ..Condition::default()
    };
    if !is_modeled(world, &condition) {
        return None;
    }
    let function = ConditionFunction::from_index(function_index);
    let entity = if function.reads_run_on_entity() {
        subject?
    } else {
        subject.unwrap_or(0)
    };
    Some(evaluate_function(function, &condition, entity, world))
}

/// The value a condition's function result is compared against.
///
/// Globals route through the `Globals` resource (mirrored from
/// `EsmIndex.globals`, #1668). `form_id` is already in global load-order
/// space (remapped at CTDA parse time), matching the resource's key space. A
/// missing resource or unknown GLOB resolves to 0.0 — Bethesda's "missing
/// GLOB defaults to 0".
pub(crate) fn comparand_value(condition: &Condition, world: &World) -> f32 {
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
    if form_id == crate::PLAYER_REFERENCE {
        return crate::papyrus_demo::PapyrusPlayerEntity::get(world);
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
/// #5979 — including for the player: its body's pool pair is
/// `PLAYER_FORM_ID_PAIR` (local 1), but a script, a ledger or a saved key
/// names the player `PlayerRef` (`0x14`), the identity
/// [`resolve_entity_by_global_form_id`] maps back. The player used to be
/// special-cased by every FormID-keyed writer; a new one that forgot keyed the
/// player under the wrong id.
///
/// #3278 — needed by `Effect::Disable`, whose sink (`ReferenceEnableState`)
/// is deliberately FormID-keyed rather than entity-keyed so a disable
/// survives its reference's cell being unloaded; the location runtime keys
/// a reference's editor location the same way.
pub fn entity_global_form_id(world: &World, entity: EntityId) -> Option<u32> {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::FormIdPool;
    if crate::papyrus_demo::PapyrusPlayerEntity::get(world) == Some(entity) {
        return Some(crate::PLAYER_REFERENCE);
    }
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
///
/// #5804 — one exhaustive `match`, so a new variant still fails to compile
/// until it is dispatched; any arm longer than a line lives by evaluator
/// family in `eval/{global,actor,reference}.rs`.
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
        // `param_1` is an AVIF FormID in FO4+ and for engine-built conditions,
        // an actor-value enum index in authored Oblivion / FO3 / FNV / Skyrim
        // data; an index that names no loaded AVIF reads the absent-AV default
        // (#5989).
        ConditionFunction::GetActorValue => {
            crate::actor_value_names::condition_actor_value_form(world, condition.param_1)
                .map_or(0.0, |avif| actor_value_view(world, entity, avif, ActorValueView::Current))
        }
        ConditionFunction::GetQuestRunning => global::get_quest_running(condition, world),
        ConditionFunction::GetGlobalValue => global::get_global_value(condition, world),
        ConditionFunction::GetRandomPercent => global::get_random_percent(world),
        ConditionFunction::GetDistance => reference::get_distance(condition, entity, world),
        ConditionFunction::GetRelationshipRank => actor::get_relationship_rank(condition, entity, world),
        ConditionFunction::GetIsGhost => truth(crate::is_ghost(world, entity)),
        ConditionFunction::GetIsPlayableRace => actor::get_is_playable_race(entity, world),
        ConditionFunction::GetPlayerInSEWorld => crate::obscript_more::player_in_se_world(world),
        ConditionFunction::GetIsSex => truth(
            crate::actor_base_traits::is_female(world, entity)
                .is_some_and(|female| female == (condition.param_1 == 1)),
        ),
        ConditionFunction::GetInWorldspace => truth(
            crate::cell_kind::in_worldspace(world, entity, condition.param_1).unwrap_or(false),
        ),
        ConditionFunction::HasKeyword => {
            truth(crate::keywords::has_keyword(world, entity, condition.param_1))
        }
        ConditionFunction::IsEssential => truth(crate::bleedout::is_essential(world, entity)),
        ConditionFunction::IsProtected => truth(crate::bleedout::is_protected(world, entity)),
        ConditionFunction::IsBleedingOut => {
            truth(crate::bleedout::is_bleeding_out(world, entity))
        }
        ConditionFunction::GetPlayerTeammate => actor::get_player_teammate(entity, world),
        ConditionFunction::GetDead => actor::get_dead(entity, world),
        ConditionFunction::GetQuestVariable => global::get_quest_variable(condition, world),
        ConditionFunction::GetStage => global::get_stage(condition, world),
        ConditionFunction::GetStageDone => global::get_stage_done(condition, world),
        ConditionFunction::GetInCell => reference::get_in_cell(condition, entity, world),
        ConditionFunction::GetFactionRank => actor::get_faction_rank(condition, entity, world),
        ConditionFunction::GetInFaction => actor::get_in_faction(condition, entity, world),
        ConditionFunction::GetLevel => actor::get_level(entity, world),
        ConditionFunction::GetEquipped => actor::get_equipped(condition, entity, world),
        ConditionFunction::GetXPForNextLevel => actor::get_xp_for_next_level(entity, world),
        ConditionFunction::IsSceneActionComplete => global::is_scene_action_complete(condition, world),
        ConditionFunction::GetIsClass
        | ConditionFunction::GetIsRace
        | ConditionFunction::GetIsID => {
            if function.run_on_identity(entity, world) == Some(condition.param_1) {
                1.0
            } else {
                0.0
            }
        }
        ConditionFunction::HasLoaded3D => reference::has_loaded_3d(entity, world),
        ConditionFunction::IsHardcore => global::is_hardcore(world),
        ConditionFunction::HasPerk => actor::has_perk(condition, entity, world),
        ConditionFunction::GetReputation => actor::get_reputation(condition, entity, world),
        ConditionFunction::GetReputationThreshold => actor::get_reputation_threshold(condition, entity, world),
        ConditionFunction::GetVMScriptVariable => global::get_vm_script_variable(condition, world),
        ConditionFunction::GetVMQuestVariable => global::get_vm_quest_variable(condition, world),
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

/// [`evaluate`] for a consumer that *acts* on a pass: it declines (`false`) on any
/// term the evaluator cannot honestly read ([`is_modeled`]) instead of letting the
/// absent-value default `0.0` satisfy an `== 0` / `<=` comparison and run what the
/// authored gate was meant to hold back (#5993). The same fail-closed rule the
/// dialogue, Story Manager, perk and package gates apply term by term.
pub fn evaluate_modeled(conditions: &ConditionList, world: &World, ctx: &ConditionContext) -> bool {
    conditions.iter().all(|condition| is_modeled(world, condition)) && evaluate(conditions, world, ctx)
}

/// The OR blocks of `conditions`, in order, as index ranges — the units
/// [`evaluate`] AND-combines.
pub(crate) fn or_blocks(
    conditions: &[Condition],
) -> impl Iterator<Item = RangeInclusive<usize>> + '_ {
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
