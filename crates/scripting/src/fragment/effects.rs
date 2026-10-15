//! The effect interpreter: reference resolution plus `apply_effect` and its
//! quest-scoped and batch wrappers.
//!
//! Split out of `fragment.rs` (#3854); the target resolvers, deferred queue,
//! per-family appliers and value evaluation live in `effects/*` (#5468) and are
//! re-exported here, so this file keeps `apply_effect`, `apply_effects` and the
//! shared transform helper.

use super::*;
use crate::condition::entity_global_form_id;
use crate::translate::effects::{FragmentGuard, ValueExpr};

mod resolve;
mod deferred;
mod inventory;
mod placement;
mod scene;
mod lock;
mod player_control;
mod vehicle_cinematic;
mod ai_combat;
mod quest_scoped;
mod evaluate;

pub(crate) use resolve::*;
pub use deferred::*;
pub use inventory::*;
use placement::*;
use scene::*;
use lock::*;
use player_control::*;
use vehicle_cinematic::*;
use ai_combat::*;
use quest_scoped::*;
pub(in crate::fragment) use evaluate::*;

/// Read one entity's [`Transform`] as an owned copy, holding no guard on
/// return (#3250).
pub(crate) fn copied_transform(world: &World, entity: EntityId) -> Option<Transform> {
    // #3250 — `World::get` returns an owning read guard. Copy the component
    // before another Transform lookup so a queued writer can never strand a
    // recursive read behind the first guard.
    world.get::<Transform>(entity).map(|transform| *transform)
}

