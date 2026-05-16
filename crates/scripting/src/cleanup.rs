//! Event cleanup system — removes transient marker components.
//!
//! Runs at the end of each frame to clear event markers, ensuring
//! events are only visible for one frame. This is the ECS equivalent
//! of "clearing the event queue."

use crate::events::{ActivateEvent, AnimationTextKeyEvents, HitEvent, TimerExpired};
use crate::papyrus_demo::mg07_door::UiMessageCommand;
use crate::papyrus_demo::{CameraShakeCommand, ControllerRumbleCommand};
use crate::quest_stages::QuestStageAdvanced;
use crate::recurring_update::OnUpdateEvent;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;

/// System: remove all transient event marker components.
///
/// Must be registered as the LAST system in the scheduler so all
/// gameplay systems have a chance to process events before cleanup.
///
/// Every new marker component introduced in the R5 prototype work
/// is added here in lockstep. The contract: if a marker is meant to
/// be visible for exactly one frame (the standard "transient event"
/// pattern), it goes here. Subscriptions (e.g.
/// [`crate::RecurringUpdate`]) deliberately do NOT — they outlive
/// individual frames and are removed by the script's own
/// `UnregisterFor*` logic.
pub fn event_cleanup_system(world: &World, _dt: f32) {
    drain_component::<ActivateEvent>(world);
    drain_component::<HitEvent>(world);
    drain_component::<TimerExpired>(world);
    drain_component::<AnimationTextKeyEvents>(world);
    // R5 prototype additions — all transient-by-design markers.
    drain_component::<OnUpdateEvent>(world);
    drain_component::<QuestStageAdvanced>(world);
    drain_component::<CameraShakeCommand>(world);
    drain_component::<ControllerRumbleCommand>(world);
    drain_component::<UiMessageCommand>(world);
}

/// Remove all instances of a component type from every entity.
fn drain_component<T: byroredux_core::ecs::storage::Component>(world: &World) {
    let Some(mut query) = world.query_mut::<T>() else {
        return;
    };
    let entities: Vec<EntityId> = query.iter().map(|(id, _)| id).collect();
    for entity in entities {
        query.remove(entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{ActivateEvent, HitEvent, TimerExpired};
    use byroredux_core::ecs::world::World;

    fn setup_world() -> World {
        let mut world = World::new();
        crate::register(&mut world);
        world
    }

    #[test]
    fn cleanup_removes_all_event_types() {
        let mut world = setup_world();
        let a = world.spawn();
        let b = world.spawn();
        let c = world.spawn();

        world.insert(a, ActivateEvent { activator: 99 });
        world.insert(
            b,
            HitEvent {
                aggressor: 1,
                source: 2,
                projectile: 3,
                power_attack: false,
                sneak_attack: false,
                bash_attack: false,
                blocked: false,
            },
        );
        world.insert(c, TimerExpired { timer_id: 5 });

        event_cleanup_system(&world, 0.0);

        assert!(!world.has::<ActivateEvent>(a));
        assert!(!world.has::<HitEvent>(b));
        assert!(!world.has::<TimerExpired>(c));
    }

    #[test]
    fn cleanup_preserves_non_event_components() {
        use byroredux_core::ecs::components::Transform;

        let mut world = setup_world();
        let e = world.spawn();
        world.insert(e, Transform::IDENTITY);
        world.insert(e, ActivateEvent { activator: 1 });

        event_cleanup_system(&world, 0.0);

        assert!(!world.has::<ActivateEvent>(e));
        assert!(world.has::<Transform>(e));
    }
}
