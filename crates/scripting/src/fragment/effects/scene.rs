//! The scene effect applier. (#5468, split from `effects.rs`)

use super::*;

/// `StartScene` / `StopScene` / `Activate` / `SetOpen` — the scene-registry
/// arms. `StartScene` doubles as Papyrus' quest `Start`/`Stop` (the
/// decompiled AST does not retain a property's declared type), which is why
/// this family is the one that needs `stages` and can return an advance.
///
/// #4340 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays
/// named in `apply_effect`'s nested-lock residual list, which the #3949
/// scan now reads across these helpers too.
pub(super) fn apply_scene_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &mut QuestStageState,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::StartScene { scene } | Effect::StopScene { scene } => {
            let scene_form_id = resolve_property_form_id(vmad, scene.property_name())?;
            let scene_entity = world
                .try_resource::<crate::scene::SceneRegistry>()
                .and_then(|registry| registry.scene_entity(scene_form_id));
            if let Some(scene_entity) = scene_entity {
                if matches!(effect, Effect::StartScene { .. }) {
                    if let Some(mut requests) = world.query_mut::<crate::scene::SceneStartRequest>()
                    {
                        requests.insert(scene_entity, crate::scene::SceneStartRequest);
                    }
                } else if let Some(mut requests) =
                    world.query_mut::<crate::scene::SceneStopRequest>()
                {
                    requests.insert(scene_entity, crate::scene::SceneStopRequest);
                }
                return None;
            }

            // `Start`/`Stop` are shared by Scene and Quest in Papyrus, while
            // the decompiled AST does not retain a property's declared type.
            // Resolve the FormID against installed definitions at dispatch.
            let quest = QuestFormId(scene_form_id);
            let (start_up_stage, shut_down_stage) = {
                let definitions = deferred.quest_definitions.as_ref()?;
                if !definitions.contains(quest) {
                    log::debug!(
                        "fragment Start/Stop skipped: '{}' resolved to unknown form {scene_form_id:08X}",
                        scene.property_name()
                    );
                    return None;
                }
                (
                    definitions.start_up_stage(quest),
                    definitions.shut_down_stage(quest),
                )
            };
            if matches!(effect, Effect::StartScene { .. }) {
                let event = stages.start_quest(quest, start_up_stage);
                if event.is_some() {
                    deferred.scene_actor_bindings_dirty = true;
                }
                event
            } else {
                // #5812 — the shared stop lifecycle (running-only
                // shut-down stage advance + the stop itself).
                let (stopped, event) = stages.stop_with_shutdown(quest, shut_down_stage);
                if stopped {
                    deferred.scene_actor_bindings_dirty = true;
                }
                event
            }
        }
        Effect::Activate { target, activator, native_only } => {
            let target_entity =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)?;
            let activator = match activator {
                Some(activator) => resolve_object(
                    vmad,
                    world,
                    context,
                    activator,
                    &deferred.scene_actor_bindings,
                )?,
                None => world
                    .try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
                    .map(|player| player.0)
                    .unwrap_or(target_entity),
            };
            // Queue rather than insert: `quest_fragment_dispatch_system` is
            // the LAST producer of `ActivateEvent` in Stage::Update, but
            // most consumers (container loot, rumble, quest_advance,
            // two_state_activator, mg07) are scheduled earlier and
            // `event_cleanup_system` drains the marker at Stage::Late the
            // same frame — so a directly-inserted marker reached none of
            // them, ever (#2654). `fragment_activation_flush_system` turns
            // this into a real `ActivateEvent` at the head of the next
            // frame, ahead of every consumer.
            if *native_only {
                deferred.native_activations.push((target_entity, activator));
            } else {
                deferred.activations.push((target_entity, activator));
            }
            None
        }
        Effect::SetOpen { target, open } => {
            let target_entity =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)?;
            if !crate::vm_state::set_two_state_open(world, target_entity, *open) {
                log::debug!(
                    "fragment SetOpen skipped: '{}' is not a recognized two-state activator",
                    target.property_name()
                );
            }
            None
        }
        _ => unreachable!("apply_scene_effect received a non-scene effect"),
    }
}
