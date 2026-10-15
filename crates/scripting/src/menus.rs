//! Menus scripts open (M47.2): the requests a fragment raises for the
//! engine's native pages to present.
//!
//! `Actor.ShowBarterMenu()` opens the barter menu with the actor's vendor
//! faction; the binary owns the menu (prices, the merchant container, the
//! page). The request is presentation, raised and consumed within a frame.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Resource, World};

/// The vendor a script asked the player to barter with this frame.
/// `NOT_SAVED_BY_DESIGN`: consumed the frame it is raised.
#[derive(Debug, Default)]
pub struct PendingBarterMenu(pub Option<EntityId>);

impl Resource for PendingBarterMenu {}

/// Ask the engine to open the barter menu with `vendor`.
pub fn request_barter(world: &World, vendor: EntityId) {
    match world.try_resource_mut::<PendingBarterMenu>() {
        Some(mut pending) => pending.0 = Some(vendor),
        None => log::debug!("ShowBarterMenu skipped: no barter menu installed"),
    }
}

/// Take this frame's barter request.
pub fn take_barter_request(world: &World) -> Option<EntityId> {
    world
        .try_resource_mut::<PendingBarterMenu>()
        .and_then(|mut pending| pending.0.take())
}

pub fn register(world: &mut World) {
    world.init_resource::<PendingBarterMenu>();
}
