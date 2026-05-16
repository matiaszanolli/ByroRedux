//! Scripting subsystem — ECS-native event system.
//!
//! Replaces the Papyrus VM with native ECS patterns:
//! - Script state → component fields
//! - Script logic → ECS systems
//! - Events → transient marker components
//! - Timers → ScriptTimer component + tick system
//!
//! The core lifecycle: event marker appears → systems process it →
//! cleanup removes it at end of frame.

pub mod cleanup;
pub mod events;
pub mod papyrus_demo;
pub mod timer;

pub use cleanup::event_cleanup_system;
pub use events::{
    ActivateEvent, AnimationTextKeyEvent, AnimationTextKeyEvents, HitEvent, TimerExpired,
};
pub use timer::{timer_tick_system, ScriptTimer};

use byroredux_core::ecs::world::World;

/// Register all scripting component storages in the world.
///
/// Call during setup so that `query_mut()` works for event markers
/// before any entity has triggered an event.
pub fn register(world: &mut World) {
    world.register::<ActivateEvent>();
    world.register::<HitEvent>();
    world.register::<TimerExpired>();
    world.register::<AnimationTextKeyEvents>();
    world.register::<ScriptTimer>();
    log::info!("Scripting subsystem initialized (ECS events + timers)");
}
