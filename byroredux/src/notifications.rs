//! Transient player feedback, independent of debug-panel visibility.
use byroredux_core::ecs::{Resource, World};
use std::collections::VecDeque;

const MAX_PENDING: usize = 8;

#[derive(Default)]
pub(crate) struct PlayerNotifications(VecDeque<String>);
impl Resource for PlayerNotifications {}

pub(crate) fn push(world: &World, message: impl Into<String>) {
    let Some(mut queue) = world.try_resource_mut::<PlayerNotifications>() else {
        return;
    };
    if queue.0.len() == MAX_PENDING {
        queue.0.pop_front();
    }
    queue.0.push_back(message.into());
}

pub(crate) fn drain(world: &World) -> Vec<String> {
    world
        .try_resource_mut::<PlayerNotifications>()
        .map(|mut queue| queue.0.drain(..).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feedback_is_bounded_ordered_and_consumed_once() {
        let mut world = World::new();
        world.insert_resource(PlayerNotifications::default());
        for i in 0..12 {
            push(&world, format!("Message {i}"));
        }
        assert_eq!(
            drain(&world),
            (4..12).map(|i| format!("Message {i}")).collect::<Vec<_>>()
        );
        assert!(drain(&world).is_empty());
    }
}