/// Apply one effect to the canonical stage/objective state (or, for the
/// object-targeting variants, to the live ECS world). Returns a
/// [`QuestStageAdvanced`] when the effect was a `SetStage` (so the caller
/// can cascade), or `None` otherwise / when a target can't resolve.
///
/// **Nested-lock safety depends on exclusive scheduling.** What this
/// function runs *inside* is one `resource_2_mut::<QuestStageState,
/// QuestObjectiveState>()` taken by [`apply_fragment_guard_free`] — a
/// single TypeId-sorted paired acquisition, scoped to one fragment's
/// effects and dropped before `deferred.apply(world)` runs. It is NOT held
/// across the cascade loop, and `QuestStageFragments` is not held at all:
/// [`quest_fragment_dispatch_system`] clones it before taking any quest
/// resource, precisely to avoid a read→write nested order.
///
/// Everything below is therefore nested under those two guards and nothing
/// else. Listed by where it is taken rather than as a hand count, because
/// the count is what rotted (#3949 — the previous version of this block
/// said "12 component-storage acquisitions" and named three resources that
/// are no longer all held together):
///
///   - **directly in the match arms** — `Globals` (write, `SetGlobalValue`),
///     `PlayerControlState` (write ×3, the `SetPlayerControls` family),
///     `EquipItemCatalog` (read), `SceneRegistry` (read),
///     `PapyrusPlayerEntity` (read), and the component storages
///     `Inventory`, `Transform` (read and write), `GlobalTransform`,
///     `ActorControlState`, `EvaluatePackageRequest`, `HorseTetherState`,
///     `MotionTypeChangeRequest` (×2), `SceneStartRequest`,
///     `SceneStopRequest`, `Locked` (write ×2, the `SetLocked` /
///     `SetLockLevel` pair — #3159), `AiCombatState` (write,
///     `Effect::StartCombat` — MQ101's dragon-attack/keep-escape combat
///     gate), `AmbientEngagement` (write, `Effect::StartCombat` taking
///     ownership of an ambient fight — #5046; taken after the
///     `AiCombatState` guard drops), `PlayerMoveRequest` (write,
///     `Effect::MovePlayerTo` — M47.2; taken alone, the engine drains it),
///     `Perks` (write, `Effect::ChangePerk` — M47.2), `ActorValueNames`
///     (read, released before `ActorValues`, `Effect::SetActorValueNamed` and
///     `Effect::DamageActorValue` — M47.2), `SceneAliasCandidate` (read, `Effect::SetBaseProtection`'s
///     actor base — M47.2), `EditorLocation` (read,
///     `Effect::MoveToEditorLocation` — M47.2; copied out and released
///     before the `Transform` write)
///   - **via [`resolve_actor`]** — `PapyrusPlayerEntity` (read)
///   - **via [`crate::condition::entity_global_form_id`]** — `FormIdPool` (read)
///   - **via [`update_actor_cinematic_state`]** — `ActorCinematicState`
///   - **via `crate::magic::{add_spell, remove_spell}`** (the
///     `AddSpell`/`RemoveSpell` arms, applied inline rather than deferred —
///     no cycle: every quest-resource system is exclusive) — `SpellList`
///     (write), then through `apply_constant_modifiers`:
///     `SpellCatalog` (read) and `ActorValues` (write), each released
///     before the next is taken
///   - **via `crate::magic::{add_race_spells, remove_race_spells}`** (the
///     #4415 racial pair, same inline shape) — `RaceSpells` (read, cloned
///     out and released) plus the `add_spell`/`remove_spell` chain above
///     once per racial spell
///
/// so ~19 distinct storage/resource types across ~24 sites. "Directly in
/// the match arms" is now one delegation hop away: #4340 moved each family
/// of arms into an `apply_<family>_effect` helper below (globals,
/// inventory, placement, scene, lock ledger, player control, vehicle
/// cinematics, AI/combat), plus the pre-existing [`resolve_actor`],
/// [`crate::condition::entity_global_form_id`], [`update_actor_cinematic_state`] and
/// [`apply_quest_scoped_effect`]. Nothing about the nesting changed — the
/// helpers run inside the same two guards, this list still inventories
/// every lock any of them takes, and the #3949 scan below reads their
/// bodies together with this function's. The `crate::magic` call-outs are
/// one hop *further* away (a different module, which the fragment-body
/// scan is structurally blind to — #5071), so the scan also reads the
/// five `magic.rs` functions those arms reach; the deferral option was
/// declined because the arms are already exclusive-lane-safe and deferring
/// would change ordering semantics P4 relies on. The
/// `FragmentExecutionQueue`
/// write is *not* in this list: it belongs to the latent-continuation path
/// in the dispatch system, outside these guards.
///
/// This is only safe because every system that touches the quest resources
/// is registered `add_exclusive` in `byroredux/src/boot/schedule/` (parallel
/// systems never run concurrently with an exclusive one), so no other
/// holder can ever form the other half of an ABBA cycle. Adding a new
/// nested component/resource lock here, or moving
/// [`quest_fragment_dispatch_system`] (or any sibling quest-resource
/// system) onto the parallel lane, needs the same analysis re-derived —
/// see SCR-D6-NEW3-03 / #2126, and record the change under the house-rule
/// doc #2270 asks for.
///
/// `SceneActorBindings` used to be part of this residual list too (every
/// `resolve_object` alias lookup read it live) but is now resolved through
/// the pre-lock snapshot in `deferred` instead (#2660) — see
/// [`DeferredFragmentEffects`]'s doc for why a snapshot is safe here.
/// Quest-definition reads use the same snapshot, and presentation /
/// alias-binding *mutations* are appended to it rather than acquired here.
/// Every production caller constructs the batch before taking quest
/// resource guards and applies it only after those guards have dropped
/// (#2269, #2539, #2660).
pub(crate) fn apply_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &mut QuestStageState,
    objectives: &mut QuestObjectiveState,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::SetGlobalValue { global, value } => {
            // Read first: the value may read `Globals` itself (`G.Mod(x)`).
            let value = evaluate_value(value, context, vmad, world, stages, deferred)?;
            let form_id = resolve_property_form_id(vmad, global.property_name())?;
            if let Some(mut globals) = world.try_resource_mut::<crate::Globals>() {
                globals.set(form_id, value);
            } else {
                log::debug!(
                    "fragment SetValue skipped: Globals resource is unavailable for {form_id:08X}"
                );
            }
            None
        }
        Effect::AddItem { .. } | Effect::RemoveItem { .. } | Effect::EquipItem { .. } => {
            apply_inventory_effect(effect, context, vmad, world, stages, deferred)
        }
        Effect::MoveTo { .. }
        | Effect::MoveToEditorLocation { .. }
        | Effect::MovePlayerTo { .. }
        | Effect::Disable { .. }
        | Effect::Enable { .. } => {
            apply_placement_effect(effect, context, vmad, world, deferred)
        }
        Effect::StartScene { .. }
        | Effect::StopScene { .. }
        | Effect::Activate { .. }
        | Effect::SetOpen { .. } => {
            apply_scene_effect(effect, context, vmad, world, stages, deferred)
        }
        Effect::SetLocked { .. } | Effect::SetLockLevel { .. } => {
            apply_lock_effect(effect, context, vmad, world, deferred)
        }
        Effect::SetPlayerRestrained { .. }
        | Effect::SetPlayerControls { .. }
        | Effect::SetPlayerAiDriven { .. }
        | Effect::SetHudCartMode { .. }
        | Effect::SetSittingRotation { .. }
        | Effect::SetInChargen { .. }
        | Effect::ShowRaceMenu
        | Effect::RequestSave { .. }
        | Effect::RegisterPlayerAnimationEvent { .. } => {
            apply_player_control_effect(effect, context, vmad, world, deferred)
        }
        Effect::PlayIdle { .. }
        | Effect::SetVehicle { .. }
        | Effect::TetherToHorse { .. }
        | Effect::SetMotionType { .. }
        | Effect::ExitCart { .. } => {
            apply_vehicle_cinematic_effect(effect, context, vmad, world, deferred)
        }
        Effect::SendStoryEvent {
            keyword,
            location,
            references,
            values,
        } => {
            let keyword = resolve_property_form_id(vmad, keyword.property_name())?;
            log::debug!(
                "fragment SendStoryEvent: quest {:08X} raises SCPT keyword {keyword:08X}",
                context.0
            );
            // A location property, or a location alias's bound location.
            let location = match location {
                None => None,
                Some(location) => resolve_location(vmad, world, context, location),
            };
            let [first, second] = references
                .clone()
                .map(|actor| actor.and_then(|actor| guard_actor(&actor, context, vmad, world, deferred)));
            // `R1` holds an entity: the first reference, else the player
            // (flagged absent).
            let carrier = first.or_else(|| {
                world
                    .try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
                    .map(|player| player.0)
            })?;
            deferred.story_events.push(
                crate::story_manager::StoryEvent {
                    mnemonic: *b"SCPT",
                    reference_1: carrier,
                    reference_2: second,
                    location_1: location,
                    location_2: None,
                    scripted: Some(crate::story_manager::ScriptedStoryEvent {
                        keyword,
                        values: *values,
                        has_reference_1: first.is_some(),
                    }),
                },
            );
            None
        }
        Effect::AddToMap { marker, fast_travel } => {
            let Some(reference) = object_form_id(vmad, world, context, marker, deferred) else {
                log::debug!("fragment AddToMap skipped: the marker does not resolve");
                return None;
            };
            deferred.map_markers.push((reference, *fast_travel));
            None
        }
        Effect::ShowMessage { message, args } => {
            let form_id = resolve_property_form_id(vmad, message.property_name())?;
            deferred.messages.push(PendingMessage::Show(form_id, args.clone()));
            None
        }
        Effect::ShowHelpMessage { message } => {
            let form_id = resolve_property_form_id(vmad, message.property_name())?;
            deferred.messages.push(PendingMessage::Show(form_id, Vec::new()));
            None
        }
        Effect::Notification { text } => {
            deferred.messages.push(PendingMessage::Notify(text.clone()));
            None
        }
        Effect::DeleteWhenAble { object } => {
            let form_id = object_form_id(vmad, world, context, object, deferred)?;
            deferred.deletions.push(form_id);
            None
        }
        Effect::ShowBarterMenu { vendor } => {
            let vendor = guard_actor(vendor, context, vmad, world, deferred)?;
            deferred.barter = Some(vendor);
            None
        }
        Effect::ChangeKeyword {
            object,
            keyword,
            change,
        } => {
            let object = guard_actor(object, context, vmad, world, deferred)?;
            let keyword = resolve_property_form_id(vmad, keyword.property_name())?;
            deferred.keyword_changes.push((object, keyword, *change));
            None
        }
        Effect::SetReferenceVariable {
            object,
            variable,
            value,
        } => {
            let Some(entity) =
                resolve_object(vmad, world, context, object, &deferred.scene_actor_bindings)
            else {
                log::debug!("fragment reference variable write skipped: the reference is not loaded");
                return None;
            };
            deferred
                .reference_variables
                .push((entity, variable.clone(), *value));
            None
        }
        Effect::SetLocal { local, value } => {
            let value = evaluate_value(value, context, vmad, world, stages, deferred)?;
            deferred.locals.insert(local.clone(), value);
            None
        }
        Effect::RewardPlayerXp { amount, direct } => {
            let amount = resolve_item_count(amount, context, vmad, world, stages, deferred)?;
            deferred.player_xp.push((amount, *direct));
            None
        }
        Effect::SetPlayerTeammate {
            actor,
            teammate,
            can_do_favor,
        } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let form = crate::condition::entity_global_form_id(world, actor)?;
            deferred.follower_changes.push((
                form,
                FollowerChange::Teammate {
                    teammate: *teammate,
                    can_do_favor: *can_do_favor,
                },
            ));
            None
        }
        Effect::SetDoingFavor { actor, doing } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let form = crate::condition::entity_global_form_id(world, actor)?;
            deferred.follower_changes.push((form, FollowerChange::DoingFavor(*doing)));
            None
        }
        Effect::SetLinkedRef {
            object,
            target,
            keyword,
        } => {
            let source = object_form_id(vmad, world, context, object, deferred)?;
            let target = match target {
                Some(target) => object_form_id(vmad, world, context, target, deferred)?,
                None => 0,
            };
            let keyword = match keyword {
                Some(keyword) => resolve_property_form_id(vmad, keyword.property_name())?,
                None => 0,
            };
            deferred.linked_ref_changes.push((source, keyword, target));
            None
        }
        Effect::SetLocRefType { object, ref_type } => {
            let reference = object_form_id(vmad, world, context, object, deferred)?;
            let ref_type = match ref_type {
                Some(ref_type) => resolve_property_form_id(vmad, ref_type.property_name())?,
                None => 0,
            };
            deferred.loc_ref_type_changes.push((reference, ref_type));
            None
        }
        Effect::AdvanceSkill { skill, magnitude } => {
            deferred.skill_advances.push((skill.clone(), *magnitude));
            None
        }
        Effect::AddAchievement { id } => {
            deferred.achievements.push(*id);
            None
        }
        Effect::IncrementStat { stat, amount } => {
            deferred.misc_stats.push((stat.clone(), *amount));
            None
        }
        Effect::PlayerCaps { delta } => {
            // `Game.GetCaps()` is `Game.GetForm(15)` in FO4's `Game.psc`.
            const CAPS: u32 = 0x0000_000F;
            let player = world
                .try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
                .map(|player| player.0)?;
            match delta.cmp(&0) {
                std::cmp::Ordering::Greater => add_items(world, player, CAPS, *delta as u32),
                std::cmp::Ordering::Less => {
                    remove_items(world, player, CAPS, delta.unsigned_abs(), None);
                }
                std::cmp::Ordering::Equal => {}
            }
            None
        }
        Effect::SetRelationshipRank { actor, other, rank } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let other = guard_actor(other, context, vmad, world, deferred)?;
            let (Some(actor), Some(other)) = (
                crate::relationships::actor_base(world, actor),
                crate::relationships::actor_base(world, other),
            ) else {
                log::debug!("fragment SetRelationshipRank skipped: an actor has no base");
                return None;
            };
            deferred
                .relationship_changes
                .push((crate::relationships::pair(actor, other), *rank));
            None
        }
        Effect::Kill { actor, killer, silent } => {
            let victim = guard_actor(actor, context, vmad, world, deferred)?;
            let killer = killer
                .as_ref()
                .and_then(|killer| guard_actor(killer, context, vmad, world, deferred));
            deferred.kills.push((victim, killer, *silent));
            None
        }
        Effect::SetBaseProtection {
            base,
            protected,
            value,
        } => {
            let base = match base {
                crate::translate::effects::ActorBaseRef::OfActor(actor) => {
                    let actor = guard_actor(actor, context, vmad, world, deferred)?;
                    world
                        .get::<crate::scene::SceneAliasCandidate>(actor)
                        .map(|candidate| candidate.base_form_id)
                }
                crate::translate::effects::ActorBaseRef::Form(form) => {
                    resolve_property_form_id(vmad, form.property_name())
                }
            };
            let Some(base) = base else {
                log::debug!("fragment SetEssential / SetProtected skipped: no actor base");
                return None;
            };
            deferred.base_protection.push((base, *protected, *value));
            None
        }
        Effect::SetLocationCleared { location, cleared } => {
            let Some(location) = resolve_location(vmad, world, context, location) else {
                log::debug!("fragment SetCleared skipped: the location does not resolve");
                return None;
            };
            deferred.location_clears.push((location, *cleared));
            None
        }
        Effect::BlockActivation { object, blocked } => {
            let Some(reference) = object_form_id(vmad, world, context, object, deferred) else {
                log::debug!("fragment BlockActivation skipped: the reference does not resolve");
                return None;
            };
            deferred.activation_blocks.push((reference, *blocked));
            None
        }
        Effect::ForceLocationTo { alias, location } => {
            let name = alias.property_name();
            let Some((quest, alias_id)) = alias_property(vmad, context, name) else {
                log::debug!("fragment ForceLocationTo skipped: '{name}' is not an alias property");
                return None;
            };
            let location = resolve_property_form_id(vmad, location.property_name())?;
            deferred
                .alias_overrides
                .push((quest, alias_id, crate::AliasOverride::ForcedLocation(location)));
            deferred.scene_actor_bindings_dirty = true;
            None
        }
        Effect::ForceAlias { alias, target } => {
            let name = alias.property_name();
            let Some((quest, alias_id)) = alias_property(vmad, context, name) else {
                log::debug!("fragment ForceRefTo/Clear skipped: '{name}' is not an alias property");
                return None;
            };
            let value = match target {
                None => crate::AliasOverride::Cleared,
                Some(target) => {
                    crate::AliasOverride::Forced(actor_reference_form_id(target, context, vmad, world, deferred)?)
                }
            };
            deferred.alias_overrides.push((quest, alias_id, value));
            deferred.scene_actor_bindings_dirty = true;
            None
        }
        Effect::SetLocationKeywordData { location, keyword, value } => {
            let location = resolve_property_form_id(vmad, location.property_name())?;
            let keyword = resolve_property_form_id(vmad, keyword.property_name())?;
            let value = evaluate_value(value, context, vmad, world, stages, deferred)?;
            deferred.location_keyword_changes.push((location, keyword, value));
            None
        }
        Effect::SetActorValue {
            actor,
            actor_value,
            value,
        } => {
            let actor = resolve_actor(
                vmad,
                world,
                context,
                actor,
                &deferred.scene_actor_bindings,
                deferred.speaker,
            )?;
            let avif = resolve_property_form_id(vmad, actor_value.property_name())?;
            let value = evaluate_value(value, context, vmad, world, stages, deferred)?;
            set_actor_value_base(world, actor, avif, value);
            None
        }
        Effect::SetActorValueNamed { actor, name, value } => {
            let actor = resolve_actor(
                vmad,
                world,
                context,
                actor,
                &deferred.scene_actor_bindings,
                deferred.speaker,
            )?;
            let Some(avif) = world
                .try_resource::<crate::ActorValueNames>()
                .and_then(|names| names.resolve(name))
            else {
                log::debug!("fragment SetAV skipped: no actor value named '{name}'");
                return None;
            };
            let value = evaluate_value(value, context, vmad, world, stages, deferred)?;
            set_actor_value_base(world, actor, avif, value);
            None
        }
        Effect::DamageActorValue {
            actor,
            actor_value,
            amount,
            restore,
        } => {
            let actor = resolve_actor(
                vmad,
                world,
                context,
                actor,
                &deferred.scene_actor_bindings,
                deferred.speaker,
            )?;
            let avif = match actor_value {
                crate::translate::effects::ActorValueArg::Form(form) => {
                    resolve_property_form_id(vmad, form.property_name())?
                }
                crate::translate::effects::ActorValueArg::Named(name) => world
                    .try_resource::<crate::ActorValueNames>()
                    .and_then(|names| names.resolve(name))?,
            };
            let amount = evaluate_value(amount, context, vmad, world, stages, deferred)?;
            let mut values = world.query_mut::<byroredux_core::ecs::components::ActorValues>()?;
            if values.get_mut(actor).is_none() {
                values.insert(actor, byroredux_core::ecs::components::ActorValues::new());
            }
            let values = values.get_mut(actor).expect("present or inserted");
            if *restore {
                values.restore(avif, amount);
            } else {
                values.apply_damage(avif, amount);
            }
            None
        }
        Effect::ChangePerk { actor, perk, add } => {
            let actor = resolve_actor(
                vmad,
                world,
                context,
                actor,
                &deferred.scene_actor_bindings,
                deferred.speaker,
            )?;
            let perk = resolve_property_form_id(vmad, perk.property_name())?;
            // `Perks` is direct component storage, written in place like
            // `ActorValues` above.
            let mut perks = world.query_mut::<byroredux_core::character::Perks>()?;
            if perks.get_mut(actor).is_none() {
                perks.insert(actor, byroredux_core::character::Perks::default());
            }
            let owned = perks.get_mut(actor).expect("present or inserted");
            if !*add {
                owned.remove(perk);
            } else if owned.rank(perk) == 0 {
                owned.set_rank(perk, 1);
            }
            None
        }
        Effect::ChangeFaction {
            actor,
            faction,
            change,
        } => {
            let actor = resolve_actor(
                vmad,
                world,
                context,
                actor,
                &deferred.scene_actor_bindings,
                deferred.speaker,
            )?;
            let faction = match change {
                crate::FactionChange::RemoveAll => 0,
                _ => resolve_property_form_id(vmad, faction.property_name())?,
            };
            deferred.faction_changes.push((actor, faction, *change));
            None
        }
        Effect::SetQuestVariable {
            quest,
            variable,
            value,
        } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            deferred.quest_variables.push((
                quest,
                crate::QuestScriptVariables::property_key(variable),
                *value,
            ));
            None
        }
        Effect::CopyQuestVariable { quest, from, to } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let from = crate::QuestScriptVariables::property_key(from);
            let to = crate::QuestScriptVariables::property_key(to);
            // The same pending-then-written chain `ModQuestVariable` reads.
            let value = deferred
                .quest_variables
                .iter()
                .rev()
                .find(|(q, k, _)| *q == quest && *k == from)
                .map(|(_, _, value)| *value)
                .or_else(|| crate::quest_variable(world, quest, from))?;
            deferred.quest_variables.push((quest, to, value));
            None
        }
        Effect::ModQuestVariable {
            quest,
            variable,
            delta,
        } => {
            // Read through the same pending-then-written-then-authored chain
            // a guard uses, so an earlier write in this fragment counts.
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let key = crate::QuestScriptVariables::property_key(variable);
            let current = deferred
                .quest_variables
                .iter()
                .rev()
                .find(|(q, k, _)| *q == quest && *k == key)
                .map(|(_, _, value)| *value)
                .or_else(|| crate::quest_variable(world, quest, key))
                .unwrap_or(0.0);
            deferred.quest_variables.push((quest, key, current + delta));
            None
        }
        Effect::SetEnemy { .. }
        | Effect::SetAlly { .. }
        | Effect::StartCombat { .. }
        | Effect::StopCombat { .. }
        | Effect::StopCombatAlarm { .. }
        | Effect::SetCrimeFaction { .. }
        | Effect::SetUnconscious { .. }
        | Effect::SetGhost { .. }
        | Effect::EvaluatePackage { .. } => {
            apply_ai_combat_effect(effect, context, vmad, world, deferred)
        }
        Effect::AddSpell { actor, spell } | Effect::RemoveSpell { actor, spell } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings, deferred.speaker)?;
            let spell = resolve_property_form_id(vmad, spell.property_name())?;
            let adding = matches!(effect, Effect::AddSpell { .. });
            let changed = if adding {
                crate::magic::add_spell(world, actor, spell)
            } else {
                crate::magic::remove_spell(world, actor, spell)
            };
            // #4822 — never drop the result silently. `magic` itself warns
            // when the actor has no `SpellList` (a lost effect); what is left
            // here is Papyrus' own idempotent no-op.
            if !changed {
                let verb = if adding { "AddSpell" } else { "RemoveSpell" };
                log::debug!(
                    "quest {context:?}: {verb}({spell:08X}) on entity {actor} changed nothing"
                );
            }
            None
        }
        // #4415 — the script-defined racial pair. The racial set was
        // level-resolved and stamped as `RaceSpells` at spawn, so the apply
        // needs no index; each member goes through the same add/remove the
        // scripted `AddSpell` uses (constant modifiers included).
        Effect::AddRaceSpells { actor } | Effect::RemoveRaceSpells { actor } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings, deferred.speaker)?;
            let adding = matches!(effect, Effect::AddRaceSpells { .. });
            let changed = if adding {
                crate::magic::add_race_spells(world, actor)
            } else {
                crate::magic::remove_race_spells(world, actor)
            };
            if !changed {
                let verb = if adding {
                    "AddRaceSpells"
                } else {
                    "RemoveRaceSpells"
                };
                log::debug!("quest {context:?}: {verb} on entity {actor} changed nothing");
            }
            None
        }
        Effect::Wait { .. } | Effect::WaitForActors3DLoaded { .. } | Effect::WaitWhile { .. } => {
            None
        }
        Effect::Conditional { .. } | Effect::QuestScope { .. } | Effect::Interpret { .. } => {
            unreachable!("conditional and quest-scoped effects are expanded by apply_effects")
        }
        _ => apply_quest_scoped_effect(effect, context, vmad, stages, objectives, deferred),
    }
}

