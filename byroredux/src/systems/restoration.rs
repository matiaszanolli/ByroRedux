//! Tick persistent restorative ingestible effects in simulation time.
use byroredux_core::ecs::components::{ActorValues, ActorVitals, Dead, TimedRestorations};
use byroredux_core::ecs::World;

pub(crate) fn restoration_system(world: &World, dt: f32) {
    if !dt.is_finite() || dt <= 0.0 {
        return;
    }
    // Snapshot eligibility before acquiring either writable component. Do not
    // revive a dead actor, including the interval before Dead is reconciled.
    let Some(active) = world.query::<TimedRestorations>() else {
        return;
    };
    let targets: Vec<_> = active
        .iter()
        .filter(|(_, e)| !e.effects.is_empty())
        .map(|(entity, _)| entity)
        .collect();
    drop(active);
    for entity in targets {
        let Some(health) = world.get::<ActorVitals>(entity).map(|v| v.health) else {
            continue;
        };
        let dead = world.get::<Dead>(entity).is_some();
        let Some((mut active, mut values)) =
            world.query_2_mut_mut::<TimedRestorations, ActorValues>()
        else {
            return;
        };
        let Some(effects) = active.get_mut(entity) else {
            continue;
        };
        let Some(values) = values.get_mut(entity) else {
            continue;
        };
        if dead || !values.current(health).is_finite() || values.current(health) <= 0.0 {
            effects.effects.clear();
        } else {
            effects.advance(values, dt);
        }
    }
}
