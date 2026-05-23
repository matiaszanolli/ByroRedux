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
pub mod quest_stages;
pub mod recurring_update;
pub mod registry;
pub mod timer;

pub use cleanup::event_cleanup_system;
pub use events::{
    ActivateEvent, AnimationTextKeyEvent, AnimationTextKeyEvents, HitEvent, TimerExpired,
};
pub use recurring_update::{recurring_update_tick_system, OnUpdateEvent, RecurringUpdate};
pub use registry::{ScriptRegistry, ScriptSpawnFn};
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
    recurring_update::register(world);
    // M47.0 Phase 1 — register the R5 prototype storages so
    // `papyrus_demo` scripts can attach their state components when
    // their owning REFR spawns. Without this call, `query_mut::<…>`
    // returns None for every script-state component and the demo
    // systems no-op. The cell-loader-driven attach lands in Phase 3;
    // this Phase 1 step is the minimum plumbing that makes the demo
    // surface live at runtime. See docs/engine/m47-0-design.md.
    papyrus_demo::register(world);
    log::info!("Scripting subsystem initialized (ECS events + timers + papyrus_demo)");
}