/// Apply a whole fragment's effects, returning the chained
/// [`QuestStageAdvanced`]s its `SetStage`s produced. Resource reads use the
/// pre-lock snapshot in `deferred`; queued mutations must be applied only after
/// releasing the quest-state guards passed here.
pub fn apply_effects(
    effects: &[Effect],
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &mut QuestStageState,
    objectives: &mut QuestObjectiveState,
    deferred: &mut DeferredFragmentEffects,
) -> Vec<QuestStageAdvanced> {
    let mut advances = Vec::new();
    // #3935 — an explicit cursor stack, not recursion. `Effect::Conditional`
    // used to build `branch ++ effects[index + 1..]` and call `apply_effects`
    // on it, so a fragment with N *sequential* conditionals cost N stack
    // frames and cloned the whole remaining program at each one — O(N^2)
    // live `Effect` clones. `MAX_CONDITIONAL_DEPTH` bounds conditional
    // *nesting* at lowering, not the sequential count, and a `.pex` function
    // may carry 65 535 instructions, so N was bounded only by the wire
    // format. Descending now pushes two borrowed slices (the branch and the
    // rest of the current program) and loops; nothing is cloned, and the
    // stack holds slices, not frames.
    let mut pending: Vec<&[Effect]> = Vec::new();
    let mut current: &[Effect] = effects;
    loop {
        let mut halted = false;
        for (index, effect) in current.iter().enumerate() {
            if let Effect::ProviderCall(call) = effect {
                deferred.provider_steps.push(DeferredProviderFragmentStep {
                    call: call.clone(),
                    context,
                    vmad: vmad.cloned(),
                    tail: remaining_program(current, index, &pending),
                });
                halted = true;
                break;
            }
            // M47.2 — a fragment the lowering declined, run by the Papyrus VM.
            if let Effect::Interpret {
                script,
                function,
                args,
                resume,
            } = effect
            {
                // The frame's interpreter budget is spent: this run, and the
                // rest of the program behind it, start on a later frame.
                if super::budget::exhausted(world) {
                    let mut tail = vec![effect.clone()];
                    tail.extend(remaining_program(current, index, &pending));
                    if super::budget::defer(world, context, vmad, tail).is_ok() {
                        halted = true;
                        break;
                    }
                }
                let (run, suspended) = super::vm::run_fragment(
                    script,
                    function,
                    args,
                    resume.as_ref(),
                    context,
                    vmad,
                    world,
                    stages,
                    objectives,
                    deferred,
                );
                advances.extend(run);
                // A `Utility.Wait`: the run resumes there, then the rest of
                // the program follows.
                if let Some((seconds, state)) = suspended {
                    let mut tail = vec![Effect::Interpret {
                        script: script.clone(),
                        function: function.clone(),
                        args: Vec::new(),
                        resume: Some(state),
                    }];
                    tail.extend(remaining_program(current, index, &pending));
                    if let Some(mut queue) = world.try_resource_mut::<FragmentExecutionQueue>() {
                        queue.pending.push(PendingFragmentExecution {
                            context,
                            vmad: vmad.cloned(),
                            effects: tail,
                            remaining_seconds: seconds,
                            resume_when: FragmentResumeCondition::DelayElapsed,
                        });
                    } else {
                        log::debug!("Papyrus VM continuation dropped: FragmentExecutionQueue is unavailable");
                    }
                    halted = true;
                    break;
                }
                continue;
            }
            // M47.2 — a quest-script body inlined into an INFO or scene
            // fragment runs against its quest: that quest is the context and
            // its VMAD binds the properties. It holds nothing latent, so it
            // runs to completion here.
            if let Effect::QuestScope {
                quest,
                effects: scoped,
                bindings,
            } = effect
            {
                let Some(quest) = resolve_quest_logged(quest, context, vmad) else {
                    continue;
                };
                let scripts = deferred.quest_scripts.clone();
                let Some(quest_vmad) = scripts.get(&quest) else {
                    log::debug!("fragment QuestScope skipped: quest {:08X} has no VMAD", quest.0);
                    continue;
                };
                // M47.2 — the inlined function's object arguments that name
                // this fragment's properties: the quest's VMAD plus one
                // script carrying each as the parameter's property, valued
                // as the fragment's.
                let bound;
                let quest_vmad = if bindings.is_empty() {
                    quest_vmad
                } else {
                    let mut merged = quest_vmad.clone();
                    merged.scripts.push(byroredux_plugin::esm::records::script_instance::ScriptInstance {
                        name: "<inline arguments>".into(),
                        status: 0,
                        properties: bindings
                            .iter()
                            .filter_map(|(parameter, argument)| {
                                let value = vmad?
                                    .scripts
                                    .iter()
                                    .find_map(|script| script.property(argument))?
                                    .value
                                    .clone();
                                Some(byroredux_plugin::esm::records::script_instance::ScriptProperty {
                                    name: parameter.clone(),
                                    status: 1,
                                    value,
                                })
                            })
                            .collect(),
                    });
                    bound = merged;
                    &bound
                };
                advances.extend(apply_effects(
                    scoped,
                    quest,
                    Some(quest_vmad),
                    world,
                    stages,
                    objectives,
                    deferred,
                ));
                continue;
            }
            if let Effect::Conditional {
                guards,
                then_effects,
                else_effects,
            } = effect
            {
                // #3785 — `is_some_and` collapsed "the guard evaluated false" and
                // "the guard's quest ref could not be resolved at all" into the
                // same `false`, and unlike every sibling `resolve_quest_logged`
                // caller (which simply skips the one effect via `?`), `false`
                // here is NOT inert — it selects `else_effects` and runs them.
                // An unresolvable guard must decline the WHOLE Conditional
                // (neither branch), matching the decline-on-unmodeled-reference
                // invariant every other site already follows.
                let mut resolved = true;
                let passes = guards.iter().all(|guard| {
                    match evaluate_guard(guard, context, vmad, world, stages, objectives, deferred) {
                        Some(pass) => pass,
                        None => {
                            resolved = false;
                            false
                        }
                    }
                });
                if !resolved {
                    // `resolve_quest_logged` already emitted a debug line per
                    // unresolved guard (correct for its inert callers); this is
                    // the one site where the consequence is a chosen branch, so
                    // it gets its own louder diagnostic.
                    log::warn!(
                        "fragment effect: Conditional guard's quest ref could not be resolved — \
                     declining the whole Conditional (neither then_effects nor else_effects run)"
                    );
                    continue;
                }
                let branch = if passes { then_effects } else { else_effects };
                // Push the continuation first, then the branch: the stack pops
                // the branch next, so the visible order is still
                // `branch ++ rest-of-program`, exactly as the flattened
                // recursion produced.
                pending.push(&current[index + 1..]);
                pending.push(branch.as_slice());
                break;
            }
            // M47.2 — a `While … Wait` poll: while its guards hold, suspend
            // with this effect at the head of the continuation, so the resume
            // checks again.
            if let Effect::WaitWhile {
                guards,
                poll_seconds,
            } = effect
            {
                let mut resolved = true;
                let holds = guards.iter().all(|guard| {
                    match evaluate_guard(guard, context, vmad, world, stages, objectives, deferred) {
                        Some(pass) => pass,
                        None => {
                            resolved = false;
                            false
                        }
                    }
                });
                if !resolved {
                    log::debug!("fragment WaitWhile ended: a guard could not be read");
                }
                if holds {
                    let mut tail = vec![effect.clone()];
                    tail.extend(remaining_program(current, index, &pending));
                    if let Some(mut queue) = world.try_resource_mut::<FragmentExecutionQueue>() {
                        queue.pending.push(PendingFragmentExecution {
                            context,
                            vmad: vmad.cloned(),
                            effects: tail,
                            remaining_seconds: *poll_seconds,
                            resume_when: FragmentResumeCondition::DelayElapsed,
                        });
                    } else {
                        log::debug!(
                            "fragment continuation dropped: FragmentExecutionQueue is unavailable"
                        );
                    }
                    halted = true;
                    break;
                }
                continue;
            }
            let suspension = match effect {
                Effect::Wait { seconds } => Some((*seconds, FragmentResumeCondition::DelayElapsed)),
                Effect::WaitForActors3DLoaded {
                    actors,
                    poll_seconds,
                } if !actors_3d_loaded(
                    vmad,
                    world,
                    context,
                    actors,
                    &deferred.scene_actor_bindings,
                ) =>
                {
                    Some((
                        *poll_seconds,
                        FragmentResumeCondition::Actors3DLoaded {
                            actors: actors.clone(),
                            poll_seconds: *poll_seconds,
                            elapsed_seconds: 0.0,
                        },
                    ))
                }
                Effect::WaitForActors3DLoaded { .. } => None,
                _ => None,
            };
            if let Some((remaining_seconds, resume_when)) = suspension {
                let tail = remaining_program(current, index, &pending);
                if !tail.is_empty() {
                    if let Some(mut queue) = world.try_resource_mut::<FragmentExecutionQueue>() {
                        queue.pending.push(PendingFragmentExecution {
                            context,
                            vmad: vmad.cloned(),
                            effects: tail,
                            remaining_seconds,
                            resume_when,
                        });
                    } else {
                        log::debug!(
                            "fragment continuation dropped: FragmentExecutionQueue is unavailable"
                        );
                    }
                }
                halted = true;
                break;
            }
            if matches!(effect, Effect::WaitForActors3DLoaded { .. }) {
                continue;
            }
            if let Some(advance) =
                apply_effect(effect, context, vmad, world, stages, objectives, deferred)
            {
                advances.push(advance);
            }
        }
        if halted {
            break;
        }
        // Either the current slice ran to its end, or a Conditional just
        // pushed its branch — both continue at the top of the stack.
        match pending.pop() {
            Some(next) => current = next,
            None => break,
        }
    }
    advances
}

/// The program still to run after `current[index]`: the rest of the current
/// slice followed by everything still on the cursor stack, in execution
/// order (#3935).
///
/// A suspension or provider barrier hands its continuation to another frame
/// (`FragmentExecutionQueue` / `DeferredFragmentEffects`), which outlives
/// the borrowed slices, so this is the one place the remainder is cloned —
/// once, at the point it actually has to be owned. `pending` is a stack, so
/// its execution order is back-to-front.
fn remaining_program(current: &[Effect], index: usize, pending: &[&[Effect]]) -> Vec<Effect> {
    let mut tail = current[index + 1..].to_vec();
    for slice in pending.iter().rev() {
        tail.extend_from_slice(slice);
    }
    tail
}
