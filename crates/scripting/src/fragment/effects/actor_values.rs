//! Actor-value writes and damage. (#6142, split from `apply_effect`)

use super::*;

/// `SetActorValue`, `SetActorValueNamed` and `DamageActorValue`.
///
/// #6142 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays named in
/// `apply_effect`'s nested-lock residual list, which the #3949 scan reads
/// across these helpers too.
pub(super) fn apply_actor_values_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &mut QuestStageState,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
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
            queue_health_death(world, actor);
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
            queue_health_death(world, actor);
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
            {
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
            }
            if !*restore {
                queue_health_death(world, actor);
            }
            None
        }
        _ => unreachable!("`apply_effect` only routes actor values effects here"),
    }
}
