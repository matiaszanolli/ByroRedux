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
mod guards;
mod actor_lists;
mod actor_values;
mod location_alias;
mod messages;
mod player_stats;
mod quest_vars;
mod reference_state;

pub(crate) use resolve::*;
pub use deferred::*;
pub use inventory::*;
use placement::*;
pub(crate) use placement::apply_move_to_effect;
use scene::*;
use lock::*;
use player_control::*;
use vehicle_cinematic::*;
use ai_combat::*;
use quest_scoped::*;
use actor_lists::*;
use actor_values::*;
use guards::*;
use location_alias::*;
use messages::*;
use player_stats::*;
use quest_vars::*;
use reference_state::*;
pub(in crate::fragment) use evaluate::*;
pub(crate) use evaluate::{
    clear_actor_value_damage, evaluate_guard_guard_free, evaluate_value_guard_free,
    mod_actor_value_permanent, queue_health_death_by,
};

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
///     `ActorControlState`, `HorseTetherState`,
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
///   - **via [`crate::package::request_package_evaluation`]** — `EvaluatePackageRequest`
///     (write, `Effect::EvaluatePackage`; the ObScript `EVP` command shares it)
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
///   - **via the inventory transfers** (`remove_items` / `add_items`, called inline
///     from the inventory family and the placement arms, #5986) — `Inventory`,
///     `EquipmentSlots`, `EquippedWeapon`, `ItemInstancePool`, and through
///     `crate::equipment::{emit_equipment_changes, emit_item_transfers}` the
///     `EquipmentEventBatch` and `ItemEventBatch` markers
///   - **via the guard / value evaluators** (`evaluate_guard`, `evaluate_value`,
///     #5986) — `Dead`, `FollowerState`, `LocationTable`, `MiscStats`,
///     `StoryManagerRng` (the `GetRandomPercent` draw) and the `ActorVitals`
///     reads, each taken and released alone
///   - **via the latent-continuation path** (#5986; this list used to say the
///     `FragmentExecutionQueue` write was *not* here, which was false) —
///     `apply_effects` and `budget::defer` take `FragmentExecutionQueue` (write) for
///     a `Wait` / `WaitWhile` / unstarted `Interpret` run, and `VmFrameBudget`
///
/// **The Papyrus VM runs under these same guards.** An `Effect::Interpret` reaches
/// `run_fragment` from `apply_effects`, and the interpreter's natives take well
/// over twenty further component / resource locks (`Inventory`, `SpellList`,
/// `Seated`, `PapyrusReferenceScripts`, `InputLayers`, `ConditionForms`,
/// `HavokImpulseRules`, `EvaluatePackageRequest`, `VmFrameBudget` write, ...).
/// This list inventories the effect model's arms and the helpers named above, not
/// that open-ended set, so it is **not** the safety argument. The invariant is:
/// everything run under the paired quest guards — effect arm, helper or VM
/// native — relies on exclusive scheduling below, and no component or resource
/// in this crate may take `QuestStageState` / `QuestObjectiveState` while holding
/// another lock. The #3949 scan below pins what the effect arms and the helpers
/// above acquire; it does not follow a native.
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
        Effect::SetGlobalValue { .. }
        | Effect::SetReferenceVariable { .. }
        | Effect::SetLocal { .. }
        | Effect::SetQuestVariable { .. }
        | Effect::CopyQuestVariable { .. }
        | Effect::ModQuestVariable { .. } => {
            apply_quest_vars_effect(effect, context, vmad, world, stages, deferred)
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
        Effect::SendStoryEvent { .. }
        | Effect::AddToMap { .. }
        | Effect::ShowMessage { .. }
        | Effect::ShowHelpMessage { .. }
        | Effect::Notification { .. }
        | Effect::ShowBarterMenu { .. } => {
            apply_messages_effect(effect, context, vmad, world, deferred)
        }
        Effect::DeleteWhenAble { .. }
        | Effect::ChangeKeyword { .. }
        | Effect::SetLinkedRef { .. }
        | Effect::SetLocRefType { .. }
        | Effect::SetRelationshipRank { .. }
        | Effect::Kill { .. }
        | Effect::SetBaseProtection { .. }
        | Effect::BlockActivation { .. } => {
            apply_reference_state_effect(effect, context, vmad, world, deferred)
        }
        Effect::RewardPlayerXp { .. }
        | Effect::SetPlayerTeammate { .. }
        | Effect::SetDoingFavor { .. }
        | Effect::AdvanceSkill { .. }
        | Effect::AddAchievement { .. }
        | Effect::IncrementStat { .. }
        | Effect::PlayerCaps { .. } => {
            apply_player_stats_effect(effect, context, vmad, world, stages, deferred)
        }
        Effect::SetLocationCleared { .. }
        | Effect::ForceLocationTo { .. }
        | Effect::ForceAlias { .. }
        | Effect::SetLocationKeywordData { .. } => {
            apply_location_alias_effect(effect, context, vmad, world, stages, deferred)
        }
        Effect::SetActorValue { .. }
        | Effect::SetActorValueNamed { .. }
        | Effect::DamageActorValue { .. } => {
            apply_actor_values_effect(effect, context, vmad, world, stages, deferred)
        }
        Effect::ChangePerk { .. }
        | Effect::ChangeFaction { .. }
        | Effect::AddSpell { .. }
        | Effect::RemoveSpell { .. }
        | Effect::AddRaceSpells { .. }
        | Effect::RemoveRaceSpells { .. } => {
            apply_actor_lists_effect(effect, context, vmad, world, deferred)
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
                        queue.push(PendingFragmentExecution {
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
                        queue.push(PendingFragmentExecution {
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
                        queue.push(PendingFragmentExecution {
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
